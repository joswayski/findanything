import AppKit
import Carbon
import CFindAnything

private final class HotKey {
    private var reference: EventHotKeyRef?
    private var handler: EventHandlerRef?
    private var context: UnsafeMutableRawPointer?
    private(set) var available = false
    init(action: @escaping () -> Void) {
        let retained = Unmanaged.passRetained(ActionBox(action))
        context = retained.toOpaque()
        var type = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        InstallEventHandler(GetApplicationEventTarget(), { _, _, pointer in
            guard let pointer else { return OSStatus(eventNotHandledErr) }
            Unmanaged<ActionBox>.fromOpaque(pointer).takeUnretainedValue().action(); return noErr
        }, 1, &type, context, &handler)
        let identifier = EventHotKeyID(signature: OSType(0x46414E59), id: 1) // FANY
        available = RegisterEventHotKey(UInt32(kVK_Space), UInt32(cmdKey | shiftKey), identifier, GetApplicationEventTarget(), 0, &reference) == noErr
    }
    deinit { if let reference { UnregisterEventHotKey(reference) }; if let handler { RemoveEventHandler(handler) }; if let context { Unmanaged<ActionBox>.fromOpaque(context).release() } }
    private final class ActionBox { let action: () -> Void; init(_ action: @escaping () -> Void) { self.action = action } }
}

private final class AppDelegate: NSObject, NSApplicationDelegate {
    let worker: EngineWorker
    private let controlWorker = EngineWorker(RustBridge())
    let launcher: LauncherController
    private var hotKey: HotKey?
    private var timer: Timer?
    private let updateItem = NSMenuItem(title: "Check for Updates…", action: nil, keyEquivalent: "")
    private let restartItem = NSMenuItem(title: "Restart to Update", action: nil, keyEquivalent: "")

    init(worker: EngineWorker) { self.worker = worker; launcher = LauncherController(worker: worker); super.init() }
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory); installMenu()
        hotKey = HotKey { [weak self] in self?.toggle() }
        if hotKey?.available != true { statusItem?.button?.toolTip = "Shortcut unavailable. Click to open Find Anything." }
        timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in self?.poll() }
        launcher.showAndFocus()
    }
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool { launcher.showAndFocus(); return true }
    private func installMenu() {
        // Native editing commands must remain available in the field editor.
        let main = NSMenu()
        let appItem = NSMenuItem(); main.addItem(appItem)
        let appMenu = NSMenu(); appMenu.addItem(withTitle: "Quit Find Anything", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"); appItem.submenu = appMenu
        let editItem = NSMenuItem(); main.addItem(editItem)
        let editMenu = NSMenu(title: "Edit")
        for (title, action, key) in [("Cut", #selector(NSText.cut(_:)), "x"), ("Copy", #selector(NSText.copy(_:)), "c"), ("Paste", #selector(NSText.paste(_:)), "v"), ("Select All", #selector(NSText.selectAll(_:)), "a")] {
            editMenu.addItem(withTitle: title, action: action, keyEquivalent: key)
        }
        editItem.submenu = editMenu; NSApp.mainMenu = main
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        item.button?.image = NSImage(systemSymbolName: "magnifyingglass", accessibilityDescription: "Find Anything")
        let menu = NSMenu(); let showItem = menu.addItem(withTitle: "Show Find Anything", action: #selector(show), keyEquivalent: ""); showItem.target = self
        updateItem.target = self; updateItem.action = #selector(checkUpdates); menu.addItem(updateItem)
        restartItem.target = self; restartItem.action = #selector(applyUpdate); restartItem.isHidden = true; menu.addItem(restartItem)
        menu.addItem(.separator()); menu.addItem(withTitle: "Quit Find Anything", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        item.menu = menu; statusItem = item
    }
    private var statusItem: NSStatusItem?
    @objc private func show() { launcher.showAndFocus() }
    private func toggle() { launcher.window?.isVisible == true ? launcher.hide() : launcher.showAndFocus() }
    @objc private func checkUpdates() { controlWorker.call(["op": "updates_check"], as: InstanceEnvelope.self) { _ in } }
    @objc private func applyUpdate() {
        controlWorker.call(["op": "updates_apply"], as: InstanceEnvelope.self) { result in
            if case .success(let response) = result, !response.ok {
                let alert = NSAlert(); alert.messageText = "Could not install update"; alert.informativeText = response.error ?? "Try again later."; alert.runModal()
            }
        }
    }
    private func poll() {
        controlWorker.call(["op": "instance_poll"], as: InstanceEnvelope.self) { [weak self] result in if case .success(let value) = result, value.activate == true { self?.launcher.showAndFocus() } }
        controlWorker.call(["op": "updates_status"], as: UpdateEnvelope.self) { [weak self] result in
            guard let self, case .success(let value) = result, let status = value.status else { return }
            self.updateItem.title = ["checking", "downloading"].contains(status.state) ? status.message : "Check for Updates…"
            self.updateItem.isEnabled = !["disabled", "checking", "downloading"].contains(status.state)
            self.updateItem.toolTip = status.message
            self.restartItem.isHidden = status.state != "ready"
        }
    }
}

@main enum Main {
    static func main() {
        let args = Array(CommandLine.arguments.dropFirst())
        if let fixtureIndex = args.firstIndex(of: "--fixture") {
            let state = args.indices.contains(fixtureIndex + 1) && !args[fixtureIndex + 1].hasPrefix("--") ? args[fixtureIndex + 1] : "results"
            runFixture(state: state, args: args); return
        }
        fa_initialize() // Velopack startup hooks must precede NSApplication and instance election.
        let bridge = RustBridge(), worker = EngineWorker(bridge)
        do {
            let response = try JSONDecoder().decode(InstanceEnvelope.self, from: bridge.request(["op": "instance_acquire"]))
            guard response.ok else { throw NSError(domain: "FindAnything", code: 1, userInfo: [NSLocalizedDescriptionKey: response.error ?? "Instance startup failed"]) }
            guard response.primary == true else { return }
        } catch {
            let alert = NSAlert(); alert.messageText = "Find Anything could not start"; alert.informativeText = error.localizedDescription; alert.runModal(); return
        }
        let app = NSApplication.shared, delegate = AppDelegate(worker: worker); app.delegate = delegate
        withExtendedLifetime(delegate) { app.run() }
    }
    private static func runFixture(state: String, args: [String]) {
        let app = NSApplication.shared; app.setActivationPolicy(.regular)
        if let index = args.firstIndex(of: "--appearance"), args.indices.contains(index + 1) { app.appearance = NSAppearance(named: args[index + 1] == "light" ? .aqua : .darkAqua) }
        let launcher = LauncherController(worker: nil, fixture: state); launcher.showAndFocus()
        if let index = args.firstIndex(of: "--screenshot"), args.indices.contains(index + 1), let view = launcher.window?.contentView {
            app.finishLaunching(); view.layoutSubtreeIfNeeded()
            let image = view.bitmapImageRepForCachingDisplay(in: view.bounds)!; view.cacheDisplay(in: view.bounds, to: image)
            do {
                guard let data = image.representation(using: .png, properties: [:]) else { fatalError("Cannot encode screenshot") }
                try data.write(to: URL(fileURLWithPath: args[index + 1]))
            } catch { fatalError("Cannot save screenshot: \(error)") }
            return
        }
        withExtendedLifetime(launcher) { app.run() }
    }
}
