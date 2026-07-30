// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import org.json.JSONArray
import org.json.JSONObject

/**
 * The cross-platform results-export document: search results plus the query
 * that found them.
 *
 * The canonical implementation and compatibility rules live in the Rust core
 * (`crates/seedfinder-core/src/results_export.rs`); the schema is documented
 * in `docs/results-export-format.md`. Keep this codec byte-compatible with
 * it: unknown envelope and per-result fields are ignored, files declaring a
 * newer `format_version` are rejected with an "update the app" message, and
 * unknown query content fails the import instead of silently changing the
 * query's meaning.
 */
object ResultsExport {
    const val FILE_FORMAT = "seed-seeker-results"
    const val FORMAT_VERSION = 1
    const val SUGGESTED_FILE_NAME = "seed-seeker-results.json"
    const val SHPD_VERSION = "3.3.8"

    data class Imported(val query: PresetQuery, val seeds: List<String>)

    private val SEED_CODE = Regex("^[A-Z]{3}-[A-Z]{3}-[A-Z]{3}$")

    /** Stable document names for the nine challenges, in mask order. */
    private val CHALLENGE_NAMES = linkedMapOf(
        "on_diet" to Challenge.NO_FOOD,
        "faith_is_my_armor" to Challenge.NO_ARMOR,
        "pharmacophobia" to Challenge.NO_HEALING,
        "barren_land" to Challenge.NO_HERBALISM,
        "swarm_intelligence" to Challenge.SWARM_INTELLIGENCE,
        "into_darkness" to Challenge.DARKNESS,
        "forbidden_runes" to Challenge.NO_SCROLLS,
        "hostile_champions" to Challenge.CHAMPION_ENEMIES,
        "badder_bosses" to Challenge.STRONGER_BOSSES,
    )

    private val QUERY_KEYS = setOf(
        "requirements",
        "max_depth",
        "require_blacksmith",
        "exclude_blacksmith_rewards",
        "fast_mode",
        "challenges",
    )
    private val REQUIREMENT_KEYS = setOf(
        "kind",
        "item",
        "tier",
        "upgrade",
        "effect",
        "uncursed",
        "source",
        "identity_group",
        "max_depth",
    )

    fun encode(query: PresetQuery, seeds: List<String>, appVersion: String): String =
        JSONObject().apply {
            put("format", FILE_FORMAT)
            put("format_version", FORMAT_VERSION)
            put("app_version", appVersion)
            put("shpd_version", SHPD_VERSION)
            put("query", encodeQuery(query))
            put("results", JSONArray().apply { seeds.forEach { put(JSONObject().put("seed", it)) } })
        }.toString(2)

    /** @throws IllegalArgumentException with a user-facing message. */
    fun decode(text: String): Imported {
        val document = runCatching { JSONObject(text) }.getOrElse {
            throw IllegalArgumentException("This is not a Seed Seeker results file (not valid JSON).")
        }
        require(document.optString("format") == FILE_FORMAT) {
            "This is not a Seed Seeker results file."
        }
        val version = document.opt("format_version") as? Number
        requireNotNull(version) { "This results file is missing its format version." }
        require(version.toInt() >= 1) { "This results file is missing its format version." }
        require(version.toInt() <= FORMAT_VERSION) {
            "This results file uses format version ${version.toInt()}, but this app understands " +
                "up to version $FORMAT_VERSION. Update Seed Seeker to import it."
        }
        val queryValue = document.optJSONObject("query")
        requireNotNull(queryValue) { "This results file is missing its query." }
        val query = decodeQuery(queryValue)
        val resultsValue = document.optJSONArray("results")
        requireNotNull(resultsValue) { "This results file is missing its results list." }
        val seeds = buildList {
            for (index in 0 until resultsValue.length()) {
                val seed = resultsValue.optJSONObject(index)?.optString("seed", "")
                require(seed != null && SEED_CODE.matches(seed)) {
                    "Result ${index + 1} does not have a valid seed code."
                }
                add(seed)
            }
        }
        return Imported(query, seeds)
    }

