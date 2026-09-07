// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog

/**
 * Local copies of the engine's query bounds
 * (`crates/seedfinder-core/src/engine_info.rs`). They stay constants so the
 * models need nothing from the native library to validate; EngineConstantsTest
 * asserts each of them against the engine's `engineInfo` document.
 */
object SearchLimits {
    /** Deepest floor a search may cover. */
    const val MAX_DEPTH = 24

    /** Tiers an "exactly tier N" requirement may name (tier 1 is starting gear). */
    val EXACT_TIERS: IntRange = 2..5

    /** Tiers an "at least / at most tier N" requirement may name. */
    val BOUNDED_TIERS: IntRange = 3..4

    /** Highest stack (same-item) group number; groups run 1..this. */
    const val IDENTITY_GROUP_MAX = 4

    /** Highest combined-level group number; groups run 1..this. */
    const val LEVEL_SUM_GROUP_MAX = 4

    /** The most items a stack may ask for, its anchor included. */
    const val STACK_MAX = 3

    /** Highest upgrade a search may name, for everything but weapons. */
    const val MAX_UPGRADE_DEFAULT = 4

    /** Highest upgrade a ring requirement may name. */
    const val MAX_UPGRADE_RING = 4

    /**
     * Highest upgrade every ring but one can carry in a single world: ring
     * drops roll +0..+2, and the only source beyond that — the Imp vault's
     * final-room prize — appears once per run.
     */
    const val MAX_UPGRADE_RING_STANDARD = 2

    /**
     * Highest upgrade a weapon requirement may name. v4.0.0's Imp vault lays
     * out a +5 tier-4 weapon among its prizes, one level above what any other
     * family can reach.
     */
    const val MAX_UPGRADE_WEAPON = 5

    /** Highest upgrade the generator puts on any item, whatever its tier. */
    const val MAX_UPGRADE_ANY_TIER = 4

    /**
     * The one weapon tier levelled past [MAX_UPGRADE_ANY_TIER], a
     * v4.0.0-BETA-3 quirk: the Imp's vault lays out one tier-4 and one tier-5
     * weapon and rolls the tier-4 one at +3..+5 while the tier-5 one stops at
     * +4, so a +5 exists only on a tier-4 weapon, melee or thrown. When
     * upstream levels the two ranges this goes away and every family caps at
     * [MAX_UPGRADE_ANY_TIER].
     */
    const val EXTRA_UPGRADE_TIER = 4

    /**
     * The highest upgrade a requirement may name once its item and tier
     * filter are known: anything that cannot be a tier-[EXTRA_UPGRADE_TIER]
     * weapon stops at [MAX_UPGRADE_ANY_TIER].
     */
    fun maximumUpgrade(kind: ItemKind, item: CatalogItem?, tierMatch: TierMatch, tier: Int): Int {
        val ceiling = kind.maximumSearchUpgrade
        if (ceiling <= MAX_UPGRADE_ANY_TIER) return ceiling
        val reachesExtraTier = if (item != null) {
            item.tier == EXTRA_UPGRADE_TIER
        } else {
            when (tierMatch) {
                TierMatch.ANY -> true
                TierMatch.EXACT -> tier == EXTRA_UPGRADE_TIER
                TierMatch.AT_LEAST -> tier <= EXTRA_UPGRADE_TIER
                TierMatch.AT_MOST -> tier >= EXTRA_UPGRADE_TIER
            }
        }
        return if (reachesExtraTier) ceiling else MAX_UPGRADE_ANY_TIER
    }

    /**
     * The highest combined level [count] rings can reach together: one ring
     * at the vault ceiling, every other at the standard roll, each counting
     * its upgrade plus one.
     */
    fun ringStackCapacity(count: Int): Int =
        (MAX_UPGRADE_RING + 1) + (count - 1) * (MAX_UPGRADE_RING_STANDARD + 1)
}

