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
 * (`crates/seedfinder-core/src/results_export.rs`, query schema in
 * `json_query.rs`); the schema is documented in
 * `docs/results-export-format.md`. Keep this codec schema-compatible with
 * it: unknown envelope and per-result fields are ignored, files declaring a
 * newer `format_version` are rejected with an "update the app" message, and
 * unknown or wrong-typed query content fails the import instead of silently
 * changing the query's meaning.
 */
object ResultsExport {
    const val FILE_FORMAT = "seed-seeker-results"
    const val FORMAT_VERSION = 1
    const val SUGGESTED_FILE_NAME = "seed-seeker-results.json"

    /** Mirrors the Rust core's `SHPD_VERSION`, the source of truth. */
    const val SHPD_VERSION = "3.3.8"

    /** Import size cap; a maximal legal results file is far below this. */
    const val MAX_FILE_BYTES = 2 * 1024 * 1024

    data class Imported(
        val query: PresetQuery,
        val seeds: List<String>,
        val shpdVersion: String?,
    )

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
        "upgrade_sum",
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
    fun decode(text: String): Imported = try {
        decodeDocument(text)
    } catch (failure: IllegalArgumentException) {
        throw failure
    } catch (_: Exception) {
        // Anything the strict readers below did not anticipate (for example
        // an org.json parse quirk) must not leak a raw exception message.
        throw IllegalArgumentException("This results file is malformed.")
    }

    private fun decodeDocument(text: String): Imported {
        val document = runCatching { JSONObject(text) }.getOrElse {
            throw IllegalArgumentException("This is not a Seed Seeker results file (not valid JSON).")
        }
        require(document.opt("format") == FILE_FORMAT) {
            "This is not a Seed Seeker results file."
        }
        val version = document.opt("format_version")
        requireNotNull(version.takeIf { it != JSONObject.NULL }) {
            "This results file is missing its format version."
        }
        // Strictly a positive integer: no booleans, strings, or fractions.
        require(version is Int || version is Long) {
            "This results file does not declare a valid format version (a positive whole number)."
        }
        val versionNumber = (version as Number).toLong()
        require(versionNumber >= 1) {
            "This results file does not declare a valid format version (a positive whole number)."
        }
        require(versionNumber <= FORMAT_VERSION) {
            "This results file uses format version $versionNumber, but this app understands " +
                "up to version $FORMAT_VERSION. Update Seed Seeker to import it."
        }
        val queryValue = document.optJSONObject("query")
        requireNotNull(queryValue) { "This results file is missing its query." }
        val query = decodeQuery(queryValue)
        val resultsValue = document.optJSONArray("results")
        requireNotNull(resultsValue) { "This results file is missing its results list." }
        val seeds = buildList {
            for (index in 0 until resultsValue.length()) {
                val seed = resultsValue.optJSONObject(index)?.opt("seed") as? String
                require(seed != null && SEED_CODE.matches(seed)) {
                    "Result ${index + 1} does not have a valid seed code " +
                        "(canonical XXX-XXX-XXX form)."
                }
                add(seed)
            }
        }
        return Imported(query, seeds, document.opt("shpd_version") as? String)
    }

