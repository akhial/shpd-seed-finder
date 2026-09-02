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
    /// `workers` is how many search threads to spawn; the engine clamps it to
    /// the host's parallelism and takes 0 as "every available core". It is a
    /// property of this machine, not of the query, so it travels beside the
    /// request rather than inside it.
    func startSearch(_ request: SearchRequest, workers: Int) async throws -> any SeedFinderSearchSession
    func startResumedSearch(_ request: SearchRequest, resumeFrom: Int64, scanLen: Int64, workers: Int) async throws -> any SeedFinderSearchSession
    func filterSeeds(_ request: SearchRequest, seeds: [String]) async throws -> [String]
    func scoutSeed(_ seed: String, challenges: Int) async throws -> ScoutWorld
}

private func ffiError(_ code: Int32) -> SeedFinderEngineError {
    switch code { case -1: .invalidArgument; case -3: .unknownHandle; default: .internalFailure }
}

/// Narrows a worker count to the FFI's `uint32_t`. A negative or absurd value
/// cannot be expressed there, and 0 already means "every available core", so
/// anything out of range becomes that rather than trapping.
private func ffiWorkers(_ workers: Int) -> UInt32 {
    guard workers > 0, let count = UInt32(exactly: workers) else { return 0 }
    return count
}

private func copiedPacket(_ pointer: UnsafeMutablePointer<UInt8>?, _ length: Int) throws -> Data {
    guard let pointer else { throw SeedFinderEngineError.invalidResponse }
    defer { seedfinder_buffer_free(pointer, length) }
    return Data(bytes: pointer, count: length)
}

/// Runs one out-buffer FFI call and copies its packet out, mapping the return
/// code to a `SeedFinderEngineError`. The entry points that use this — the
/// results, share, seed-code, decision and engine-info codecs — only transform
/// bytes, so like `QueryContinuation` they stay synchronous.
func enginePacket(
    _ call: (UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?, UnsafeMutablePointer<Int>?) -> Int32
) throws -> Data {
    var pointer: UnsafeMutablePointer<UInt8>?
    var length = 0
    let code = call(&pointer, &length)
    guard code == 0 else { throw ffiError(code) }
    return try copiedPacket(pointer, length)
}

/// The engine's refine soundness predicate, bridged rather than re-derived:
/// whether the query document in `candidate` continues the one in `base` —
/// an identical floor limit and challenge set, world conditions (the
/// blacksmith flags and the Wandmaker filter) at least as strict as the base's,
/// and every base requirement covered by a distinct candidate requirement at least
/// as strict — equal or strengthened (a named item, a tightened bound).
///
/// Unlike the session calls this is synchronous: the decision gates Start
/// Search, and the native side only decodes two packets and compares them.
/// It is deliberately outside `SeedFinderEngine` — the rule is the engine's
/// regardless of which engine runs the search, so a test double cannot answer
/// it differently.
public enum QueryContinuation {
    /// Anything but a definite yes (a "no", or an undecodable document the FFI
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

/// What pressing Start Search does with a query, decided by the engine rather
/// than re-derived: `seedfinder_decide_start` answers the whole multi-way
/// choice of `docs/search-semantics.md`, continuation predicate and item
/// sharing included, so no frontend can drift from the others.
public enum StartDecision {
    /// The decision for `candidate` against the session's Target and, when the
    /// last concluded run was itself detached, that run's query.
    ///
    /// A query the engine cannot decode decides nothing, so the answer falls
    /// back to a full scan that touches nothing: `.anchor` when there is no
    /// Target to preserve, `.detached` when there is.
    public static func decide(candidate: SearchRequest, target: SearchRequest?,
                              targetSetEmpty: Bool, targetHasUncoveredSeeds: Bool,
                              detachedBase: SearchRequest?) -> StartMode {
        guard let target else { return .anchor }
        guard let candidatePacket = try? QueryDocument.encode(candidate),
              let targetPacket = try? QueryDocument.encode(target) else { return .detached }
        let basePacket = detachedBase.flatMap { try? QueryDocument.encode($0) }
        guard let packet = try? enginePacket({ out, length in
                  candidatePacket.withUnsafeBytes { candidateBytes in
                      targetPacket.withUnsafeBytes { targetBytes in
                          withOptionalBytes(basePacket) { basePointer, baseLength in
                              seedfinder_decide_start(
                                  candidateBytes.bindMemory(to: UInt8.self).baseAddress,
                                  candidateBytes.count,
                                  targetBytes.bindMemory(to: UInt8.self).baseAddress,
                                  targetBytes.count,
                                  targetSetEmpty ? 1 : 0, targetHasUncoveredSeeds ? 1 : 0,
                                  basePointer, baseLength, out, length)
                          }
                      }
                  }
              }),
              let name = String(data: packet, encoding: .utf8),
              let mode = StartMode(engineName: name) else { return .detached }
        return mode
    }
}

/// Passes an absent packet to the FFI as the null pointer it expects.
private func withOptionalBytes<T>(_ data: Data?, _ body: (UnsafePointer<UInt8>?, Int) -> T) -> T {
    guard let data else { return body(nil, 0) }
    return data.withUnsafeBytes { body($0.bindMemory(to: UInt8.self).baseAddress, $0.count) }
}

/// Which items of a scouted world explain a query's requirements, decided by
/// the engine rather than re-derived: `seedfinder_scout_matches` runs the same
/// maximum-partial-assignment the matcher uses, so the marks agree with the
/// search that produced the seed.
///
/// Like `QueryContinuation` this is synchronous and outside `SeedFinderEngine`:
/// the selection is the engine's whatever engine ran the search.
public struct ScoutMatches: Sendable {
    /// Indices into the scouted world's item list, in the order
    /// `scoutSeed(_:challenges:)` returns it.
    public let matched: Set<Int>
    /// How many conditions the marks satisfy, and how many there are. An
    /// alternative group is one slot however many members it has; a
    /// combined-level group counts once, its contributing items are all
    /// marked when it reaches its total, and items serving a short group are
    /// not marked at all.
    public let matchedRequirements: Int
    public let totalRequirements: Int