enum class ItemKind(
    val label: String,
    val singularLabel: String,
    val modifierLabel: String?,
    /** The highest upgrade a search may name for this family. */
    val maximumSearchUpgrade: Int,
) {
    WEAPON("Weapons", "weapon", "Enchantment", SearchLimits.MAX_UPGRADE_WEAPON),
    ARMOR("Armor", "armor", "Glyph", SearchLimits.MAX_UPGRADE_DEFAULT),
    WAND("Wands", "wand", null, SearchLimits.MAX_UPGRADE_DEFAULT),
    RING("Rings", "ring", null, SearchLimits.MAX_UPGRADE_RING),

    // Wire kind IDs 4 and 5 (the enum ordinal is the wire ID): weapon
    // requirements narrowed to one weapon class. Catalog items always carry
    // the WEAPON family, never a narrowed kind.
    MELEE_WEAPON("Melee weapons", "melee weapon", "Enchantment", SearchLimits.MAX_UPGRADE_WEAPON),
    THROWN_WEAPON("Thrown weapons", "thrown weapon", "Enchantment", SearchLimits.MAX_UPGRADE_WEAPON),
    TRINKET("Trinket", "trinket", null, 0),
    ;

    /** The broad item family this kind belongs to. */
    val family: ItemKind
        get() = if (this == MELEE_WEAPON || this == THROWN_WEAPON) WEAPON else this

    /** The weapon class this kind restricts to, or null when unrestricted. */
    val weaponClass: WeaponClass?
        get() = when (this) {
            MELEE_WEAPON -> WeaponClass.MELEE
            THROWN_WEAPON -> WeaponClass.THROWN
            else -> null
        }

    /** Whether a catalog item can satisfy a requirement of this kind. */
    fun accepts(item: CatalogItem): Boolean =
        item.kind == family && (weaponClass == null || item.weaponClass == weaponClass)
}

/** Melee/thrown classification of weapon catalog entries. */
enum class WeaponClass { MELEE, THROWN }

data class CatalogItem(
    val id: String,
    val name: String,
    val kind: ItemKind,
    val spriteIndex: Int,
    val tier: Int? = null,
    val typeIconIndex: Int? = null,
    val weaponClass: WeaponClass? = null,
) {
    /**
     * Whether this is a tipped dart. Every shop stocks tipped darts and any
     * dart can be tipped by hand, so the item picker never offers them —
     * though a scouted world still lists the ones it rolled. The engine's
     * catalog keeps the `_dart` suffix unambiguous (the plain dart has no
     * entry), and its wasm cross-check test pins the suffix to the tipped set.
     */
    val isTippedDart: Boolean get() = id.endsWith("_dart")
}

/**
 * Which enchantment, glyph or curse a weapon/armor requirement accepts.
 * Names are the catalog's own spellings, which are also the engine's wire names.
 */
sealed interface EffectFilter {
    /** No constraint: enchanted, cursed and plain items all match. */
    data object Any : EffectFilter

    /** Some non-curse enchantment or glyph of the item's family. */
    data object AnyEnchantment : EffectFilter

    /** One of the named effects; a single name is the classic "exactly this" filter. */
    data class OneOf(val names: List<String>) : EffectFilter {
        init {
            require(names.isNotEmpty()) { "An effect list needs at least one entry" }
            require(names.distinct().size == names.size) { "An effect list cannot repeat a name" }
        }
    }

    companion object {
        /** The filter for a single optional effect name. */
        fun named(name: String?): EffectFilter = name?.let { OneOf(listOf(it)) } ?: Any

        /**
         * The canonical filter for a set of names of [kind]'s family: names are
         * reordered to catalog order, the full non-curse family set collapses
         * to [AnyEnchantment], and an empty selection means [Any].
         */
        fun of(names: Collection<String>, kind: ItemKind): EffectFilter {
            val order = ItemCatalog.modifiersFor(kind)
            val ordered = order.filter { it in names } + names.filter { it !in order }
            if (ordered.isEmpty()) return Any
            val enchantments = ItemCatalog.enchantmentsFor(kind)
            if (enchantments.isNotEmpty() && ordered.toSet() == enchantments.toSet()) return AnyEnchantment
            return OneOf(ordered)
        }
    }
}

/**
 * Membership in a combined-level group: the members' *levels* must add up to
 * at least [atLeast], where one matched item contributes its upgrade plus one.
 * Members are optional, so the group reads "up to N items reaching [atLeast]
 * levels" — one +2 ring satisfies a total of 3 on its own, and so does a +0
 * with a +1.
 */
data class LevelSum(val group: Int, val atLeast: Int) {
    init {
        require(group in 1..SearchLimits.LEVEL_SUM_GROUP_MAX) {
            "A combined-level group must be 1..${SearchLimits.LEVEL_SUM_GROUP_MAX}"
        }
        require(atLeast >= 1) { "A combined level must be at least 1" }
    }
}