    private fun encodeQuery(query: PresetQuery) = JSONObject().apply {
        put(
            "requirements",
            JSONArray().apply { query.requirements.forEach { put(encodeRequirement(it)) } },
        )
        if (query.maximumDepth != 24) put("max_depth", query.maximumDepth)
        if (query.requireBlacksmith) put("require_blacksmith", true)
        if (query.excludeBlacksmithRewards) put("exclude_blacksmith_rewards", true)
        if (query.fastMode) put("fast_mode", true)
        val challenges = CHALLENGE_NAMES.entries
            .filter { (_, challenge) -> query.challenges and challenge.bit != 0 }
            .map { (name, _) -> name }
        if (challenges.isNotEmpty()) put("challenges", JSONArray(challenges))
    }

    private fun encodeRequirement(requirement: ItemRequirement) = JSONObject().apply {
        put("kind", requirement.kind.name.lowercase())
        requirement.item?.let { put("item", it.id) }
        when (requirement.tierMatch) {
            TierMatch.ANY -> {}
            TierMatch.EXACT -> put("tier", JSONObject().put("exact", requirement.tier))
            TierMatch.AT_LEAST -> put("tier", JSONObject().put("at_least", requirement.tier))
            TierMatch.AT_MOST -> put("tier", JSONObject().put("at_most", requirement.tier))
        }
        when (requirement.upgradeMatch) {
            UpgradeMatch.ANY -> {}
            UpgradeMatch.EXACT -> put("upgrade", requirement.upgrade)
            UpgradeMatch.AT_LEAST -> put("upgrade", JSONObject().put("at_least", requirement.upgrade))
        }
        requirement.modifier?.let { put("effect", it) }
        if (requirement.requireUncursed) put("uncursed", true)
        requirement.source?.let { put("source", it.name.lowercase()) }
        requirement.identityGroup?.let { put("identity_group", it) }
        requirement.maximumDepth?.let { put("max_depth", it) }
    }

    private fun decodeQuery(value: JSONObject): PresetQuery {
        for (key in value.keys()) {
            require(key in QUERY_KEYS) {
                "The query in this results file uses an unknown field \"$key\". " +
                    "Update Seed Seeker to import it."
            }
        }
        val requirementsValue = value.optJSONArray("requirements")
        requireNotNull(requirementsValue) { "The query in this results file has no requirements list." }
        val requirements = buildList {
            for (index in 0 until requirementsValue.length()) {
                val entry = requirementsValue.optJSONObject(index)
                requireNotNull(entry) { "Requirement ${index + 1} is not a JSON object." }
                add(
                    runCatching { decodeRequirement(entry, index) }.getOrElse { failure ->
                        throw IllegalArgumentException("Requirement ${index + 1}: ${failure.message}")
                    },
                )
            }
        }
        require(requirements.isNotEmpty()) { "The query in this results file has no requirements." }
        val challenges = value.optJSONArray("challenges")?.let { names ->
            var mask = 0
            for (index in 0 until names.length()) {
                val name = names.optString(index, "")
                val challenge = CHALLENGE_NAMES[name.lowercase()]
                requireNotNull(challenge) { "The query in this results file uses an unknown challenge \"$name\"." }
                mask = mask or challenge.bit
            }
            mask
        } ?: 0
        return PresetQuery(
            requirements = requirements,
            maximumDepth = if (value.has("max_depth")) value.getInt("max_depth") else 24,
            requireBlacksmith = value.optBoolean("require_blacksmith"),
            excludeBlacksmithRewards = value.optBoolean("exclude_blacksmith_rewards"),
            fastMode = value.optBoolean("fast_mode"),
            challenges = challenges,
        )
    }

