import CSeedFinder
import Foundation

public enum SeedFinderEngineError: Error, LocalizedError, Sendable {
    case invalidArgument, internalFailure, unknownHandle, invalidResponse
    public var errorDescription: String? {
        switch self {
        case .invalidArgument: "The engine rejected the request"
        case .internalFailure: "The native engine failed"
        case .unknownHandle: "The native search session is closed"
        case .invalidResponse: "The native engine returned an invalid response"
        }
    }
}

public protocol SeedFinderSearchSession: Sendable {
    func poll(_ maximum: Int) async throws -> [SeedResult]
    func status() async throws -> SearchStatus
    func resumeHint() async throws -> ResumeHint
    func cancel() async
    func close() async
}

public protocol SeedFinderEngine: Sendable {
    func startSearch(_ request: SearchRequest) async throws -> any SeedFinderSearchSession
    func startResumedSearch(_ request: SearchRequest, resumeFrom: Int64, scanLen: Int64) async throws -> any SeedFinderSearchSession
    func filterSeeds(_ request: SearchRequest, seeds: [String]) async throws -> [String]
    func scoutSeed(_ seed: String, challenges: Int) async throws -> ScoutWorld
}

private func ffiError(_ code: Int32) -> SeedFinderEngineError {
    switch code { case -1: .invalidArgument; case -3: .unknownHandle; default: .internalFailure }
}

private func copiedPacket(_ pointer: UnsafeMutablePointer<UInt8>?, _ length: Int) throws -> Data {
    guard let pointer else { throw SeedFinderEngineError.invalidResponse }
    defer { seedfinder_buffer_free(pointer, length) }
    return Data(bytes: pointer, count: length)
}

/// The engine's refine soundness predicate, bridged rather than re-derived:
/// whether the SSF7 query in `candidate` continues the one in `base` —
/// identical scope (floor limit, challenges, blacksmith flags, fast mode) and
/// every base requirement covered by a distinct candidate requirement at least
/// as strict — equal or strengthened (a named item, a tightened bound).
///
/// Unlike the session calls this is synchronous: the decision gates Start
/// Search, and the native side only decodes two packets and compares them.
/// It is deliberately outside `SeedFinderEngine` — the rule is the engine's
/// regardless of which engine runs the search, so a test double cannot answer
/// it differently.
public enum QueryContinuation {
    /// Anything but a definite yes (a "no", or an undecodable packet the FFI
    /// reports negative) reads as "does not continue", which is the safe
    /// direction: the search re-anchors and rescans instead of reusing results
    /// whose coverage it cannot claim.
    public static func continues(_ candidate: Data, base: Data) -> Bool {
        candidate.withUnsafeBytes { candidateBytes in
            base.withUnsafeBytes { baseBytes in
                seedfinder_query_continues(candidateBytes.bindMemory(to: UInt8.self).baseAddress, candidateBytes.count,
                                           baseBytes.bindMemory(to: UInt8.self).baseAddress, baseBytes.count) == 1
            }
        }
    }
}

public struct ProductionSeedFinderEngine: SeedFinderEngine {
    public init() {}

    public func startSearch(_ request: SearchRequest) async throws -> any SeedFinderSearchSession {
        let encoded = try QueryCodec.encode(request)
        let handle: Int64 = await Task.detached {
            encoded.withUnsafeBytes { bytes in seedfinder_start_search(bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count) }
        }.value
        guard handle != 0 else { throw SeedFinderEngineError.invalidArgument }
        return NativeSearchSession(handle: handle, requirementCount: request.requirements.count)
    }

    public func startResumedSearch(_ request: SearchRequest, resumeFrom: Int64, scanLen: Int64) async throws -> any SeedFinderSearchSession {
        let encoded = try QueryCodec.encode(request)
        let handle: Int64 = await Task.detached {
            encoded.withUnsafeBytes { bytes in
                seedfinder_start_resumed_search(bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count,
                                                UInt64(bitPattern: resumeFrom), UInt64(bitPattern: scanLen))
            }
        }.value
        guard handle != 0 else { throw SeedFinderEngineError.invalidArgument }
        return NativeSearchSession(handle: handle, requirementCount: request.requirements.count)
    }

