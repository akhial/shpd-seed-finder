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

    /**
     * Upstream revision pin. For v4.0.0 no source has been published, so this
     * holds the SHA-256 digest of the official release JAR instead of a
     * commit hash.
     */
    val shpdCommit: String by lazy { document.getString("shpdCommit") }

    private val limits: JSONObject by lazy { document.getJSONObject("limits") }

    /** Largest results file the engine's importer accepts, in bytes. */
    val resultsFileMaxBytes: Int by lazy { limits.getInt("resultsFileMaxBytes") }
}