data class ItemRequirement(
    val key: Long,
    val item: CatalogItem?,
    val upgrade: Int,
    val effect: EffectFilter = EffectFilter.Any,
    val kind: ItemKind = item?.kind ?: error("A wildcard requirement must specify its category"),
    val tier: Int = 0,
    val tierMatch: TierMatch = TierMatch.ANY,
    val upgradeMatch: UpgradeMatch = UpgradeMatch.EXACT,
    val source: ScoutItemSource? = null,
    val identityGroup: Int? = null,
    val maximumDepth: Int? = null,
    val requireUncursed: Boolean = false,
    /**
     * Session-local id of the "any of these" slot this row belongs to, or
     * null for a slot of its own. Members of one group count as a single
     * requirement that any of them satisfies.
     */
    val alternativeGroup: Int? = null,
    /** Membership in a combined-level group; never together with [alternativeGroup]. */
    val levelSum: LevelSum? = null,
) {
    init {
        require(item == null || kind.accepts(item)) { "Selected item must belong to its category" }
        require(kind != ItemKind.TRINKET || item != null) { "Select a trinket" }
        val tierable = item == null && kind.family in setOf(ItemKind.WEAPON, ItemKind.ARMOR)
        val validTier = when (tierMatch) {
            TierMatch.ANY -> tier == 0
            TierMatch.EXACT -> tierable && tier in SearchLimits.EXACT_TIERS
            TierMatch.AT_LEAST, TierMatch.AT_MOST -> tierable && tier in SearchLimits.BOUNDED_TIERS
        }
        require(validTier) {
            "Tier predicate requires a wildcard weapon or armor and a non-redundant tier"
        }
        val upgradeCeiling = SearchLimits.maximumUpgrade(kind, item, tierMatch, tier)
        val validUpgrade = when (upgradeMatch) {
            UpgradeMatch.ANY -> upgrade == 0
            UpgradeMatch.EXACT -> upgrade in 1..upgradeCeiling
            UpgradeMatch.AT_LEAST -> upgrade in 0..upgradeCeiling
        }
        require(validUpgrade) {
            "Upgrade predicate is invalid for ${kind.label}"
        }
        require(kind.modifierLabel != null || effect == EffectFilter.Any) {
            "${kind.label} cannot carry an effect requirement"
        }
        if (effect is EffectFilter.OneOf) {
            val known = ItemCatalog.modifiersFor(kind)
            require(effect.names.all { it in known }) {
                "Effect list names an effect ${kind.singularLabel}s cannot carry"
            }
            val curses = ItemCatalog.cursesFor(kind)
            require(!requireUncursed || effect.names.any { it !in curses }) {
                "An uncursed item cannot have a curse"
            }
        }
        require(identityGroup == null || identityGroup in 1..SearchLimits.IDENTITY_GROUP_MAX) {
            "A stack group must be 1..${SearchLimits.IDENTITY_GROUP_MAX}"
        }
        require(maximumDepth == null || maximumDepth in 1..SearchLimits.MAX_DEPTH) {
            "Item floor limit must be 1..${SearchLimits.MAX_DEPTH}"
        }
        require(alternativeGroup == null || alternativeGroup >= 1) { "Alternative group ids start at 1" }
        require(alternativeGroup == null || levelSum == null) {
            "An either/or alternative cannot count a combined level"
        }
    }

    /** The one effect this requirement pins, for the sprite glow; null for any other filter. */
    val singleEffect: String?
        get() = (effect as? EffectFilter.OneOf)?.names?.singleOrNull()

    /** The highest upgrade an item satisfying this requirement can carry. */
    val maximumUpgrade: Int
        get() = if (upgradeMatch == UpgradeMatch.EXACT) upgrade else upgradeCeiling

    /** The highest upgrade this requirement may name, its item and tier filter included. */
    val upgradeCeiling: Int
        get() = SearchLimits.maximumUpgrade(kind, item, tierMatch, tier)

    /**
     * The most *levels* this requirement can contribute to a combined total:
     * its highest upgrade plus one, since every matched item counts itself.
     */
    val maximumLevel: Int
        get() = maximumUpgrade + 1

    /**
     * Whether this constrains nothing beyond its category — the shape a
     * stack's extra copies take. A narrowed weapon kind is a constraint; a
     * per-item floor limit is a placement bound, not an item property, and
     * does not count.
     */
    val isBare: Boolean
        get() = item == null &&
            kind == kind.family &&
            tierMatch == TierMatch.ANY &&
            upgradeMatch == UpgradeMatch.ANY &&
            effect == EffectFilter.Any &&
            !requireUncursed &&
            source == null

    /** Human-readable effect constraint, or null when any effect is accepted. */
    val effectLabel: String?
        get() = when (val filter = effect) {
            EffectFilter.Any -> null
            EffectFilter.AnyEnchantment -> "any ${kind.modifierLabel?.lowercase() ?: "enchantment"}"
            is EffectFilter.OneOf -> filter.names.joinToString("/")
        }

    val description: String
        get() = if (kind == ItemKind.TRINKET) "Trinket" else buildString {
            append(
                when (upgradeMatch) {
                    UpgradeMatch.ANY -> "Any upgrade"
                    UpgradeMatch.EXACT -> "+$upgrade exactly"
                    UpgradeMatch.AT_LEAST -> "+$upgrade or higher"
                },
            )
            effectLabel?.let {
                append(" • ")
                append(it)
            }
            if (requireUncursed) append(" • uncursed")
            source?.let {
                append(" • ")
                append(it.label)
            }
            levelSum?.let {
                append(" • combined level ≥ ")
                append(it.atLeast)
            }
            maximumDepth?.let {
                append(" • by floor ")
                append(it)
            }
        }

    val title: String
        get() = item?.name ?: when (tierMatch) {
            TierMatch.ANY -> "Any ${kind.singularLabel}"
            TierMatch.EXACT -> "Any Tier $tier ${kind.singularLabel}"
            TierMatch.AT_LEAST -> "Any Tier $tier+ ${kind.singularLabel}"
            TierMatch.AT_MOST -> "Any Tier $tier or lower ${kind.singularLabel}"
        }
}

