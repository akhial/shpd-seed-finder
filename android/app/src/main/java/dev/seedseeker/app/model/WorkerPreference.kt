// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import android.content.SharedPreferences

private const val WORKER_COUNT_KEY = "worker_count"

/**
 * How many search threads this device spawns, stored beside the other
 * device-local settings in the app's own SharedPreferences.
 *
 * It is a property of the device, not of the query: it never enters a query
 * document, a preset, a results file, a share link or the continuation rule,
 * so importing someone else's search never changes how hard this phone works,
 * and changing it never makes a running query look like a different one.
 *
 * [ceiling] is the host's parallelism (`SearchWorkers.ceiling`). An unset
 * preference means "use every core", and a value saved when the ceiling was
 * higher — a restored backup, or the same account on a smaller device — is
 * clamped on the way out rather than rejected.
 */
class WorkerPreference(
    private val preferences: SharedPreferences,
    val ceiling: Int,
) {
    init {
        require(ceiling >= 1) { "The worker ceiling is at least one core" }
    }

    /** The stored count clamped into `1..ceiling`; the ceiling when unset. */
    fun load(): Int {
        // Anything below one — the "unset" default included — means the
        // device has never chosen, so it searches with everything it has.
        val saved = preferences.getInt(WORKER_COUNT_KEY, 0)
        return if (saved < 1) ceiling else minOf(saved, ceiling)
    }

    /** Stores [count] clamped into `1..ceiling` and returns what was stored. */
    fun save(count: Int): Int {
        val clamped = count.coerceIn(1, ceiling)
        preferences.edit().putInt(WORKER_COUNT_KEY, clamped).apply()
        return clamped
    }
}