    private fun decodeRequirement(entry: JSONObject, index: Int): ItemRequirement {
        for (key in entry.keys()) {
            require(key in REQUIREMENT_KEYS) { "unknown field \"$key\" — update Seed Seeker to import it" }
        }
        val item = entry.stringOrNull("item")?.let { id ->
            requireNotNull(ItemCatalog.findById(id)) { "unknown item \"$id\"" }
        }
        val kind = entry.stringOrNull("kind")?.let { name ->
            requireNotNull(ItemKind.entries.firstOrNull { it.name.equals(name, ignoreCase = true) }) {
                "unknown category \"$name\""
            }
        } ?: item?.kind ?: throw IllegalArgumentException("a category is required when no item is set")
        var tier = 0
        var tierMatch = TierMatch.ANY
        when (val tierValue = entry.opt("tier")) {
            null, JSONObject.NULL -> {}
            is String -> require(tierValue.equals("any", ignoreCase = true)) {
                "unknown tier mode \"$tierValue\""
            }
            is JSONObject -> {
                require(tierValue.length() == 1) { "unrecognized tier filter" }
                when {
                    tierValue.has("exact") -> {
                        tier = tierValue.getInt("exact")
                        tierMatch = TierMatch.EXACT
                    }
                    tierValue.has("at_least") -> {
                        tier = tierValue.getInt("at_least")
                        tierMatch = TierMatch.AT_LEAST
                    }
                    tierValue.has("at_most") -> {
                        tier = tierValue.getInt("at_most")
                        tierMatch = TierMatch.AT_MOST
                    }
                    else -> throw IllegalArgumentException("unrecognized tier filter")
                }
            }
            else -> throw IllegalArgumentException("unrecognized tier filter")
        }
        var upgrade = 0
        var upgradeMatch = UpgradeMatch.ANY
        when (val upgradeValue = entry.opt("upgrade")) {
            null, JSONObject.NULL -> {}
            is Number -> {
                upgrade = upgradeValue.toInt()
                upgradeMatch = UpgradeMatch.EXACT
            }
            is String -> require(upgradeValue.equals("any", ignoreCase = true)) {
                "unknown upgrade mode \"$upgradeValue\""
            }
            is JSONObject -> {
                require(upgradeValue.length() == 1) { "unrecognized upgrade filter" }
                when {
                    upgradeValue.has("exact") -> {
                        upgrade = upgradeValue.getInt("exact")
                        upgradeMatch = UpgradeMatch.EXACT
                    }
                    upgradeValue.has("at_least") -> {
                        upgrade = upgradeValue.getInt("at_least")
                        upgradeMatch = UpgradeMatch.AT_LEAST
                    }
                    else -> throw IllegalArgumentException("unrecognized upgrade filter")
                }
            }
            else -> throw IllegalArgumentException("unrecognized upgrade filter")
        }
        val modifier = entry.stringOrNull("effect")?.let { name ->
            requireNotNull(
                ItemCatalog.modifiersFor(kind).firstOrNull { it.equals(name, ignoreCase = true) },
            ) { "unknown effect \"$name\"" }
        }
        val source = entry.stringOrNull("source")?.let { name ->
            requireNotNull(
                ScoutItemSource.entries.firstOrNull { it.name.equals(name, ignoreCase = true) },
            ) { "unknown source \"$name\"" }
        }
        return ItemRequirement(
            key = index + 1L,
            item = item,
            upgrade = upgrade,
            modifier = modifier,
            kind = kind,
            tier = tier,
            tierMatch = tierMatch,
            upgradeMatch = upgradeMatch,
            source = source,
            identityGroup = if (entry.isNull("identity_group")) null else entry.getInt("identity_group"),
            maximumDepth = if (entry.isNull("max_depth")) null else entry.getInt("max_depth"),
            requireUncursed = entry.optBoolean("uncursed"),
        )
    }

    private fun JSONObject.stringOrNull(key: String): String? =
        if (isNull(key)) null else getString(key).takeIf(String::isNotEmpty)
}
