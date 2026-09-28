param([string]$Binary = 'target/debug/findanything.exe', [string]$Output = 'native-smoke')

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class NativeSmoke {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window, ref Point point);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
    private delegate bool EnumWindowsProc(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    public static IntPtr FindLauncher(int processId) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, _) => {
            GetWindowThreadProcessId(window, out uint owner);
            var title = new StringBuilder(256);
            GetWindowText(window, title, title.Capacity);
            if (owner == processId && IsWindowVisible(window) && title.ToString() == "Find Anything"
                && GetClientRect(window, out Rect rect) && rect.Right > 0 && rect.Bottom > 0) {
                found = window;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    public static void ClickClient(IntPtr window, int logicalX, int logicalY) {
        double scale = GetDpiForWindow(window) / 96.0;
        var point = new Point { X = (int)Math.Round(logicalX * scale), Y = (int)Math.Round(logicalY * scale) };
        if (!ClientToScreen(window, ref point) || !SetCursorPos(point.X, point.Y))
            throw new InvalidOperationException("Could not position native pointer");
        mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
        mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
    }
}
'@

function Check($Condition, [string]$Message) { if (!$Condition) { throw $Message } }

$Binary = (Resolve-Path $Binary).Path
Check (!(Test-Path $Output)) "Output path already exists (use a unique path): $Output"
$Output = (New-Item -ItemType Directory -Path $Output).FullName

function Start-Fixture([string[]]$Arguments) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Binary
    $info.UseShellExecute = $false
    foreach ($argument in $Arguments) { [void]$info.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    Check ($process.Start()) 'Could not start native fixture'
    return $process
}

function Stop-Fixture([Diagnostics.Process]$Process) {
    if (!$Process.HasExited) {
        $Process.Kill($true)
        Check ($Process.WaitForExit(5000)) 'Fixture process would not stop'
    }
    $Process.Dispose()
}

function Wait-Exit([Diagnostics.Process]$Process, [string]$Description) {
    if (!$Process.WaitForExit(15000)) {
        Stop-Fixture $Process
        throw "Timed out waiting for $Description"
    }
    Check ($Process.ExitCode -eq 0) "$Description exited with code $($Process.ExitCode)"
}

function Read-Probe([string]$Path, [scriptblock]$Ready, [string]$Description) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        if (Test-Path $Path) {
            try {
                $state = Get-Content -Raw $Path | ConvertFrom-Json
                if (& $Ready $state) { return $state }
            } catch { } # The renderer may be replacing the probe while it is read.
        }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    $lastState = if (Test-Path $Path) { Get-Content -Raw $Path } else { '(missing)' }
    throw "Timed out waiting for probe: $Description; foreground=$([NativeSmoke]::GetForegroundWindow()); state=$lastState"
}

function Assert-Results($State, [int]$Count) {
    Check ($State.results.Count -eq $Count) "Expected $Count results, got $($State.results.Count)"
}

# These are renderer-owned captures: each isolated fixture writes a GPU image and
# its state sidecar, then exits. No live profile, discovery, or network is used.
$expectedTitles = @('Browser', 'Displays', 'Project notes.md')
$sizes = @{}
foreach ($stateName in @('results', 'selected', 'query', 'minimum', 'empty', 'error')) {
    $png = Join-Path $Output "windows-$stateName.png"
    $process = Start-Fixture @('--fixture', $stateName, '--screenshot', $png)
    Wait-Exit $process "$stateName screenshot"
    $process.Dispose()
    $json = [IO.Path]::ChangeExtension($png, '.json')
    Check ((Test-Path $png) -and (Test-Path $json)) "$stateName did not write both snapshot files"
    $snapshot = Get-Content -Raw $json | ConvertFrom-Json
    $image = [Drawing.Image]::FromFile($png)
    try {
        Check ($image.Width -gt 0 -and $image.Height -gt 0) "$stateName screenshot is empty"
        $sizes[$stateName] = @($image.Width, $image.Height)
    } finally { $image.Dispose() }

    switch ($stateName) {
        'results'  { Check (($snapshot.results -join '|') -eq ($expectedTitles -join '|')) 'Results snapshot changed'; Check ($snapshot.selected -eq 0) 'Results selection must be zero' }
        'selected' { Check (($snapshot.results -join '|') -eq ($expectedTitles -join '|')) 'Selected snapshot results changed'; Check ($snapshot.selected -eq 1) 'Selected fixture must select index 1' }
        'query'    { Check ($snapshot.query -eq 'display') 'Query fixture text changed'; Check (($snapshot.results -join '|') -eq 'Displays') 'Query fixture must contain only Displays' }
        'minimum'  { Check (($snapshot.results -join '|') -eq ($expectedTitles -join '|')) 'Minimum snapshot results changed' }
        'empty'    { Assert-Results $snapshot 0; Check ($null -eq $snapshot.error) 'Empty fixture unexpectedly has an error' }
        'error'    { Assert-Results $snapshot 0; Check (![string]::IsNullOrWhiteSpace([string]$snapshot.error)) 'Error fixture has no error' }
    }
}
Check ($sizes.minimum[0] -lt $sizes.results[0] -and $sizes.minimum[1] -lt $sizes.results[1]) 'Minimum screenshot must be smaller than the default screenshot'

