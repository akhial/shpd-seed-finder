// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog

enum class ItemKind(
    val label: String,
    val singularLabel: String,
    val modifierLabel: String?,
    val maximumSearchUpgrade: Int,
) {
    WEAPON("Weapons", "weapon", "Enchantment", 3),
    ARMOR("Armor", "armor", "Glyph", 3),
    WAND("Wands", "wand", null, 3),
    RING("Rings", "ring", null, 4),

    // Wire kind IDs 4 and 5 (the enum ordinal is the wire ID): weapon
    // requirements narrowed to one weapon class. Catalog items always carry
    // the WEAPON family, never a narrowed kind.
    MELEE_WEAPON("Melee weapons", "melee weapon", "Enchantment", 3),
    THROWN_WEAPON("Thrown weapons", "thrown weapon", "Enchantment", 3),
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
)

data class ItemRequirement(
    val key: Long,
    val item: CatalogItem?,
    val upgrade: Int,
    val modifier: String? = null,
    val kind: ItemKind = item?.kind ?: error("A wildcard requirement must specify its category"),
    val tier: Int = 0,
    val tierMatch: TierMatch = TierMatch.ANY,
    val upgradeMatch: UpgradeMatch = UpgradeMatch.EXACT,
    val source: ScoutItemSource? = null,
    val identityGroup: Int? = null,
    val maximumDepth: Int? = null,
    val requireUncursed: Boolean = false,
) {
    init {
        require(item == null || kind.accepts(item)) { "Selected item must belong to its category" }
        val tierable = item == null && kind.family in setOf(ItemKind.WEAPON, ItemKind.ARMOR)
        val validTier = when (tierMatch) {
            TierMatch.ANY -> tier == 0
            TierMatch.EXACT -> tierable && tier in 2..5
            TierMatch.AT_LEAST, TierMatch.AT_MOST -> tierable && tier in 3..4
        }
        require(validTier) {
            "Tier predicate requires a wildcard weapon or armor and a non-redundant tier"
        }
        val validUpgrade = when (upgradeMatch) {
            UpgradeMatch.ANY -> upgrade == 0
            UpgradeMatch.EXACT -> upgrade in 1..kind.maximumSearchUpgrade
            UpgradeMatch.AT_LEAST -> upgrade in 0..kind.maximumSearchUpgrade
        }
        require(validUpgrade) {
            "Upgrade predicate is invalid for ${kind.label}"
        }
        require(kind.modifierLabel != null || modifier == null) {
            "${kind.label} cannot carry a modifier requirement"
        }
        require(!requireUncursed || modifier !in ItemCatalog.cursesFor(kind)) {
            "An uncursed item cannot have a curse"
        }
        require(identityGroup == null || identityGroup in 1..4) { "Same-item group must be A..D" }
        require(maximumDepth == null || maximumDepth in 1..24) { "Item floor limit must be 1..24" }
    }

    val description: String
        get() = buildString {
            append(
                when (upgradeMatch) {
                    UpgradeMatch.ANY -> "Any upgrade"
                    UpgradeMatch.EXACT -> "+$upgrade exactly"
                    UpgradeMatch.AT_LEAST -> "+$upgrade or higher"
                },
            )
            modifier?.let {
                append(" • ")
                append(it)
            }
            if (requireUncursed) append(" • uncursed")
            source?.let {
                append(" • ")
                append(it.label)
            }
            identityGroup?.let {
                append(" • same item group ")
                append(('A'.code + it - 1).toChar())
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

/** Floors offered by floor-limit selectors: 1..24 minus the empty boss floors. */
val FLOOR_LIMIT_OPTIONS: List<Int> = (1..24).filterNot(EMPTY_BOSS_FLOORS::contains)

/** Snaps an empty boss-floor limit to the equivalent floor below it (5→4, 10→9, 15→14). */
fun normalizeFloorLimit(depth: Int): Int = if (depth in EMPTY_BOSS_FLOORS) depth - 1 else depth

data class SearchRequest(
    val requirements: List<ItemRequirement>,
    val maximumDepth: Int = 24,
    val challenges: Int = 0,
    val requireBlacksmith: Boolean = false,
    /** Prevent the Blacksmith's 2,000-favor Smith choice from satisfying item requirements. */
    val excludeBlacksmithRewards: Boolean = false,
    /**
     * Faster but non-exhaustive: +3 weapon/armor requirements only consider
     * quest rewards, skipping seeds whose sole match is a Crypt or
     * Sacrificial-fire prize. Found seeds are always genuine matches.
     */
    val fastMode: Boolean = false,
) {
    init {
        require(requirements.isNotEmpty()) { "At least one requirement is needed" }
        require(maximumDepth in 1..24) { "Maximum floor must be 1..24" }
        require(challenges in 0..Challenge.ALL_MASK) { "Challenge mask must be 0..${Challenge.ALL_MASK}" }
    }
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
)

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
    MONK(ScoutQuestGiver.IMP, "Monks"),
    GOLEM(ScoutQuestGiver.IMP, "Golems"),
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
