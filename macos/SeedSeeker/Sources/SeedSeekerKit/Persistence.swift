import Foundation

public struct SavedQuery: Codable, Sendable {
    public var requirements: [ItemRequirement]
    public var maximumDepth: Int
    public var requireBlacksmith: Bool
    public var excludeBlacksmithRewards: Bool
    public var wandmakerQuest: WandmakerQuest?
    public var challenges: Int
    public init(requirements: [ItemRequirement] = [], maximumDepth: Int = 24,
                requireBlacksmith: Bool = false, excludeBlacksmithRewards: Bool = false,
                wandmakerQuest: WandmakerQuest? = nil,
                challenges: Int = 0) {
        self.requirements = requirements; self.maximumDepth = maximumDepth
        self.requireBlacksmith = requireBlacksmith
        self.excludeBlacksmithRewards = excludeBlacksmithRewards
        self.wandmakerQuest = wandmakerQuest
        self.challenges = challenges
    }
    private enum CodingKeys: String, CodingKey {
        case requirements, maximumDepth, requireBlacksmith, excludeBlacksmithRewards
        case wandmakerQuest, challenges
    }
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        requirements = try container.decode([ItemRequirement].self, forKey: .requirements)
        // Queries saved before empty boss floors were removed may hold 5/10/15;
        // snap them to the equivalent limit below.
        maximumDepth = FloorLimits.normalize(try container.decode(Int.self, forKey: .maximumDepth))
        requireBlacksmith = try container.decode(Bool.self, forKey: .requireBlacksmith)
        excludeBlacksmithRewards = try container.decodeIfPresent(
            Bool.self, forKey: .excludeBlacksmithRewards) ?? false
        // A quest id a newer build knows falls back to "any" rather than
        // discarding the whole saved query.
        wandmakerQuest = (try? container.decodeIfPresent(WandmakerQuest.self, forKey: .wandmakerQuest)) ?? nil
        // Queries saved while the fast-mode toggle existed carry a `fastMode`
        // key; it has no coding key any more, so decoding skips it and the
        // query loads as an ordinary full search.
        challenges = try container.decodeIfPresent(Int.self, forKey: .challenges) ?? 0
    }
    public func validated() -> SavedQuery? {
        guard (1...SearchLimits.maxDepth).contains(maximumDepth), (0...SearchLimits.challengeMask).contains(challenges) else { return nil }
        for requirement in requirements {
            if let item = requirement.item, ItemCatalog.findById(item.id) != item { return nil }
            // The validating initializer also checks every effect name
            // against the catalog.
            guard (try? ItemRequirement(key: requirement.key, item: requirement.item,
                upgrade: requirement.upgrade, effect: requirement.effect, kind: requirement.kind,
                tier: requirement.tier, tierMatch: requirement.tierMatch,
                upgradeMatch: requirement.upgradeMatch, source: requirement.source,
                identityGroup: requirement.identityGroup,
                maximumDepth: requirement.maximumDepth,
                requireUncursed: requirement.requireUncursed,
                alternativeGroup: requirement.alternativeGroup,
                levelSum: requirement.levelSum, selectTrinket: requirement.selectTrinket)) != nil else { return nil }
        }
        // A combined-level group that no longer adds up, or a same-item group
        // constrained twice, still loads — the editor shows why the search
        // cannot start — but the engine would refuse it, so the group checks
        // are the request's, not this loader's.
        return self
    }
}

public struct QueryPreset: Codable, Hashable, Identifiable, Sendable {
    public let id: UUID
    public var name: String
    public var query: SavedQuery

    public init(id: UUID = UUID(), name: String, query: SavedQuery) {
        self.id = id; self.name = name; self.query = query
    }
}

extension SavedQuery: Hashable {}

public enum BuiltInPresets {
    public static let all: [QueryPreset] = [staff21, staff22, wandBonanza, ringOfWealth21, tier4Weapon26]

    /// The floor limit the vault presets carry: floor 19 is the last floor the
    /// Imp — and so the vault holding its levelled prizes — can appear on, so a
    /// deeper scan only costs time.
    private static let vaultFloorLimit = 19

