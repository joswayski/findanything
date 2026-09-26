import Foundation

struct SearchResult: Codable, Equatable {
    let id, kind, title, subtitle: String
    let score: Double
    let reason: String
}

struct SearchResponse: Codable, Equatable {
    let results: [SearchResult]
    let semanticStatus: String
    let semanticMessage: String?
}

struct Envelope<T: Decodable>: Decodable {
    let ok: Bool
    let error: String?
    let response: T?
}

struct UpdateStatus: Codable, Equatable { let state, message: String }
struct UpdateEnvelope: Decodable { let ok: Bool; let error: String?; let status: UpdateStatus? }
struct InstanceEnvelope: Decodable { let ok: Bool; let error: String?; let primary: Bool?; let activate: Bool? }

final class SearchState {
    private(set) var generation = 0
    private var acceptedGeneration = -1
    private(set) var results: [SearchResult] = []
    private(set) var selected = 0
    func begin() -> Int { generation += 1; return generation }
    @discardableResult func accept(_ value: [SearchResult], generation candidate: Int) -> Bool {
        guard candidate == generation else { return false }
        acceptedGeneration = candidate
        results = value; selected = min(selected, max(0, value.count - 1)); return true
    }
    func move(_ delta: Int) { selected = min(max(0, selected + delta), max(0, results.count - 1)) }
    func resetSelection() { selected = 0 }
    func selectedResult(generation candidate: Int) -> SearchResult? {
        guard candidate == generation, acceptedGeneration == generation, results.indices.contains(selected) else { return nil }
        return results[selected]
    }
}
