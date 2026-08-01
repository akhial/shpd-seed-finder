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
            "Results — 7 · kept 5 of 12",
            header(7, state = SearchState.COMPLETED, refineSummary = 5 to 12),
        )
        assertEquals(
            "Results — 6 · kept 5 of 12 · cancelled",
            header(6, state = SearchState.CANCELLED, refineSummary = 5 to 12),
        )
    }

    @Test
    fun refineFilterPhaseHasNoCountsYet() {
        // While the previous seeds are being re-verified there is no summary yet.
        assertEquals(
            "Results — 12 · refining",
            header(12, state = null, isSearching = true, refinePhase = RefinePhase.FILTERING),
        )
    }

    @Test
    fun onlyTheFilterPhaseSaysRefining() {
        // Once the kept seeds are settled the resumed scan is an ordinary search,
        // so the header stops claiming to be refining but keeps the kept counts.
        assertEquals(
            "Results — 39 · kept 23 of 329 · searching",
            header(
                39,
                state = SearchState.RUNNING,
                isSearching = true,
                refinePhase = RefinePhase.SCANNING,
                refineSummary = 23 to 329,
            ),
        )
    }

    private fun header(
        resultCount: Int,
        state: SearchState?,
        isSearching: Boolean = false,
        refinePhase: RefinePhase? = null,
        refineSummary: Pair<Int, Int>? = null,
    ) = resultsHeaderText(resultCount, state, isSearching, refinePhase, refineSummary)
}
