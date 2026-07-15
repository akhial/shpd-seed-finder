import Foundation

public struct SavedQuery: Codable, Sendable {
    public var requirements: [ItemRequirement]
    public var maximumDepth: Int
    public var requireBlacksmith: Bool
    public var excludeBlacksmithRewards: Bool
    public var fastMode: Bool
    public var challenges: Int
    public init(requirements: [ItemRequirement] = [], maximumDepth: Int = 24,
                requireBlacksmith: Bool = false, excludeBlacksmithRewards: Bool = false,
                fastMode: Bool = false, challenges: Int = 0) {
        self.requirements = requirements; self.maximumDepth = maximumDepth
        self.requireBlacksmith = requireBlacksmith
        self.excludeBlacksmithRewards = excludeBlacksmithRewards; self.fastMode = fastMode
        self.challenges = challenges
    }
    private enum CodingKeys: String, CodingKey {
        case requirements, maximumDepth, requireBlacksmith, excludeBlacksmithRewards, fastMode, challenges
    }
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        requirements = try container.decode([ItemRequirement].self, forKey: .requirements)
        maximumDepth = try container.decode(Int.self, forKey: .maximumDepth)
        requireBlacksmith = try container.decode(Bool.self, forKey: .requireBlacksmith)
        excludeBlacksmithRewards = try container.decodeIfPresent(
            Bool.self, forKey: .excludeBlacksmithRewards) ?? false
        // Saved queries predating the fast-mode toggle omit the key.
        fastMode = try container.decodeIfPresent(Bool.self, forKey: .fastMode) ?? false
        challenges = try container.decodeIfPresent(Int.self, forKey: .challenges) ?? 0
    }
    public func validated() -> SavedQuery? {
        guard (1...24).contains(maximumDepth), (0...511).contains(challenges) else { return nil }
        for requirement in requirements {
            if let item = requirement.item, ItemCatalog.findById(item.id) != item { return nil }
            if let modifier = requirement.modifier,
               !ItemCatalog.modifiersFor(requirement.kind).contains(modifier) { return nil }
            guard (try? ItemRequirement(key: requirement.key, item: requirement.item,
                upgrade: requirement.upgrade, modifier: requirement.modifier, kind: requirement.kind,
                tier: requirement.tier, tierMatch: requirement.tierMatch,
                upgradeMatch: requirement.upgradeMatch, source: requirement.source,
                identityGroup: requirement.identityGroup,
                maximumDepth: requirement.maximumDepth,
                requireUncursed: requirement.requireUncursed)) != nil else { return nil }
        }
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
        guard let data = text.data(using: .utf8),
              let presets = try? JSONDecoder().decode([QueryPreset].self, from: data) else { return [] }
        return presets.filter { !$0.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && $0.query.validated() != nil }
    }
}