    public static let staff21 = QueryPreset(
        id: UUID(uuidString: "C3DB688D-3D7D-43F0-B10E-9BCBEA272101")!,
        name: "+21 Staff",
        query: SavedQuery(requirements: [
            try! ItemRequirement(key: 1, item: nil, upgrade: 3, kind: .wand,
                                 upgradeMatch: .exactly, identityGroup: 1),
            try! ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .wand,
                                 upgradeMatch: .any, identityGroup: 1),
            try! ItemRequirement(key: 3, item: nil, upgrade: 0, kind: .wand,
                                 upgradeMatch: .any, identityGroup: 1),
            try! ItemRequirement(key: 4, item: nil, upgrade: 1, kind: .wand,
                                 upgradeMatch: .atLeast),
        ]))

    /// The +21 stack anchored one level higher, on the +4 wand v4.0.0's Imp
    /// vault lays out among its prizes.
    public static let staff22 = QueryPreset(
        id: UUID(uuidString: "C3DB688D-3D7D-43F0-B10E-9BCBEA272104")!,
        name: "+22 Staff",
        query: SavedQuery(requirements: [
            try! ItemRequirement(key: 1, item: nil, upgrade: 4, kind: .wand,
                                 upgradeMatch: .exactly, identityGroup: 1),
            try! ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .wand,
                                 upgradeMatch: .any, identityGroup: 1),
            try! ItemRequirement(key: 3, item: nil, upgrade: 0, kind: .wand,
                                 upgradeMatch: .any, identityGroup: 1),
            try! ItemRequirement(key: 4, item: nil, upgrade: 1, kind: .wand,
                                 upgradeMatch: .atLeast),
        ], maximumDepth: vaultFloorLimit))

    public static let wandBonanza = QueryPreset(
        id: UUID(uuidString: "C3DB688D-3D7D-43F0-B10E-9BCBEA272103")!,
        name: "Wand Bonanza",
        query: SavedQuery(requirements: [
            try! ItemRequirement(key: 1, item: nil, upgrade: 3, kind: .wand,
                                 upgradeMatch: .exactly),
            try! ItemRequirement(key: 2, item: nil, upgrade: 2, kind: .wand,
                                 upgradeMatch: .exactly, maximumDepth: 4),
            try! ItemRequirement(key: 3, item: nil, upgrade: 2, kind: .wand,
                                 upgradeMatch: .exactly, maximumDepth: 4),
            try! ItemRequirement(key: 4, item: nil, upgrade: 2, kind: .wand,
                                 upgradeMatch: .exactly),
        ]))

    public static let ringOfWealth21 = QueryPreset(
        id: UUID(uuidString: "C3DB688D-3D7D-43F0-B10E-9BCBEA272102")!,
        name: "+21 Ring of Wealth",
        query: SavedQuery(requirements: [
            try! ItemRequirement(key: 1, item: ItemCatalog.findById("ring_wealth"), upgrade: 4,
                                 kind: .ring, upgradeMatch: .exactly, source: .impReward),
            try! ItemRequirement(key: 2, item: ItemCatalog.findById("ring_wealth"), upgrade: 2,
                                 kind: .ring, upgradeMatch: .exactly),
            try! ItemRequirement(key: 3, item: ItemCatalog.findById("ring_wealth"), upgrade: 0,
                                 kind: .ring, upgradeMatch: .any),
        ]))

    /// A tier-4 weapon at the +5 only the vault reaches, with two more of the
    /// same weapon to pour into it.
    public static let tier4Weapon26 = QueryPreset(
        id: UUID(uuidString: "C3DB688D-3D7D-43F0-B10E-9BCBEA272105")!,
        name: "+26 Tier 4 Weapon",
        query: SavedQuery(requirements: [
            try! ItemRequirement(key: 1, item: nil, upgrade: 5, kind: .weapon,
                                 tier: 4, tierMatch: .exactly, upgradeMatch: .exactly,
                                 identityGroup: 1),
            try! ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .weapon,
                                 upgradeMatch: .any, identityGroup: 1),
            try! ItemRequirement(key: 3, item: nil, upgrade: 0, kind: .weapon,
                                 upgradeMatch: .any, identityGroup: 1),
        ], maximumDepth: vaultFloorLimit))
}

public enum QueryPersistence {
    public static func encode(_ query: SavedQuery) -> String? {
        guard let data = try? JSONEncoder().encode(query) else { return nil }
        return String(data: data, encoding: .utf8)
    }
    public static func decode(_ text: String) -> SavedQuery {
        guard let data = text.data(using: .utf8), let value = try? JSONDecoder().decode(SavedQuery.self, from: data),
              let validated = value.validated() else { return SavedQuery() }
        return validated
    }
}

/// The worker count — how many search threads the engine spawns — as a
/// device-local preference.
///
/// Deliberately not a `SavedQuery` field: it describes this machine's cores,
/// not the seeds a query matches, so it stays out of query documents, presets,
/// results exports, share links and the continuation predicate. A query that
/// travels to another machine must search the same seeds there.
///
/// The stored form is a plain `UserDefaults` integer under
/// `WorkerPersistence.defaultsKey`, read through `@AppStorage` like the other
/// preferences; `unset` is the absent-value default, so a machine that has
/// never touched the selector searches on every core.
public enum WorkerPersistence {
    public static let defaultsKey = "workerCount"

    /// The stored value meaning "never chosen": use every available core.
    /// It is also what the FFI reads as "all cores", so it needs no
    /// translation on the way down.
    public static let unset = 0

    /// The saved preference read back against this machine's ceiling: unset
    /// (or nonsense, including a negative left by a hand-edited defaults
    /// entry) means every core, and a count saved on a bigger machine is
    /// clamped down rather than discarded.
    public static func resolve(saved: Int, ceiling: Int) -> Int {
        let ceiling = max(1, ceiling)
        guard saved > 0 else { return ceiling }
        return clamp(saved, ceiling: ceiling)
    }

    /// A chosen count confined to `1...ceiling`.
    public static func clamp(_ value: Int, ceiling: Int) -> Int {
        min(max(1, value), max(1, ceiling))
    }
}

public enum PresetPersistence {
    public static func encode(_ presets: [QueryPreset]) -> String? {
        guard let data = try? JSONEncoder().encode(presets) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    public static func decode(_ text: String) -> [QueryPreset] {
        // Decode per element so one unreadable preset (for example, written
        // by a newer build with kinds this build predates) drops only
        // itself, never the whole collection.
        guard let data = text.data(using: .utf8),
              let elements = (try? JSONSerialization.jsonObject(with: data)) as? [Any] else { return [] }
        let presets = elements.compactMap { element -> QueryPreset? in
            guard JSONSerialization.isValidJSONObject(element),
                  let elementData = try? JSONSerialization.data(withJSONObject: element) else { return nil }
            return try? JSONDecoder().decode(QueryPreset.self, from: elementData)
        }
        return presets.filter { !$0.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && $0.query.validated() != nil }
    }
}
