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

/**
 * Effect predicate attached to one requirement. Only weapons and armor carry
 * effects; wands and rings always use [Any].
 */
sealed interface EffectRequirement {
    /** Wildcard: any effect, or none at all. */
    data object Any : EffectRequirement

    /** The item must carry some non-curse enchantment (weapons) or glyph (armor). */
    data object AnyEnchantment : EffectRequirement

    /** The item must carry one of these same-family effects. */
    data class OneOf(val effects: List<String>) : EffectRequirement
}

data class ItemRequirement(
    val key: Long,
    val item: CatalogItem?,
    val upgrade: Int,
    val effect: EffectRequirement = EffectRequirement.Any,
    val kind: ItemKind = item?.kind ?: error("A wildcard requirement must specify its category"),
    val tier: Int = 0,
    val tierMatch: TierMatch = TierMatch.ANY,
    val upgradeMatch: UpgradeMatch = UpgradeMatch.EXACT,
    val source: ScoutItemSource? = null,
    val identityGroup: Int? = null,
    val maximumDepth: Int? = null,
    val requireUncursed: Boolean = false,
    /** Requirements sharing a group are alternatives: one match satisfies the whole group. */
    val alternativeGroup: Int? = null,
    /** Combined-upgrade group label shared with other requirements (A..D as 1..4). */
    val upgradeSumGroup: Int? = null,
    /** Minimum combined upgrade total agreed on by every member of the sum group. */
    val upgradeSumTotal: Int? = null,
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
        require(kind.modifierLabel != null || effect == EffectRequirement.Any) {
            "${kind.label} cannot carry an effect requirement"
        }
        if (effect is EffectRequirement.OneOf) {
            require(effect.effects.isNotEmpty()) { "An effect set needs at least one member" }
            require(effect.effects.toSet().size == effect.effects.size) {
                "An effect set cannot repeat a member"
            }
            require(effect.effects.all { it in ItemCatalog.modifiersFor(kind) }) {
                "Effects must belong to ${kind.label}"
            }
            require(!requireUncursed || !effect.effects.all { it in ItemCatalog.cursesFor(kind) }) {
                "An uncursed item cannot be limited to curses"
            }
        }
        require(identityGroup == null || identityGroup in 1..4) { "Same-item group must be A..D" }
        require(maximumDepth == null || maximumDepth in 1..24) { "Item floor limit must be 1..24" }
        require(alternativeGroup == null || alternativeGroup in 1..255) {
            "Alternative group must be 1..255"
        }
        require((upgradeSumGroup == null) == (upgradeSumTotal == null)) {
            "A combined upgrade needs both a group and a total"
        }
        require(upgradeSumGroup == null || upgradeSumGroup in 1..4) {
            "Combined upgrade group must be A..D"
        }
        require(upgradeSumTotal == null || upgradeSumTotal in 1..8) {
            "Combined upgrade total must be 1..8"
        }
        require(alternativeGroup == null || upgradeSumGroup == null) {
            "A combined upgrade group cannot include alternative requirements"
        }
    }

    /**
     * Condensed effect phrase, or `null` when any effect is fine: a single
     * name, up to four names joined with "or", "any of N enchantments" for
     * larger sets, and "any enchantment"/"any glyph" for the full non-curse
     * family.
     */
    val effectSummary: String?
        get() {
            val familyWord = when (kind.family) {
                ItemKind.WEAPON -> "enchantment"
                ItemKind.ARMOR -> "glyph"
                else -> return null
            }
            return when (val wanted = effect) {
                EffectRequirement.Any -> null
                EffectRequirement.AnyEnchantment -> "any $familyWord"
                is EffectRequirement.OneOf -> {
                    val nonCurse = ItemCatalog.modifiersFor(kind) - ItemCatalog.cursesFor(kind).toSet()
                    when {
                        wanted.effects.toSet() == nonCurse.toSet() -> "any $familyWord"
                        wanted.effects.size == 1 -> wanted.effects.single()
                        wanted.effects.size <= 4 ->
                            wanted.effects.dropLast(1).joinToString(", ") + " or " + wanted.effects.last()
                        else -> "any of ${wanted.effects.size} ${familyWord}s"
                    }
                }
            }
        }

    /** The only wanted effect, if exactly one is named; drives the sprite glow. */
    val singleEffect: String?
        get() = (effect as? EffectRequirement.OneOf)?.effects?.singleOrNull()

    val description: String
        get() = buildString {
            append(
                when (upgradeMatch) {
                    UpgradeMatch.ANY -> "Any upgrade"
                    UpgradeMatch.EXACT -> "+$upgrade exactly"
                    UpgradeMatch.AT_LEAST -> "+$upgrade or higher"
                },
            )
            effectSummary?.let {
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
            upgradeSumTotal?.let {
                append(" • combined +")
                append(it)
                append(" total (group ")
                append(('A'.code + (upgradeSumGroup ?: 1) - 1).toChar())
                append(")")
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

    /**
     * Mirrors the engine's attainability rule for combined-upgrade groups:
     * the members' reachable upgrades must be able to add up to the total.
     * The engine would reject such a request with an unspecific error.
     */
    fun unattainableUpgradeSumMessage(): String? {
        val groups = requirements.filter { it.upgradeSumGroup != null }.groupBy { it.upgradeSumGroup }
        for ((group, members) in groups.toSortedMap(compareBy { it })) {
            val total = members.mapNotNull { it.upgradeSumTotal }.max()
            val reachable = members.sumOf {
                if (it.upgradeMatch == UpgradeMatch.EXACT) it.upgrade else it.kind.maximumSearchUpgrade
            }
            if (total > reachable) {
                val label = ('A'.code + (group ?: 1) - 1).toChar()
                return "Combined upgrade group $label asks for +$total but its items can reach at most +$reachable together."
            }
        }
        return null
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

/**
 * Number of query slots: an alternative ("any of") group counts once, since
 * any single member satisfies it, and every plain requirement counts once.
 */
val List<ItemRequirement>.slotCount: Int
    get() = count { it.alternativeGroup == null } +
        mapNotNull { it.alternativeGroup }.toSet().size

data class SeedResult(
    val seed: String,
    val matchedRequirements: Int,
)

data class ScoutWorld(
    val seed: String,
    val items: List<ScoutItem>,
)

data class ScoutItem(
    val item: CatalogItem,
    val depth: Int,
    val upgrade: Int,
    val effect: String?,
    val cursed: Boolean,
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

data class SearchBatch(val results: List<SeedResult>)
