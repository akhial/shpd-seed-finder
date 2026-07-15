// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import java.util.UUID

data class PresetQuery(
    val requirements: List<ItemRequirement>,
    val maximumDepth: Int = 24,
    val requireBlacksmith: Boolean = false,
    val excludeBlacksmithRewards: Boolean = false,
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

    val all = listOf(staff21, wandBonanza, ringOfWealth21)
}
