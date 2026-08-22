// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The floor-limit selector and the challenge badges over the constants the
 * engine publishes through `engineInfo` — the empty boss floors, the search
 * depth limit and which challenges the generator consults — rather than over
 * Kotlin copies of them.
 */
class FloorLimitsTest {
    @Test
    fun theEngineNamesTheEmptyBossFloorsAndTheGeneratingChallenges() {
        assertEquals(setOf(5, 10, 15), EMPTY_BOSS_FLOORS)
        assertEquals(
            listOf(Challenge.NO_HERBALISM, Challenge.DARKNESS, Challenge.NO_SCROLLS),
            Challenge.entries.filter { it.changesLevelGeneration },
        )
    }

    @Test
    fun floorLimitOptionsSkipEmptyBossFloors() {
        assertEquals(21, FLOOR_LIMIT_OPTIONS.size)
        assertFalse(FLOOR_LIMIT_OPTIONS.any { it in EMPTY_BOSS_FLOORS })
        assertTrue(FLOOR_LIMIT_OPTIONS.contains(20))
        assertEquals(1, FLOOR_LIMIT_OPTIONS.first())
        assertEquals(24, FLOOR_LIMIT_OPTIONS.last())
    }

    @Test
    fun floorLimitIndexMapsFloorsAndSnapsOffListValuesBelow() {
        assertEquals(0, floorLimitIndex(1))
        assertEquals(3, floorLimitIndex(4))
        // Empty boss floors map to the index of the equivalent floor below.
        assertEquals(3, floorLimitIndex(5))
        assertEquals(7, floorLimitIndex(10))
        assertEquals(11, floorLimitIndex(15))
        assertEquals(16, floorLimitIndex(20))
        assertEquals(20, floorLimitIndex(24))
        // Out-of-range values snap to the nearest option below, never the first slot.
        assertEquals(20, floorLimitIndex(30))
        assertEquals(0, floorLimitIndex(0))
        // Round-trips: every selectable floor maps to its own slot.
        FLOOR_LIMIT_OPTIONS.forEachIndexed { index, floor ->
            assertEquals(index, floorLimitIndex(floor))
        }
    }

    @Test
    fun normalizeSnapsEmptyBossFloorsToTheFloorBelow() {
        assertEquals(4, normalizeFloorLimit(5))
        assertEquals(9, normalizeFloorLimit(10))
        assertEquals(14, normalizeFloorLimit(15))
        assertEquals(4, normalizeFloorLimit(4))
        assertEquals(20, normalizeFloorLimit(20))
        assertEquals(24, normalizeFloorLimit(24))
    }
}