    public func filterSeeds(_ request: SearchRequest, seeds: [String]) async throws -> [String] {
        guard !seeds.isEmpty else { return [] }
        let encoded = try QueryCodec.encode(request)
        let values: [UInt64] = try seeds.map { seed in
            guard let value = SeedCode.value(of: seed) else { throw SeedFinderEngineError.invalidArgument }
            return UInt64(value)
        }
        let count = request.requirements.count
        let packet: Data = try await Task.detached {
            var pointer: UnsafeMutablePointer<UInt8>?
            var length = 0
            let code = encoded.withUnsafeBytes { requestBytes in
                values.withUnsafeBufferPointer { seedValues in
                    seedfinder_filter_seeds(requestBytes.bindMemory(to: UInt8.self).baseAddress, requestBytes.count,
                                            seedValues.baseAddress, seedValues.count, &pointer, &length)
                }
            }
            guard code == 0 else { throw ffiError(code) }
            return try copiedPacket(pointer, length)
        }.value
        return try ResultCodec.decode(packet, requirementCount: count).map(\.seed)
    }

    public func scoutSeed(_ seed: String, challenges: Int = 0) async throws -> ScoutWorld {
        let request = try ScoutCodec.encodeRequest(seed: seed, challenges: challenges)
        let packet: Data = try await Task.detached {
            var pointer: UnsafeMutablePointer<UInt8>?
            var length = 0
            let code = request.withUnsafeBytes { bytes in
                seedfinder_scout(bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count, &pointer, &length)
            }
            guard code == 0 else { throw ffiError(code) }
            return try copiedPacket(pointer, length)
        }.value
        let world = try ScoutCodec.decode(packet)
        guard world.seed == seed else { throw SeedFinderEngineError.invalidResponse }
        return world
    }
}

private final class NativeSearchSession: SeedFinderSearchSession, @unchecked Sendable {
    private let handle: Int64
    private let requirementCount: Int
    private let lock = NSLock()
    private var closed = false
    init(handle: Int64, requirementCount: Int) { self.handle = handle; self.requirementCount = requirementCount }

    private func activeHandle() throws -> Int64 {
        lock.lock(); defer { lock.unlock() }
        guard !closed else { throw SeedFinderEngineError.unknownHandle }
        return handle
    }
    private func markClosed() -> Bool {
        lock.lock(); defer { lock.unlock() }
        let wasOpen = !closed; closed = true
        return wasOpen
    }
    func poll(_ maximum: Int) async throws -> [SeedResult] {
        guard (1...1024).contains(maximum) else { throw SeedFinderEngineError.invalidArgument }
        let handle = try activeHandle(), count = requirementCount
        let packet: Data = try await Task.detached {
            var pointer: UnsafeMutablePointer<UInt8>?; var length = 0
            let code = seedfinder_poll(handle, UInt32(maximum), &pointer, &length)
            guard code == 0 else { throw ffiError(code) }
            return try copiedPacket(pointer, length)
        }.value
        return try ResultCodec.decode(packet, requirementCount: count)
    }
    func status() async throws -> SearchStatus {
        let handle = try activeHandle()
        return try await Task.detached {
            var values = [Int64](repeating: 0, count: 5)
            let code = seedfinder_status(handle, &values)
            guard code == 0 else { throw ffiError(code) }
            guard let state = SearchState(rawValue: Int(values[0])) else { throw SeedFinderEngineError.invalidResponse }
            let probability = Double(bitPattern: UInt64(bitPattern: values[4]))
            guard probability.isFinite, (0...1).contains(probability) else { throw SeedFinderEngineError.invalidResponse }
            return SearchStatus(state: state, scannedSeeds: max(0, values[1]), totalSeeds: max(0, values[2]), errorCode: values[3], matchProbability: probability)
        }.value
    }
    func resumeHint() async throws -> ResumeHint {
        let handle = try activeHandle()
        return try await Task.detached {
            var values = [Int64](repeating: 0, count: 2)
            let code = seedfinder_resume_hint(handle, &values)
            guard code == 0 else { throw ffiError(code) }
            return ResumeHint(position: values[0], remaining: values[1])
        }.value
    }
    func cancel() async {
        guard let handle = try? activeHandle() else { return }
        await Task.detached { seedfinder_cancel(handle) }.value
    }
    func close() async {
        if markClosed() { await Task.detached { seedfinder_close(self.handle) }.value }
    }
    deinit {
        if markClosed() {
            let handle = handle
            Task.detached { seedfinder_close(handle) }
        }
    }
}
