// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.model.SearchState
import dev.seedseeker.app.model.SearchStatus
import org.junit.Assert.assertEquals
import org.junit.Test

class SearchEstimateTextTest {
    @Test
    fun formatsObservedProbabilityRateAndTimeToNextSeed() {
        val status = SearchStatus(
            state = SearchState.RUNNING,
            scannedSeeds = 10_000_000,
            totalSeeds = 5_429_503_678_976,
            matchProbability = 13.0 / 10_000_000.0,
        )

        assertEquals(
            "p 1.3x10^-4% · est 2.8 minutes",
            searchEstimateText(status, 4_600.0),
        )
    }

    @Test
    fun estimatesUntilTheFirstMatch() {
        val status = SearchStatus(SearchState.RUNNING, 50_000, 5_429_503_678_976)

        assertEquals(
            "p estimating… · est —",
            searchEstimateText(status, 4_600.0),
        )
    }

    @Test
    fun formatsElapsedTime() {
        assertEquals("30s", formatElapsedTime(30))
        assertEquals("2m 5s", formatElapsedTime(125))
        assertEquals("1h 2m", formatElapsedTime(3_725))
    }
}
