// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import org.json.JSONObject

/**
 * The engine's own constants, published by [JniBindings.engineInfo].
 *
 * Every frontend used to hardcode its own copy of the upstream version, the
 * query bounds and the game-data tables; the engine owns them
 * (`crates/seedfinder-core/src/engine_info.rs`) and hands them out as one JSON
 * document, so the app reads them here instead of mirroring them.
 *
 * The document is read once, lazily, from the native library every APK
 * packages — the same library JVM unit tests load through `buildHostJni` — so
 * there is nothing to install at startup and no separate test seam.
 */
object EngineInfo {
    private val document: JSONObject by lazy {
        JSONObject(String(JniBindings.engineInfo(), Charsets.UTF_8))
    }

    /** Upstream Shattered Pixel Dungeon version this engine reproduces. */
    val shpdVersion: String by lazy { document.getString("shpdVersion") }

    private val limits: JSONObject by lazy { document.getJSONObject("limits") }

    /** Deepest floor a search may cover. */
    val maxDepth: Int by lazy { limits.getInt("max_depth") }

    /** Tiers an "exactly tier N" requirement may name. */
    val exactTiers: IntRange by lazy {
        limits.getInt("exact_tier_min")..limits.getInt("exact_tier_max")
    }

    /** Tiers an "at least/at most tier N" requirement may name. */
    val boundedTiers: IntRange by lazy {
        limits.getInt("bounded_tier_min")..limits.getInt("bounded_tier_max")
    }

    /** Highest same-item group number (groups run 1..this, shown as A..D). */
    val identityGroupMax: Int by lazy { limits.getInt("identity_group_max") }

    /** Highest upgrade level a search may name, for everything but rings. */
    val maxUpgradeDefault: Int by lazy { limits.getInt("max_upgrade_default") }

    /** Highest upgrade level a ring requirement may name. */
    val maxUpgradeRing: Int by lazy { limits.getInt("max_upgrade_ring") }

    /** How many results one run collects, and one import restores. */
    val maxResults: Int by lazy { limits.getInt("max_results") }

    /** Largest results file the engine's importer accepts, in bytes. */
    val resultsFileMaxBytes: Int by lazy { limits.getInt("results_file_max_bytes") }

    /** Boss floors that generate no searchable items. */
    val emptyBossFloors: Set<Int> by lazy {
        val floors = document.getJSONArray("empty_boss_floors")
        buildSet { for (index in 0 until floors.length()) add(floors.getInt(index)) }
    }

    /** The floors a quest giver's quest can sit on, by its document name. */
    fun questWindow(giver: String): IntRange {
        val window = document.getJSONObject("quest_windows").getJSONArray(giver)
        return window.getInt(0)..window.getInt(1)
    }

    /** Whether a challenge bit changes level generation, and so the seeds a search finds. */
    fun changesLevelGeneration(mask: Int): Boolean = generatingChallengeMask and mask != 0

    /** Every challenge bit the engine knows, as one mask. */
    val allChallengesMask: Int by lazy { challengeMask { true } }

    private val generatingChallengeMask: Int by lazy {
        challengeMask { it.getBoolean("changes_level_generation") }
    }

    private fun challengeMask(include: (JSONObject) -> Boolean): Int {
        val challenges = document.getJSONArray("challenges")
        return (0 until challenges.length()).fold(0) { mask, index ->
            val challenge = challenges.getJSONObject(index)
            if (include(challenge)) mask or challenge.getInt("mask") else mask
        }
    }
}
