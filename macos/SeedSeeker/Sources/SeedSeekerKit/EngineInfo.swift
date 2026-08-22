import CSeedFinder
import Foundation

/// The engine's own constants, read once from `seedfinder_engine_info`.
///
/// Every value here is a fact about the linked Rust engine — the upstream game
/// version it targets, the bounds its validator applies, the game data its
/// generator uses — so the app reads them from the engine instead of keeping
/// mirrors that can drift.
public struct EngineInfo: Sendable {
    /// The bounds the engine's own query validator applies.
    public struct Limits: Sendable {
        public let maxDepth: Int
        public let exactTierMin: Int
        public let exactTierMax: Int
        public let boundedTierMin: Int
        public let boundedTierMax: Int
        public let identityGroupMax: Int
        public let maxUpgradeDefault: Int
        public let maxUpgradeRing: Int
        public let maxResults: Int
        public let resultsFileMaxBytes: Int
    }

    /// One challenge bit: its document name, its mask, and whether the
    /// generator consults it (which changes what a seed generates).
    public struct ChallengeInfo: Sendable {
        public let name: String
        public let mask: Int
        public let changesLevelGeneration: Bool
    }

    /// Upstream Shattered Pixel Dungeon version the engine targets.
    public let shpdVersion: String
    public let limits: Limits
    /// Boss floors that generate no searchable items, so a floor limit on one
    /// means the same as the floor below it.
    public let emptyBossFloors: Set<Int>
    /// The floors each quest giver can appear on.
    public let questWindows: [ScoutQuestKind: ClosedRange<Int>]
    /// Every challenge the engine knows, in mask order.
    public let challenges: [ChallengeInfo]

    /// Every challenge bit together: the largest legal challenge mask.
    public var challengeMask: Int { challenges.reduce(0) { $0 | $1.mask } }

    /// The one instance, loaded on first use.
    public static let shared = load()

    /// The engine's own name for a quest giver, as `quest_windows` keys them.
    private static let questNames: [(name: String, kind: ScoutQuestKind)] = [
        ("ghost", .ghost), ("wandmaker", .wandmaker),
        ("blacksmith", .blacksmith), ("imp", .imp),
    ]

    private static func load() -> EngineInfo {
        guard let packet = try? enginePacket({ out, length in
                  seedfinder_engine_info(out, length)
              }),
              let document = (try? JSONSerialization.jsonObject(with: packet)) as? [String: Any],
              let shpdVersion = document["shpdVersion"] as? String,
              let limitsValue = document["limits"] as? [String: Any],
              let limits = decodeLimits(limitsValue),
              let bossFloors = document["empty_boss_floors"] as? [Int],
              let windowsValue = document["quest_windows"] as? [String: Any],
              let windows = decodeQuestWindows(windowsValue),
              let challengesValue = document["challenges"] as? [[String: Any]]
        else {
            // The document is a constant of the statically linked engine, so
            // there is no runtime condition under which it can be missing.
            preconditionFailure("the linked engine returned no usable engine-info document")
        }
        let challenges = challengesValue.compactMap { entry -> ChallengeInfo? in
            guard let name = entry["name"] as? String, let mask = entry["mask"] as? Int,
                  let changes = entry["changes_level_generation"] as? Bool else { return nil }
            return ChallengeInfo(name: name, mask: mask, changesLevelGeneration: changes)
        }
        precondition(challenges.count == challengesValue.count,
                     "the engine listed a challenge this build cannot read")
        return EngineInfo(shpdVersion: shpdVersion, limits: limits,
                          emptyBossFloors: Set(bossFloors), questWindows: windows,
                          challenges: challenges)
    }

    private static func decodeLimits(_ value: [String: Any]) -> Limits? {
        func field(_ key: String) -> Int? { value[key] as? Int }
        guard let maxDepth = field("max_depth"),
              let exactTierMin = field("exact_tier_min"), let exactTierMax = field("exact_tier_max"),
              let boundedTierMin = field("bounded_tier_min"),
              let boundedTierMax = field("bounded_tier_max"),
              let identityGroupMax = field("identity_group_max"),
              let maxUpgradeDefault = field("max_upgrade_default"),
              let maxUpgradeRing = field("max_upgrade_ring"),
              let maxResults = field("max_results"),
              let resultsFileMaxBytes = field("results_file_max_bytes") else { return nil }
        return Limits(maxDepth: maxDepth, exactTierMin: exactTierMin, exactTierMax: exactTierMax,
                      boundedTierMin: boundedTierMin, boundedTierMax: boundedTierMax,
                      identityGroupMax: identityGroupMax, maxUpgradeDefault: maxUpgradeDefault,
                      maxUpgradeRing: maxUpgradeRing, maxResults: maxResults,
                      resultsFileMaxBytes: resultsFileMaxBytes)
    }

    private static func decodeQuestWindows(_ value: [String: Any]) -> [ScoutQuestKind: ClosedRange<Int>]? {
        var windows: [ScoutQuestKind: ClosedRange<Int>] = [:]
        for (name, kind) in questNames {
            guard let window = value[name] as? [Int], window.count == 2,
                  window[0] <= window[1] else { return nil }
            windows[kind] = window[0]...window[1]
        }
        return windows
    }
}
