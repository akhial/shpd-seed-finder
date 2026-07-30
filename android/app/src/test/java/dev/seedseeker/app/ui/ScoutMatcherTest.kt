// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.CatalogItem
import dev.seedseeker.app.model.EffectRequirement
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
    private val spear = ItemCatalog.weapons.first { it.id == "spear" }
    private val sword = ItemCatalog.weapons.first { it.id == "sword" }
    private val ringMight = ItemCatalog.rings.first { it.id == "ring_might" }
    private val ringHaste = ItemCatalog.rings.first { it.id == "ring_haste" }

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

    @Test
    fun anAlternativeGroupIsSatisfiedByOneMemberAndOccupiesOneSlot() {
        fun alternative(key: Long, item: CatalogItem, upgrade: Int) = ItemRequirement(
            key = key,
            item = item,
            upgrade = upgrade,
            upgradeMatch = UpgradeMatch.EXACT,
            alternativeGroup = 1,
        )
        val requirements = listOf(
            alternative(1, spear, 3),
            alternative(2, sword, 1),
        )
        val matchingSword = listOf(worldItem(sword, upgrade = 1))
        assertEquals(setOf(0), scoutMatchIndices(matchingSword, requirements))

        // The whole group is one slot: a second sword requirement outside the
        // group needs a distinct item.
        val swordAndEither = requirements + ItemRequirement(
            key = 3,
            item = sword,
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
        )
        assertEquals(setOf(0), scoutMatchIndices(matchingSword, swordAndEither))
        assertEquals(
            setOf(0, 1),
            scoutMatchIndices(matchingSword + worldItem(sword, upgrade = 0), swordAndEither),
        )
    }

    @Test
    fun combinedUpgradeTotalsAcceptMatchingPairsAndRejectShortOnes() {
        fun ring(key: Long) = ItemRequirement(
            key = key,
            item = ringMight,
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            identityGroup = 1,
            upgradeSumGroup = 1,
            upgradeSumTotal = 2,
        )
        val pair = listOf(ring(1), ring(2))

        val zeroAndTwo = listOf(worldItem(ringMight, 0), worldItem(ringMight, 2))
        assertEquals(setOf(0, 1), scoutMatchIndices(zeroAndTwo, pair))

        val oneAndOne = listOf(worldItem(ringMight, 1), worldItem(ringMight, 1))
        assertEquals(setOf(0, 1), scoutMatchIndices(oneAndOne, pair))

        val zeroAndZero = listOf(worldItem(ringMight, 0), worldItem(ringMight, 0))
        assertEquals(emptySet<Int>(), scoutMatchIndices(zeroAndZero, pair))

        // The identity group still applies: different rings never pair up.
        val mixedRings = listOf(worldItem(ringMight, 1), worldItem(ringHaste, 1))
        assertEquals(emptySet<Int>(), scoutMatchIndices(mixedRings, pair))
    }

    @Test
    fun aFailedSumPairIsNotHighlightedWhileAnIndependentMatchStillIs() {
        fun ring(key: Long) = ItemRequirement(
            key = key,
            item = ringMight,
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            identityGroup = 1,
            upgradeSumGroup = 1,
            upgradeSumTotal = 4,
        )
        val requirements = listOf(
            ring(1),
            ring(2),
            ItemRequirement(key = 3, item = sword, upgrade = 0, upgradeMatch = UpgradeMatch.ANY),
        )
        val items = listOf(
            worldItem(ringMight, 0),
            worldItem(sword, 1),
            worldItem(ringMight, 1),
        )

        assertEquals(setOf(1), scoutMatchIndices(items, requirements))
    }

    @Test
    fun effectPredicatesMatchOneOfSetsAndAnyEnchantment() {
        val oneOf = ItemRequirement(
            key = 1,
            item = sword,
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            effect = EffectRequirement.OneOf(listOf("Blocking", "Vampiric")),
        )
        val blocking = worldItem(sword, 0).copy(effect = "Blocking")
        val blazing = worldItem(sword, 0).copy(effect = "Blazing")
        val plain = worldItem(sword, 0)
        assertEquals(setOf(0), scoutMatchIndices(listOf(blocking), listOf(oneOf)))
        assertEquals(emptySet<Int>(), scoutMatchIndices(listOf(blazing, plain), listOf(oneOf)))

        val anyEnchantment = oneOf.copy(effect = EffectRequirement.AnyEnchantment)
        assertEquals(setOf(0), scoutMatchIndices(listOf(blazing), listOf(anyEnchantment)))
        val cursed = worldItem(sword, 0).copy(effect = "Annoying", cursed = true)
        assertEquals(emptySet<Int>(), scoutMatchIndices(listOf(plain, cursed), listOf(anyEnchantment)))
    }

    private fun worldItem(item: CatalogItem, upgrade: Int) = ScoutItem(
        item = item,
        depth = 3,
        upgrade = upgrade,
        effect = null,
        cursed = false,
        source = ScoutItemSource.HEAP,
        accessibility = ScoutAccessibility.Independent,
    )

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
