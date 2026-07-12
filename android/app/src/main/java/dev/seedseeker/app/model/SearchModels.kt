// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

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
}

data class CatalogItem(
    val id: String,
    val name: String,
    val kind: ItemKind,
    val spriteIndex: Int,
    val tier: Int? = null,
    val typeIconIndex: Int? = null,
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
) {
    init {
        require(item == null || item.kind == kind) { "Selected item must belong to its category" }
        val validTier = when (tierMatch) {
            TierMatch.ANY -> tier == 0
            TierMatch.EXACT, TierMatch.AT_LEAST ->
                item == null && kind in setOf(ItemKind.WEAPON, ItemKind.ARMOR) && tier in 2..5
        }
        require(validTier) { "Tier predicate requires any tier-2 through tier-5 weapon or armor" }
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
        }
}

enum class TierMatch(val label: String) {
    ANY("Any tier"),
    EXACT("Exactly"),
    AT_LEAST("At least"),
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
