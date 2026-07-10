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

public enum ModelValidationError: Error, Equatable, LocalizedError {
    case itemKind, upgrade, modifier, identityGroup, emptyRequirements, maximumDepth
    public var errorDescription: String? {
        switch self {
        case .itemKind: "Selected item must belong to its category"
        case .upgrade: "Upgrade predicate is invalid"
        case .modifier: "This category cannot carry a modifier requirement"
        case .identityGroup: "Same-item group must be A..D"
        case .emptyRequirements: "At least one requirement is needed"
        case .maximumDepth: "Maximum floor must be 1..24"
        }
    }
}

public struct ItemRequirement: Codable, Hashable, Identifiable, Sendable {
    public var key: Int64
    public var item: CatalogItem?
    public var upgrade: Int
    public var modifier: String?
    public var kind: ItemKind
    public var upgradeMatch: UpgradeMatch
    public var source: ScoutItemSource?
    public var identityGroup: Int?
    public var id: Int64 { key }

    public init(key: Int64, item: CatalogItem?, upgrade: Int, modifier: String? = nil,
                kind: ItemKind, upgradeMatch: UpgradeMatch = .exactly,
                source: ScoutItemSource? = nil, identityGroup: Int? = nil) throws {
        guard item == nil || item?.kind == kind else { throw ModelValidationError.itemKind }
        let valid = switch upgradeMatch {
        case .any: upgrade == 0
        case .exactly: (1...kind.maximumSearchUpgrade).contains(upgrade)
        case .atLeast: (0...kind.maximumSearchUpgrade).contains(upgrade)
        }
        guard valid else { throw ModelValidationError.upgrade }
        guard kind.modifierLabel != nil || modifier == nil else { throw ModelValidationError.modifier }
        guard identityGroup == nil || (1...4).contains(identityGroup!) else { throw ModelValidationError.identityGroup }
        self.key = key; self.item = item; self.upgrade = upgrade; self.modifier = modifier
        self.kind = kind; self.upgradeMatch = upgradeMatch; self.source = source
        self.identityGroup = identityGroup
    }

    public var title: String { item?.name ?? "Any \(kind.singularLabel)" }
    public var description: String {
        var text = switch upgradeMatch {
        case .any: "Any upgrade"
        case .exactly: "+\(upgrade) exactly"
        case .atLeast: "+\(upgrade) or higher"
        }
        if let modifier { text += " • \(modifier)" }
        if let source { text += " • \(source.label)" }
        if let identityGroup, let scalar = UnicodeScalar(64 + identityGroup) { text += " • same item group \(Character(scalar))" }
        return text
    }
}

public struct SearchRequest: Codable, Sendable {
    public var requirements: [ItemRequirement]
    public var maximumDepth: Int
    public var requireBlacksmith: Bool
    /// Faster but non-exhaustive: +3 weapon/armor requirements only consider
    /// quest rewards, skipping seeds whose sole match is a Crypt or
    /// Sacrificial-fire prize. Found seeds are always genuine matches.
    public var fastMode: Bool

    public init(requirements: [ItemRequirement], maximumDepth: Int = 24,
                requireBlacksmith: Bool = false, fastMode: Bool = false) throws {
        guard !requirements.isEmpty else { throw ModelValidationError.emptyRequirements }
        guard (1...24).contains(maximumDepth) else { throw ModelValidationError.maximumDepth }
        self.requirements = requirements; self.maximumDepth = maximumDepth
        self.requireBlacksmith = requireBlacksmith
        self.fastMode = fastMode
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
}
