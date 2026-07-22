// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.update

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class UpdateCheckerTest {
    private val url = UpdateChecker.RELEASES_PAGE

    @Test
    fun newerVersionIsReported() {
        assertEquals(UpdateInfo("0.6.0", url), UpdateChecker.newer("v0.6.0", "0.5.2", url))
        assertEquals("1.0.0", UpdateChecker.newer("v1.0.0", "0.5.2", url)?.version)
        assertEquals("0.5.10", UpdateChecker.newer("0.5.10", "0.5.2", url)?.version)
        assertEquals("0.6", UpdateChecker.newer("v0.6", "0.5.2", url)?.version)
    }

    @Test
    fun sameOrOlderVersionIsIgnored() {
        assertNull(UpdateChecker.newer("v0.5.2", "0.5.2", url))
        assertNull(UpdateChecker.newer("v0.5.1", "0.5.2", url))
        assertNull(UpdateChecker.newer("v0.4.9", "0.5.2", url))
        assertNull(UpdateChecker.newer("v0.5", "0.5.0", url))
    }

    @Test
    fun suffixesAndPrefixesAreStripped() {
        // The debug build's versionName carries a "-demo" suffix.
        assertEquals("0.6.0", UpdateChecker.newer("v0.6.0-rc1", "0.5.2-demo", url)?.version)
        assertEquals("1.2.3", UpdateChecker.displayVersion("v1.2.3-beta"))
        assertEquals("2.0.0", UpdateChecker.displayVersion(" V2.0.0 "))
    }

    @Test
    fun garbageIsIgnored() {
        assertNull(UpdateChecker.newer("nightly", "0.5.2", url))
        assertNull(UpdateChecker.newer("v9.9.9", "unknown", url))
        assertNull(UpdateChecker.newer("", "0.5.2", url))
    }
}