    public init(matched: Set<Int>, matchedRequirements: Int, totalRequirements: Int) {
        self.matched = matched
        self.matchedRequirements = matchedRequirements
        self.totalRequirements = totalRequirements
    }

    /// Marks the world identified by `request` — the very SSQ2 packet the
    /// scout call took, so both describe the same world — against the query
    /// document in `query`.
    public static func mark(_ request: Data, query: Data) throws -> ScoutMatches {
        let packet = try enginePacket { out, length in
            request.withUnsafeBytes { requestBytes in
                query.withUnsafeBytes { queryBytes in
                    seedfinder_scout_matches(
                        requestBytes.bindMemory(to: UInt8.self).baseAddress, requestBytes.count,
                        queryBytes.bindMemory(to: UInt8.self).baseAddress, queryBytes.count,
                        out, length)
                }
            }
        }
        guard let document = (try? JSONSerialization.jsonObject(with: packet)) as? [String: Any],
              let matched = document["matched"] as? [Int],
              let matchedRequirements = document["matchedRequirements"] as? Int,
              let totalRequirements = document["totalRequirements"] as? Int else {
            throw SeedFinderEngineError.invalidResponse
        }
        return ScoutMatches(matched: Set(matched), matchedRequirements: matchedRequirements,
                            totalRequirements: totalRequirements)
    }

    /// Marks the world `seed` generates under `challenges` against `query`.
    public static func mark(seed: String, challenges: Int, query: SearchRequest) throws -> ScoutMatches {
        try mark(ScoutCodec.encodeRequest(seed: seed, challenges: challenges),
                 query: QueryDocument.encode(query))
    }
}

public struct ProductionSeedFinderEngine: SeedFinderEngine {
    public init() {}

    public func startSearch(_ request: SearchRequest, workers: Int) async throws -> any SeedFinderSearchSession {
        let encoded = try QueryDocument.encode(request)
        let count = ffiWorkers(workers)
        let handle: Int64 = await Task.detached {
            encoded.withUnsafeBytes { bytes in seedfinder_start_search(bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count, count) }
        }.value
        guard handle != 0 else { throw SeedFinderEngineError.invalidArgument }
        return NativeSearchSession(handle: handle, requirementCount: request.requirements.slotCount)
    }

    public func startResumedSearch(_ request: SearchRequest, resumeFrom: Int64, scanLen: Int64, workers: Int) async throws -> any SeedFinderSearchSession {
        let encoded = try QueryDocument.encode(request)
        let count = ffiWorkers(workers)
        let handle: Int64 = await Task.detached {
            encoded.withUnsafeBytes { bytes in
                seedfinder_start_resumed_search(bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count,
                                                UInt64(bitPattern: resumeFrom), UInt64(bitPattern: scanLen), count)
            }
        }.value
        guard handle != 0 else { throw SeedFinderEngineError.invalidArgument }
        return NativeSearchSession(handle: handle, requirementCount: request.requirements.slotCount)
    }

    public func filterSeeds(_ request: SearchRequest, seeds: [String]) async throws -> [String] {
        guard !seeds.isEmpty else { return [] }
        let encoded = try QueryDocument.encode(request)
        let values: [UInt64] = try seeds.map { seed in
            guard let parsed = SeedCode.parse(seed) else { throw SeedFinderEngineError.invalidArgument }
            return UInt64(parsed.value)
        }
        let count = request.requirements.slotCount
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
        // The manifest says which items the run holds and the gem block says
        // what its rings look like; both ride in the one SSC3 packet, so the
        // decoded world is already whole.
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
