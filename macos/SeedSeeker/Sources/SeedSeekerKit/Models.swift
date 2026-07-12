import Foundation

public enum ItemKind: Int, Codable, CaseIterable, Sendable {
    case weapon, armor, wand, ring

    public var label: String { ["Weapons", "Armor", "Wands", "Rings"][rawValue] }
    public var singularLabel: String { ["weapon", "armor", "wand", "ring"][rawValue] }
    public var modifierLabel: String? { self == .weapon ? "Enchantment" : self == .armor ? "Glyph" : nil }
    public var maximumSearchUpgrade: Int { self == .ring ? 4 : 3 }
}

public struct CatalogItem: Codable, Hashable, Identifiable, Sendable {
    public let id: String
    public let name: String
    public let kind: ItemKind
    public let spriteIndex: Int
    public let tier: Int?

    public init(id: String, name: String, kind: ItemKind, spriteIndex: Int, tier: Int? = nil) {
        self.id = id; self.name = name; self.kind = kind; self.spriteIndex = spriteIndex; self.tier = tier
    }
}

public enum UpgradeMatch: Int, Codable, CaseIterable, Sendable {
    case any, exactly, atLeast
    public var label: String { ["Any", "Exactly", "At least"][rawValue] }
}

public enum TierMatch: Int, Codable, CaseIterable, Sendable {
    case any, exactly, atLeast
    public var label: String { ["Any tier", "Exactly", "At least"][rawValue] }
}

public enum ScoutItemSource: Int, Codable, CaseIterable, Sendable {
    case heap, chest, lockedChest, crystalChest, tomb, skeleton, sacrificialFire, mimic
    case goldenMimic, crystalMimic, statue, armoredStatue, shop, ghostReward
    case wandmakerReward, blacksmithReward, impReward

    public var label: String {
        ["Heap", "Chest", "Locked chest", "Crystal chest", "Tomb", "Skeleton",
         "Sacrificial fire", "Mimic", "Golden mimic", "Crystal mimic", "Statue",
         "Armored statue", "Shop", "Ghost reward", "Wandmaker reward",
         "Blacksmith reward", "Imp reward"][rawValue]
    }
}

public enum Challenge: Int, CaseIterable, Sendable {
    case noFood = 1
    case noArmor = 2
    case noHealing = 4
    case noHerbalism = 8
    case swarmIntelligence = 16
    case darkness = 32
    case noScrolls = 64
    case championEnemies = 128
    case strongerBosses = 256

    public var label: String {
        switch self {
        case .noFood: "On diet"
        case .noArmor: "Faith is my armor"
        case .noHealing: "Pharmacophobia"
        case .noHerbalism: "Barren land"
        case .swarmIntelligence: "Swarm intelligence"
        case .darkness: "Into darkness"
        case .noScrolls: "Forbidden runes"
        case .championEnemies: "Hostile champions"
        case .strongerBosses: "Badder bosses"
        }
    }

    public var changesLevelGeneration: Bool {
        self == .noHerbalism || self == .darkness || self == .noScrolls
    }
}

public enum ModelValidationError: Error, Equatable, LocalizedError {
    case itemKind, tier, upgrade, modifier, identityGroup, itemMaximumDepth, emptyRequirements, maximumDepth, challenges
    public var errorDescription: String? {
        switch self {
        case .itemKind: "Selected item must belong to its category"
        case .tier: "Tier predicate requires any tier-2 through tier-5 weapon or armor"
        case .upgrade: "Upgrade predicate is invalid"
        case .modifier: "This category cannot carry a modifier requirement"
        case .identityGroup: "Same-item group must be A..D"
        case .itemMaximumDepth: "Item floor limit must be 1..24"
        case .emptyRequirements: "At least one requirement is needed"
        case .maximumDepth: "Maximum floor must be 1..24"
        case .challenges: "Challenge mask must be 0..511"
        }
    }
}

public struct ItemRequirement: Codable, Hashable, Identifiable, Sendable {
    public var key: Int64
    public var item: CatalogItem?
    public var upgrade: Int
    public var modifier: String?
    public var kind: ItemKind
    public var tier: Int
    public var tierMatch: TierMatch
    public var upgradeMatch: UpgradeMatch
    public var source: ScoutItemSource?
    public var identityGroup: Int?
    public var maximumDepth: Int?
    public var id: Int64 { key }

    public init(key: Int64, item: CatalogItem?, upgrade: Int, modifier: String? = nil,
                kind: ItemKind, tier: Int = 0, tierMatch: TierMatch = .any,
                upgradeMatch: UpgradeMatch = .exactly,
                source: ScoutItemSource? = nil, identityGroup: Int? = nil,
                maximumDepth: Int? = nil) throws {
        guard item == nil || item?.kind == kind else { throw ModelValidationError.itemKind }
        let validTier = switch tierMatch {
        case .any: tier == 0
        case .exactly, .atLeast:
            item == nil && (kind == .weapon || kind == .armor) && (2...5).contains(tier)
        }
        guard validTier else { throw ModelValidationError.tier }
        let valid = switch upgradeMatch {
        case .any: upgrade == 0
        case .exactly: (1...kind.maximumSearchUpgrade).contains(upgrade)
        case .atLeast: (0...kind.maximumSearchUpgrade).contains(upgrade)
        }
        guard valid else { throw ModelValidationError.upgrade }
        guard kind.modifierLabel != nil || modifier == nil else { throw ModelValidationError.modifier }
        guard identityGroup == nil || (1...4).contains(identityGroup!) else { throw ModelValidationError.identityGroup }
        guard maximumDepth == nil || (1...24).contains(maximumDepth!) else { throw ModelValidationError.itemMaximumDepth }
        self.key = key; self.item = item; self.upgrade = upgrade; self.modifier = modifier
        self.kind = kind; self.tier = tier; self.tierMatch = tierMatch
        self.upgradeMatch = upgradeMatch; self.source = source
        self.identityGroup = identityGroup
        self.maximumDepth = maximumDepth
    }