    private fun encodeQuery(query: PresetQuery) = JSONObject().apply {
        // An alternative group serializes as one {"any_of": [...]} entry at
        // its first member's position, holding every member in requirement
        // order; decode assigns the groups fresh sequential ids, preserving
        // the structure. A single-member group is the same query as a plain
        // requirement and collapses to one.
        val entries = JSONArray()
        val emittedGroups = mutableSetOf<Int>()
        for (requirement in query.requirements) {
            val group = requirement.alternativeGroup
            if (group == null) {
                entries.put(encodeRequirement(requirement))
                continue
            }
            if (!emittedGroups.add(group)) continue
            val members = query.requirements.filter { it.alternativeGroup == group }
            if (members.size == 1) {
                entries.put(encodeRequirement(members.single()))
            } else {
                entries.put(
                    JSONObject().put(
                        "any_of",
                        JSONArray().apply { members.forEach { put(encodeRequirement(it)) } },
                    ),
                )
            }
        }
        put("requirements", entries)
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
        // Narrowed weapon kinds keep their document names (melee_weapon,
        // thrown_weapon); widening them to "weapon" would silently change the
        // query's meaning on import.
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
        when (val effect = requirement.effect) {
            EffectRequirement.Any -> {}
            EffectRequirement.AnyEnchantment -> put("effect", "any_enchantment")
            is EffectRequirement.OneOf -> {
                // The full non-curse family set uses the shorthand; one
                // effect stays a bare name; anything else lists its members.
                val nonCurse = ItemCatalog.modifiersFor(requirement.kind).toSet() -
                    ItemCatalog.cursesFor(requirement.kind).toSet()
                put(
                    "effect",
                    when {
                        effect.effects.toSet() == nonCurse -> "any_enchantment"
                        effect.effects.size == 1 -> effect.effects.single()
                        else -> JSONArray(effect.effects)
                    },
                )
            }
        }
        if (requirement.requireUncursed) put("uncursed", true)
        requirement.source?.let { put("source", it.name.lowercase()) }
        requirement.identityGroup?.let { put("identity_group", it) }
        requirement.maximumDepth?.let { put("max_depth", it) }
        if (requirement.upgradeSumGroup != null && requirement.upgradeSumTotal != null) {
            put(
                "upgrade_sum",
                JSONObject()
                    .put("group", requirement.upgradeSumGroup)
                    .put("at_least", requirement.upgradeSumTotal),
            )
        }
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
        // An entry is a plain requirement or an {"any_of": [...]} alternative
        // group satisfied by any single member. Groups get fresh sequential
        // ids in entry order.
        val requirements = buildList {
            var nextAlternativeGroup = 0
            for (index in 0 until requirementsValue.length()) {
                val entry = requirementsValue.optJSONObject(index)
                requireNotNull(entry) { "Requirement ${index + 1} is not a JSON object." }
                runCatching {
                    if (entry.has("any_of")) {
                        for (key in entry.keys()) {
                            require(key == "any_of") {
                                "unknown field \"$key\" — update Seed Seeker to import it"
                            }
                        }
                        val members = entry.optJSONArray("any_of")
                        require(members != null && members.length() > 0) {
                            "\"any_of\" needs at least one requirement"
                        }
                        // A one-member group is the same query as a plain
                        // requirement.
                        val group = if (members.length() > 1) ++nextAlternativeGroup else null
                        for (memberIndex in 0 until members.length()) {
                            val member = members.optJSONObject(memberIndex)
                            requireNotNull(member) { "\"any_of\" members must be JSON objects" }
                            add(decodeRequirement(member, size + 1L, group, insideGroup = true))
                        }
                    } else {
                        add(decodeRequirement(entry, size + 1L, alternativeGroup = null, insideGroup = false))
                    }
                }.getOrElse { failure ->
                    throw IllegalArgumentException("Requirement ${index + 1}: ${failure.message}")
                }
            }
        }
        require(requirements.isNotEmpty()) { "The query in this results file has no requirements." }
        // Members of a combined-upgrade group must agree on the total; the
        // engine would reject such a search with an unspecific error.
        val groupTotals = mutableMapOf<Int, Int>()
        for (requirement in requirements) {
            val group = requirement.upgradeSumGroup ?: continue
            val total = requirement.upgradeSumTotal ?: continue
            val previous = groupTotals.put(group, total)
            require(previous == null || previous == total) {
                "Combined upgrade group $group members disagree on the required total."
            }
        }
        val challengesValue = value.opt("challenges")
        var challenges = 0
        if (challengesValue != null && challengesValue != JSONObject.NULL) {
            require(challengesValue is JSONArray) {
                "\"challenges\" must be a list of challenge names"
            }
            for (index in 0 until challengesValue.length()) {
                val name = challengesValue.opt(index)
                val challenge = (name as? String)?.let(CHALLENGE_NAMES::get)
                requireNotNull(challenge) {
                    "The query in this results file uses an unknown challenge \"$name\"."
                }
                challenges = challenges or challenge.bit
            }
        }
        val maximumDepth = value.strictIntOrNull("max_depth") ?: 24
        require(maximumDepth in 1..24) { "Maximum floor must be 1..24." }
        return PresetQuery(
            requirements = requirements,
            maximumDepth = maximumDepth,
            requireBlacksmith = value.strictBool("require_blacksmith"),
            excludeBlacksmithRewards = value.strictBool("exclude_blacksmith_rewards"),
            fastMode = value.strictBool("fast_mode"),
            challenges = challenges,
        )
    }

