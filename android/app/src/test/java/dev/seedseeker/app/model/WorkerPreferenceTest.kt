// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.catalog.PackagedCatalog
import dev.seedseeker.app.engine.QueryDocument
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The worker count is the one setting the app keeps for the device rather
 * than for the query: it is clamped to what this device can offer, defaults
 * to all of it, and stays out of everything a query travels in.
 */
class WorkerPreferenceTest {
    init { PackagedCatalog.install() }

    @Test
    fun anUnsetPreferenceSearchesWithEveryCore() {
        assertEquals(8, WorkerPreference(MemoryPreferences(), ceiling = 8).load())
        assertEquals(1, WorkerPreference(MemoryPreferences(), ceiling = 1).load())
    }

    @Test
    fun savedCountsRoundTripAndAreClampedIntoTheCeiling() {
        val preferences = MemoryPreferences()
        val preference = WorkerPreference(preferences, ceiling = 4)

        assertEquals(3, preference.save(3))
        assertEquals(3, WorkerPreference(preferences, ceiling = 4).load())

        // Saving out of range stores the clamped value, so a later read never
        // has to repair it.
        assertEquals(4, preference.save(9))
        assertEquals(4, WorkerPreference(preferences, ceiling = 4).load())
        assertEquals(1, preference.save(0))
        assertEquals(1, WorkerPreference(preferences, ceiling = 4).load())
        assertEquals(1, preference.save(-7))
        assertEquals(1, WorkerPreference(preferences, ceiling = 4).load())
    }

    /**
     * The same install can meet a smaller ceiling — a restored backup, or a
     * value written before the device reported fewer cores — so a stored count
     * is clamped on the way out instead of being trusted or discarded.
     */
    @Test
    fun aStoredCountAboveTheCeilingIsClampedOnLoad() {
        val preferences = MemoryPreferences()
        WorkerPreference(preferences, ceiling = 16).save(12)
        assertEquals(4, WorkerPreference(preferences, ceiling = 4).load())
        assertEquals(1, WorkerPreference(preferences, ceiling = 1).load())
    }

    /** A count below one is what "never chosen" looks like: use every core. */
    @Test
    fun aStoredCountBelowOneFallsBackToTheCeiling() {
        val preferences = MemoryPreferences()
        preferences.edit().putInt("worker_count", -3).apply()
        assertEquals(6, WorkerPreference(preferences, ceiling = 6).load())
    }

    @Test(expected = IllegalArgumentException::class)
    fun aCeilingBelowOneIsRejected() {
        WorkerPreference(MemoryPreferences(), ceiling = 0)
    }

    /**
     * The count is device-local: it may not reach a query document (what the
     * engine is handed, and what share links carry), a results file or a saved
     * preset, or refining and importing would depend on the phone that ran the
     * search.
     */
    @Test
    fun theWorkerCountNeverEntersAQueryDocumentPresetOrExport() {
        val preferences = MemoryPreferences()
        WorkerPreference(preferences, ceiling = 8).save(2)
        val request = SearchRequest(
            requirements = listOf(
                ItemRequirement(1, ItemCatalog.wands.first { it.id == "wand_frost" }, 2),
            ),
            maximumDepth = 14,
            requireBlacksmith = true,
        )

        val document = String(QueryDocument.encode(request), Charsets.UTF_8)
        assertFalse(document, document.contains("worker", ignoreCase = true))
        assertFalse(document, document.contains("core", ignoreCase = true))

        val exported = ResultsExport.encodeQuery(request).toString()
        assertFalse(exported, exported.contains("worker", ignoreCase = true))

        PresetStorage(preferences).save(
            listOf(QueryPreset(id = "p", name = "Preset", query = request.toPresetQuery())),
        )
        val saved = preferences.getString("user_presets", "") ?: ""
        assertTrue(saved.contains("Preset"))
        assertFalse(saved, saved.contains("worker", ignoreCase = true))

        // And the preference the presets sit beside is untouched by saving one.
        assertEquals(2, WorkerPreference(preferences, ceiling = 8).load())
    }
}