/**
 * The slots of a requirement list: an alternative group is one slot at its
 * first member's position holding every member in list order; every other
 * requirement is a slot of its own. This is what the engine counts as a
 * requirement, so headers and result counts use it.
 */
fun List<ItemRequirement>.slots(): List<List<ItemRequirement>> {
    val slots = mutableListOf<MutableList<ItemRequirement>>()
    val slotByGroup = mutableMapOf<Int, MutableList<ItemRequirement>>()
    for (requirement in this) {
        val group = requirement.alternativeGroup
        if (group == null) {
            slots += mutableListOf(requirement)
        } else {
            val slot = slotByGroup[group]
            if (slot == null) {
                val fresh = mutableListOf(requirement)
                slotByGroup[group] = fresh
                slots += fresh
            } else {
                slot += requirement
            }
        }
    }
    return slots
}

/** How many slots — engine-level requirements — the list holds. */
fun List<ItemRequirement>.slotCount(): Int = slots().size

/**
 * The first problem that would make the engine refuse this requirement list,
 * as a user-facing message, or null when it is runnable. [SearchRequest]
 * enforces the same rules; this form exists so the editor can show the
 * message instead of silently disabling Search.
 */
fun List<ItemRequirement>.validationProblem(): String? {
    if (isEmpty()) return "Add at least one requirement."
    // A stack (identity group) has one anchor unit — a lone requirement or one
    // whole alternative group — that may constrain the item it binds to; every
    // other member is a bare copy of the same category.
    val identityGroups = filter { it.identityGroup != null }.groupBy { it.identityGroup!! }
    for ((_, members) in identityGroups.toSortedMap()) {
        if (members.map { it.kind.family }.distinct().size > 1) {
            return "The copies of a stack must share its category."
        }
        val units = members.filterNot { it.isBare }
            .map { it.alternativeGroup?.let { group -> "alt:$group" } ?: "req:${it.key}" }
            .distinct()
        if (units.size > 1) {
            return "Only one item of a stack can carry constraints; the extra copies are plain."
        }
    }
    // Combined-level groups: rings only, and one shared, reachable total,
    // counted in levels (upgrade plus one per item).
    val sumGroups = filter { it.levelSum != null }.groupBy { it.levelSum!!.group }
    for ((_, members) in sumGroups.toSortedMap()) {
        if (members.any { it.kind.family != ItemKind.RING }) {
            return "Only rings can count levels together."
        }
        val totals = members.map { it.levelSum!!.atLeast }.distinct()
        if (totals.size > 1) {
            return "A stack must share one combined level " +
                "(it has ${totals.sorted().joinToString(" and ")})."
        }
        // Each member's own ceiling, bounded by what a world generates: only
        // the Imp vault's one prize levels a ring past the standard roll.
        val reachable = minOf(
            members.sumOf { it.maximumLevel },
            SearchLimits.ringStackCapacity(members.size),
        )
        val needed = totals.single()
        if (needed > reachable) {
            return "A combined level of $needed needs more items: " +
                "these ${members.size} can reach $reachable."
        }
    }
    return null
}