    private enum CodingKeys: String, CodingKey {
        case key, item, upgrade, modifier, kind, tier, tierMatch, upgradeMatch, source, identityGroup, maximumDepth
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        try self.init(
            key: values.decode(Int64.self, forKey: .key),
            item: values.decodeIfPresent(CatalogItem.self, forKey: .item),
            upgrade: values.decode(Int.self, forKey: .upgrade),
            modifier: values.decodeIfPresent(String.self, forKey: .modifier),
            kind: values.decode(ItemKind.self, forKey: .kind),
            tier: values.decodeIfPresent(Int.self, forKey: .tier) ?? 0,
            tierMatch: values.decodeIfPresent(TierMatch.self, forKey: .tierMatch) ?? .any,
            upgradeMatch: values.decode(UpgradeMatch.self, forKey: .upgradeMatch),
            source: values.decodeIfPresent(ScoutItemSource.self, forKey: .source),
            identityGroup: values.decodeIfPresent(Int.self, forKey: .identityGroup),
            maximumDepth: values.decodeIfPresent(Int.self, forKey: .maximumDepth)
        )
    }

    public func encode(to encoder: Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        try values.encode(key, forKey: .key); try values.encodeIfPresent(item, forKey: .item)
        try values.encode(upgrade, forKey: .upgrade); try values.encodeIfPresent(modifier, forKey: .modifier)
        try values.encode(kind, forKey: .kind); try values.encode(tier, forKey: .tier)
        try values.encode(tierMatch, forKey: .tierMatch); try values.encode(upgradeMatch, forKey: .upgradeMatch)
        try values.encodeIfPresent(source, forKey: .source)
        try values.encodeIfPresent(identityGroup, forKey: .identityGroup)
        try values.encodeIfPresent(maximumDepth, forKey: .maximumDepth)
    }

    public var title: String {
        if let item { return item.name }
        return switch tierMatch {
        case .any: "Any \(kind.singularLabel)"
        case .exactly: "Any Tier \(tier) \(kind.singularLabel)"
        case .atLeast: "Any Tier \(tier)+ \(kind.singularLabel)"
        }
    }
    public var description: String {
        var text = switch upgradeMatch {
        case .any: "Any upgrade"
        case .exactly: "+\(upgrade) exactly"
        case .atLeast: "+\(upgrade) or higher"
        }
        if let modifier { text += " • \(modifier)" }
        if let source { text += " • \(source.label)" }
        if let identityGroup, let scalar = UnicodeScalar(64 + identityGroup) { text += " • same item group \(Character(scalar))" }
        if let maximumDepth { text += " • by floor \(maximumDepth)" }
        return text
    }
}

public struct SearchRequest: Codable, Sendable {
    public var requirements: [ItemRequirement]
    public var maximumDepth: Int
    public var requireBlacksmith: Bool
    /// Prevents the 2,000-favor Smith choice from satisfying item requirements.
    public var excludeBlacksmithRewards: Bool
    /// Faster but non-exhaustive: +3 weapon/armor requirements only consider
    /// quest rewards, skipping seeds whose sole match is a Crypt or
    /// Sacrificial-fire prize. Found seeds are always genuine matches.
    public var fastMode: Bool
    public var challenges: Int

    public init(requirements: [ItemRequirement], maximumDepth: Int = 24,
                requireBlacksmith: Bool = false, excludeBlacksmithRewards: Bool = false,
                fastMode: Bool = false, challenges: Int = 0) throws {
        guard !requirements.isEmpty else { throw ModelValidationError.emptyRequirements }
        guard (1...24).contains(maximumDepth) else { throw ModelValidationError.maximumDepth }
        guard (0...511).contains(challenges) else { throw ModelValidationError.challenges }
        self.requirements = requirements; self.maximumDepth = maximumDepth
        self.requireBlacksmith = requireBlacksmith
        self.excludeBlacksmithRewards = excludeBlacksmithRewards
        self.fastMode = fastMode
        self.challenges = challenges
    }
}

public struct SeedResult: Hashable, Identifiable, Sendable {
    public let seed: String
    public let matchedRequirements: Int
    public var id: String { seed }
    public init(seed: String, matchedRequirements: Int) { self.seed = seed; self.matchedRequirements = matchedRequirements }
}

public struct ScoutWorld: Sendable {
    public let seed: String
    public let items: [ScoutItem]
    public init(seed: String, items: [ScoutItem]) { self.seed = seed; self.items = items }
}

public struct ScoutItem: Identifiable, Sendable {
    public let item: CatalogItem
    public let depth: Int
    public let upgrade: Int
    public let effect: String?
    public let cursed: Bool
    public let source: ScoutItemSource
    public let accessibility: ScoutAccessibility
    public var id: String { "\(depth):\(item.id):\(upgrade):\(source.rawValue):\(accessibility)" }
}

public enum ScoutAccessibility: Hashable, Sendable {
    case independent
    case choice(group: Int, option: Int)
    case scenarios(group: Int, mask: UInt64)
}

public enum SearchState: Int, Sendable { case running, completed, cancelled, failed }

public struct SearchStatus: Sendable {
    public let state: SearchState
    public let scannedSeeds: Int64
    public let totalSeeds: Int64
    public let errorCode: Int64
    public let matchProbability: Double
}
