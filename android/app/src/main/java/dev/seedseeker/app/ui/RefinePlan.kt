// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.SeedResult
import dev.seedseeker.app.model.isRefinementOf

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

/** A finished (completed or cancelled) run that a follow-up query may refine or continue. */
internal data class FinishedRun(
    val request: SearchRequest,
    val resumeFrom: Long,
    val remaining: Long,
    val results: List<SeedResult>,
)

/**
 * The session's anchor, per docs/search-semantics.md: the first concluded (completed or
 * cancelled) search — or an import — establishes it, and only Clear discards it. [results] is
 * every seed the Target Query's traversal has delivered; refines always filter this full set,
 * never the last run's survivors, which is what lets a loosened query bring seeds back.
 * [remaining] is zero for imports, which carry no coverage.
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
 * Whether two queries name a common item: some requirement of each has the same kind, and
 * either both name the same item or at least one names none (a kind-level requirement subsumes
 * every item of its kind). Scope and challenge differences are deliberately ignored — a filter
 * re-verifies seeds from scratch — so this only estimates whether the Target Set is enriched
 * for the candidate query's matches.
 */
internal fun sharesRequirement(candidate: SearchRequest, base: SearchRequest): Boolean =
    candidate.requirements.any { left ->
        base.requirements.any { right ->
            left.kind == right.kind &&
                (left.item == null || right.item == null || left.item.id == right.item.id)
        }
    }

/**
 * The single gate for what Search runs, per docs/search-semantics.md. The Target Set is the
 * anchor: a continuation of the Target Query refines it (filter the full Target Set, then
 * resume its uncovered remainder), a query sharing an item filters that full set without
 * scanning, and anything else scans the whole range without touching the Target — continuing
 * [lastRun] instead when that run was itself detached and the query continues it. An empty
 * Target Set holds nothing worth preserving, so a non-continuing query re-anchors on this
 * search instead of filtering nothing.
 */
internal fun startPlanFor(
    request: SearchRequest,
    target: TargetState?,
    lastRun: FinishedRun?,
    lastRunKind: StartMode?,
): StartPlan {
    if (target == null) return StartPlan(StartMode.ANCHOR)
    val continuesTarget = request.isRefinementOf(target.request)
    if (target.results.isEmpty() && !(continuesTarget && target.remaining > 0)) {
        return StartPlan(StartMode.ANCHOR)
    }
    if (continuesTarget) {
        return StartPlan(
            StartMode.TARGET_REFINE,
            RefineSpec(target.resumeFrom, target.remaining, target.results),
        )
    }
    if (sharesRequirement(request, target.request)) {
        return StartPlan(StartMode.TARGET_FILTER, RefineSpec(target.resumeFrom, 0, target.results))
    }
    if (lastRunKind == StartMode.DETACHED && lastRun != null && request.isRefinementOf(lastRun.request)) {
        return StartPlan(
            StartMode.CONTINUE_DETACHED,
            RefineSpec(lastRun.resumeFrom, lastRun.remaining, lastRun.results),
        )
    }
    return StartPlan(StartMode.DETACHED)
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
