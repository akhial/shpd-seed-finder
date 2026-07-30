// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FloorLimitsTest {
    @Test
    fun floorLimitOptionsSkipEmptyBossFloors() {
        assertEquals(21, FLOOR_LIMIT_OPTIONS.size)
        assertFalse(FLOOR_LIMIT_OPTIONS.any { it in EMPTY_BOSS_FLOORS })
        assertTrue(FLOOR_LIMIT_OPTIONS.contains(20))
        assertEquals(1, FLOOR_LIMIT_OPTIONS.first())
        assertEquals(24, FLOOR_LIMIT_OPTIONS.last())
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