    private fun decodeRequirement(
        entry: JSONObject,
        key: Long,
        alternativeGroup: Int?,
        insideGroup: Boolean,
    ): ItemRequirement {
        for (field in entry.keys()) {
            require(field in REQUIREMENT_KEYS) { "unknown field \"$field\" — update Seed Seeker to import it" }
        }
        val item = entry.strictStringOrNull("item")?.let { id ->
            requireNotNull(ItemCatalog.findById(id)) { "unknown item \"$id\"" }
        }
        // Enum names match the core decoder exactly (lowercase snake_case);
        // only effect names and the "any" keyword are matched case-insensitively.
        val kind = entry.strictStringOrNull("kind")?.let { name ->
            requireNotNull(ItemKind.entries.firstOrNull { it.name.lowercase() == name }) {
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
                        tier = tierValue.strictInt("exact")
                        tierMatch = TierMatch.EXACT
                    }
                    tierValue.has("at_least") -> {
                        tier = tierValue.strictInt("at_least")
                        tierMatch = TierMatch.AT_LEAST
                    }
                    tierValue.has("at_most") -> {
                        tier = tierValue.strictInt("at_most")
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
            is Int, is Long -> {
                upgrade = (upgradeValue as Number).toInt()
                upgradeMatch = UpgradeMatch.EXACT
            }
            is String -> require(upgradeValue.equals("any", ignoreCase = true)) {
                "unknown upgrade mode \"$upgradeValue\""
            }
            is JSONObject -> {
                require(upgradeValue.length() == 1) { "unrecognized upgrade filter" }
                when {
                    upgradeValue.has("exact") -> {
                        upgrade = upgradeValue.strictInt("exact")
                        upgradeMatch = UpgradeMatch.EXACT
                    }
                    upgradeValue.has("at_least") -> {
                        upgrade = upgradeValue.strictInt("at_least")
                        upgradeMatch = UpgradeMatch.AT_LEAST
                    }
                    else -> throw IllegalArgumentException("unrecognized upgrade filter")
                }
            }
            else -> throw IllegalArgumentException("unrecognized upgrade filter")
        }
        val effect = decodeEffect(entry.opt("effect"), kind)
        var upgradeSumGroup: Int? = null
        var upgradeSumTotal: Int? = null
        when (val sumValue = entry.opt("upgrade_sum")) {
            null, JSONObject.NULL -> {}
            is JSONObject -> {
                require(!insideGroup) {
                    "a combined upgrade total cannot sit inside an \"any_of\" group"
                }
                for (field in sumValue.keys()) {
                    require(field == "group" || field == "at_least") {
                        "unknown field \"$field\" — update Seed Seeker to import it"
                    }
                }
                upgradeSumGroup = sumValue.strictInt("group")
                upgradeSumTotal = sumValue.strictInt("at_least")
            }
            else -> throw IllegalArgumentException("\"upgrade_sum\" must be an object")
        }
        val source = entry.strictStringOrNull("source")?.let { name ->
            requireNotNull(ScoutItemSource.entries.firstOrNull { it.name.lowercase() == name }) {
                "unknown source \"$name\""
            }
        }
        return ItemRequirement(
            key = key,
            item = item,
            upgrade = upgrade,
            effect = effect,
            kind = kind,
            tier = tier,
            tierMatch = tierMatch,
            upgradeMatch = upgradeMatch,
            source = source,
            identityGroup = entry.strictIntOrNull("identity_group"),
            maximumDepth = entry.strictIntOrNull("max_depth"),
            requireUncursed = entry.strictBool("uncursed"),
            alternativeGroup = alternativeGroup,
            upgradeSumGroup = upgradeSumGroup,
            upgradeSumTotal = upgradeSumTotal,
        )
    }

    /**
     * Decodes the effect field's three wire forms: a single effect name, a
     * list of same-family names, or the "any_enchantment" shorthand for the
     * family's full non-curse set. Names match case-insensitively and
     * canonicalize to the catalog spelling; duplicates collapse.
     */
    private fun decodeEffect(value: Any?, kind: ItemKind): EffectRequirement {
        if (value == null || value == JSONObject.NULL) return EffectRequirement.Any
        fun canonical(name: Any?): String {
            require(name is String) { "\"effect\" names must be strings" }
            return requireNotNull(
                ItemCatalog.modifiersFor(kind).firstOrNull { it.equals(name, ignoreCase = true) },
            ) { "unknown effect \"$name\"" }
        }
        return when (value) {
            is String ->
                if (value.equals("any_enchantment", ignoreCase = true)) {
                    require(kind.modifierLabel != null) {
                        "\"any_enchantment\" requires a weapon or armor"
                    }
                    EffectRequirement.AnyEnchantment
                } else {
                    EffectRequirement.OneOf(listOf(canonical(value)))
                }
            is JSONArray -> {
                require(value.length() > 0) { "an effect list needs at least one name" }
                val names = buildList {
                    for (index in 0 until value.length()) {
                        val match = canonical(value.opt(index))
                        if (match !in this) add(match)
                    }
                }
                EffectRequirement.OneOf(names)
            }
            else -> throw IllegalArgumentException(
                "\"effect\" must be an effect name, a list of names, or \"any_enchantment\"",
            )
        }
    }

    // Strict typed readers: a present-but-wrong-type value is an error, never
    // silently coerced or treated as absent. JSON null counts as absent for
    // the optional string/int fields, matching the core decoder.
    private fun JSONObject.strictStringOrNull(key: String): String? {
        val value = opt(key)
        if (value == null || value == JSONObject.NULL) return null
        require(value is String) { "\"$key\" must be a string" }
        return value
    }

    private fun JSONObject.strictIntOrNull(key: String): Int? {
        val value = opt(key)
        if (value == null || value == JSONObject.NULL) return null
        require(value is Int || value is Long) { "\"$key\" must be a whole number" }
        return (value as Number).toInt()
    }

    private fun JSONObject.strictInt(key: String): Int =
        requireNotNull(strictIntOrNull(key)) { "\"$key\" must be a whole number" }

    private fun JSONObject.strictBool(key: String): Boolean {
        val value = opt(key) ?: return false
        require(value is Boolean) { "\"$key\" must be true or false" }
        return value
    }
}
