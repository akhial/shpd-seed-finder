// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.engine.JniBindings
import org.json.JSONObject

/**
 * Shareable-link codec for search queries, backed by the Rust core.
 *
 * A deep link carries a whole query as a short base64url code, e.g.
 * `https://shpd-seed-seeker.web.app/#q=EAGWhMA`. The codec — the bit stream,
 * the frozen code tables, and the link grammar — lives only in
 * `crates/seedfinder-core/src/deep_link.rs` and is reached through
 * [JniBindings]; this wrapper converts between the app's models and the
 * canonical JSON query document at the boundary, the same convention the
 * macOS and Windows apps use over the C FFI.
 */
object DeepLink {
    /**
     * Encodes a query as a full shareable web link.
     *
     * @throws IllegalArgumentException with a user-facing message.
     */
    fun encodeLink(query: PresetQuery): String {
        require(query.requirements.isNotEmpty()) { "Add at least one requirement to share a search." }
        val document = ResultsExport.encodeQuery(query).toString()
        return String(JniBindings.shareEncode(document.toByteArray()), Charsets.UTF_8)
    }

    /**
     * Pulls the share code out of user-facing link text.
     *
     * Accepts full web links (`…#q=CODE` or `…?q=CODE`), custom-scheme links
     * (`seedseeker://q/CODE`), and bare codes. Returns null for text without
     * any plausible code, so callers can ignore non-share links silently.
     */
    fun extractCode(text: String): String? =
        JniBindings.shareExtract(text.toByteArray())?.let { String(it, Charsets.UTF_8) }

    /**
     * Decodes any accepted link form — see [extractCode] — back into the
     * query it carries.
     *
     * @throws IllegalArgumentException with the codec's message when no code
     * is present or the code fails to decode.
     */
    fun decode(text: String): PresetQuery {
        val document = JniBindings.shareDecode(text.toByteArray())
        return ResultsExport.decodeQuery(JSONObject(String(document, Charsets.UTF_8)))
    }
}