# Exercise the actual Windows/winit input route. The probe only observes state;
# it never mutates it. Forms SendKeys emits OS keyboard input to the foreground
# window and clicks are real user32 pointer events.
$probe = Join-Path $Output 'interactive.json'
Set-Clipboard -Value '資料🚀'
Check ((Get-Clipboard -Raw) -eq '資料🚀') 'Unicode clipboard preparation failed'
$process = Start-Fixture @('--fixture', 'results', '--probe', $probe)
try {
    $initial = Read-Probe $probe { param($s) $s.results.Count -eq 3 -and $s.queryFocused } 'initial focused results'
    for ($i = 0; $i -lt 100; $i++) {
        $process.Refresh()
        $window = [NativeSmoke]::FindLauncher($process.Id)
        if ($window -ne [IntPtr]::Zero) { break }
        Check (!$process.HasExited) 'Interactive fixture exited before opening a window'
        Start-Sleep -Milliseconds 50
    }
    Check ($window -ne [IntPtr]::Zero) 'Interactive fixture has no native window'
    Check ([NativeSmoke]::SetForegroundWindow($window)) 'Could not foreground fixture window'
    Start-Sleep -Milliseconds 150

    $rect = [NativeSmoke+Rect]::new()
    Check ([NativeSmoke]::GetClientRect($window, [ref]$rect)) 'Could not read client size'
    $scale = [NativeSmoke]::GetDpiForWindow($window) / 96.0
    Check ([Math]::Abs(($rect.Right / $scale) - 680) -le 1 -and [Math]::Abs(($rect.Bottom / $scale) - 440) -le 1) "Expected 680x440 logical client, got $($rect.Right / $scale)x$($rect.Bottom / $scale)"

    [NativeSmoke]::ClickClient($window, 200, 46)
    [Windows.Forms.SendKeys]::SendWait('display')
    $typed = Read-Probe $probe { param($s) $s.query -eq 'display' -and $s.results.Count -eq 1 } 'typed display query'
    Check ($typed.results[0] -eq 'Displays') 'Typing display must leave one Displays result'

    [NativeSmoke]::ClickClient($window, 580, 46)
    $cleared = Read-Probe $probe { param($s) $s.query -eq '' -and $s.results.Count -eq 3 -and $s.queryFocused } 'clear click restoring focus and results'

    [Windows.Forms.SendKeys]::SendWait('{DOWN}')
    [void](Read-Probe $probe { param($s) $s.selected -eq 1 -and $s.queryFocused } 'Down selecting index 1 while retaining editor focus')

    Check ([NativeSmoke]::GetForegroundWindow() -eq $window) 'Fixture lost foreground before paste'
    [Windows.Forms.SendKeys]::SendWait('^a')
    [void](Read-Probe $probe { param($s) $s.queryFocused } 'Ctrl+A retaining editor focus')
    [Windows.Forms.SendKeys]::SendWait('^v')
    [void](Read-Probe $probe { param($s) $s.query -eq '資料🚀' -and $s.results.Count -eq 0 } 'Unicode clipboard input including surrogate pair')

    # Restore results before testing the menu. Escape here must close only the
    # menu, leaving the fixture and query focus alive.
    [NativeSmoke]::ClickClient($window, 580, 46)
    [void](Read-Probe $probe { param($s) $s.query -eq '' -and $s.results.Count -eq 3 } 'clear after Unicode')
    [NativeSmoke]::ClickClient($window, 634, 46)
    [void](Read-Probe $probe { param($s) $s.menu } 'menu open')
    [Windows.Forms.SendKeys]::SendWait('{ESC}')
    $closed = Read-Probe $probe { param($s) !$s.menu } 'Escape closing menu only'
    Check (!$process.HasExited -and $closed.queryFocused) 'Escape in menu must keep the focused launcher open'
} finally {
    Stop-Fixture $process
}

Write-Output 'PASS: six GPU fixture snapshots; real Windows typing, clear click, arrows, Unicode, focus, and menu Escape verified through the fixture probe'
