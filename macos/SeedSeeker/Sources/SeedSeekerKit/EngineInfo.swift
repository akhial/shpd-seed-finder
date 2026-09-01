import CSeedFinder
import Foundation

/// The engine's own constants, read once from `seedfinder_engine_info`.
///
/// Every value here is a fact about the linked Rust engine — the upstream game
/// version it targets, the bounds its validator applies, the game data its
/// generator uses — so the app reads them from the engine instead of keeping
/// mirrors that can drift.
public struct EngineInfo: Sendable {
    /// Upstream Shattered Pixel Dungeon version the engine targets.
    public let shpdVersion: String

    /// Upstream revision pin. For v4.0.0 no source has been published, so
    /// this holds the SHA-256 digest of the official release JAR instead of
    /// a commit hash.
    public let shpdCommit: String

    /// The one instance, loaded on first use.
    public static let shared = load()

    /// Logical processors available to search workers, never less than one:
    /// the ceiling of the worker selector. Unlike the constants above this
    /// describes the host rather than the build, so it is read per call
    /// instead of being frozen into `shared`.
    public static var availableWorkers: Int { max(1, Int(seedfinder_available_workers())) }

    private static func load() -> EngineInfo {
        guard let packet = try? enginePacket({ out, length in
                  seedfinder_engine_info(out, length)
              }),
              let document = (try? JSONSerialization.jsonObject(with: packet)) as? [String: Any],
              let shpdVersion = document["shpdVersion"] as? String,
              let shpdCommit = document["shpdCommit"] as? String
        else {
            // The document is a constant of the statically linked engine, so
            // there is no runtime condition under which it can be missing.
            preconditionFailure("the linked engine returned no usable engine-info document")
        }
        return EngineInfo(shpdVersion: shpdVersion, shpdCommit: shpdCommit)
    }
}
