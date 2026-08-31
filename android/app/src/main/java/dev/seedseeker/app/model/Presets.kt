// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import java.util.UUID

data class PresetQuery(
    val requirements: List<ItemRequirement>,
    val maximumDepth: Int = 24,
    val requireBlacksmith: Boolean = false,
    val excludeBlacksmithRewards: Boolean = false,
    val wandmakerQuest: WandmakerQuest? = null,
    val fastMode: Boolean = false,
    val challenges: Int = 0,
)

data class QueryPreset(
    val id: String = UUID.randomUUID().toString(),
    val name: String,
    val query: PresetQuery,
    val isBuiltIn: Boolean = false,
)

object BuiltInPresets {
    /**
     * The floor limit the vault presets carry: floor 19 is the last floor the
     * Imp — and so the vault holding its levelled prizes — can appear on, so a
     * deeper scan only costs time.
     */
    private const val VAULT_FLOOR_LIMIT = 19

    val staff21 = QueryPreset(
        id = "staff-21",
        name = "+21 Staff",
        isBuiltIn = true,
        query = PresetQuery(
            requirements = listOf(
                ItemRequirement(1, null, 3, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.EXACT, identityGroup = 1),
                ItemRequirement(2, null, 0, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.ANY, identityGroup = 1),
                ItemRequirement(3, null, 0, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.ANY, identityGroup = 1),
                ItemRequirement(4, null, 1, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.AT_LEAST),
            ),
        ),
    )

    /**
     * The +21 stack anchored one level higher, on the +4 wand v4.0.0's Imp
     * vault lays out among its prizes.
     */
    val staff22 = QueryPreset(
        id = "staff-22",
        name = "+22 Staff",
        isBuiltIn = true,
        query = PresetQuery(
            requirements = listOf(
                ItemRequirement(1, null, 4, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.EXACT, identityGroup = 1),
                ItemRequirement(2, null, 0, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.ANY, identityGroup = 1),
                ItemRequirement(3, null, 0, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.ANY, identityGroup = 1),
                ItemRequirement(4, null, 1, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.AT_LEAST),
            ),
            maximumDepth = VAULT_FLOOR_LIMIT,
        ),
    )

    val wandBonanza = QueryPreset(
        id = "wand-bonanza",
        name = "Wand Bonanza",
        isBuiltIn = true,
        query = PresetQuery(
            requirements = listOf(
                ItemRequirement(1, null, 3, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.EXACT),
                ItemRequirement(2, null, 2, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.EXACT, maximumDepth = 4),
                ItemRequirement(3, null, 2, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.EXACT, maximumDepth = 4),
                ItemRequirement(4, null, 2, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.EXACT),
            ),
        ),
    )

    val ringOfWealth21 = QueryPreset(
        id = "ring-of-wealth-21",
        name = "+21 Ring of Wealth",
        isBuiltIn = true,
        query = PresetQuery(
            requirements = listOf(
                ItemRequirement(
                    1,
                    ItemCatalog.findById("ring_wealth"),
                    4,
                    kind = ItemKind.RING,
                    upgradeMatch = UpgradeMatch.EXACT,
                    source = ScoutItemSource.IMP_REWARD,
                ),
                ItemRequirement(
                    2,
                    ItemCatalog.findById("ring_wealth"),
                    2,
                    kind = ItemKind.RING,
                    upgradeMatch = UpgradeMatch.EXACT,
                ),
                ItemRequirement(
                    3,
                    ItemCatalog.findById("ring_wealth"),
                    0,
                    kind = ItemKind.RING,
                    upgradeMatch = UpgradeMatch.ANY,
                ),
            ),
        ),
    )

    /**
     * A tier-4 weapon at the +5 only the vault reaches, with two more of the
     * same weapon to pour into it.
     */
    val tier4Weapon26 = QueryPreset(
        id = "tier-4-weapon-26",
        name = "+26 Tier 4 Weapon",
        isBuiltIn = true,
        query = PresetQuery(
            requirements = listOf(
                ItemRequirement(
                    1,
                    null,
                    5,
                    kind = ItemKind.WEAPON,
                    tier = 4,
                    tierMatch = TierMatch.EXACT,
                    upgradeMatch = UpgradeMatch.EXACT,
                    identityGroup = 1,
                ),
                ItemRequirement(2, null, 0, kind = ItemKind.WEAPON, upgradeMatch = UpgradeMatch.ANY, identityGroup = 1),
                ItemRequirement(3, null, 0, kind = ItemKind.WEAPON, upgradeMatch = UpgradeMatch.ANY, identityGroup = 1),
            ),
            maximumDepth = VAULT_FLOOR_LIMIT,
        ),
    )

    val all = listOf(staff21, staff22, wandBonanza, ringOfWealth21, tier4Weapon26)
}
