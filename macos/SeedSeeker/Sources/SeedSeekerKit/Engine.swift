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
    func cancel() async
    func close() async
}

public protocol SeedFinderEngine: Sendable {
    func startSearch(_ request: SearchRequest) async throws -> any SeedFinderSearchSession
    func scoutSeed(_ seed: String) async throws -> ScoutWorld
}

private func ffiError(_ code: Int32) -> SeedFinderEngineError {
    switch code { case -1: .invalidArgument; case -3: .unknownHandle; default: .internalFailure }
}

private func copiedPacket(_ pointer: UnsafeMutablePointer<UInt8>?, _ length: Int) throws -> Data {
    guard let pointer else { throw SeedFinderEngineError.invalidResponse }
    defer { seedfinder_buffer_free(pointer, length) }
    return Data(bytes: pointer, count: length)
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

    public func scoutSeed(_ seed: String) async throws -> ScoutWorld {
        guard SeedCode.isCanonical(seed) else { throw SeedFinderEngineError.invalidArgument }
        let request = Data(seed.utf8)
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
