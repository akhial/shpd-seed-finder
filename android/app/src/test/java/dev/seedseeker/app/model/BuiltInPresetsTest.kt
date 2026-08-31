// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class BuiltInPresetsTest {
    @Test
    fun staffPresetMatchesRequestedRequirements() {
        val requirements = BuiltInPresets.staff21.query.requirements

        assertEquals(4, requirements.size)
        assertTrue(requirements.all { it.kind == ItemKind.WAND })
        assertEquals(listOf(UpgradeMatch.EXACT, UpgradeMatch.ANY, UpgradeMatch.ANY, UpgradeMatch.AT_LEAST), requirements.map { it.upgradeMatch })
        assertEquals(listOf(3, 0, 0, 1), requirements.map { it.upgrade })
        assertEquals(listOf(1, 1, 1, null), requirements.map { it.identityGroup })
    }

    @Test
    fun staff22PresetAsksForTheVaultWand() {
        val preset = BuiltInPresets.staff22
        val requirements = preset.query.requirements

        assertEquals(19, preset.query.maximumDepth)
        assertEquals(4, requirements.size)
        assertTrue(requirements.all { it.kind == ItemKind.WAND })
        assertEquals(listOf(UpgradeMatch.EXACT, UpgradeMatch.ANY, UpgradeMatch.ANY, UpgradeMatch.AT_LEAST), requirements.map { it.upgradeMatch })
        assertEquals(listOf(4, 0, 0, 1), requirements.map { it.upgrade })
        assertEquals(listOf(1, 1, 1, null), requirements.map { it.identityGroup })
    }

    @Test
    fun tier4WeaponPresetStacksThreeCopiesOnAPlusFive() {
        val preset = BuiltInPresets.tier4Weapon26
        val requirements = preset.query.requirements

        assertEquals(19, preset.query.maximumDepth)
        assertEquals(3, requirements.size)
        assertTrue(requirements.all { it.kind == ItemKind.WEAPON && it.identityGroup == 1 })
        assertEquals(listOf(UpgradeMatch.EXACT, UpgradeMatch.ANY, UpgradeMatch.ANY), requirements.map { it.upgradeMatch })
        assertEquals(listOf(5, 0, 0), requirements.map { it.upgrade })
        assertEquals(listOf(TierMatch.EXACT, TierMatch.ANY, TierMatch.ANY), requirements.map { it.tierMatch })
        assertEquals(listOf(4, 0, 0), requirements.map { it.tier })
    }

    @Test
    fun wandBonanzaPresetMatchesRequestedRequirements() {
        val requirements = BuiltInPresets.wandBonanza.query.requirements

        assertEquals(4, requirements.size)
        assertTrue(requirements.all { it.kind == ItemKind.WAND && it.item == null })
        assertEquals(listOf(UpgradeMatch.EXACT, UpgradeMatch.EXACT, UpgradeMatch.EXACT, UpgradeMatch.EXACT), requirements.map { it.upgradeMatch })
        assertEquals(listOf(3, 2, 2, 2), requirements.map { it.upgrade })
        assertEquals(listOf(null, 4, 4, null), requirements.map { it.maximumDepth })
        assertEquals(listOf(null, null, null, null), requirements.map { it.identityGroup })
    }

    @Test
    fun ringOfWealthPresetMatchesRequestedRequirements() {
        val requirements = BuiltInPresets.ringOfWealth21.query.requirements

        assertEquals(listOf("ring_wealth", "ring_wealth", "ring_wealth"), requirements.map { it.item?.id })
        assertEquals(listOf(UpgradeMatch.EXACT, UpgradeMatch.EXACT, UpgradeMatch.ANY), requirements.map { it.upgradeMatch })
        assertEquals(listOf(4, 2, 0), requirements.map { it.upgrade })
        assertEquals(listOf(null, null, null), requirements.map { it.maximumDepth })
        assertEquals(ScoutItemSource.IMP_REWARD, requirements.first().source)
    }

    /**
     * Only one member of a stack may constrain which item it binds to, so the
     * extra copies stay bare.
     */
    @Test
    fun everyStackCarriesExactlyOneAnchor() {
        for (preset in BuiltInPresets.all) {
            val stacks = preset.query.requirements.filter { it.identityGroup != null }.groupBy { it.identityGroup }
            for ((group, members) in stacks) {
                val anchors = members.count {
                    it.upgradeMatch != UpgradeMatch.ANY || it.tierMatch != TierMatch.ANY ||
                        it.item != null || it.source != null
                }
                assertEquals("${preset.name} stack $group", 1, anchors)
            }
        }
    }
}
