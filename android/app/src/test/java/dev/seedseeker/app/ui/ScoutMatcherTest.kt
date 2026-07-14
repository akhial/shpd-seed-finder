// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.model.CatalogItem
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutAccessibility
import dev.seedseeker.app.model.ScoutItem
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.UpgradeMatch
import org.junit.Assert.assertEquals
import org.junit.Test

class ScoutMatcherTest {
    private val warding = CatalogItem("wand_warding", "Wand of Warding", ItemKind.WAND, 218)
    private val light = CatalogItem("wand_prismatic_light", "Wand of Prismatic Light", ItemKind.WAND, 213)

    @Test
    fun selectsOnlyOneMutuallyExclusiveReward() {
        val requirement = ItemRequirement(
            key = 1,
            item = null,
            kind = ItemKind.WAND,
            upgrade = 3,
            upgradeMatch = UpgradeMatch.EXACT,
            source = ScoutItemSource.WANDMAKER_REWARD,
        )
        val items = listOf(
            scoutItem(warding, ScoutAccessibility.Choice(2, 0)),
            scoutItem(light, ScoutAccessibility.Choice(2, 1)),
        )

        assertEquals(setOf(0), scoutMatchIndices(items, listOf(requirement)))
    }

    @Test
    fun intersectsScenarioMasksAcrossDistinctRequirements() {
        val requirements = listOf(warding, light).mapIndexed { index, item ->
            ItemRequirement(
                key = index.toLong(),
                item = item,
                kind = ItemKind.WAND,
                upgrade = 3,
                upgradeMatch = UpgradeMatch.EXACT,
            )
        }
        val compatible = listOf(
            scoutItem(warding, ScoutAccessibility.Scenarios(4, 0b11UL)),
            scoutItem(light, ScoutAccessibility.Scenarios(4, 0b10UL)),
        )
        val incompatible = listOf(
            compatible[0],
            scoutItem(light, ScoutAccessibility.Scenarios(4, 0b100UL)),
        )

        assertEquals(setOf(0, 1), scoutMatchIndices(compatible, requirements))
        assertEquals(1, scoutMatchIndices(incompatible, requirements).size)
    }

    @Test
    fun uncursedRequirementRejectsCursedCopies() {
        val requirement = ItemRequirement(
            key = 1,
            item = warding,
            upgrade = 3,
            requireUncursed = true,
        )
        val clean = scoutItem(warding, ScoutAccessibility.Independent)
        val cursed = clean.copy(cursed = true)

        assertEquals(setOf(0), scoutMatchIndices(listOf(clean, cursed), listOf(requirement)))
        assertEquals(emptySet<Int>(), scoutMatchIndices(listOf(cursed), listOf(requirement)))
    }

    private fun scoutItem(item: CatalogItem, accessibility: ScoutAccessibility) = ScoutItem(
        item = item,
        depth = 8,
        upgrade = 3,
        effect = null,
        cursed = false,
        source = ScoutItemSource.WANDMAKER_REWARD,
        accessibility = accessibility,
    )
}
