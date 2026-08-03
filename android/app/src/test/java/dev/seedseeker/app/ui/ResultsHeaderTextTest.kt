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
    fun refineFilterPhaseSaysRefining() {
        // The kept-of counts live in a snackbar now, so the header only carries the phase.
        assertEquals(
            "Results — 12 · refining",
            header(12, state = null, isSearching = true, refinePhase = RefinePhase.FILTERING),
        )
    }

    @Test
    fun onlyTheFilterPhaseSaysRefining() {
        // Once the kept seeds are settled the resumed scan is an ordinary search,
        // so the header stops claiming to be refining.
        assertEquals(
            "Results — 39 · searching",
            header(
                39,
                state = SearchState.RUNNING,
                isSearching = true,
                refinePhase = RefinePhase.SCANNING,
            ),
        )
    }

    private fun header(
        resultCount: Int,
        state: SearchState?,
        isSearching: Boolean = false,
        refinePhase: RefinePhase? = null,
    ) = resultsHeaderText(resultCount, state, isSearching, refinePhase)
}
