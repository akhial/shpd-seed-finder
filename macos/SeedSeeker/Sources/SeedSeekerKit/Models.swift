import Foundation

/// Local copies of the engine's query bounds
/// (`crates/seedfinder-core/src/engine_info.rs`). They stay constants so the
/// models need nothing from the engine to validate; `EngineConstantsTests`
/// asserts each of them against the engine's `seedfinder_engine_info` document.
public enum SearchLimits {
    /// Deepest floor a search may cover.
    public static let maxDepth = 24
    /// Tiers an "exactly tier N" requirement may name (tier 1 is starting gear).
    public static let exactTiers = 2...5
    /// Tiers an "at least / at most tier N" requirement may name.
    public static let boundedTiers = 3...4
    /// Highest same-item group number (groups run 1...this, shown as A..D).
    public static let identityGroupMax = 4
    /// Highest combined-level group number (groups run 1...this, shown as A..D).
    public static let levelSumGroupMax = 4
    /// The most items one board chip may ask for, its anchor included. Unlike
    /// its neighbours this is the board's own bound, not the engine's — the
    /// engine takes any number of copies; three is what a chip badge can say
    /// without turning into a list.
    public static let stackMax = 3
    /// Highest upgrade a search may name for armor, wands and rings.
    public static let maxUpgradeDefault = 4
    /// Highest upgrade a ring requirement may name.
    public static let maxUpgradeRing = 4
    /// Highest upgrade every ring but one can carry in a single world: ring
    /// drops roll +0...+2, and the only source beyond that — the Imp vault's
    /// final-room prize — appears once per run.
    public static let maxUpgradeRingStandard = 2
    /// Highest upgrade a weapon requirement may name: v4.0.0's Imp vault
    /// reaches +5 on a tier-4 weapon or thrown weapon.
    public static let maxUpgradeWeapon = 5
    /// Upgrade transferred to an artifact in the Imp vault.
    public static let maxUpgradeArtifact = 5
    /// Highest upgrade the generator puts on any item, whatever its tier.
    public static let maxUpgradeAnyTier = 4
    /// The one weapon tier levelled past `maxUpgradeAnyTier`, a
    /// v4.0.0-BETA-3 quirk: the Imp's vault lays out one tier-4 and one
    /// tier-5 weapon and rolls the tier-4 one at +3...+5 while the tier-5 one
    /// stops at +4, so a +5 exists only on a tier-4 weapon, melee or thrown.
    /// When upstream levels the two ranges this goes away and every family
    /// caps at `maxUpgradeAnyTier`.
    public static let extraUpgradeTier = 4

    /// The highest upgrade a requirement may name once its item and tier
    /// filter are known. Only weapons have a ceiling that depends on tier.
    public static func maximumUpgrade(kind: ItemKind, item: CatalogItem?, tier: Int, tierMatch: TierMatch) -> Int {
        let ceiling = kind.maximumSearchUpgrade
        guard kind.family == .weapon, ceiling > maxUpgradeAnyTier else { return ceiling }
        let reachesExtraTier = if let item {
            item.tier == extraUpgradeTier
        } else {
            switch tierMatch {
            case .any: true
            case .exactly: tier == extraUpgradeTier
            case .atLeast: tier <= extraUpgradeTier
            case .atMost: tier >= extraUpgradeTier
            }
        }
        return reachesExtraTier ? ceiling : maxUpgradeAnyTier
    }

    /// The highest combined level `count` rings can reach together: one ring
    /// at the vault ceiling, every other at the standard roll, each counting
    /// its upgrade plus one.
    public static func ringStackCapacity(_ count: Int) -> Int {
        maxUpgradeRing + 1 + (count - 1) * (maxUpgradeRingStandard + 1)
    }
    /// Every challenge bit together: the largest legal challenge mask.
    public static let challengeMask = 511
}

public enum ItemKind: Int, Codable, CaseIterable, Sendable {
    // The raw value is the saved-query kind ID: 0...3 are the original
    // families and 4/5 narrow a weapon requirement to one weapon class, so
    // saved queries from older builds keep their meaning.
    case weapon, armor, wand, ring, meleeWeapon, thrownWeapon, trinket, artifact

