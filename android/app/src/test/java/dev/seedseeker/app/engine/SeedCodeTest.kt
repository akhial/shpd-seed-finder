// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class SeedCodeTest {
    @Test
    fun valueTreatsSeedsAsNineBase26Letters() {
        assertEquals(0L, SeedCode.value("AAA-AAA-AAA"))
        assertEquals(1L, SeedCode.value("AAA-AAA-AAB"))
        assertEquals(26L, SeedCode.value("AAA-AAA-ABA"))
        assertEquals(5_429_503_678_975L, SeedCode.value("ZZZ-ZZZ-ZZZ"))
    }

    @Test
    fun valueRejectsNonCanonicalSeeds() {
        assertThrows(IllegalArgumentException::class.java) { SeedCode.value("AAAAAAAAA") }
        assertThrows(IllegalArgumentException::class.java) { SeedCode.value("aaa-aaa-aaa") }
        assertThrows(IllegalArgumentException::class.java) { SeedCode.value("AAA-AAA") }
    }
}
