import Foundation

public enum ItemKind: Int, Codable, CaseIterable, Sendable {
    // The raw value doubles as the SSF7 wire kind ID: 0...3 are the original
    // families and 4/5 narrow a weapon requirement to one weapon class, so
    // saved queries and packets from older builds keep their meaning.
    case weapon, armor, wand, ring, meleeWeapon, thrownWeapon

    public var label: String { ["Weapons", "Armor", "Wands", "Rings", "Melee weapons", "Thrown weapons"][rawValue] }
    public var singularLabel: String { ["weapon", "armor", "wand", "ring", "melee weapon", "thrown weapon"][rawValue] }
    public var modifierLabel: String? { family == .weapon ? "Enchantment" : family == .armor ? "Glyph" : nil }
    public var maximumSearchUpgrade: Int { self == .ring ? 4 : 3 }

    /// The broad item family; catalog items always carry the family.
    public var family: ItemKind { self == .meleeWeapon || self == .thrownWeapon ? .weapon : self }
    /// The weapon class this kind restricts to, or nil when unrestricted.
    public var weaponClass: WeaponClass? { self == .meleeWeapon ? .melee : self == .thrownWeapon ? .thrown : nil }
    /// Whether a catalog item can satisfy a requirement of this kind.
    public func accepts(_ item: CatalogItem) -> Bool {
        item.kind == family && (weaponClass == nil || ItemCatalog.weaponClass(of: item.id) == weaponClass)
    }
}

/// Melee/thrown classification of weapon catalog entries.
public enum WeaponClass: Sendable, Equatable {
    case melee, thrown
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
    case any, exactly, atLeast, atMost
    public var label: String { ["Any tier", "Exactly", "At least", "At most"][rawValue] }
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
    case itemKind, tier, upgrade, modifier, uncursedCurse, identityGroup, itemMaximumDepth, emptyRequirements, maximumDepth, challenges
    case alternativeGroup, upgradeSum, upgradeSumInsideAlternative
    public var errorDescription: String? {
        switch self {
        case .itemKind: "Selected item must belong to its category"
        case .tier: "Tier predicate requires a wildcard weapon or armor and a non-redundant tier"
        case .upgrade: "Upgrade predicate is invalid"
        case .modifier: "Effect requirement does not fit this category"
        case .uncursedCurse: "An uncursed item cannot be limited to curses"
        case .identityGroup: "Same-item group must be A..D"
        case .itemMaximumDepth: "Item floor limit must be 1..24"
        case .emptyRequirements: "At least one requirement is needed"
        case .maximumDepth: "Maximum floor must be 1..24"
        case .challenges: "Challenge mask must be 0..511"
        case .alternativeGroup: "Alternative group must be 1..255"
        case .upgradeSum: "Combined upgrade needs a group 1..255 and a total 1..8"
        case .upgradeSumInsideAlternative: "An alternative requirement cannot join a combined upgrade group"
        }
    }
}

/// Effect predicate for one requirement: a wildcard (any effect or none), any
/// non-curse enchantment/glyph of the item's family, or one of a specific set
/// of same-family effects.
public enum EffectPredicate: Hashable, Sendable {
    case any
    case anyEnchantment
    case oneOf([String])

    /// Whether a scouted item's effect satisfies this predicate for `kind`.
    public func matches(_ effect: String?, kind: ItemKind) -> Bool {
        switch self {
        case .any: true
        case .anyEnchantment:
            effect.map { !ItemCatalog.cursesFor(kind).contains($0) } ?? false
        case .oneOf(let names):
            effect.map { names.contains($0) } ?? false
        }
    }
}

public struct ItemRequirement: Codable, Hashable, Identifiable, Sendable {
    public var key: Int64
    public var item: CatalogItem?
    public var upgrade: Int
    public var effect: EffectPredicate
    public var kind: ItemKind
    public var tier: Int
    public var tierMatch: TierMatch
    public var upgradeMatch: UpgradeMatch
    public var source: ScoutItemSource?
    public var identityGroup: Int?
    public var maximumDepth: Int?
    public var requireUncursed: Bool
    /// Requirements sharing a non-zero group are alternatives: any single
    /// matching item satisfies the whole group.
    public var alternativeGroup: Int?
    /// Requirements sharing a non-zero sum group must be matched by distinct
    /// items whose upgrade levels total at least `upgradeSumTotal`.
    public var upgradeSumGroup: Int?
    public var upgradeSumTotal: Int?
    public var id: Int64 { key }

