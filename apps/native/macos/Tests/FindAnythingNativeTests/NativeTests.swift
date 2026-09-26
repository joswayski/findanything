import XCTest
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
}
