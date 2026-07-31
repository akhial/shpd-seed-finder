// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ScoutResultNavigationTest {
    private val seeds = listOf("AAA-AAA-AAA", "BBB-BBB-BBB", "CCC-CCC-CCC")

    @Test
    fun `position locates a scouted seed inside the results`() {
        assertEquals(0, ScoutResultNavigation.position(seeds, "AAA-AAA-AAA"))
        assertEquals(2, ScoutResultNavigation.position(seeds, "CCC-CCC-CCC"))
    }

    @Test
    fun `position is null outside the results`() {
        assertNull(ScoutResultNavigation.position(seeds, "ZZZ-ZZZ-ZZZ"))
        assertNull(ScoutResultNavigation.position(seeds, null))
        assertNull(ScoutResultNavigation.position(seeds, ""))
        assertNull(ScoutResultNavigation.position(emptyList(), "AAA-AAA-AAA"))
    }

    @Test
    fun `step moves forward and backward`() {
        assertEquals("BBB-BBB-BBB", ScoutResultNavigation.step(seeds, "AAA-AAA-AAA", 1))
        assertEquals("CCC-CCC-CCC", ScoutResultNavigation.step(seeds, "BBB-BBB-BBB", 1))
        assertEquals("BBB-BBB-BBB", ScoutResultNavigation.step(seeds, "CCC-CCC-CCC", -1))
    }

    @Test
    fun `step does not wrap past the ends`() {
        assertNull(ScoutResultNavigation.step(seeds, "AAA-AAA-AAA", -1))
        assertNull(ScoutResultNavigation.step(seeds, "CCC-CCC-CCC", 1))
    }

    @Test
    fun `step clamps larger jumps to the list ends`() {
        assertEquals("CCC-CCC-CCC", ScoutResultNavigation.step(seeds, "BBB-BBB-BBB", 5))
        assertEquals("AAA-AAA-AAA", ScoutResultNavigation.step(seeds, "BBB-BBB-BBB", -5))
    }

    @Test
    fun `step is inert without an anchor in the results`() {
        assertNull(ScoutResultNavigation.step(seeds, "ZZZ-ZZZ-ZZZ", 1))
        assertNull(ScoutResultNavigation.step(seeds, null, 1))
        assertNull(ScoutResultNavigation.step(emptyList(), "AAA-AAA-AAA", 1))
        assertNull(ScoutResultNavigation.step(listOf("AAA-AAA-AAA"), "AAA-AAA-AAA", 1))
    }
}
