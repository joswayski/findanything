import XCTest
import AppKit
@testable import FindAnythingNative

final class NativeTests: XCTestCase {
    private let a = SearchResult(id: "a", kind: "application", title: "A", subtitle: "App", score: 1, reason: "match")
    func testDecodesSearchEnvelope() throws {
        let data = #"{"ok":true,"response":{"results":[{"id":"a","kind":"application","title":"A","subtitle":"App","score":1,"reason":"match"}],"semanticStatus":"ready"}}"#.data(using: .utf8)!
        let value = try JSONDecoder().decode(Envelope<SearchResponse>.self, from: data)
        XCTAssertTrue(value.ok); XCTAssertEqual(value.response?.results.first?.title, "A")
    }
    func testRejectsStaleGenerationAndActivation() {
        let state = SearchState(), old = state.begin(), current = state.begin()
        XCTAssertFalse(state.accept([a], generation: old)); XCTAssertTrue(state.accept([a], generation: current))
        XCTAssertNil(state.selectedResult(generation: old)); XCTAssertEqual(state.selectedResult(generation: current), a)
        let pending = state.begin()
        XCTAssertNil(state.selectedResult(generation: pending))
        XCTAssertFalse(state.accept([a], generation: current))
    }
    func testSelectionBoundaries() {
        let state = SearchState(), generation = state.begin(); _ = state.accept([a, a], generation: generation)
        state.move(-4); XCTAssertEqual(state.selected, 0); state.move(8); XCTAssertEqual(state.selected, 1); state.move(1); XCTAssertEqual(state.selected, 1)
    }

    func testSearchCellReservesIconsAndCentersTextAtBothWidths() {
        let cell = GraphiteSearchCell(textCell: "")
        cell.font = Fonts.regular(Graphite.search_size)
        let expectedHeight = ceil(cell.font!.ascender - cell.font!.descender + cell.font!.leading)
        for width in [CGFloat(520), CGFloat(640)] {
            let bounds = NSRect(x: 7, y: 3, width: width, height: Graphite.search_height)
            let text = cell.searchTextRect(forBounds: bounds)
            XCTAssertEqual(text.midY, bounds.midY)
            XCTAssertEqual(text.height, expectedHeight)
            XCTAssertGreaterThan(text.height, 18, "Search text height must follow the 18-point font's layout")
            XCTAssertGreaterThan(text.minX, cell.searchButtonRect(forBounds: bounds).maxX)
            XCTAssertLessThan(text.maxX, cell.cancelButtonRect(forBounds: bounds).minX)
            XCTAssertEqual(cell.drawingRect(forBounds: bounds), text)
        }
    }

    func testLauncherRetainsNativeFieldEditorAndUnicodeInput() throws {
        _ = NSApplication.shared
        let launcher = LauncherController(worker: nil, fixture: "results")
        defer { launcher.hide() }
        let content = try XCTUnwrap(launcher.window?.contentView)
        let stack = try XCTUnwrap(content.subviews.compactMap { $0 as? NSStackView }.first)
        let field = try XCTUnwrap(stack.arrangedSubviews.compactMap { $0 as? NSSearchField }.first)
        XCTAssertTrue(field.isEditable)
        XCTAssertTrue(field.isSelectable)
        XCTAssertEqual(field.font?.pointSize, Graphite.search_size)
        XCTAssertEqual(field.font?.fontName, "Inter-Regular")
        let searchCell = try XCTUnwrap(field.cell as? NSSearchFieldCell)
        XCTAssertEqual(searchCell.searchButtonCell?.image?.size, NSSize(width: 16, height: 16))
        XCTAssertEqual(searchCell.cancelButtonCell?.image?.size, NSSize(width: 16, height: 16))
        XCTAssertNotNil(searchCell.cancelButtonCell?.action, "Replacing the clear icon must retain the native clear action")
        launcher.showAndFocus()
        content.layoutSubtreeIfNeeded()
        XCTAssertEqual(field.frame.height, Graphite.search_height)
        XCTAssertEqual(field.searchButtonBounds.midY, field.bounds.midY)
        XCTAssertEqual(field.searchButtonBounds.minX, field.bounds.minX + 12)
        XCTAssertEqual(field.cancelButtonBounds.midY, field.bounds.midY)
        let placeholderFont = field.placeholderAttributedString?.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        XCTAssertEqual(placeholderFont?.fontName, "Inter-Regular")
        let editor = try XCTUnwrap(field.currentEditor() as? NSTextView)
        let origin = field.convert(NSPoint.zero, from: editor)
        XCTAssertGreaterThanOrEqual(origin.x, 30, "Focused editor must reserve the search icon")
        let editorFrame = field.convert(editor.frame, from: editor.superview)
        let expectedEditorFrame = searchCell.searchTextRect(forBounds: field.bounds)
        XCTAssertEqual(editorFrame.midY, expectedEditorFrame.midY, accuracy: 1, "Focused editor must retain centered cell geometry")
        XCTAssertGreaterThan(editorFrame.height, 18, "Focused editor must not clip 18-point search text")
        editor.insertText("資料🚀", replacementRange: NSRange(location: 0, length: 0))
        XCTAssertEqual(field.stringValue, "資料🚀")
        searchCell.cancelButtonCell?.performClick(field)
        XCTAssertEqual(field.stringValue, "", "Lucide clear button must retain its native behavior")
    }
}
