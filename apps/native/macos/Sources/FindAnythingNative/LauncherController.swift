import AppKit

// A borderless search cell otherwise reuses the text field's top-aligned
// drawing rect, overlapping its search button in a taller Graphite control.
final class GraphiteSearchCell: NSSearchFieldCell {
    override func searchTextRect(forBounds rect: NSRect) -> NSRect {
        NSRect(x: rect.minX + 32, y: rect.midY - 9, width: max(0, rect.width - 64), height: 18)
    }
    override func drawingRect(forBounds rect: NSRect) -> NSRect { searchTextRect(forBounds: rect) }
    override func titleRect(forBounds rect: NSRect) -> NSRect { searchTextRect(forBounds: rect) }
    override func searchButtonRect(forBounds rect: NSRect) -> NSRect {
        NSRect(x: rect.minX + 10, y: rect.midY - 8, width: 16, height: 16)
    }
    override func cancelButtonRect(forBounds rect: NSRect) -> NSRect {
        NSRect(x: rect.maxX - 26, y: rect.midY - 8, width: 16, height: 16)
    }
    override func select(withFrame rect: NSRect, in controlView: NSView, editor textObj: NSText, delegate: Any?, start: Int, length: Int) {
        super.select(withFrame: searchTextRect(forBounds: rect), in: controlView, editor: textObj, delegate: delegate, start: start, length: length)
    }
    override func edit(withFrame rect: NSRect, in controlView: NSView, editor textObj: NSText, delegate: Any?, event: NSEvent?) {
        super.edit(withFrame: searchTextRect(forBounds: rect), in: controlView, editor: textObj, delegate: delegate, event: event)
    }
}

final class QueryField: NSSearchField {
    var keyHandler: ((NSEvent) -> Bool)?

    override func keyDown(with event: NSEvent) {
        if keyHandler?(event) == true { return }
        super.keyDown(with: event)
    }

    override func becomeFirstResponder() -> Bool {
        let accepted = super.becomeFirstResponder()
        updateFocus(accepted)
        return accepted
    }

    override func resignFirstResponder() -> Bool {
        let accepted = super.resignFirstResponder()
        if accepted { updateFocus(false) }
        return accepted
    }

    private func updateFocus(_ focused: Bool) {
        layer?.borderColor = (focused ? Graphite.accent : Graphite.border_strong).cgColor
        layer?.borderWidth = focused ? 2 : 1
    }
}

final class GraphiteRowView: NSTableRowView {
    override func drawSelection(in dirtyRect: NSRect) {
        guard selectionHighlightStyle != .none else { return }
        let bounds = bounds.insetBy(dx: 0, dy: 2)
        Graphite.selected.setFill()
        NSBezierPath(roundedRect: bounds, xRadius: Graphite.radius, yRadius: Graphite.radius).fill()
        Graphite.accent.setFill()
        NSBezierPath(roundedRect: NSRect(x: bounds.minX, y: bounds.minY, width: 2, height: bounds.height), xRadius: 1, yRadius: 1).fill()
    }
}

final class ResultCell: NSTableCellView {
    private let icon = NSImageView()
    private let title = NSTextField(labelWithString: "")
    private let detail = NSTextField(labelWithString: "")

    override init(frame: NSRect) {
        super.init(frame: frame)
        icon.contentTintColor = Graphite.secondary
        title.font = .systemFont(ofSize: Graphite.body_size, weight: .semibold)
        title.textColor = Graphite.text_strong
        title.lineBreakMode = .byTruncatingTail
        detail.font = .systemFont(ofSize: Graphite.metadata_size)
        detail.textColor = Graphite.muted
        detail.lineBreakMode = .byTruncatingMiddle
        [icon, title, detail].forEach { $0.translatesAutoresizingMaskIntoConstraints = false; addSubview($0) }
        NSLayoutConstraint.activate([
            icon.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            icon.centerYAnchor.constraint(equalTo: centerYAnchor),
            icon.widthAnchor.constraint(equalToConstant: Graphite.icon_size),
            icon.heightAnchor.constraint(equalToConstant: Graphite.icon_size),
            title.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 12),
            title.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            title.bottomAnchor.constraint(equalTo: centerYAnchor, constant: -1),
            detail.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            detail.trailingAnchor.constraint(equalTo: title.trailingAnchor),
            detail.topAnchor.constraint(equalTo: centerYAnchor, constant: 2)
        ])
    }

    required init?(coder: NSCoder) { nil }

    func fill(_ result: SearchResult) {
        let lucide: LucideIcon
        switch result.kind {
        case "system_action": lucide = .slidersHorizontal
        case "file": lucide = .file
        case "application": lucide = .appWindow
        default: lucide = .search
        }
        icon.image = LucideImage.make(
            lucide,
            pointSize: Graphite.icon_size,
            accessibilityDescription: result.kind.replacingOccurrences(of: "_", with: " ")
        )
        title.stringValue = result.title
        detail.stringValue = [result.subtitle, result.reason].filter { !$0.isEmpty }.joined(separator: " · ")
        toolTip = [result.title, detail.stringValue].filter { !$0.isEmpty }.joined(separator: " — ")
        setAccessibilityLabel(toolTip ?? result.title)
    }
}

