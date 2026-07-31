// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

/**
 * Navigation through the ordered list of search-result seeds while scouting.
 *
 * The scout screen can be reached either from a search result or by typing a
 * seed by hand; navigation is only meaningful in the first case, so every
 * helper returns null when the current seed is not a search result.
 */
object ScoutResultNavigation {
    /** 0-based index of [seed] within [seeds], or null when it is not one of them. */
    fun position(seeds: List<String>, seed: String?): Int? {
        if (seed.isNullOrEmpty()) return null
        return seeds.indexOf(seed).takeIf { it >= 0 }
    }

    /**
     * The seed [delta] steps away from [seed] in the results, clamped to the
     * list ends. Null when [seed] is not a search result or the step would not
     * move (already at the first or last result).
     */
    fun step(seeds: List<String>, seed: String?, delta: Int): String? {
        val index = position(seeds, seed) ?: return null
        val target = (index + delta).coerceIn(0, seeds.lastIndex)
        return if (target == index) null else seeds[target]
    }
}