enum class TierMatch(val label: String) {
    ANY("Any tier"),
    EXACT("Exactly"),
    AT_LEAST("At least"),
    AT_MOST("At most"),
}

enum class UpgradeMatch(val label: String) {
    ANY("Any"),
    EXACT("Exactly"),
    AT_LEAST("At least"),
}

/**
 * Boss floors that generate no searchable items. The engine treats a floor
 * limit of 5/10/15 exactly like 4/9/14, so floor-limit selectors skip them.
 * Floor 20 stays selectable: the Imp shop gives the City boss floor stock.
 */
val EMPTY_BOSS_FLOORS: Set<Int> = setOf(5, 10, 15)

/** Floors offered by floor-limit selectors: 1..MAX_DEPTH minus the empty boss floors. */
val FLOOR_LIMIT_OPTIONS: List<Int> = (1..SearchLimits.MAX_DEPTH).filterNot(EMPTY_BOSS_FLOORS::contains)

/** Snaps an empty boss-floor limit to the equivalent floor below it (5→4, 10→9, 15→14). */
fun normalizeFloorLimit(depth: Int): Int = if (depth in EMPTY_BOSS_FLOORS) depth - 1 else depth

/**
 * The selector index of a floor limit within [FLOOR_LIMIT_OPTIONS]; off-list
 * values snap to the nearest option below (or the first option).
 */
fun floorLimitIndex(depth: Int): Int {
    val floor = normalizeFloorLimit(depth)
    val exact = FLOOR_LIMIT_OPTIONS.indexOf(floor)
    if (exact >= 0) return exact
    return FLOOR_LIMIT_OPTIONS.indexOfLast { it <= floor }.coerceAtLeast(0)
}

/**
 * The Wandmaker quest a search can demand, or `null` for any of them.
 *
 * Only this giver's variant is worth filtering on: its quest item — corpse
 * dust, an elemental ember, or a rotberry seed — can be used in the dungeon
 * instead of being handed in. The other three quests only change the fight.
 */
enum class WandmakerQuest(val variant: ScoutQuestVariant, val documentName: String) {
    CORPSE_DUST(ScoutQuestVariant.CORPSE_DUST, "corpse_dust"),
    ELEMENTAL_EMBERS(ScoutQuestVariant.ELEMENTAL_EMBERS, "elemental_embers"),
    ROTBERRY(ScoutQuestVariant.ROTBERRY, "rotberry"),
    ;

    /** The game's own one-based quest value. */
    val wireId: Int
        get() = ordinal + 1

    val label: String
        get() = variant.label

    companion object {
        /** Resolves the stable snake_case name used by shared query documents. */
        fun named(name: String): WandmakerQuest? = entries.firstOrNull { it.documentName == name }
    }
}

data class SearchRequest(
    val requirements: List<ItemRequirement>,
    val maximumDepth: Int = SearchLimits.MAX_DEPTH,
    val challenges: Int = 0,
    val requireBlacksmith: Boolean = false,
    /** Prevent the Blacksmith's 2,000-favor Smith choice from satisfying item requirements. */
    val excludeBlacksmithRewards: Boolean = false,
    /** Which Wandmaker quest the run must roll; null accepts any. */
    val wandmakerQuest: WandmakerQuest? = null,
) {
    init {
        require(requirements.isNotEmpty()) { "At least one requirement is needed" }
        requirements.validationProblem()?.let { throw IllegalArgumentException(it) }
        require(maximumDepth in 1..SearchLimits.MAX_DEPTH) { "Maximum floor must be 1..${SearchLimits.MAX_DEPTH}" }
        require(challenges in 0..Challenge.ALL_MASK) { "Challenge mask must be 0..${Challenge.ALL_MASK}" }
    }

    /** How many slots the engine sees: what result rows report as matched requirements. */
    val slotCount: Int
        get() = requirements.slotCount()
}