    public init(key: Int64, item: CatalogItem?, upgrade: Int, effect: EffectPredicate = .any,
                kind: ItemKind, tier: Int = 0, tierMatch: TierMatch = .any,
                upgradeMatch: UpgradeMatch = .exactly,
                source: ScoutItemSource? = nil, identityGroup: Int? = nil,
                maximumDepth: Int? = nil, requireUncursed: Bool = false,
                alternativeGroup: Int? = nil,
                upgradeSumGroup: Int? = nil, upgradeSumTotal: Int? = nil) throws {
        guard item == nil || item.map(kind.accepts) == true else { throw ModelValidationError.itemKind }
        let tierable = item == nil && (kind.family == .weapon || kind.family == .armor)
        let validTier = switch tierMatch {
        case .any: tier == 0
        case .exactly: tierable && (2...5).contains(tier)
        case .atLeast, .atMost: tierable && (3...4).contains(tier)
        }
        guard validTier else { throw ModelValidationError.tier }
        let valid = switch upgradeMatch {
        case .any: upgrade == 0
        case .exactly: (1...kind.maximumSearchUpgrade).contains(upgrade)
        case .atLeast: (0...kind.maximumSearchUpgrade).contains(upgrade)
        }
        guard valid else { throw ModelValidationError.upgrade }
        switch effect {
        case .any:
            break
        case .anyEnchantment:
            guard kind.modifierLabel != nil else { throw ModelValidationError.modifier }
        case .oneOf(let names):
            guard kind.modifierLabel != nil, !names.isEmpty, names.count <= 32,
                  names.allSatisfy({ ItemCatalog.modifiersFor(kind).contains($0) }) else {
                throw ModelValidationError.modifier
            }
            guard !requireUncursed || !names.allSatisfy({ ItemCatalog.cursesFor(kind).contains($0) }) else {
                throw ModelValidationError.uncursedCurse
            }
        }
        guard identityGroup == nil || (1...4).contains(identityGroup!) else { throw ModelValidationError.identityGroup }
        guard maximumDepth == nil || (1...24).contains(maximumDepth!) else { throw ModelValidationError.itemMaximumDepth }
        guard alternativeGroup == nil || (1...255).contains(alternativeGroup!) else {
            throw ModelValidationError.alternativeGroup
        }
        switch (upgradeSumGroup, upgradeSumTotal) {
        case (nil, nil):
            break
        case (let group?, let total?):
            guard (1...255).contains(group), (1...8).contains(total) else {
                throw ModelValidationError.upgradeSum
            }
            guard alternativeGroup == nil else { throw ModelValidationError.upgradeSumInsideAlternative }
        default:
            throw ModelValidationError.upgradeSum
        }
        self.key = key; self.item = item; self.upgrade = upgrade; self.effect = effect
        self.kind = kind; self.tier = tier; self.tierMatch = tierMatch
        self.upgradeMatch = upgradeMatch; self.source = source
        self.identityGroup = identityGroup
        self.maximumDepth = maximumDepth
        self.requireUncursed = requireUncursed
        self.alternativeGroup = alternativeGroup
        self.upgradeSumGroup = upgradeSumGroup
        self.upgradeSumTotal = upgradeSumTotal
    }