    public var label: String { ["Weapons", "Armor", "Wands", "Rings", "Melee weapons", "Thrown weapons", "Trinket", "Artifacts"][rawValue] }
    public var singularLabel: String { ["weapon", "armor", "wand", "ring", "melee weapon", "thrown weapon", "trinket", "artifact"][rawValue] }
    public var modifierLabel: String? { family == .weapon ? "Enchantment" : family == .armor ? "Glyph" : nil }
    /// The non-curse effects of this family — enchantments or glyphs — in the
    /// shared catalog asset's order.
    public var enchantmentNames: [String] { ItemCatalog.enchantmentsFor(self) }
    /// The highest upgrade a search may name for this family.
    public var maximumSearchUpgrade: Int {
        family == .artifact ? SearchLimits.maxUpgradeArtifact : family == .trinket ? 0 : family == .weapon ? SearchLimits.maxUpgradeWeapon
            : family == .ring ? SearchLimits.maxUpgradeRing : SearchLimits.maxUpgradeDefault
    }

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
    /// The item's own `items.png` cell — for a ring, the cell of its *class*,
    /// which is its identity rather than the cell any particular run draws it
    /// in. Use ``spriteIndex(in:)`` wherever a run's ring gems are known.
    public let spriteIndex: Int
    public let tier: Int?
    /// A ring's `item_icons.png` glyph cell — the 0…11 class index that tells
    /// one ring from another whatever gem the run gave it. Nil for everything
    /// that is not a ring, which carries no glyph. Mirrors the Android
    /// client's `CatalogItem.typeIconIndex` and the asset's `typeIcon`.
    public let typeIconIndex: Int?

    public init(id: String, name: String, kind: ItemKind, spriteIndex: Int, tier: Int? = nil,
                typeIconIndex: Int? = nil) {
        self.id = id; self.name = name; self.kind = kind; self.spriteIndex = spriteIndex
        self.tier = tier; self.typeIconIndex = typeIconIndex
    }

    /// Whether this is a tipped dart. Every shop stocks tipped darts and any
    /// dart can be tipped by hand, so the item picker never offers them —
    /// though a scouted world still lists the ones it rolled. The engine's
    /// catalog keeps the `_dart` suffix unambiguous (the plain dart has no
    /// entry), and its wasm cross-check test pins the suffix to the tipped set.
    public var isTippedDart: Bool { id.hasSuffix("_dart") }

    private enum CodingKeys: String, CodingKey {
        case id, name, kind, spriteIndex, tier, typeIconIndex
    }

    /// Saved queries written before the glyph was carried hold every other
    /// field, so an absent one is that older file rather than catalog drift.
    /// It is taken from the entry the id names, leaving `SavedQuery.validated()`
    /// — which drops a query whose item no longer equals its catalog entry — to
    /// compare only what a saved query really pins.
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
        kind = try container.decode(ItemKind.self, forKey: .kind)
        spriteIndex = try container.decode(Int.self, forKey: .spriteIndex)
        tier = try container.decodeIfPresent(Int.self, forKey: .tier)
        typeIconIndex = try container.decodeIfPresent(Int.self, forKey: .typeIconIndex)
            ?? ItemCatalog.findById(id)?.typeIconIndex
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
    case wandmakerReward, blacksmithReward, impReward, vaultTreasure

