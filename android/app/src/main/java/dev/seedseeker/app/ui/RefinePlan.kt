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
 * How a Search tap must run [request]: refining is implicit, so a query that narrows [base] — or
 * leaves it unchanged, which simply continues that run — reuses its seeds and resumes where it
 * stopped. Every other query returns null for a fresh scan: no base at all (never searched,
 * imported results, a failed run, or results the user cleared), a widened or edited query, or
 * any scope change.
 */
internal fun refinePlanFor(request: SearchRequest, base: FinishedRun?): RefineSpec? {
    if (base == null || !request.isRefinementOf(base.request)) return null
    return RefineSpec(base.resumeFrom, base.remaining, base.results)
}
