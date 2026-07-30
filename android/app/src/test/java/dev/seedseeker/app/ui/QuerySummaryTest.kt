// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.EffectRequirement
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.TierMatch
import dev.seedseeker.app.model.UpgradeMatch
import org.junit.Assert.assertEquals
import org.junit.Test

class QuerySummaryTest {
    @Test
    fun detailLineCondensesAnExactItemRequirement() {
        val requirement = ItemRequirement(
            key = 1,
            item = ItemCatalog.weapons.first { it.id == "sword" },
            upgrade = 2,
            effect = EffectRequirement.OneOf(listOf("Lucky")),
            maximumDepth = 12,
        )

        assertEquals("+2 · Lucky · ≤ floor 12", requirementDetailLine(requirement))
    }

    @Test
    fun detailLineJoinsSmallEffectSetsWithOr() {
        val requirement = ItemRequirement(
            key = 1,
            item = ItemCatalog.weapons.first { it.id == "greatshield" },
            upgrade = 2,
            effect = EffectRequirement.OneOf(listOf("Blocking", "Projecting", "Vampiric")),
        )

        assertEquals("+2 · Blocking, Projecting or Vampiric", requirementDetailLine(requirement))
    }

    @Test
    fun detailLineCountsLargeEffectSets() {
        val requirement = ItemRequirement(
            key = 1,
            item = ItemCatalog.weapons.first { it.id == "sword" },
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            effect = EffectRequirement.OneOf(ItemCatalog.enchantments.take(5)),
        )

        assertEquals("any of 5 enchantments", requirementDetailLine(requirement))
    }

    @Test
    fun detailLineNamesAnyEnchantmentAndTheEquivalentFullSet() {
        val anyEnchantment = ItemRequirement(
            key = 1,
            item = ItemCatalog.weapons.first { it.id == "sword" },
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            effect = EffectRequirement.AnyEnchantment,
        )
        assertEquals("any enchantment", requirementDetailLine(anyEnchantment))

        val fullSet = anyEnchantment.copy(effect = EffectRequirement.OneOf(ItemCatalog.enchantments))
        assertEquals("any enchantment", requirementDetailLine(fullSet))

        val anyGlyph = ItemRequirement(
            key = 2,
            item = ItemCatalog.armor.first { it.id == "plate_armor" },
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            effect = EffectRequirement.AnyEnchantment,
        )
        assertEquals("any glyph", requirementDetailLine(anyGlyph))
    }

    @Test
    fun detailLineShowsTheCombinedUpgradeTotal() {
        val requirement = ItemRequirement(
            key = 1,
            item = ItemCatalog.rings.first { it.id == "ring_might" },
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            identityGroup = 1,
            upgradeSumGroup = 1,
            upgradeSumTotal = 2,
        )

        assertEquals("grp A · combined +2 total (group A)", requirementDetailLine(requirement))
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
    }
}
