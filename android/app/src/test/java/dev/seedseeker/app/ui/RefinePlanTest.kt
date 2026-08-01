// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.Challenge
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.SeedResult
import dev.seedseeker.app.model.UpgradeMatch
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Search is the only entry point: docs/search-semantics.md decides what it does. A query
 * continuing the Target Query refines the full Target Set and resumes its coverage, one sharing
 * an item filters that set, and anything else scans detached without touching the Target.
 */
class RefinePlanTest {
    private val frost = ItemRequirement(1, ItemCatalog.wands.first { it.id == "wand_frost" }, 2)
    private val fireblast =
        ItemRequirement(2, ItemCatalog.wands.first { it.id == "wand_fireblast" }, 3)
    private val ring = ItemRequirement(3, ItemCatalog.rings.first(), 1)
    private val anyWand =
        ItemRequirement(4, null, 0, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.ANY)

    private val seeds = listOf(SeedResult("AAA-AAA-AAA", 1), SeedResult("BBB-BBB-BBB", 1))
    private val target =
        TargetState(request(frost), results = seeds, resumeFrom = 4_096, remaining = 512)

    @Test
    fun withoutATargetEverySearchAnchors() {
        assertEquals(StartPlan(StartMode.ANCHOR), startPlanFor(request(frost), null, null, null))
    }

    @Test
    fun aContinuingQueryRefinesTheTarget() {
        // Narrowed, unchanged, and re-keyed queries all continue the Target Query; the plan
        // filters the full Target Set and resumes the target's own coverage.
        val expected =
            StartPlan(StartMode.TARGET_REFINE, RefineSpec(4_096, 512, seeds))
        assertEquals(expected, startPlanFor(request(frost, fireblast), target, null, null))
        assertEquals(expected, startPlanFor(request(frost), target, null, null))
        assertEquals(expected, startPlanFor(request(frost.copy(key = 41)), target, null, null))
    }

    @Test
    fun theRefineBaseIsTheTargetSetNotTheLastRunsSurvivors() {
        // After a narrowing filter dropped seeds from the display, loosening back to a query
        // that shares the frost requirement still filters the full Target Set.
        val narrowedRun = FinishedRun(request(frost, fireblast), 8_192, 256, seeds.take(1))
        val plan = startPlanFor(
            request(frost, maximumDepth = 12), target, narrowedRun, StartMode.TARGET_REFINE,
        )
        assertEquals(StartPlan(StartMode.TARGET_FILTER, RefineSpec(4_096, 0, seeds)), plan)
    }

    @Test
    fun aQuerySharingAnItemFiltersWithoutScanning() {
        // A scope change breaks continuation but keeps the shared requirement; the plan
        // filters the Target Set with nothing left to resume.
        val scopeChanged = startPlanFor(
            request(frost, challenges = Challenge.DARKNESS.bit), target, null, null,
        )
        assertEquals(StartPlan(StartMode.TARGET_FILTER, RefineSpec(4_096, 0, seeds)), scopeChanged)
        // A kind-level wildcard subsumes every item of its kind.
        val wildcard = startPlanFor(request(anyWand, maximumDepth = 12), target, null, null)
        assertEquals(StartMode.TARGET_FILTER, wildcard.mode)
    }

    @Test
    fun anUnrelatedQueryScansDetached() {
        val plan = startPlanFor(request(ring), target, null, null)
        assertEquals(StartPlan(StartMode.DETACHED), plan)
        // A different item of the Target's kind is just as unrelated.
        assertEquals(
            StartPlan(StartMode.DETACHED),
            startPlanFor(request(fireblast), TargetState(request(frost), seeds, 0, 0), null, null),
        )
    }

    @Test
    fun aQueryContinuingTheLastDetachedScanContinuesIt() {
        val detachedRun = FinishedRun(request(ring), resumeFrom = 2_048, remaining = 128, results = seeds.take(1))
        val plan = startPlanFor(request(ring), target, detachedRun, StartMode.DETACHED)
        assertEquals(
            StartPlan(StartMode.CONTINUE_DETACHED, RefineSpec(2_048, 128, seeds.take(1))),
            plan,
        )
        // A continued detached scan is remembered as detached, so a further
        // continuation keeps threading onto the same scan.
        assertEquals(StartMode.DETACHED, StartMode.CONTINUE_DETACHED.concludedKind)
        // Without a detached predecessor the same query rescans from scratch.
        assertEquals(
            StartPlan(StartMode.DETACHED),
            startPlanFor(request(ring), target, detachedRun, StartMode.TARGET_FILTER),
        )
        assertEquals(
            StartPlan(StartMode.DETACHED),
            startPlanFor(request(ring), target, null, null),
        )
    }

    @Test
    fun anEmptyTargetSetOnlyResumesItsOwnContinuation() {
        val empty = target.copy(results = emptyList())
        // A continuing query with coverage left still resumes the target scan.
        assertEquals(
            StartPlan(StartMode.TARGET_REFINE, RefineSpec(4_096, 512, emptyList())),
            startPlanFor(request(frost, fireblast), empty, null, null),
        )
        // With nothing left to scan — or for any other query — the search re-anchors.
        assertEquals(
            StartPlan(StartMode.ANCHOR),
            startPlanFor(request(frost), empty.copy(remaining = 0), null, null),
        )
        assertEquals(StartPlan(StartMode.ANCHOR), startPlanFor(request(anyWand), empty, null, null))
        assertEquals(StartPlan(StartMode.ANCHOR), startPlanFor(request(ring), empty, null, null))
    }

    @Test
    fun sharingNeedsACommonKindAndACompatibleItem() {
        assertTrue(sharesRequirement(request(frost), request(frost, maximumDepth = 12)))
        assertTrue(sharesRequirement(request(anyWand), request(frost)))
        assertTrue(sharesRequirement(request(frost), request(anyWand)))
        assertTrue(sharesRequirement(request(ring, frost), request(frost)))
        assertFalse(sharesRequirement(request(frost), request(fireblast)))
        assertFalse(sharesRequirement(request(ring), request(frost)))
    }

    @Test
    fun anAnchorRunEstablishesTheTarget() {
        val settled = settledTarget(null, StartMode.ANCHOR, request(frost), seeds, 4_096, 512)
        assertEquals(target, settled)
    }

    @Test
    fun aTargetRefineGrowsTheSetAndAdvancesTheCoverage() {
        val newFind = SeedResult("CCC-CCC-CCC", 2)
        // The run's results are its survivors plus new finds; survivors never duplicate.
        val settled = settledTarget(
            target, StartMode.TARGET_REFINE, request(frost, fireblast),
            listOf(seeds.first(), newFind), resumeFrom = 8_192, remaining = 0,
        )
        assertEquals(
            target.copy(results = seeds + newFind, resumeFrom = 8_192, remaining = 0),
            settled,
        )
        // The Target Query itself never changes — the refined request is not adopted.
        assertEquals(request(frost), settled?.request)
    }

    @Test
    fun filtersAndDetachedRunsLeaveTheTargetUntouched() {
        for (mode in listOf(StartMode.TARGET_FILTER, StartMode.CONTINUE_DETACHED, StartMode.DETACHED)) {
            assertSame(target, settledTarget(target, mode, request(ring), seeds.take(1), 1, 1))
        }
        assertNull(settledTarget(null, StartMode.DETACHED, request(ring), seeds, 1, 1))
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
