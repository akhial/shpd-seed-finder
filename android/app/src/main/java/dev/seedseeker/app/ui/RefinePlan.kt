// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.SeedResult

/**
 * How many rows the displayed results list holds at most. A run's full collection — filter
 * survivors plus scanned finds — is uncapped and is what feeds the Target Set and any later
 * refine's filter base; only the LazyColumn's rows stop here (an uncapped list is what a
 * several-thousand-row UI hang is made of).
 */
internal const val RESULT_CAP = 1_024

/** The displayed slice of a run's collected results: discovery order, at most [RESULT_CAP] rows. */
internal fun displayedResults(collected: List<SeedResult>): List<SeedResult> =
    if (collected.size <= RESULT_CAP) collected else collected.subList(0, RESULT_CAP)

/**
 * Which half of an in-flight refine run is executing. Only [FILTERING] is "refining" to the user;
 * once the kept seeds have been re-verified the run is an ordinary search over the resumed window.
 */
enum class RefinePhase {
    /** Re-verifying the previous run's seeds against the new query. */
    FILTERING,

    /** Scanning the seeds the base run never reached. */
    SCANNING,
}

/** Resume window and previously shown seeds a refine run starts from. */
internal data class RefineSpec(
    val resumeFrom: Long,
    val remaining: Long,
    val keepSeeds: List<SeedResult>,
)

/**
 * A finished (completed or cancelled) run that a follow-up query may refine or continue.
 * [results] is the run's full collected set — never the [RESULT_CAP]-row display slice — so a
 * detached continuation's filter base keeps the finds the screen had no room for.
 */
internal data class FinishedRun(
    val request: SearchRequest,
    val resumeFrom: Long,
    val remaining: Long,
    val results: List<SeedResult>,
)

/**
 * The session's anchor, per docs/search-semantics.md: the first concluded (completed or
 * cancelled) search — or an import — establishes it, and only Clear discards it. [results] is
 * every seed the Target Query's traversal has delivered, uncapped ([RESULT_CAP] limits only the
 * displayed list); refines always filter this full set, never the last run's survivors, which is
 * what lets a loosened query bring seeds back. [remaining] is zero for imports, which carry no
 * coverage.
 */
internal data class TargetState(
    val request: SearchRequest,
    val results: List<SeedResult>,
    val resumeFrom: Long,
    val remaining: Long,
)

/** What pressing Search does with a query, per docs/search-semantics.md. */
internal enum class StartMode {
    /** Fresh full-range scan that establishes the Target on conclusion. */
    ANCHOR,

    /** Filter the Target Set, then resume the target's uncovered remainder. */
    TARGET_REFINE,

    /** Filter the Target Set only; the set and its coverage stay untouched. */
    TARGET_FILTER,

    /** Continue the previous detached scan (filter its results, resume its remainder). */
    CONTINUE_DETACHED,

    /** Fresh full-range scan that leaves the Target untouched. */
    DETACHED,
}

/** How a concluded run is remembered for the next start decision: a continued detached scan
 * stays detached, so a further continuation keeps threading onto the same scan. */
internal val StartMode.concludedKind: StartMode
    get() = if (this == StartMode.CONTINUE_DETACHED) StartMode.DETACHED else this

/** The chosen mode plus the filter-and-resume window it starts from, when it has one. */
internal data class StartPlan(val mode: StartMode, val refine: RefineSpec? = null)

/**
 * What Search runs, per docs/search-semantics.md: the engine makes the choice — a continuation
 * of the Target Query refines it, a query sharing an item filters it, anything else scans
 * detached or continues the last detached run — and this function supplies the session state the
 * choice reads and turns the answer into the window that mode starts from.
 *
 * [decideStart] is `NativeSeedFinder.decideStart`, i.e. `query::decide_start` over the wire,
 * passed in so this stays a pure function over the session's state. The continuation predicate
 * and the sharing relation are both part of that one call and are never re-derived here.
 */
internal fun startPlanFor(
    request: SearchRequest,
    target: TargetState?,
    lastRun: FinishedRun?,
    lastRunKind: StartMode?,
    decideStart: (SearchRequest, SearchRequest?, Boolean, Boolean, SearchRequest?) -> String,
): StartPlan {
    // A run is a continuation base only when it was itself detached.
    val detachedBase = lastRun?.request?.takeIf { lastRunKind == StartMode.DETACHED }
    val decision = decideStart(
        request,
        target?.request,
        target == null || target.results.isEmpty(),
        (target?.remaining ?: 0L) > 0L,
        detachedBase,
    )
    return when (decision) {
        "anchor" -> StartPlan(StartMode.ANCHOR)
        "target-refine" -> {
            val anchor = checkNotNull(target) { "A target refine needs a Target" }
            StartPlan(
                StartMode.TARGET_REFINE,
                RefineSpec(anchor.resumeFrom, anchor.remaining, anchor.results),
            )
        }
        "target-filter" -> {
            val anchor = checkNotNull(target) { "A target filter needs a Target" }
            StartPlan(StartMode.TARGET_FILTER, RefineSpec(anchor.resumeFrom, 0, anchor.results))
        }
        "continue-detached" -> {
            val base = checkNotNull(lastRun) { "Continuing a detached scan needs that run" }
            StartPlan(
                StartMode.CONTINUE_DETACHED,
                RefineSpec(base.resumeFrom, base.remaining, base.results),
            )
        }
        "detached" -> StartPlan(StartMode.DETACHED)
        else -> error("Unknown start decision '$decision'")
    }
}

/**
 * Folds a run that just concluded (completed or cancelled — never failed) into the Target: an
 * anchor run establishes it from its own results and coverage, a target refine grows the set
 * with the resumed scan's new finds and advances the coverage, and a target filter or detached
 * run leaves it exactly as it was. The Target Query itself never changes here — a refine's
 * finds match it by construction.
 */
internal fun settledTarget(
    target: TargetState?,
    mode: StartMode,
    request: SearchRequest,
    results: List<SeedResult>,
    resumeFrom: Long,
    remaining: Long,
): TargetState? = when (mode) {
    StartMode.ANCHOR -> TargetState(request, results, resumeFrom, remaining)
    StartMode.TARGET_REFINE -> {
        val anchor = checkNotNull(target) { "A target refine needs a Target to refine" }
        // The refine's survivors were already members; only new finds join the set.
        val known = anchor.results.mapTo(mutableSetOf()) { it.seed }
        anchor.copy(
            results = anchor.results + results.filterNot { it.seed in known },
            resumeFrom = resumeFrom,
            remaining = remaining,
        )
    }
    StartMode.TARGET_FILTER, StartMode.CONTINUE_DETACHED, StartMode.DETACHED -> target
}
