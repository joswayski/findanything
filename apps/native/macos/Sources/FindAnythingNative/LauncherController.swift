import AppKit

final class QueryField: NSSearchField {
    var keyHandler: ((NSEvent) -> Bool)?
    override func keyDown(with event: NSEvent) {
        if keyHandler?(event) == true { return }
        super.keyDown(with: event)
    }
}

final class ResultCell: NSTableCellView {
    let icon = NSTextField(labelWithString: "")
    let title = NSTextField(labelWithString: "")
    let detail = NSTextField(labelWithString: "")
    override init(frame: NSRect) {
        super.init(frame: frame)
        icon.alignment = .center; icon.font = .systemFont(ofSize: 22, weight: .medium)
        icon.wantsLayer = true; icon.layer?.cornerRadius = 9; icon.layer?.backgroundColor = NSColor.controlAccentColor.cgColor
        title.font = .systemFont(ofSize: 16, weight: .semibold)
        detail.font = .systemFont(ofSize: 11); detail.textColor = .secondaryLabelColor
        [icon, title, detail].forEach { $0.translatesAutoresizingMaskIntoConstraints = false; addSubview($0) }
        NSLayoutConstraint.activate([
            icon.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12), icon.centerYAnchor.constraint(equalTo: centerYAnchor), icon.widthAnchor.constraint(equalToConstant: 42), icon.heightAnchor.constraint(equalToConstant: 42),
            title.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 13), title.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12), title.bottomAnchor.constraint(equalTo: centerYAnchor, constant: -1),
            detail.leadingAnchor.constraint(equalTo: title.leadingAnchor), detail.trailingAnchor.constraint(equalTo: title.trailingAnchor), detail.topAnchor.constraint(equalTo: centerYAnchor, constant: 3)
        ])
    }
    required init?(coder: NSCoder) { nil }
    func fill(_ result: SearchResult) {
        icon.stringValue = result.kind == "system_action" ? "⌁" : result.kind == "file" ? "⌑" : String(result.title.prefix(1)).uppercased()
        title.stringValue = result.subtitle.isEmpty ? result.title : "\(result.title)   \(result.subtitle)"
        detail.stringValue = result.reason
        toolTip = "\(result.title) — \(result.reason)"
    }
}

final class LauncherController: NSWindowController, NSWindowDelegate, NSSearchFieldDelegate, NSTableViewDataSource, NSTableViewDelegate {
    let state = SearchState()
    private let worker: EngineWorker?
    private let query = QueryField()
    private let heading = NSTextField(labelWithString: "APPS & ACTIONS")
    private let table = NSTableView()
    private let scroll = NSScrollView()
    private let status = NSTextField(labelWithString: "●  Preparing index")
    private let message = NSTextField(labelWithString: "")
    private var searchTimer: Timer?
    private var activating = false
    var onHide: (() -> Void)?

    init(worker: EngineWorker?, fixture: String? = nil) {
        self.worker = worker
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 760, height: 548), styleMask: [.titled, .closable, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
        window.minSize = NSSize(width: 640, height: 420); window.titleVisibility = .hidden; window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false; window.backgroundColor = .windowBackgroundColor
        super.init(window: window)
        window.delegate = self
        buildUI()
        if let fixture { loadFixture(fixture) } else { scheduleSearch() }
    }
    required init?(coder: NSCoder) { nil }

