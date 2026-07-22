// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.update

import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/** A newer published release, as reported by the GitHub Releases API. */
data class UpdateInfo(val version: String, val url: String)

object UpdateChecker {
    const val RELEASES_PAGE = "https://github.com/akhial/shpd-seed-seeker/releases/latest"
    private const val API_URL = "https://api.github.com/repos/akhial/shpd-seed-seeker/releases/latest"
    private const val TIMEOUT_MILLIS = 10_000

    /**
     * Returns the latest release when it is strictly newer than [current];
     * null when up to date or on any network or parsing failure. Blocks —
     * call from a background dispatcher. [fakeLatest] stands in for the
     * latest release tag, bypassing the network (debug builds only).
     */
    fun check(current: String, fakeLatest: String? = null): UpdateInfo? = runCatching {
        if (fakeLatest != null) return newer(fakeLatest, current, RELEASES_PAGE)
        val connection = URL(API_URL).openConnection() as HttpURLConnection
        try {
            connection.connectTimeout = TIMEOUT_MILLIS
            connection.readTimeout = TIMEOUT_MILLIS
            connection.setRequestProperty("Accept", "application/vnd.github+json")
            if (connection.responseCode != 200) return null
            val release = JSONObject(connection.inputStream.bufferedReader().readText())
            val url = release.optString("html_url").ifEmpty { RELEASES_PAGE }
            newer(release.getString("tag_name"), current, url)
        } finally {
            connection.disconnect()
        }
    }.getOrNull()

    internal fun newer(latest: String, current: String, url: String): UpdateInfo? {
        val latestParts = parse(latest) ?: return null
        val currentParts = parse(current) ?: return null
        for (index in 0 until maxOf(latestParts.size, currentParts.size)) {
            val left = latestParts.getOrElse(index) { 0 }
            val right = currentParts.getOrElse(index) { 0 }
            if (left != right) {
                return if (left > right) UpdateInfo(displayVersion(latest), url) else null
            }
        }
        return null
    }

    /** Strips the tag prefix and any pre-release suffix: "v1.2.3-beta" → "1.2.3". */
    internal fun displayVersion(tag: String): String =
        tag.trim().removePrefix("v").removePrefix("V").substringBefore('-')

    private fun parse(version: String): List<Int>? = displayVersion(version)
        .split('.')
        .map { it.toIntOrNull() ?: return null }
        .takeIf { it.isNotEmpty() }
}
