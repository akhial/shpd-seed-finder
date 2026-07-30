// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.model.SearchState
import org.junit.Assert.assertEquals
import org.junit.Test

class ResultsHeaderTextTest {
    @Test
    fun plainSearchLifecycle() {
        assertEquals("Results", header(0, state = null))
        assertEquals("Results — 3 · live", header(3, state = SearchState.RUNNING, isSearching = true))
        assertEquals("Results — 3 found", header(3, state = SearchState.COMPLETED))
        assertEquals("Results — 3 · cancelled", header(3, state = SearchState.CANCELLED))
    }

    @Test
    fun refineShowsKeptOfPreviousCounts() {
        assertEquals(
            "Results — 5 · kept 5 of 12 · refining",
            header(5, state = SearchState.RUNNING, isSearching = true, isRefined = true, refineSummary = 5 to 12),
        )
        assertEquals(
            "Results — 7 · kept 5 of 12",
            header(7, state = SearchState.COMPLETED, isRefined = true, refineSummary = 5 to 12),
        )
        assertEquals(
            "Results — 6 · kept 5 of 12 · cancelled",
            header(6, state = SearchState.CANCELLED, isRefined = true, refineSummary = 5 to 12),
        )
    }

    @Test
    fun refineFilterPhaseHasNoCountsYet() {
        // While the previous seeds are being re-verified there is no summary yet.
        assertEquals(
            "Results — 12 · refining",
            header(12, state = null, isSearching = true, isRefined = true),
        )
    }

    private fun header(
        resultCount: Int,
        state: SearchState?,
        isSearching: Boolean = false,
        isRefined: Boolean = false,
        refineSummary: Pair<Int, Int>? = null,
    ) = resultsHeaderText(resultCount, state, isSearching, isRefined, refineSummary)
}