    private func buildUI() {
        guard let content = window?.contentView else { return }
        query.controlSize = .large
        query.placeholderString = "Find anything…"; query.font = .systemFont(ofSize: NSFont.systemFontSize(for: .large)); query.delegate = self
        query.setAccessibilityLabel("Search applications, settings, and files")
        query.sendsSearchStringImmediately = true; query.translatesAutoresizingMaskIntoConstraints = false
        query.keyHandler = { [weak self] event in self?.handleKey(event) ?? false }
        heading.font = .systemFont(ofSize: 11, weight: .semibold); heading.textColor = .secondaryLabelColor
        table.headerView = nil; table.rowHeight = 66; table.selectionHighlightStyle = .regular; table.delegate = self; table.dataSource = self
        table.addTableColumn(NSTableColumn(identifier: NSUserInterfaceItemIdentifier("result")))
        table.target = self; table.action = #selector(activateSelected)
        table.setAccessibilityLabel("Search results")
        scroll.documentView = table; scroll.hasVerticalScroller = true; scroll.drawsBackground = false
        status.textColor = .secondaryLabelColor; status.font = .systemFont(ofSize: 11)
        message.alignment = .center; message.textColor = .secondaryLabelColor; message.maximumNumberOfLines = 3
        let footer = NSStackView(views: [status, NSTextField(labelWithString: "↑ ↓  Navigate     ↵  Open     esc  Close")]); footer.distribution = .fillEqually
        let stack = NSStackView(views: [query, heading, scroll, message, footer]); stack.orientation = .vertical; stack.alignment = .leading; stack.spacing = 10; stack.edgeInsets = NSEdgeInsets(top: 32, left: 24, bottom: 16, right: 24); stack.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(stack)
        for view in [query, heading, scroll, message, footer] {
            view.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -48).isActive = true
        }
        scroll.setContentHuggingPriority(.defaultLow, for: .vertical)
        NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo: content.leadingAnchor), stack.trailingAnchor.constraint(equalTo: content.trailingAnchor), stack.topAnchor.constraint(equalTo: content.topAnchor), stack.bottomAnchor.constraint(equalTo: content.bottomAnchor), query.heightAnchor.constraint(equalToConstant: 32), heading.heightAnchor.constraint(equalToConstant: 20), footer.heightAnchor.constraint(equalToConstant: 24), message.heightAnchor.constraint(greaterThanOrEqualToConstant: 0)])
    }

    func controlTextDidChange(_ obj: Notification) { state.resetSelection(); heading.stringValue = query.stringValue.isEmpty ? "APPS & ACTIONS" : "BEST MATCHES"; scheduleSearch() }
    private func scheduleSearch() {
        guard !activating else { return }
        let generation = state.begin()
        worker?.invalidate(generation)
        searchTimer?.invalidate()
        searchTimer = Timer.scheduledTimer(withTimeInterval: 0.032, repeats: false) { [weak self] _ in self?.search(generation: generation) }
    }
    private func search(generation: Int) {
        guard let worker else { return }
        let value = query.stringValue
        worker.call(["op": "search", "query": value], generation: generation, as: Envelope<SearchResponse>.self) { [weak self] result in
            guard let self else { return }
            switch result {
            case .success(let envelope) where envelope.ok && envelope.response != nil:
                let response = envelope.response!
                guard self.state.accept(response.results, generation: generation) else { return }
                self.status.stringValue = self.semanticLabel(response.semanticStatus)
                self.status.toolTip = response.semanticMessage; self.message.stringValue = response.results.isEmpty ? "No local matches yet.\nTry an app, a setting, or a filename." : ""
                self.table.reloadData(); self.selectAndScroll()
                if response.semanticStatus == "warming" {
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                        guard let self, self.state.generation == generation, self.query.stringValue == value else { return }
                        self.scheduleSearch()
                    }
                }
            case .success(let envelope): self.showError(envelope.error ?? "Search failed", generation: generation)
            case .failure(let error): self.showError(error.localizedDescription, generation: generation)
            }
        }
    }
    private func showError(_ error: String, generation: Int) { guard generation == state.generation else { return }; message.stringValue = "Search hit a snag.\n\(error)" }
    private func semanticLabel(_ value: String) -> String { value == "ready" ? "●  Semantic ready" : value == "unavailable" ? "●  Keyword mode" : value == "error" ? "●  Semantic error" : "●  Preparing index" }
    func numberOfRows(in tableView: NSTableView) -> Int { state.results.count }
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? { let cell = ResultCell(); cell.fill(state.results[row]); return cell }
    func tableViewSelectionDidChange(_ notification: Notification) { if table.selectedRow >= 0 { state.move(table.selectedRow - state.selected) } }
    private func handleKey(_ event: NSEvent) -> Bool {
        switch event.keyCode {
        case 125: state.move(1); selectAndScroll(); return true
        case 126: state.move(-1); selectAndScroll(); return true
        case 36, 76: activateSelected(); return true
        case 53: hide(); return true
        default: return false
        }
    }
    // NSSearchField uses a shared NSTextView field editor. Its command delegate,
    // not the field's keyDown, receives arrows/Return/Escape while editing.
    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        switch selector {
        case #selector(NSResponder.moveDown(_:)): state.move(1); selectAndScroll(); return true
        case #selector(NSResponder.moveUp(_:)): state.move(-1); selectAndScroll(); return true
        case #selector(NSResponder.insertNewline(_:)): activateSelected(); return true
        case #selector(NSResponder.cancelOperation(_:)): hide(); return true
        default: return false
        }
    }
    private func selectAndScroll() { guard !state.results.isEmpty else { table.deselectAll(nil); return }; table.selectRowIndexes(IndexSet(integer: state.selected), byExtendingSelection: false); table.scrollRowToVisible(state.selected) }
    @objc private func activateSelected() {
        let generation = state.generation
        guard !activating, let result = state.selectedResult(generation: generation), let worker else { return }
        activating = true
        query.isEnabled = false
        worker.call(["op": "activate", "id": result.id, "query": query.stringValue], generation: generation, as: InstanceEnvelope.self) { [weak self] response in
            guard let self else { return }
            self.activating = false; self.query.isEnabled = true
            guard generation == self.state.generation else { return }
            switch response { case .success(let value) where value.ok: self.hide(); case .success(let value): self.message.stringValue = value.error ?? "Could not open result"; case .failure(let error): self.message.stringValue = error.localizedDescription }
        }
    }
    func showAndFocus() { window?.center(); showWindow(nil); window?.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true); window?.makeFirstResponder(query); query.selectText(nil) }
    func hide() { window?.orderOut(nil); onHide?() }
    func windowShouldClose(_ sender: NSWindow) -> Bool { hide(); return false }
    private func loadFixture(_ fixture: String) {
        if fixture == "error" { message.stringValue = "Search hit a snag.\nFixture engine unavailable"; status.stringValue = "●  Semantic error"; return }
        if fixture == "empty" { message.stringValue = "No local matches yet.\nTry an app, a setting, or a filename."; return }
        let samples = [SearchResult(id: "app", kind: "application", title: "Calendar", subtitle: "Application", score: 1, reason: "Local application"), SearchResult(id: "file", kind: "file", title: "Project notes", subtitle: "~/Documents", score: 0.8, reason: "Filename match")]
        _ = state.accept(samples, generation: state.begin()); table.reloadData(); selectAndScroll(); status.stringValue = "●  Semantic ready"
    }
}