enum class Challenge(
    val bit: Int,
    val displayName: String,
    val changesLevelGeneration: Boolean = false,
) {
    NO_FOOD(1, "On diet"),
    NO_ARMOR(2, "Faith is my armor"),
    NO_HEALING(4, "Pharmacophobia"),
    NO_HERBALISM(8, "Barren land", changesLevelGeneration = true),
    SWARM_INTELLIGENCE(16, "Swarm intelligence"),
    DARKNESS(32, "Into darkness", changesLevelGeneration = true),
    NO_SCROLLS(64, "Forbidden runes", changesLevelGeneration = true),
    CHAMPION_ENEMIES(128, "Hostile champions"),
    STRONGER_BOSSES(256, "Badder bosses"),
    ;

    companion object {
        /** Every challenge bit together: the largest legal challenge mask. */
        const val ALL_MASK = 511
    }
}

data class SeedResult(
    val seed: String,
    val matchedRequirements: Int,
)

data class ScoutWorld(
    val seed: String,
    val items: List<ScoutItem>,
    val quests: List<ScoutQuest>,
    /**
     * The gems this run gave the ring classes, which decide what colour each
     * ring in [items] is drawn. It rides in the same scout packet as [items],
     * so every generated world states its own; there is no default, because a
     * world that quietly claimed [RingGems.CATALOG] would draw a seed's rings
     * in the wrong twelve colours rather than fail. Only a world no run stands
     * behind — the demo engine's — names the catalog table itself.
     */
    val ringGems: RingGems,
    val trinketOrder: List<CatalogItem> = emptyList(),
)

/**
 * The gems one run hands the twelve ring classes: `ordinals[cls]` is the gem
 * ordinal the run gave the ring whose class index is `cls` — which is also that
 * class's position in `ItemCatalog.rings` and its [CatalogItem.typeIconIndex].
 *
 * Shattered Pixel Dungeon shuffles `Ring.gems` in `Dungeon.init()`, before the
 * first floor exists and before any challenge is read, so which gem a ring
 * shows follows from the seed alone (docs/COMPATIBILITY.md). The engine
 * reproduces that shuffle and publishes it in the `SSC3` scout packet, beside
 * the items it colours; nothing here re-derives it.
 */
data class RingGems(val ordinals: List<Int>) {
    init {
        require(ordinals.size == RING_CLASS_COUNT) {
            "A run's ring gems must be $RING_CLASS_COUNT ordinals, not ${ordinals.size}"
        }
        require(ordinals.toSet().size == RING_CLASS_COUNT && ordinals.all { it in 0 until RING_CLASS_COUNT }) {
            "A run's ring gems must be a permutation of 0..${RING_CLASS_COUNT - 1}, not $ordinals"
        }
    }

    /**
     * The `items.png` cell [item] is drawn from in this run: this run's gem for
     * the ring class [CatalogItem.typeIconIndex] names, and the item's own
     * catalog cell for everything that names no class — which today is
     * everything but a ring, since the catalog gives a `typeIcon` to rings
     * alone. Every surface showing an item that belongs to a seed must draw
     * this rather than [CatalogItem.spriteIndex], or every seed renders the
     * same twelve ring colours.
     */
    fun spriteIndexFor(item: CatalogItem): Int {
        // The class index is the one the catalog states, not one read back out
        // of a sprite cell: a cell says which gem, never which ring.
        val ringClass = item.typeIconIndex ?: return item.spriteIndex
        if (ringClass !in ordinals.indices) return item.spriteIndex
        return RING_SPRITE_BASE + ordinals[ringClass]
    }

    companion object {
        /** `ItemSpriteSheet.RINGS`: the atlas cell the twelve gem sprites start at. */
        const val RING_SPRITE_BASE = 224

        /** `Generator.Category.RING.classes.length`, which is also `Ring.gems.length`. */
        const val RING_CLASS_COUNT = 12

        /**
         * The unshuffled table: every ring drawn at its own class's cell. This
         * is what the catalog carries and what a surface with no run to ask —
         * the requirement board, the query editor's pickers — shows.
         */
        val CATALOG = RingGems((0 until RING_CLASS_COUNT).toList())
    }
}

