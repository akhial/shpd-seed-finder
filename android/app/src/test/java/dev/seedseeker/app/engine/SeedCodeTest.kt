// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

/**
 * Seed-code text handling, asserted against the engine that owns it: these
 * cases call `JniBindings.formatSeedCode`/`parseSeedCode` through the host
 * build of the Rust library (see QueryContinuationTest for how Gradle provides
 * it), so the app can never drift from `seed::format_input` and
 * `DungeonSeed::from_code`.
 */
class SeedCodeTest {
    @Test
    fun valueTreatsSeedsAsNineBase26Letters() {
        assertEquals(0L, SeedCode.value("AAA-AAA-AAA"))
        assertEquals(1L, SeedCode.value("AAA-AAA-AAB"))
        assertEquals(26L, SeedCode.value("AAA-AAA-ABA"))
        assertEquals(5_429_503_678_975L, SeedCode.value("ZZZ-ZZZ-ZZZ"))
    }

    @Test
    fun formatInputMasksAsYouTypeIntoCanonicalGroups() {
        assertEquals("", SeedCode.formatInput(""))
        assertEquals("AB", SeedCode.formatInput("ab"))
        assertEquals("ABC-D", SeedCode.formatInput("abcd"))
        assertEquals("ABC-DEF-GHI", SeedCode.formatInput("abc def ghi jkl"))
        assertEquals("ABC-DEF-GHI", SeedCode.formatInput("a1b2c3-d4e5f6.g7h8i9"))
    }

    @Test
    fun nonAsciiLettersAreDroppedBeforeUppercasing() {
        // The masker filters to ASCII letters and only then uppercases. A
        // Kotlin uppercase() first would turn U+0131 (Turkish dotless i) into
        // "I" and silently type a different seed.
        assertEquals("", SeedCode.formatInput("\u0131"))
        assertEquals("ABC", SeedCode.formatInput("\u0131abc"))
        assertEquals("ABC-DEF-GHI", SeedCode.formatInput("abc-def-gh\u0131i"))
    }

    @Test
    fun valueRejectsNonCanonicalSeeds() {
        assertThrows(IllegalArgumentException::class.java) { SeedCode.value("AAAAAAAAA") }
        assertThrows(IllegalArgumentException::class.java) { SeedCode.value("aaa-aaa-aaa") }
        assertThrows(IllegalArgumentException::class.java) { SeedCode.value("AAA-AAA") }
    }
}
