// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.Challenge
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.SeedResult
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Search is the only entry point: refining happens automatically whenever the
 * pending query merely narrows the last finished run.
 */
class RefinePlanTest {
    private val frost = ItemRequirement(1, ItemCatalog.wands.first { it.id == "wand_frost" }, 2)
    private val fireblast =
        ItemRequirement(2, ItemCatalog.wands.first { it.id == "wand_fireblast" }, 3)

    private val seeds = listOf(SeedResult("AAA-AAA-AAA", 1), SeedResult("BBB-BBB-BBB", 1))
    private val base = FinishedRun(request(frost), resumeFrom = 4_096, remaining = 512, results = seeds)

    @Test
    fun aNarrowedQueryRefinesAutomatically() {
        val plan = refinePlanFor(request(frost, fireblast), base)
        assertEquals(RefineSpec(4_096, 512, seeds), plan)
    }

    @Test
    fun aFullyScannedBaseStillRefinesWithNothingLeftToResume() {
        val exhausted = base.copy(remaining = 0)
        assertEquals(RefineSpec(4_096, 0, seeds), refinePlanFor(request(frost, fireblast), exhausted))
    }

    @Test
    fun anIneligibleQueryRunsFresh() {
        // Same query, a widened one, an edited requirement, and a scope change.
        assertNull(refinePlanFor(request(frost), base))
        assertNull(refinePlanFor(request(fireblast), base))
        assertNull(refinePlanFor(request(frost.copy(upgrade = 3), fireblast), base))
        assertNull(refinePlanFor(request(frost, fireblast, maximumDepth = 12), base))
        assertNull(refinePlanFor(request(frost, fireblast, challenges = Challenge.DARKNESS.bit), base))
    }

    @Test
    fun withoutABaseEvenANarrowedQueryRunsFresh() {
        // No base covers a first search, imported results, a failed run, and a
        // results list the user cleared.
        assertNull(refinePlanFor(request(frost, fireblast), null))
    }

    private fun request(
        vararg requirements: ItemRequirement,
        maximumDepth: Int = 24,
        challenges: Int = 0,
    ) = SearchRequest(
        requirements = requirements.toList(),
        maximumDepth = maximumDepth,
        challenges = challenges,
    )
}
