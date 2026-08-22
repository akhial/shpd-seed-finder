import Foundation

public struct SavedQuery: Codable, Sendable {
    public var requirements: [ItemRequirement]
    public var maximumDepth: Int
    public var requireBlacksmith: Bool
    public var excludeBlacksmithRewards: Bool
    public var wandmakerQuest: WandmakerQuest?
    public var fastMode: Bool
    public var challenges: Int
    public init(requirements: [ItemRequirement] = [], maximumDepth: Int = 24,
                requireBlacksmith: Bool = false, excludeBlacksmithRewards: Bool = false,
                wandmakerQuest: WandmakerQuest? = nil,
                fastMode: Bool = false, challenges: Int = 0) {
        self.requirements = requirements; self.maximumDepth = maximumDepth
        self.requireBlacksmith = requireBlacksmith
        self.excludeBlacksmithRewards = excludeBlacksmithRewards
        self.wandmakerQuest = wandmakerQuest; self.fastMode = fastMode
        self.challenges = challenges
    }
    private enum CodingKeys: String, CodingKey {
        case requirements, maximumDepth, requireBlacksmith, excludeBlacksmithRewards
        case wandmakerQuest, fastMode, challenges
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
        // Saved queries predating the fast-mode toggle omit the key.
        fastMode = try container.decodeIfPresent(Bool.self, forKey: .fastMode) ?? false
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
                upgradeSum: requirement.upgradeSum)) != nil else { return nil }
        }
        // A combined-upgrade group that no longer adds up still loads — the
        // editor shows why the search cannot start — but the engine would
        // refuse it, so the sum check is the request's, not this loader's.
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
    public static let all: [QueryPreset] = [staff21, wandBonanza, ringOfWealth21]

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