    public var label: String {
        ["Heap", "Chest", "Locked chest", "Crystal chest", "Tomb", "Skeleton",
         "Sacrificial fire", "Mimic", "Golden mimic", "Crystal mimic", "Statue",
         "Armored statue", "Shop", "Ghost reward", "Wandmaker reward",
         "Blacksmith reward", "Imp reward", "Vault treasure"][rawValue]
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

/// Floor-limit helpers shared by every floor selector.
///
/// Boss floors 5, 10 and 15 generate no searchable items: the engine treats
/// a floor limit of 5/10/15 exactly like 4/9/14, so selectors skip them.
/// Floor 20 stays selectable because the Imp shop gives the City boss floor
/// searchable stock.
public enum FloorLimits {
    public static let emptyBossFloors: Set<Int> = [5, 10, 15]

    /// Floors offered by floor-limit selectors: 1...maxDepth minus the empty boss floors.
    public static let options: [Int] = (1...SearchLimits.maxDepth).filter { !emptyBossFloors.contains($0) }

    /// Snaps an empty boss-floor limit to the equivalent floor below it (5→4, 10→9, 15→14).
    public static func normalize(_ depth: Int) -> Int {
        emptyBossFloors.contains(depth) ? depth - 1 : depth
    }

    /// The selector index of a floor limit within `options`; off-list values
    /// snap to the nearest option below (or the first option).
    public static func index(of depth: Int) -> Int {
        let floor = normalize(depth)
        if let exact = options.firstIndex(of: floor) { return exact }
        return options.lastIndex(where: { $0 <= floor }) ?? 0
    }
}

/// Shown as A..D: the letter of a 1-based group number.
public func groupLetter(_ group: Int) -> String {
    UnicodeScalar(64 + group).map { String(Character($0)) } ?? "\(group)"
}

public enum ModelValidationError: Error, Equatable, LocalizedError {
    case itemKind, tier, upgrade, modifier, effect, uncursedCurse, identityGroup, itemMaximumDepth
    case identityGroupMixedKinds(group: Int)
    case identityGroupOverconstrained(group: Int)
    case levelSum, levelSumOutsideRings, levelSumInAlternative
    case levelSumMismatch(group: Int)
    case levelSumUnattainable(group: Int, needed: Int, maximum: Int)
    case emptyRequirements, maximumDepth, challenges
    public var errorDescription: String? {
        switch self {
        case .itemKind: "Selected item must belong to its category"
        case .tier: "Tier predicate requires a wildcard weapon or armor and a non-redundant tier"
        case .upgrade: "Upgrade predicate is invalid"
        case .modifier: "This category cannot carry an effect requirement"
        case .effect: "Effect requirement names an unknown effect"
        case .uncursedCurse: "An uncursed item cannot have a curse"
        case .identityGroup: "Same-item group must be A..D"
        case .identityGroupMixedKinds(let group):
            "Same-item group \(groupLetter(group)) mixes different categories"
        case .identityGroupOverconstrained(let group):
            "Same-item group \(groupLetter(group)) can describe one item (or one set of alternatives); its other members must be plain"
        case .itemMaximumDepth: "Item floor limit must be 1..\(SearchLimits.maxDepth)"
        case .levelSum: "Combined level group must be A..D with a total of at least 1"
        case .levelSumOutsideRings: "Only rings can count levels together"
        case .levelSumInAlternative: "An alternative cannot be part of a combined level group"
        case .levelSumMismatch(let group):
            "Combined level group \(groupLetter(group)) must share one total across its items"
        case .levelSumUnattainable(let group, let needed, let maximum):
            "Combined level group \(groupLetter(group)) needs \(needed) levels but its items can reach at most \(maximum)"
        case .emptyRequirements: "At least one requirement is needed"
        case .maximumDepth: "Maximum floor must be 1..\(SearchLimits.maxDepth)"
        case .challenges: "Challenge mask must be 0..\(SearchLimits.challengeMask)"
        }
    }
}

/// Which enchantment, glyph or curse a requirement demands.
///
/// `oneOf` holds wire names in the catalog asset's order (enchantments, then
/// curses, each alphabetical); the full non-curse
/// family set is always `anyEnchantment`, so equal predicates compare equal.
public enum EffectFilter: Hashable, Sendable {
    /// Any effect, or none at all.
    case any
    /// Some enchantment or glyph — anything but a curse or a plain item.
    case anyEnchantment
    /// One of these effects (a single name is the classic "with Blazing").
    case oneOf([String])

    public var isAny: Bool { self == .any }
    /// The names this filter lists explicitly.
    public var names: [String] {
        if case .oneOf(let names) = self { return names }
        return []
    }
    /// The one effect a single-effect filter names (what older builds saved
    /// as `modifier`).
    public var singleName: String? { names.count == 1 ? names[0] : nil }
    /// The effect whose glow the sprite pulses with: the single effect, or the
    /// first of a set. "Any enchantment" has no one colour.
    public var glowName: String? { names.first }

    /// Ordered into the catalog asset's order, deduplicated, and collapsed to
    /// `anyEnchantment` when the set is the family's whole non-curse list.
    /// Returns nil when a name is not an effect of `kind`.
    func normalized(for kind: ItemKind) -> EffectFilter? {
        guard case .oneOf(let raw) = self else { return self }
        let known = ItemCatalog.modifiersFor(kind)
        let names = known.filter { raw.contains($0) }
        guard !names.isEmpty, Set(raw).count == names.count else { return nil }
        return names == kind.enchantmentNames ? .anyEnchantment : .oneOf(names)
    }

    /// Whether every listed effect is a curse (an "uncursed" item could never match).
    func isCursesOnly(for kind: ItemKind) -> Bool {
        let curses = ItemCatalog.cursesFor(kind)
        return !names.isEmpty && names.allSatisfy(curses.contains)
    }

    /// Human description: "Blazing", "Blocking/Projecting", "any enchantment".
    public func label(for kind: ItemKind) -> String? {
        switch self {
        case .any: nil
        case .anyEnchantment: "any \((kind.modifierLabel ?? "enchantment").lowercased())"
        case .oneOf(let names): names.joined(separator: "/")
        }
    }
}

/// Membership in a combined-level group: the members' *levels* must add up to
/// at least `atLeast`, which every member shares, where a matched item counts
/// its upgrade plus one. Members are optional, so the group reads "up to N
/// items reaching `atLeast` levels" — one +2 ring satisfies a total of 3 on
/// its own, and so does a +0 with a +1.
public struct LevelSum: Codable, Hashable, Sendable {
    public var group: Int
    public var atLeast: Int
    public init(group: Int, atLeast: Int) { self.group = group; self.atLeast = atLeast }
}

public struct ItemRequirement: Codable, Hashable, Identifiable, Sendable {
    public var key: Int64
    public var item: CatalogItem?
    public var upgrade: Int
    public var effect: EffectFilter
    public var kind: ItemKind
    public var tier: Int
    public var tierMatch: TierMatch
    public var upgradeMatch: UpgradeMatch
    public var source: ScoutItemSource?
    public var identityGroup: Int?
    public var maximumDepth: Int?
    public var requireUncursed: Bool
    /// Requirements sharing a group are alternatives for one slot: any member
    /// satisfies it. The number is session-local; documents renumber.
    public var alternativeGroup: Int?
    /// Membership in a combined-level group (never on an alternative).
    public var levelSum: LevelSum?
    public var id: Int64 { key }

    /// The single effect this requirement names, if it names exactly one.
    public var modifier: String? { effect.singleName }

    public init(key: Int64, item: CatalogItem?, upgrade: Int, modifier: String? = nil,
                effect: EffectFilter = .any,
                kind: ItemKind, tier: Int = 0, tierMatch: TierMatch = .any,
                upgradeMatch: UpgradeMatch = .exactly,
                source: ScoutItemSource? = nil, identityGroup: Int? = nil,
                maximumDepth: Int? = nil, requireUncursed: Bool = false,
                alternativeGroup: Int? = nil, levelSum: LevelSum? = nil) throws {
        guard (kind != .trinket && kind != .artifact) || item != nil else { throw ModelValidationError.itemKind }
        guard item == nil || item.map(kind.accepts) == true else { throw ModelValidationError.itemKind }
        let tierable = item == nil && (kind.family == .weapon || kind.family == .armor)
        let validTier = switch tierMatch {
        case .any: tier == 0
        case .exactly: tierable && SearchLimits.exactTiers.contains(tier)
        case .atLeast, .atMost: tierable && SearchLimits.boundedTiers.contains(tier)
        }
        guard validTier else { throw ModelValidationError.tier }
        let maximumUpgrade = SearchLimits.maximumUpgrade(kind: kind, item: item, tier: tier, tierMatch: tierMatch)
        let valid = switch upgradeMatch {
        case .any: upgrade == 0
        case .exactly: upgrade >= 1 && upgrade <= maximumUpgrade
        case .atLeast: (0...maximumUpgrade).contains(upgrade)
        }
        guard valid else { throw ModelValidationError.upgrade }
        // `modifier` is the classic single-effect spelling; `effect` wins when both are given.
        let requested = effect.isAny ? modifier.map { EffectFilter.oneOf([$0]) } ?? .any : effect
        guard kind.modifierLabel != nil || requested.isAny else { throw ModelValidationError.modifier }
        guard let effect = requested.normalized(for: kind) else { throw ModelValidationError.effect }
        guard !requireUncursed || !effect.isCursesOnly(for: kind) else {
            throw ModelValidationError.uncursedCurse
        }
        guard identityGroup == nil || (1...SearchLimits.identityGroupMax).contains(identityGroup!) else { throw ModelValidationError.identityGroup }
        guard maximumDepth == nil || (1...SearchLimits.maxDepth).contains(maximumDepth!) else { throw ModelValidationError.itemMaximumDepth }
        if let levelSum {
            guard (1...SearchLimits.levelSumGroupMax).contains(levelSum.group), levelSum.atLeast >= 1 else {
                throw ModelValidationError.levelSum
            }
            // Levels only combine meaningfully across rings — a ring's effect
            // scales with its level, so a +0 and a +1 together grant what one
            // +2 does. No other family adds up that way.
            guard kind.family == .ring else { throw ModelValidationError.levelSumOutsideRings }
            guard alternativeGroup == nil else { throw ModelValidationError.levelSumInAlternative }
        }
        self.key = key; self.item = item; self.upgrade = upgrade; self.effect = effect
        self.kind = kind; self.tier = tier; self.tierMatch = tierMatch
        self.upgradeMatch = upgradeMatch; self.source = source
        self.identityGroup = identityGroup
        self.maximumDepth = maximumDepth
        self.requireUncursed = requireUncursed
        self.alternativeGroup = alternativeGroup
        self.levelSum = levelSum
    }

    /// The most upgrade levels an item matching this requirement can contribute
    /// to a combined total: an exact upgrade counts as itself, anything else as
    /// the family's cap.
    public var maximumContributedUpgrade: Int {
        upgradeMatch == .exactly ? upgrade : maximumUpgrade
    }

    /// The highest upgrade this requirement may name, its item and tier
    /// filter included.
    public var maximumUpgrade: Int {
        SearchLimits.maximumUpgrade(kind: kind, item: item, tier: tier, tierMatch: tierMatch)
    }

    /// The most *levels* an item matching this requirement can contribute to
    /// a combined total: its highest upgrade plus one, since every matched
    /// item counts itself.
    public var maximumLevel: Int { maximumContributedUpgrade + 1 }

    /// Whether this constrains nothing beyond its category — the shape a
    /// same-item group's extra copies take. A narrowed weapon kind is a
    /// constraint; a per-item floor limit is a placement bound, not an item
    /// property, and does not count.
    public var isBare: Bool {
        item == nil && kind == kind.family && tierMatch == .any && upgradeMatch == .any
            && effect == .any && !requireUncursed && source == nil
    }

    private enum CodingKeys: String, CodingKey {
        case key, item, upgrade, modifier, effect, kind, tier, tierMatch, upgradeMatch, source
        case identityGroup, maximumDepth, requireUncursed, alternativeGroup, levelSum
    }

    /// How the saved-query JSON spells the effect filter, beside the classic
    /// `modifier` key that a single effect keeps using.
    private static let anyEnchantmentName = "any_enchantment"

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        var effect = EffectFilter.any
        if let names = try? values.decodeIfPresent([String].self, forKey: .effect) {
            effect = .oneOf(names)
        } else if let name = try? values.decodeIfPresent(String.self, forKey: .effect) {
            guard name == Self.anyEnchantmentName else {
                throw DecodingError.dataCorruptedError(forKey: .effect, in: values,
                                                       debugDescription: "unknown effect filter")
            }
            effect = .anyEnchantment
        }
        try self.init(
            key: values.decode(Int64.self, forKey: .key),
            item: values.decodeIfPresent(CatalogItem.self, forKey: .item),
            upgrade: values.decode(Int.self, forKey: .upgrade),
            modifier: values.decodeIfPresent(String.self, forKey: .modifier),
            effect: effect,
            kind: values.decode(ItemKind.self, forKey: .kind),
            tier: values.decodeIfPresent(Int.self, forKey: .tier) ?? 0,
            tierMatch: values.decodeIfPresent(TierMatch.self, forKey: .tierMatch) ?? .any,
            upgradeMatch: values.decode(UpgradeMatch.self, forKey: .upgradeMatch),
            source: values.decodeIfPresent(ScoutItemSource.self, forKey: .source),
            identityGroup: values.decodeIfPresent(Int.self, forKey: .identityGroup),
            // Requirements saved before empty boss floors were removed may hold
            // 5/10/15; snap them to the equivalent limit below.
            maximumDepth: values.decodeIfPresent(Int.self, forKey: .maximumDepth).map(FloorLimits.normalize),
            requireUncursed: values.decodeIfPresent(Bool.self, forKey: .requireUncursed) ?? false,
            alternativeGroup: values.decodeIfPresent(Int.self, forKey: .alternativeGroup),
            levelSum: values.decodeIfPresent(LevelSum.self, forKey: .levelSum)
        )
    }

    public func encode(to encoder: Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        try values.encode(key, forKey: .key); try values.encodeIfPresent(item, forKey: .item)
        try values.encode(upgrade, forKey: .upgrade)
        // A single effect stays under `modifier`, so older builds still read it.
        switch effect {
        case .any: break
        case .anyEnchantment: try values.encode(Self.anyEnchantmentName, forKey: .effect)
        case .oneOf(let names):
            if names.count == 1 { try values.encode(names[0], forKey: .modifier) }
            else { try values.encode(names, forKey: .effect) }
        }
        try values.encode(kind, forKey: .kind); try values.encode(tier, forKey: .tier)
        try values.encode(tierMatch, forKey: .tierMatch); try values.encode(upgradeMatch, forKey: .upgradeMatch)
        try values.encodeIfPresent(source, forKey: .source)
        try values.encodeIfPresent(identityGroup, forKey: .identityGroup)
        try values.encodeIfPresent(maximumDepth, forKey: .maximumDepth)
        try values.encode(requireUncursed, forKey: .requireUncursed)
        try values.encodeIfPresent(alternativeGroup, forKey: .alternativeGroup)
        try values.encodeIfPresent(levelSum, forKey: .levelSum)
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
    public var description: String {
        var text = switch upgradeMatch {
        case .any: "Any upgrade"
        case .exactly: "+\(upgrade) exactly"
        case .atLeast: "+\(upgrade) or higher"
        }
        if let effect = effect.label(for: kind) { text += " • \(effect)" }
        if requireUncursed { text += " • uncursed" }
        if let source { text += " • \(source.label)" }
        // The board says the relationships — a stack through its ×N badge, a
        // combined level through its Σ badge — so the line names only what the
        // chip itself cannot show. The group numbers are an encoding detail and
        // were never anything a reader could act on.
        if let levelSum { text += " • levels ≥ \(levelSum.atLeast) together" }
        if let maximumDepth { text += " • by floor \(maximumDepth)" }
        return text
    }
}

extension Array where Element == ItemRequirement {
    /// The query's slots in order: an alternative group is one slot at the
    /// position of its first member, holding its members in requirement order;
    /// every other requirement is a slot of its own.
    public var slots: [[ItemRequirement]] {
        var slots: [[ItemRequirement]] = []
        var slotOfGroup: [Int: Int] = [:]
        for requirement in self {
            if let group = requirement.alternativeGroup {
                if let index = slotOfGroup[group] {
                    slots[index].append(requirement)
                } else {
                    slotOfGroup[group] = slots.count
                    slots.append([requirement])
                }
            } else {
                slots.append([requirement])
            }
        }
        return slots
    }

    /// How many slots there are — what the app counts as "requirements".
    public var slotCount: Int { slots.count }

    /// Checks the rules spanning requirements, as the engine will: a same-item
    /// group is a stack — one anchor unit (a lone requirement, or the members
    /// of one alternative group) that may constrain the item, plus plain
    /// copies of its category — and every combined-level group agrees on one
    /// total that its members can reach together, counted in levels.
    public func validateGroups() throws {
        let stacks = Dictionary(grouping: self.filter { $0.identityGroup != nil }, by: { $0.identityGroup! })
        for (group, members) in stacks.sorted(by: { $0.key < $1.key }) {
            guard Set(members.map(\.kind.family)).count == 1 else {
                throw ModelValidationError.identityGroupMixedKinds(group: group)
            }
            // Members of one alternative group form a single unit.
            let units = Set(members.filter { !$0.isBare }.map { member in
                member.alternativeGroup.map { "alternative \($0)" } ?? "requirement \(member.key)"
            })
            guard units.count <= 1 else {
                throw ModelValidationError.identityGroupOverconstrained(group: group)
            }
        }
        let sums = Dictionary(grouping: self.compactMap { requirement in
            requirement.levelSum.map { ($0, requirement) }
        }, by: { $0.0.group })
        for (group, members) in sums.sorted(by: { $0.key < $1.key }) {
            let totals = Set(members.map(\.0.atLeast))
            guard totals.count == 1, let needed = totals.first else {
                throw ModelValidationError.levelSumMismatch(group: group)
            }
            // Each member's own ceiling, bounded by what a world generates: it
            // levels at most one ring — the Imp vault's prize — past the
            // standard roll, so N rings never reach N times the family cap.
            let maximum = Swift.min(members.reduce(0) { $0 + $1.1.maximumLevel },
                                    SearchLimits.ringStackCapacity(members.count))
            guard needed <= maximum else {
                throw ModelValidationError.levelSumUnattainable(group: group, needed: needed, maximum: maximum)
            }
        }
    }
}

/// The Wandmaker quest a search can demand, or `nil` for any of them.
///
/// Only this giver's variant is worth filtering on: its quest item — corpse
/// dust, an elemental ember, or a rotberry seed — can be used in the dungeon
/// instead of being handed in. The other three quests only change the fight.
public enum WandmakerQuest: Int, CaseIterable, Codable, Sendable {
    // The raw value doubles as the 1-based variant index.
    case corpseDust = 1, elementalEmbers, rotberry

    public var variant: ScoutQuestVariant { ScoutQuestKind.wandmaker.variants[rawValue - 1] }
    public var label: String { variant.label }
    /// Stable snake_case name used by the shared JSON query document.
    public var documentName: String {
        switch self {
        case .corpseDust: "corpse_dust"
        case .elementalEmbers: "elemental_embers"
        case .rotberry: "rotberry"
        }
    }
    public static func named(_ name: String) -> WandmakerQuest? {
        allCases.first { $0.documentName == name }
    }
}

public struct SearchRequest: Codable, Sendable {
    public var requirements: [ItemRequirement]
    public var maximumDepth: Int
    public var requireBlacksmith: Bool
    /// Prevents the 2,000-favor Smith choice from satisfying item requirements.
    public var excludeBlacksmithRewards: Bool
    /// Which Wandmaker quest the run must roll; `nil` accepts any.
    public var wandmakerQuest: WandmakerQuest?
    public var challenges: Int

    public init(requirements: [ItemRequirement], maximumDepth: Int = SearchLimits.maxDepth,
                requireBlacksmith: Bool = false, excludeBlacksmithRewards: Bool = false,
                wandmakerQuest: WandmakerQuest? = nil,
                challenges: Int = 0) throws {
        guard !requirements.isEmpty else { throw ModelValidationError.emptyRequirements }
        guard (1...SearchLimits.maxDepth).contains(maximumDepth) else { throw ModelValidationError.maximumDepth }
        guard (0...SearchLimits.challengeMask).contains(challenges) else { throw ModelValidationError.challenges }
        try requirements.validateGroups()
        self.requirements = requirements; self.maximumDepth = maximumDepth
        self.requireBlacksmith = requireBlacksmith
        self.excludeBlacksmithRewards = excludeBlacksmithRewards
        self.wandmakerQuest = wandmakerQuest
        self.challenges = challenges
    }
}

extension SearchRequest {
    /// Whether this request refines `base`: an identical floor limit and
    /// challenge set, world conditions (the blacksmith settings and
    /// the Wandmaker filter) at least as strict as `base`'s, plus, for every base
    /// requirement, a distinct requirement of this request at least as strict
    /// — equal, added-to, or strengthened (a named item, a tightened bound).
    ///
    /// Equality qualifies deliberately: restarting an unchanged query must
    /// continue the session — the filter keeps every seed and the scan resumes
    /// where it stopped — rather than throw the results away and rescan.
    ///
    /// The rule itself is the engine's (`SearchQuery::continues`, bridged as
    /// `seedfinder_query_continues`): both queries go over the same canonical
    /// JSON document the search takes, so refine eligibility is decided once
    /// for every platform instead of being re-derived here. Row identity
    /// (`key`) drops out for free — it is not part of the document. A query
    /// that cannot be encoded continues nothing.
    public func isRefinement(of base: SearchRequest) -> Bool {
        guard let candidate = try? QueryDocument.encode(self),
              let encodedBase = try? QueryDocument.encode(base) else { return false }
        return QueryContinuation.continues(candidate, base: encodedBase)
    }
}

/// Where a follow-up search must pick up to complete a stopped session's
/// seed-space coverage: `remaining` seeds starting at numeric seed `position`.
public struct ResumeHint: Sendable {
    public let position: Int64
    public let remaining: Int64
    public init(position: Int64, remaining: Int64) {
        self.position = position; self.remaining = remaining
    }
}

public struct SeedResult: Hashable, Identifiable, Sendable {
    public let seed: String
    public let matchedRequirements: Int
    public var id: String { seed }
    public init(seed: String, matchedRequirements: Int) { self.seed = seed; self.matchedRequirements = matchedRequirements }
}

/// Raw values are the SSC5 feeling IDs and the dungeon icon frame columns.
public enum FloorFeeling: Int, CaseIterable, Sendable {
    case none = 0, chasm, water, grass, dark, large, traps, secrets

    public var label: String {
        switch self {
        case .none: "Normal floor"
        case .chasm: "Chasms floor"
        case .water: "Water floor"
        case .grass: "Grass floor"
        case .dark: "Dark floor"
        case .large: "Large floor"
        case .traps: "Traps floor"
        case .secrets: "Secrets floor"
        }
    }
}

public struct ScoutWorld: Sendable {
    public let seed: String
    public let quests: [ScoutQuest]
    public let items: [ScoutItem]
    /// The gem this run gave each ring class, and so the cell every ring in
    /// ``items`` is drawn in. It follows from the seed alone, so it belongs to
    /// the world beside the manifest; a world assembled without one (a test
    /// fixture, a stub engine) falls back to the catalog's own table.
    public let ringGems: RingGems
    public let trinketOrder: [CatalogItem]
    public let feelings: [Int: FloorFeeling]
    public init(seed: String, quests: [ScoutQuest] = [], items: [ScoutItem],
                ringGems: RingGems = .catalogDefault, trinketOrder: [CatalogItem] = [],
                feelings: [Int: FloorFeeling] = [:]) {
        self.seed = seed; self.quests = quests; self.items = items; self.ringGems = ringGems
        self.trinketOrder = trinketOrder
        self.feelings = feelings
    }
}

/// The quest giver a scouted world rolled on one of its quest floors.
public enum ScoutQuestKind: Int, CaseIterable, Sendable {
    // The raw value doubles as the SSC3 wire quest ID.
    case ghost = 1, wandmaker, blacksmith, imp

    public var giverLabel: String {
        switch self {
        case .ghost: "Sad ghost"
        case .wandmaker: "Wandmaker"
        case .blacksmith: "Blacksmith"
        case .imp: "Imp"
        }
    }
    /// Floors on which this quest can appear.
    public var depthRange: ClosedRange<Int> {
        switch self {
        case .ghost: 2...4
        case .wandmaker: 7...9
        case .blacksmith: 12...14
        case .imp: 17...19
        }
    }
    /// Wire variants in SSC3 order; a quest's variant byte is a 1-based index.
    public var variants: [ScoutQuestVariant] {
        switch self {
        case .ghost: [.fetidRat, .gnollTrickster, .greatCrab]
        case .wandmaker: [.corpseDust, .elementalEmbers, .rotberry]
        case .blacksmith: [.crystal, .gnoll]
        case .imp: [.vault]
        }
    }
}

/// The concrete variant a quest giver rolled.
public enum ScoutQuestVariant: Hashable, Sendable {
    case fetidRat, gnollTrickster, greatCrab
    case corpseDust, elementalEmbers, rotberry
    case crystal, gnoll
    /// v4.0.0 replaced the Imp's monk and golem hunts with the vault heist;
    /// it is his only variant, so the giver and the variant say the same thing.
    case vault

    public var kind: ScoutQuestKind {
        switch self {
        case .fetidRat, .gnollTrickster, .greatCrab: .ghost
        case .corpseDust, .elementalEmbers, .rotberry: .wandmaker
        case .crystal, .gnoll: .blacksmith
        case .vault: .imp
        }
    }
    public var label: String {
        switch self {
        case .fetidRat: "Fetid rat"
        case .gnollTrickster: "Gnoll trickster"
        case .greatCrab: "Great crab"
        case .corpseDust: "Corpse dust"
        case .elementalEmbers: "Elemental embers"
        case .rotberry: "Rotberry"
        case .crystal: "Crystal spire"
        case .gnoll: "Gnoll geomancer"
        case .vault: "Vault"
        }
    }
}

/// One rolled quest in a scouted world: the variant identifies the giver.
public struct ScoutQuest: Hashable, Identifiable, Sendable {
    public let variant: ScoutQuestVariant
    public let depth: Int
    public var kind: ScoutQuestKind { variant.kind }
    public var id: Int { kind.rawValue }
    public init(variant: ScoutQuestVariant, depth: Int) {
        self.variant = variant; self.depth = depth
    }
}

public struct ScoutItem: Identifiable, Sendable {
    /// Mirrors transferUpgrade followed by visiblyUpgraded, with positive half-up rounding.
    public var displayedUpgrade: Int {
        let cap: Int
        switch item.id {
        case "sandals_of_nature": cap = 3
        case "ethereal_chains", "timekeepers_hourglass": cap = 5
        default: return upgrade
        }
        let internalLevel = (upgrade * cap + 5) / 10
        return (internalLevel * 10 + cap / 2) / cap
    }
    public let item: CatalogItem
    public let depth: Int
    public let upgrade: Int
    public let effect: String?
    public let cursed: Bool
    public let secret: Bool
    public let source: ScoutItemSource
    public let accessibility: ScoutAccessibility
    public var id: String { "\(depth):\(item.id):\(upgrade):\(source.rawValue):\(accessibility)" }
    public init(item: CatalogItem, depth: Int, upgrade: Int, effect: String? = nil,
                cursed: Bool = false, source: ScoutItemSource,
                accessibility: ScoutAccessibility = .independent, secret: Bool = false) {
        self.item = item; self.depth = depth; self.upgrade = upgrade; self.effect = effect
        self.cursed = cursed; self.secret = secret
        self.source = source; self.accessibility = accessibility
    }
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