final class LauncherController: NSWindowController, NSWindowDelegate, NSSearchFieldDelegate, NSTableViewDataSource, NSTableViewDelegate {
    let state = SearchState()
    private let worker: EngineWorker?
    private let query = QueryField()
    private let heading = NSTextField(labelWithString: "Apps & actions")
    private let table = NSTableView()
    private let scroll = NSScrollView()
    private let status = NSTextField(labelWithString: "Preparing index")
    private let message = NSTextField(labelWithString: "")
    private var searchTimer: Timer?
    private var activating = false
    var onHide: (() -> Void)?

    init(worker: EngineWorker?, fixture: String? = nil) {
        self.worker = worker
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: Graphite.window_width, height: Graphite.window_height),
            styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false
        )
        window.minSize = NSSize(width: Graphite.minimum_width, height: Graphite.minimum_height)
        window.title = "Find Anything"
        window.titleVisibility = .hidden
        window.isReleasedWhenClosed = false
        window.appearance = NSAppearance(named: .darkAqua)
        window.backgroundColor = Graphite.chrome
        super.init(window: window)
        window.delegate = self
        buildUI()
        if let fixture { loadFixture(fixture) } else { scheduleSearch() }
    }
    required init?(coder: NSCoder) { nil }

    private func buildUI() {
        guard let content = window?.contentView else { return }
        content.wantsLayer = true
        content.layer?.backgroundColor = Graphite.bg.cgColor

        query.cell = GraphiteSearchCell(textCell: "")
        query.isEditable = true
        query.isSelectable = true
        query.controlSize = .regular
        query.placeholderString = "Find anything…"
        query.font = .systemFont(ofSize: Graphite.body_size)
        query.textColor = Graphite.text
        query.delegate = self
        query.setAccessibilityLabel("Search applications, settings, and files")
        query.sendsSearchStringImmediately = true
        query.translatesAutoresizingMaskIntoConstraints = false
        query.isBezeled = false
        query.drawsBackground = false
        query.backgroundColor = Graphite.control
        query.focusRingType = .none
        query.wantsLayer = true
        query.layer?.backgroundColor = Graphite.control.cgColor
        query.layer?.cornerRadius = Graphite.radius
        query.layer?.borderWidth = 1
        query.layer?.borderColor = Graphite.border_strong.cgColor
        query.keyHandler = { [weak self] event in self?.handleKey(event) ?? false }
        let searchImage = LucideImage.make(.search, pointSize: 16, accessibilityDescription: "Search")
        let clearImage = LucideImage.make(.x, pointSize: 16, accessibilityDescription: "Clear search")
        if let cell = query.cell as? NSSearchFieldCell {
            cell.searchButtonCell?.image = searchImage
            cell.searchButtonCell?.alternateImage = searchImage
            cell.cancelButtonCell?.image = clearImage
            cell.cancelButtonCell?.alternateImage = clearImage
        }

        heading.font = .systemFont(ofSize: Graphite.metadata_size, weight: .semibold)
        heading.textColor = Graphite.secondary

        table.headerView = nil
        table.style = .plain
        table.rowHeight = Graphite.row_height
        table.intercellSpacing = .zero
        table.selectionHighlightStyle = .regular
        table.backgroundColor = .clear
        table.gridStyleMask = []
        table.delegate = self
        table.dataSource = self
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("result"))
        column.resizingMask = .autoresizingMask
        table.addTableColumn(column)
        table.target = self
        table.action = #selector(activateSelected)
        table.setAccessibilityLabel("Search results")
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.drawsBackground = false
        scroll.borderType = .noBorder

        status.textColor = Graphite.muted
        status.font = .systemFont(ofSize: Graphite.metadata_size)
        let shortcuts = NSTextField(labelWithString: "↑/↓ Navigate   Enter Open   Esc Close")
        shortcuts.alignment = .right
        shortcuts.textColor = Graphite.faint
        shortcuts.font = .systemFont(ofSize: Graphite.metadata_size)
        let footer = NSStackView(views: [status, shortcuts])
        footer.distribution = .fillEqually
        footer.alignment = .centerY

        message.alignment = .center
        message.textColor = Graphite.muted
        message.font = .systemFont(ofSize: Graphite.body_size)
        message.maximumNumberOfLines = 3
        message.translatesAutoresizingMaskIntoConstraints = false

        let results = NSView()
        results.translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false
        results.addSubview(scroll)
        results.addSubview(message)
        NSLayoutConstraint.activate([
            scroll.leadingAnchor.constraint(equalTo: results.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: results.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: results.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: results.bottomAnchor),
            message.leadingAnchor.constraint(greaterThanOrEqualTo: results.leadingAnchor, constant: Graphite.inset),
            message.trailingAnchor.constraint(lessThanOrEqualTo: results.trailingAnchor, constant: -Graphite.inset),
            message.centerXAnchor.constraint(equalTo: results.centerXAnchor),
            message.centerYAnchor.constraint(equalTo: results.centerYAnchor)
        ])

        let stack = NSStackView(views: [query, heading, results, footer])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = Graphite.gap
        stack.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(stack)
        [query, heading, results, footer].forEach { $0.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true }
        results.setContentHuggingPriority(.defaultLow, for: .vertical)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: Graphite.inset),
            stack.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -Graphite.inset),
            stack.topAnchor.constraint(equalTo: content.topAnchor, constant: Graphite.inset),
            stack.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -Graphite.inset),
            query.heightAnchor.constraint(equalToConstant: Graphite.search_height),
            heading.heightAnchor.constraint(equalToConstant: Graphite.section_height),
            footer.heightAnchor.constraint(equalToConstant: Graphite.footer_height)
        ])
    }

    func controlTextDidChange(_ obj: Notification) {
        state.resetSelection()
        heading.stringValue = query.stringValue.isEmpty ? "Apps & actions" : "Best matches"
        scheduleSearch()
    }

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
                self.status.toolTip = response.semanticMessage
                self.setMessage(response.results.isEmpty ? "No local matches yet.\nTry an app, a setting, or a filename." : "")
                self.table.reloadData()
                self.selectAndScroll()
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

    private func setMessage(_ value: String, error: Bool = false) {
        if error, let newline = value.firstIndex(of: "\n") {
            let rendered = NSMutableAttributedString(
                string: String(value[..<newline]),
                attributes: [.foregroundColor: Graphite.danger, .font: NSFont.systemFont(ofSize: Graphite.body_size, weight: .semibold)]
            )
            rendered.append(NSAttributedString(
                string: String(value[newline...]),
                attributes: [.foregroundColor: Graphite.muted, .font: NSFont.systemFont(ofSize: Graphite.body_size)]
            ))
            message.attributedStringValue = rendered
        } else {
            message.stringValue = value
            message.textColor = Graphite.muted
        }
        message.isHidden = value.isEmpty
    }

    private func showError(_ error: String, generation: Int) {
        guard generation == state.generation else { return }
        setMessage("Search hit a snag.\n\(error)", error: true)
    }

    private func semanticLabel(_ value: String) -> String {
        value == "ready" ? "Semantic ready" : value == "unavailable" ? "Keyword mode" : value == "error" ? "Semantic error" : "Preparing index"
    }

    func numberOfRows(in tableView: NSTableView) -> Int { state.results.count }
    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? { GraphiteRowView() }
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let cell = ResultCell()
        cell.fill(state.results[row])
        return cell
    }
    func tableViewSelectionDidChange(_ notification: Notification) {
        if table.selectedRow >= 0 { state.move(table.selectedRow - state.selected) }
    }

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

    private func selectAndScroll() {
        guard !state.results.isEmpty else { table.deselectAll(nil); return }
        table.selectRowIndexes(IndexSet(integer: state.selected), byExtendingSelection: false)
        table.scrollRowToVisible(state.selected)
    }

    @objc private func activateSelected() {
        let generation = state.generation
        guard !activating, let result = state.selectedResult(generation: generation), let worker else { return }
        activating = true
        query.isEnabled = false
        worker.call(["op": "activate", "id": result.id, "query": query.stringValue], generation: generation, as: InstanceEnvelope.self) { [weak self] response in
            guard let self else { return }
            self.activating = false
            self.query.isEnabled = true
            guard generation == self.state.generation else { return }
            switch response {
            case .success(let value) where value.ok: self.hide()
            case .success(let value): self.setMessage(value.error ?? "Could not open result", error: true)
            case .failure(let error): self.setMessage(error.localizedDescription, error: true)
            }
        }
    }

    func showAndFocus() {
        window?.center()
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        window?.makeFirstResponder(query)
        query.selectText(nil)
    }
    func hide() { window?.orderOut(nil); onHide?() }
    func windowShouldClose(_ sender: NSWindow) -> Bool { hide(); return false }

    private func loadFixture(_ fixture: String) {
        status.stringValue = "Keyword mode"
        if fixture == "minimum" {
            window?.setContentSize(NSSize(width: Graphite.minimum_width, height: Graphite.minimum_height))
        }
        if fixture == "error" {
            setMessage("Search hit a snag.\nFixture engine unavailable", error: true)
            return
        }
        if fixture == "empty" {
            setMessage("No local matches yet.\nTry an app, a setting, or a filename.")
            return
        }
        var samples = [
            SearchResult(id: "browser", kind: "application", title: "Browser", subtitle: "Application", score: 1, reason: "Title match"),
            SearchResult(id: "displays", kind: "system_action", title: "Displays", subtitle: "System Settings", score: 0.9, reason: "Suggested"),
            SearchResult(id: "file", kind: "file", title: "Project notes.md", subtitle: "~/Documents", score: 0.8, reason: "Filename match")
        ]
        if fixture == "query" {
            query.stringValue = "display"
            heading.stringValue = "Best matches"
            samples = samples.filter { $0.title.lowercased().contains("display") }
        }
        _ = state.accept(samples, generation: state.begin())
        if fixture == "selected" { state.move(1) }
        table.reloadData()
        selectAndScroll()
        status.stringValue = "Keyword mode"
    }
}
