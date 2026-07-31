// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.SeedResult
import dev.seedseeker.app.model.isRefinementOf

/** Resume window and previously shown seeds a refine run starts from. */
internal data class RefineSpec(
    val resumeFrom: Long,
    val remaining: Long,
    val keepSeeds: List<SeedResult>,
)

/** A finished (completed or cancelled) run that a stricter follow-up query may refine. */
internal data class FinishedRun(
    val request: SearchRequest,
    val resumeFrom: Long,
    val remaining: Long,
    val results: List<SeedResult>,
)

/**
 * How a Search tap must run [request]: refining is implicit, so a query that only narrows
 * [base] reuses that run's seeds and resumes where it stopped, and every other query — no
 * base at all (never searched, imported results, a failed run, or results the user cleared),
 * a widened or edited query, or any scope change — returns null for a fresh scan.
 */
internal fun refinePlanFor(request: SearchRequest, base: FinishedRun?): RefineSpec? {
    if (base == null || !request.isRefinementOf(base.request)) return null
    return RefineSpec(base.resumeFrom, base.remaining, base.results)
}
