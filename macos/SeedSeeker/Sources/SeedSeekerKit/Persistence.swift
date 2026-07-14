import Foundation

public struct SavedQuery: Codable, Sendable {
    public var requirements: [ItemRequirement]
    public var maximumDepth: Int
    public var requireBlacksmith: Bool
    public var excludeBlacksmithRewards: Bool
    public var fastMode: Bool
    public init(requirements: [ItemRequirement] = [], maximumDepth: Int = 24,
                requireBlacksmith: Bool = false, excludeBlacksmithRewards: Bool = false,
                fastMode: Bool = false) {
        self.requirements = requirements; self.maximumDepth = maximumDepth
        self.requireBlacksmith = requireBlacksmith
        self.excludeBlacksmithRewards = excludeBlacksmithRewards; self.fastMode = fastMode
    }
    private enum CodingKeys: String, CodingKey {
        case requirements, maximumDepth, requireBlacksmith, excludeBlacksmithRewards, fastMode
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
    }
    public func validated() -> SavedQuery? {
        guard (1...24).contains(maximumDepth) else { return nil }
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