    private enum CodingKeys: String, CodingKey {
        case key, item, upgrade, modifier, effectMode, effectNames, kind, tier, tierMatch, upgradeMatch
        case source, identityGroup, maximumDepth, requireUncursed
        case alternativeGroup, upgradeSumGroup, upgradeSumTotal
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        // New saves carry an effect mode; legacy saves at most a single
        // modifier name, which loads as a one-element specific set.
        let effect: EffectPredicate
        switch try values.decodeIfPresent(Int.self, forKey: .effectMode) {
        case 0: effect = .any
        case 1: effect = .anyEnchantment
        case 2: effect = .oneOf(try values.decodeIfPresent([String].self, forKey: .effectNames) ?? [])
        case .some:
            throw DecodingError.dataCorruptedError(forKey: .effectMode, in: values,
                                                   debugDescription: "Unknown effect mode")
        case nil:
            effect = (try values.decodeIfPresent(String.self, forKey: .modifier)).map { .oneOf([$0]) } ?? .any
        }
        try self.init(
            key: values.decode(Int64.self, forKey: .key),
            item: values.decodeIfPresent(CatalogItem.self, forKey: .item),
            upgrade: values.decode(Int.self, forKey: .upgrade),
            effect: effect,
            kind: values.decode(ItemKind.self, forKey: .kind),
            tier: values.decodeIfPresent(Int.self, forKey: .tier) ?? 0,
            tierMatch: values.decodeIfPresent(TierMatch.self, forKey: .tierMatch) ?? .any,
            upgradeMatch: values.decode(UpgradeMatch.self, forKey: .upgradeMatch),
            source: values.decodeIfPresent(ScoutItemSource.self, forKey: .source),
            identityGroup: values.decodeIfPresent(Int.self, forKey: .identityGroup),
            maximumDepth: values.decodeIfPresent(Int.self, forKey: .maximumDepth),
            requireUncursed: values.decodeIfPresent(Bool.self, forKey: .requireUncursed) ?? false,
            alternativeGroup: values.decodeIfPresent(Int.self, forKey: .alternativeGroup),
            upgradeSumGroup: values.decodeIfPresent(Int.self, forKey: .upgradeSumGroup),
            upgradeSumTotal: values.decodeIfPresent(Int.self, forKey: .upgradeSumTotal)
        )
    }

    public func encode(to encoder: Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        try values.encode(key, forKey: .key); try values.encodeIfPresent(item, forKey: .item)
        try values.encode(upgrade, forKey: .upgrade)
        switch effect {
        case .any:
            try values.encode(0, forKey: .effectMode)
        case .anyEnchantment:
            try values.encode(1, forKey: .effectMode)
        case .oneOf(let names):
            try values.encode(2, forKey: .effectMode)
            try values.encode(names, forKey: .effectNames)
        }
        try values.encode(kind, forKey: .kind); try values.encode(tier, forKey: .tier)
        try values.encode(tierMatch, forKey: .tierMatch); try values.encode(upgradeMatch, forKey: .upgradeMatch)
        try values.encodeIfPresent(source, forKey: .source)
        try values.encodeIfPresent(identityGroup, forKey: .identityGroup)
        try values.encodeIfPresent(maximumDepth, forKey: .maximumDepth)
        try values.encode(requireUncursed, forKey: .requireUncursed)
        try values.encodeIfPresent(alternativeGroup, forKey: .alternativeGroup)
        try values.encodeIfPresent(upgradeSumGroup, forKey: .upgradeSumGroup)
        try values.encodeIfPresent(upgradeSumTotal, forKey: .upgradeSumTotal)
    }

    /// The single wanted effect name, when exactly one is required (used for
    /// the sprite glow preview).
    public var primaryEffectName: String? {
        if case .oneOf(let names) = effect, names.count == 1 { return names[0] }
        return nil
    }

    public var title: String {
        if let item { return item.name }
        return switch tierMatch {
        case .any: "Any \(kind.singularLabel)"
        case .exactly: "Any Tier \(tier) \(kind.singularLabel)"
        case .atLeast: "Any Tier \(tier)+ \(kind.singularLabel)"
        case .atMost: "Any Tier \(tier) or lower \(kind.singularLabel)"
        }
    }

    /// Human summary of the effect predicate, or nil for the wildcard.
    public var effectSummary: String? {
        let family = kind.family == .weapon ? "enchantment" : "glyph"
        switch effect {
        case .any:
            return nil
        case .anyEnchantment:
            return "any \(family)"
        case .oneOf(let names):
            let nonCurse = kind.family == .weapon ? ItemCatalog.enchantments : ItemCatalog.glyphs
            if Set(names) == Set(nonCurse) { return "any \(family)" }
            if names.count == 1 { return names[0] }
            if names.count <= 4 {
                return names.dropLast().joined(separator: ", ") + " or " + names.last!
            }
            return "any of \(names.count) \(family)s"
        }
    }