/** One rolled quest: which giver spawned, the variant it rolled, and its host floor. */
data class ScoutQuest(
    val variant: ScoutQuestVariant,
    val depth: Int,
) {
    init {
        require(depth in variant.giver.depths) {
            "${variant.giver.label} quest floor must be in ${variant.giver.depths}"
        }
    }

    val giver: ScoutQuestGiver
        get() = variant.giver
}

enum class ScoutQuestGiver(val label: String, val depths: IntRange) {
    GHOST("Sad ghost", 2..4),
    WANDMAKER("Wandmaker", 7..9),
    BLACKSMITH("Blacksmith", 12..14),
    IMP("Imp", 17..19),
}

/** Declaration order within each giver matches the wire variant codes 1..n. */
enum class ScoutQuestVariant(val giver: ScoutQuestGiver, val label: String) {
    FETID_RAT(ScoutQuestGiver.GHOST, "Fetid rat"),
    GNOLL_TRICKSTER(ScoutQuestGiver.GHOST, "Gnoll trickster"),
    GREAT_CRAB(ScoutQuestGiver.GHOST, "Great crab"),
    CORPSE_DUST(ScoutQuestGiver.WANDMAKER, "Corpse dust"),
    ELEMENTAL_EMBERS(ScoutQuestGiver.WANDMAKER, "Elemental embers"),
    ROTBERRY(ScoutQuestGiver.WANDMAKER, "Rotberry"),
    CRYSTAL(ScoutQuestGiver.BLACKSMITH, "Crystal spire"),
    GNOLL(ScoutQuestGiver.BLACKSMITH, "Gnoll geomancer"),
    // v4.0.0 replaced the Imp's Monk/Golem token hunts with one vault
    // expedition, so this giver has a single variant.
    VAULT(ScoutQuestGiver.IMP, "Vault"),
    ;

    companion object {
        fun variantsFor(giver: ScoutQuestGiver): List<ScoutQuestVariant> =
            entries.filter { it.giver == giver }
    }
}

data class ScoutItem(
    val item: CatalogItem,
    val depth: Int,
    val upgrade: Int,
    val effect: String?,
    val cursed: Boolean,
    val secret: Boolean = false,
    val source: ScoutItemSource,
    val accessibility: ScoutAccessibility,
)

enum class ScoutItemSource(val label: String) {
    HEAP("Heap"),
    CHEST("Chest"),
    LOCKED_CHEST("Locked chest"),
    CRYSTAL_CHEST("Crystal chest"),
    TOMB("Tomb"),
    SKELETON("Skeleton"),
    SACRIFICIAL_FIRE("Sacrificial fire"),
    MIMIC("Mimic"),
    GOLDEN_MIMIC("Golden mimic"),
    CRYSTAL_MIMIC("Crystal mimic"),
    STATUE("Statue"),
    ARMORED_STATUE("Armored statue"),
    SHOP("Shop"),
    GHOST_REWARD("Ghost reward"),
    WANDMAKER_REWARD("Wandmaker reward"),
    BLACKSMITH_REWARD("Blacksmith reward"),
    IMP_REWARD("Imp reward"),

    /** Equipment the v4.0.0 Imp vault's treasure rooms lay out on the way in. */
    VAULT_TREASURE("Vault treasure"),
}

sealed interface ScoutAccessibility {
    data object Independent : ScoutAccessibility

    data class Choice(
        val group: Int,
        val option: Int,
    ) : ScoutAccessibility

    data class Scenarios(
        val group: Int,
        val mask: ULong,
    ) : ScoutAccessibility
}

enum class SearchState {
    RUNNING,
    COMPLETED,
    CANCELLED,
    FAILED,
}

data class SearchStatus(
    val state: SearchState,
    val scannedSeeds: Long,
    val totalSeeds: Long,
    val errorCode: Long = 0,
    val matchProbability: Double = 0.0,
)

/** Where and how much a follow-up traversal must scan to finish a session's coverage. */
data class ResumeHint(
    val position: Long,
    val remaining: Long,
)

data class SearchBatch(val results: List<SeedResult>)
