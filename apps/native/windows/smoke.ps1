param([string]$Binary = 'target/debug/findanything.exe', [string]$Output = 'native-smoke')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class NativeSmoke {
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h, int id);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder b, int n);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder b, int n);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageW")] public static extern IntPtr SetText(IntPtr h, uint m, IntPtr w, string text);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageW")] public static extern IntPtr ReadItem(IntPtr h, uint m, IntPtr w, StringBuilder text);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    public struct Rect { public int Left, Top, Right, Bottom; }
}
'@
$Binary = (Resolve-Path $Binary).Path
New-Item -ItemType Directory -Path $Output -Force | Out-Null
function Check($Condition, $Message) { if (!$Condition) { throw $Message } }
function Capture($Handle, $Name) {
    $r = New-Object NativeSmoke+Rect
    [NativeSmoke]::GetWindowRect($Handle, [ref]$r) | Out-Null
    $bitmap = New-Object System.Drawing.Bitmap(($r.Right-$r.Left), ($r.Bottom-$r.Top))
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try { Check ([NativeSmoke]::PrintWindow($Handle, $dc, 0)) 'PrintWindow failed' }
    finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
    try { $bitmap.Save((Join-Path (Resolve-Path $Output) "$Name.png"), [System.Drawing.Imaging.ImageFormat]::Png) }
    finally { $bitmap.Dispose() }
}
foreach ($state in @('results', 'empty', 'error')) {
    $process = Start-Process $Binary -ArgumentList @('--fixture', $state) -PassThru
    try {
        for ($i=0; $i -lt 100; $i++) {
            $process.Refresh()
            Check (!$process.HasExited) "Native process exited before showing $state"
            if ($process.MainWindowHandle -ne [IntPtr]::Zero -and
                [NativeSmoke]::GetDlgItem($process.MainWindowHandle, 101) -ne [IntPtr]::Zero -and
                [NativeSmoke]::GetDlgItem($process.MainWindowHandle, 102) -ne [IntPtr]::Zero) { break }
            Start-Sleep -Milliseconds 100
        }
        $h = $process.MainWindowHandle
        Check ($h -ne [IntPtr]::Zero) 'No native window'
        $edit = [NativeSmoke]::GetDlgItem($h, 101)
        $list = [NativeSmoke]::GetDlgItem($h, 102)
        foreach ($pair in @(@($edit,'Edit'), @($list,'ListBox'))) {
            $name = New-Object Text.StringBuilder 100
            [NativeSmoke]::GetClassName($pair[0], $name, 100) | Out-Null
            Check ($name.ToString() -ieq $pair[1]) "Expected OS-native $($pair[1]), got $name"
        }
        Start-Sleep -Milliseconds 400
        $count = [NativeSmoke]::SendMessage($list, 0x18B, 0, 0).ToInt32()
        if ($state -eq 'results') {
            Check ($count -eq 3) "Expected 3 native result rows, got $count"
            Capture $h 'windows-results'
            [NativeSmoke]::PostMessage($edit, 0x100, 0x28, 0) | Out-Null
            Start-Sleep -Milliseconds 150
            Check ([NativeSmoke]::SendMessage($list, 0x188, 0, 0).ToInt32() -eq 1) 'Down must select second result'
            [NativeSmoke]::SetText($edit, 0xC, 0, 'display') | Out-Null
            Start-Sleep -Milliseconds 250
            Check ([NativeSmoke]::SendMessage($list, 0x18B, 0, 0).ToInt32() -eq 1) 'Filter must leave one result'
            $label = New-Object Text.StringBuilder 512
            [NativeSmoke]::ReadItem($list, 0x189, 0, $label) | Out-Null
            Check ($label.ToString() -eq 'Displays — System Settings — Suggested') "Wrong filtered row: $label"
            Capture $h 'windows-query'
            [NativeSmoke]::SetText($edit, 0xC, 0, '資料🚀') | Out-Null
            Start-Sleep -Milliseconds 150
            Check ([NativeSmoke]::SendMessage($list, 0x18B, 0, 0).ToInt32() -eq 0) 'Unicode unmatched query must clear results'
        } else {
            Check ($count -eq 0) 'Empty/error fixture must have no rows'
            $text = New-Object Text.StringBuilder 512
            [NativeSmoke]::GetWindowText([NativeSmoke]::GetDlgItem($h,103), $text,512) | Out-Null
            $expected = if ($state -eq 'empty') {'No local matches'} else {'Search hit a snag'}
            Check ($text.ToString().Contains($expected)) "Missing $state message: $text"
            Capture $h "windows-$state"
        }
    } finally {
        if (!$process.HasExited) { Stop-Process -Id $process.Id; $process.WaitForExit() }
        $process.Dispose()
    }
}
Write-Output 'PASS: Win32 Edit/ListBox controls, keyboard selection, filtered search, Unicode input, empty/error states and screenshots'
