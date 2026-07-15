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
}
