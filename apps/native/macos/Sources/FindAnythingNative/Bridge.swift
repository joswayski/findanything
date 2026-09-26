import Foundation
import CFindAnything

protocol EngineCalling { func request(_ object: [String: Any]) throws -> Data }

final class RustBridge: EngineCalling {
    func request(_ object: [String: Any]) throws -> Data {
        let input = try JSONSerialization.data(withJSONObject: object)
        guard let json = String(data: input, encoding: .utf8) else { throw BridgeError.invalid }
        return try json.withCString { pointer in
            guard let output = fa_request(pointer) else { throw BridgeError.invalid }
            defer { fa_string_free(output) }
            return Data(String(cString: output).utf8)
        }
    }
    enum BridgeError: Error { case invalid }
}

final class EngineWorker {
    private let queue = DispatchQueue(label: "app.findanything.engine", qos: .userInitiated)
    private let bridge: EngineCalling
    private let lock = NSLock()
    private var generation = 0
    init(_ bridge: EngineCalling) { self.bridge = bridge }
    func invalidate(_ value: Int) { lock.lock(); generation = value; lock.unlock() }
    private func isCurrent(_ value: Int?) -> Bool {
        lock.lock(); defer { lock.unlock() }
        return value == nil || value == generation
    }
    func call<T: Decodable>(_ request: [String: Any], generation: Int? = nil, as: T.Type, completion: @escaping (Result<T, Error>) -> Void) {
        queue.async {
            guard self.isCurrent(generation) else { return }
            let value = Result { try JSONDecoder().decode(T.self, from: self.bridge.request(request)) }
            DispatchQueue.main.async { completion(value) }
        }
    }
}
