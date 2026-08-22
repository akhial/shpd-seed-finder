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

    /// The one instance, loaded on first use.
    public static let shared = load()

    private static func load() -> EngineInfo {
        guard let packet = try? enginePacket({ out, length in
                  seedfinder_engine_info(out, length)
              }),
              let document = (try? JSONSerialization.jsonObject(with: packet)) as? [String: Any],
              let shpdVersion = document["shpdVersion"] as? String
        else {
            // The document is a constant of the statically linked engine, so
            // there is no runtime condition under which it can be missing.
            preconditionFailure("the linked engine returned no usable engine-info document")
        }
        return EngineInfo(shpdVersion: shpdVersion)
    }
}
