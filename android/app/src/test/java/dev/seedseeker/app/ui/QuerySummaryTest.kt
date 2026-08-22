// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.catalog.PackagedCatalog
import dev.seedseeker.app.model.EffectFilter
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.TierMatch
import dev.seedseeker.app.model.UpgradeMatch
import dev.seedseeker.app.model.UpgradeSum
import dev.seedseeker.app.model.WandmakerQuest
import org.junit.Assert.assertEquals
import org.junit.Test

class QuerySummaryTest {
    init { PackagedCatalog.install() }

    @Test
    fun detailLineCondensesAnExactItemRequirement() {
        val requirement = ItemRequirement(
            key = 1,
            item = ItemCatalog.weapons.first { it.id == "sword" },
            upgrade = 2,
            effect = EffectFilter.named("Lucky"),
            maximumDepth = 12,
        )

        assertEquals("+2 · Lucky · ≤ floor 12", requirementDetailLine(requirement))
    }

    @Test
    fun detailLineCondensesAWildcardRequirement() {
        val requirement = ItemRequirement(
            key = 2,
            item = null,
            kind = ItemKind.ARMOR,
            upgrade = 1,
            upgradeMatch = UpgradeMatch.AT_LEAST,
            tierMatch = TierMatch.AT_LEAST,
            tier = 3,
            requireUncursed = true,
            source = ScoutItemSource.GHOST_REWARD,
            identityGroup = 2,
        )

        assertEquals(
            "≥+1 · uncursed · Ghost reward · grp B",
            requirementDetailLine(requirement),
        )
    }

    @Test
    fun detailLineIsEmptyForAnUnconstrainedRequirement() {
        val requirement = ItemRequirement(
            key = 3,
            item = ItemCatalog.weapons.first { it.id == "sword" },
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
        )

        assertEquals("", requirementDetailLine(requirement))
    }

    @Test
    fun detailLineDescribesEffectSetsAnyEnchantmentAndCombinedUpgradeGroups() {
        val greatshield = ItemCatalog.weapons.first { it.id == "greatshield" }
        assertEquals(
            "+2 · Blocking/Projecting/Vampiric",
            requirementDetailLine(
                ItemRequirement(
                    key = 1,
                    item = greatshield,
                    upgrade = 2,
                    effect = EffectFilter.OneOf(listOf("Blocking", "Projecting", "Vampiric")),
                ),
            ),
        )
        assertEquals(
            "any glyph · uncursed",
            requirementDetailLine(
                ItemRequirement(
                    key = 2,
                    item = null,
                    kind = ItemKind.ARMOR,
                    upgrade = 0,
                    upgradeMatch = UpgradeMatch.ANY,
                    effect = EffectFilter.AnyEnchantment,
                    requireUncursed = true,
                ),
            ),
        )
        val might = ItemRequirement(
            key = 3,
            item = ItemCatalog.rings.first { it.id == "ring_might" },
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            identityGroup = 1,
            upgradeSum = UpgradeSum(group = 1, atLeast = 4),
            maximumDepth = 4,
        )
        assertEquals("grp A · sum A ≥ +4 · ≤ floor 4", requirementDetailLine(might))
        assertEquals(
            "Any upgrade • same item group A • combined upgrade group A ≥ +4 • by floor 4",
            might.description,
        )
        assertEquals("any of 3", alternativesSummary(3))
        assertEquals("1 of 2 requirements", scoutMatchText(1, 2))
        assertEquals("1 of 1 requirement", scoutMatchText(1, 1))
    }

    @Test
    fun scopeSummaryListsOnlyActiveConstraints() {
        assertEquals(
            "≤ floor 24",
            scopeSummaryText(24, requireBlacksmith = false, excludeBlacksmithRewards = false, fastMode = false, challenges = 0),
        )
        assertEquals(
            "≤ floor 12 · smith · no smith rewards · fast · 2 challenges",
            scopeSummaryText(12, requireBlacksmith = true, excludeBlacksmithRewards = true, fastMode = true, challenges = 0b101),
        )
        assertEquals(
            "≤ floor 1 · 1 challenge",
            scopeSummaryText(1, requireBlacksmith = false, excludeBlacksmithRewards = false, fastMode = false, challenges = 16),
        )
        assertEquals(
            "≤ floor 9 · corpse dust",
            scopeSummaryText(
                9,
                requireBlacksmith = false,
                excludeBlacksmithRewards = false,
                wandmakerQuest = WandmakerQuest.CORPSE_DUST,
                fastMode = false,
                challenges = 0,
            ),
        )
    }
}