    public var description: String {
        var text = switch upgradeMatch {
        case .any: "Any upgrade"
        case .exactly: "+\(upgrade) exactly"
        case .atLeast: "+\(upgrade) or higher"
        }
        if let effectSummary { text += " • \(effectSummary)" }
        if requireUncursed { text += " • uncursed" }
        if let source { text += " • \(source.label)" }
        if let identityGroup, let scalar = UnicodeScalar(64 + identityGroup) { text += " • same item group \(Character(scalar))" }
        if let upgradeSumGroup, let upgradeSumTotal {
            let label = UnicodeScalar(64 + upgradeSumGroup).map { String(Character($0)) } ?? "\(upgradeSumGroup)"
            text += " • combined +\(upgradeSumTotal) total (group \(label))"
        }
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

    /// Mirrors the engine's attainability rule for combined-upgrade groups:
    /// the members' reachable upgrades must be able to add up to the total.
    /// The engine would reject such a request with an unspecific error.
    public var unattainableUpgradeSumMessage: String? {
        let groups = Dictionary(grouping: requirements.filter { $0.upgradeSumGroup != nil },
                                by: { $0.upgradeSumGroup ?? 0 })
        for (group, members) in groups.sorted(by: { $0.key < $1.key }) {
            let total = members.compactMap(\.upgradeSumTotal).max() ?? 0
            let reachable = members.reduce(0) { sum, member in
                sum + (member.upgradeMatch == .exactly ? member.upgrade : member.kind.maximumSearchUpgrade)
            }
            if total > reachable {
                let label = UnicodeScalar(64 + group).map { String(Character($0)) } ?? "\(group)"
                return "Combined upgrade group \(label) asks for +\(total) but its items can reach at most +\(reachable) together."
            }
        }
        return nil
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
    public init(item: CatalogItem, depth: Int, upgrade: Int, effect: String? = nil,
                cursed: Bool = false, source: ScoutItemSource,
                accessibility: ScoutAccessibility = .independent) {
        self.item = item; self.depth = depth; self.upgrade = upgrade; self.effect = effect
        self.cursed = cursed; self.source = source; self.accessibility = accessibility
    }
}

public enum ScoutAccessibility: Hashable, Sendable {
    case independent
    case choice(group: Int, option: Int)
    case scenarios(group: Int, mask: UInt64)
}

/// Selects a deterministic, jointly obtainable set of scouted items for the
/// current requirements, mirroring the engine's `best_match_indices`.
///
/// Alternative groups collapse into one slot satisfied by any single member's
/// match; every other requirement is its own slot, and every slot must be
/// served by a distinct item. Choice and scenario groups are constrained to a
/// compatible outcome, identity groups to one item type, and combined-upgrade
/// groups count all-or-nothing: members of an incomplete or short group are
/// not highlighted.
public func scoutMatchIndices(items: [ScoutItem], requirements: [ItemRequirement],
                              maximumDepth: Int = 24,
                              excludeBlacksmithRewards: Bool = false) -> Set<Int> {
    /// Highest upgrade any generated item can carry (+4 rings from the Imp).
    let maximumItemUpgrade = 4
    func matches(_ item: ScoutItem, _ requirement: ItemRequirement) -> Bool {
        guard item.depth <= maximumDepth,
              item.depth <= (requirement.maximumDepth ?? maximumDepth),
              !excludeBlacksmithRewards || item.source != .blacksmithReward,
              requirement.kind.accepts(item.item),
              requirement.item == nil || requirement.item?.id == item.item.id,
              requirement.effect.matches(item.effect, kind: requirement.kind),
              !requirement.requireUncursed || !item.cursed,
              requirement.source == nil || requirement.source == item.source else { return false }
        let tierMatches = switch requirement.tierMatch {
        case .any: true
        case .exactly: item.item.tier == requirement.tier
        case .atLeast: item.item.tier.map { $0 >= requirement.tier } ?? false
        case .atMost: item.item.tier.map { $0 <= requirement.tier } ?? false
        }
        let upgradeMatches = switch requirement.upgradeMatch {
        case .any: true
        case .exactly: item.upgrade == requirement.upgrade
        case .atLeast: item.upgrade >= requirement.upgrade
        }
        return tierMatches && upgradeMatches
    }

    // Build slots: an alternative group is one slot whose candidates are the
    // union of its members' matches; each candidate remembers its member.
    var slotOfGroup: [Int: Int] = [:]
    var slots: [[(index: Int, requirement: ItemRequirement)]] = []
    var sumGroups: [Int: (members: Int, minimumTotal: Int)] = [:]
    for requirement in requirements {
        let slot: Int
        if let group = requirement.alternativeGroup {
            if let existing = slotOfGroup[group] {
                slot = existing
            } else {
                slots.append([]); slot = slots.count - 1; slotOfGroup[group] = slot
            }
        } else {
            slots.append([]); slot = slots.count - 1
        }
        if let group = requirement.upgradeSumGroup, let total = requirement.upgradeSumTotal {
            var entry = sumGroups[group] ?? (members: 0, minimumTotal: total)
            entry.members += 1
            sumGroups[group] = entry
        }
        for index in items.indices where matches(items[index], requirement) {
            slots[slot].append((index, requirement))
        }
    }
    // Fail early by assigning the most constrained slot first.
    slots.sort { $0.count < $1.count }

    var used = Set<Int>()
    var selected: [(index: Int, sumGroup: Int?)] = []
    var best = Set<Int>()
    var scenarios: [Int: UInt64] = [:]
    var identities: [Int: String] = [:]
    /// Assigned member count and accumulated upgrade total per sum group.
    var sums: [Int: (assigned: Int, total: Int)] = [:]

    /// Items serving an incomplete or short combined-upgrade group do not
    /// count and are not highlighted.
    func countedSelection() -> Set<Int> {
        let failed = Set(sums.compactMap { group, state -> Int? in
            let wanted = sumGroups[group] ?? (members: 0, minimumTotal: 0)
            return state.assigned < wanted.members || state.total < wanted.minimumTotal ? group : nil
        })
        return Set(selected.compactMap { entry in
            entry.sumGroup.map { failed.contains($0) } == true ? nil : entry.index
        })
    }

    func visit(_ position: Int) {
        guard position < slots.count else {
            let counted = countedSelection()
            if counted.count > best.count { best = counted }
            return
        }
        // The remaining slots bound what this branch can still add.
        if selected.count + slots.count - position <= best.count { return }
        for (index, requirement) in slots[position] where !used.contains(index) {
            let item = items[index]
            var oldIdentity: String?
            if let group = requirement.identityGroup {
                if let identity = identities[group], identity != item.item.id { continue }
                oldIdentity = identities.updateValue(item.item.id, forKey: group)
            }
            func rewindIdentity() {
                if let identityGroup = requirement.identityGroup {
                    if let oldIdentity { identities[identityGroup] = oldIdentity }
                    else { identities.removeValue(forKey: identityGroup) }
                }
            }
            let constraint: (Int, UInt64)? = switch item.accessibility {
            case .independent: nil
            case .choice(let group, let option): (group, UInt64(1) << UInt64(option))
            case .scenarios(let group, let mask): (group, mask)
            }
            var oldScenarios: UInt64?
            if let (group, mask) = constraint {
                let compatible = (scenarios[group] ?? UInt64.max) & mask
                if compatible == 0 { rewindIdentity(); continue }
                oldScenarios = scenarios.updateValue(compatible, forKey: group)
            }
            func rewindScenario() {
                if let (group, _) = constraint {
                    if let oldScenarios { scenarios[group] = oldScenarios }
                    else { scenarios.removeValue(forKey: group) }
                }
            }
            var oldSum: (assigned: Int, total: Int)?
            if let group = requirement.upgradeSumGroup {
                let wanted = sumGroups[group] ?? (members: 0, minimumTotal: 0)
                let state = sums[group] ?? (assigned: 0, total: 0)
                let assigned = state.assigned + 1
                let total = state.total + item.upgrade
                let remaining = max(0, wanted.members - assigned)
                // Prune once the total is out of reach even with maximal
                // upgrades on every remaining member.
                if total + remaining * maximumItemUpgrade < wanted.minimumTotal {
                    rewindScenario(); rewindIdentity(); continue
                }
                oldSum = sums.updateValue((assigned, total), forKey: group)
            }
            used.insert(index); selected.append((index, requirement.upgradeSumGroup))
            visit(position + 1)
            selected.removeLast(); used.remove(index)
            if let group = requirement.upgradeSumGroup {
                if let oldSum { sums[group] = oldSum }
                else { sums.removeValue(forKey: group) }
            }
            rewindScenario()
            rewindIdentity()
        }
        visit(position + 1)
    }
    visit(0)
    return best
}

public enum SearchState: Int, Sendable { case running, completed, cancelled, failed }

public struct SearchStatus: Sendable {
    public let state: SearchState
    public let scannedSeeds: Int64
    public let totalSeeds: Int64
    public let errorCode: Int64
    public let matchProbability: Double
}
