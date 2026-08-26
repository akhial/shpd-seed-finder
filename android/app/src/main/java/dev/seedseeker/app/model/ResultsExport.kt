// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.engine.JniBindings
import org.json.JSONArray
import org.json.JSONObject

/**
 * The cross-platform results-export document: search results plus the query
 * that found them.
 *
 * The codec — the envelope, the compatibility rules, the strict query
 * validation, the 2 MiB import cap and the dedupe-and-cap every importer
 * applies — lives only in the Rust core
 * (`crates/seedfinder-core/src/results_export.rs`, documented in
 * `docs/results-export-format.md`) and is reached through [JniBindings]. What
 * remains here is the mapping between the canonical JSON query document and
 * the app's own models, the same boundary convention [DeepLink] uses: the
 * engine has already validated the document, so this side reads it leniently
 * and never re-derives its schema.
 */
object ResultsExport {
    const val SUGGESTED_FILE_NAME = "seed-seeker-results.json"

    /**
     * One imported file. [seeds] is already deduplicated and capped by the
     * engine and [dropped] counts the exported entries that step removed.
     */
    data class Imported(
        val query: PresetQuery,
        val seeds: List<String>,
        val dropped: Int,
        val shpdVersion: String?,
    )

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

    /** @throws IllegalArgumentException with the codec's message. */
    fun encode(query: PresetQuery, seeds: List<String>, appVersion: String): String {
        val request = JSONObject().apply {
            put("query", encodeQuery(query))
            put("seeds", JSONArray(seeds))
            put("app_version", appVersion)
        }
        return String(JniBindings.resultsEncode(request.toString().toByteArray()), Charsets.UTF_8)
    }

    /** @throws IllegalArgumentException with the codec's message. */
    fun decode(text: String): Imported {
        val document = JSONObject(
            String(JniBindings.resultsDecode(text.toByteArray()), Charsets.UTF_8),
        )
        val seeds = document.getJSONArray("seeds")
        return Imported(
            query = decodeQuery(document.getJSONObject("query")),
            seeds = List(seeds.length()) { seeds.getString(it) },
            dropped = document.getInt("dropped"),
            shpdVersion = document.opt("shpd_version") as? String,
        )
    }

    /** The query half of the document; [DeepLink] and the engine transport share it with the Rust codec. */
    internal fun encodeQuery(query: PresetQuery) = JSONObject().apply {
        put("requirements", encodeRequirements(query.requirements))
        if (query.maximumDepth != 24) put("max_depth", query.maximumDepth)
        if (query.requireBlacksmith) put("require_blacksmith", true)
        if (query.excludeBlacksmithRewards) put("exclude_blacksmith_rewards", true)
        query.wandmakerQuest?.let { put("wandmaker_quest", it.documentName) }
        if (query.fastMode) put("fast_mode", true)
        val challenges = CHALLENGE_NAMES.entries
            .filter { (_, challenge) -> query.challenges and challenge.bit != 0 }
            .map { (name, _) -> name }
        if (challenges.isNotEmpty()) put("challenges", JSONArray(challenges))
    }

    /** The same document for a runnable request; what every query-taking engine call sends. */
    fun encodeQuery(request: SearchRequest): JSONObject = encodeQuery(request.toPresetQuery())

    /**
     * One entry per slot: an alternative group becomes a single `any_of`
     * entry at its first member's position, holding the members in
     * requirement order; a single-member group is written as a plain
     * requirement.
     */
    private fun encodeRequirements(requirements: List<ItemRequirement>) = JSONArray().apply {
        requirements.slots().forEach { slot ->
            val members = slot.map(::encodeRequirement)
            if (members.size == 1) {
                put(members.single())
            } else {
                put(JSONObject().put("any_of", JSONArray(members)))
            }
        }
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
        encodeEffect(requirement.effect, requirement.kind)?.let { put("effect", it) }
        if (requirement.requireUncursed) put("uncursed", true)
        requirement.source?.let { put("source", it.name.lowercase()) }
        requirement.identityGroup?.let { put("identity_group", it) }
        requirement.maximumDepth?.let { put("max_depth", it) }
        requirement.levelSum?.let {
            put("level_sum", JSONObject().put("group", it.group).put("at_least", it.atLeast))
        }
    }

    /**
     * The shared writer rule: one effect is a bare name, the full non-curse
     * family set is `"any_enchantment"`, anything else lists its members in
     * catalog order. [EffectFilter.of] already canonicalized the selection.
     */
    private fun encodeEffect(effect: EffectFilter, kind: ItemKind): Any? = when (effect) {
        EffectFilter.Any -> null
        EffectFilter.AnyEnchantment -> ANY_ENCHANTMENT
        is EffectFilter.OneOf -> when (val canonical = EffectFilter.of(effect.names, kind)) {
            EffectFilter.AnyEnchantment -> ANY_ENCHANTMENT
            is EffectFilter.OneOf -> canonical.names.singleOrNull() ?: JSONArray(canonical.names)
            EffectFilter.Any -> null
        }
    }

    /**
     * Reads a canonical query document the engine has already decoded and
     * validated, so every field is trusted here: unknown names cannot occur
     * and are simply ignored rather than re-checked.
     */
    internal fun decodeQuery(value: JSONObject): PresetQuery {
        val requirementsValue = value.optJSONArray("requirements") ?: JSONArray()
        // Alternative groups get fresh sequential ids in document order.
        var nextGroup = 1
        // Messages name the document entry (1-based), as the core does: every
        // member of an any_of group reports the group's position.
        val requirements = buildList {
            for (index in 0 until requirementsValue.length()) {
                val entry = requirementsValue.getJSONObject(index)
                val members = entry.optJSONArray("any_of")
                if (members == null) {
                    add(decodeRequirement(entry, index, key = size + 1L))
                } else {
                    val group = if (members.length() > 1) nextGroup++ else null
                    for (member in 0 until members.length()) {
                        add(
                            decodeRequirement(members.getJSONObject(member), index, key = size + 1L)
                                .copy(alternativeGroup = group),
                        )
                    }
                }
            }
        }
        val challengesValue = value.optJSONArray("challenges") ?: JSONArray()
        var challenges = 0
        for (index in 0 until challengesValue.length()) {
            CHALLENGE_NAMES[challengesValue.optString(index)]?.let { challenges = challenges or it.bit }
        }
        return PresetQuery(
            requirements = requirements,
            maximumDepth = value.optInt("max_depth", 24),
            requireBlacksmith = value.optBoolean("require_blacksmith"),
            excludeBlacksmithRewards = value.optBoolean("exclude_blacksmith_rewards"),
            wandmakerQuest = (value.opt("wandmaker_quest") as? String)?.let(WandmakerQuest::named),
            fastMode = value.optBoolean("fast_mode"),
            challenges = challenges,
        )
    }

    /**
     * Reads the wire effect forms — absent, a bare name, the
     * `"any_enchantment"` shorthand, or a list of names — canonicalizing the
     * names to the catalog's spellings (the engine matched them
     * case-insensitively) and the selection to its shared canonical form.
     */
    private fun decodeEffect(value: Any?, kind: ItemKind): EffectFilter {
        val known = ItemCatalog.modifiersFor(kind)
        fun canonical(name: String) = known.firstOrNull { it.equals(name, ignoreCase = true) } ?: name
        return when (value) {
            null -> EffectFilter.Any
            is String ->
                if (value.equals(ANY_ENCHANTMENT, ignoreCase = true)) {
                    EffectFilter.AnyEnchantment
                } else {
                    EffectFilter.OneOf(listOf(canonical(value)))
                }
            is JSONArray -> EffectFilter.of(List(value.length()) { canonical(value.getString(it)) }, kind)
            else -> throw IllegalArgumentException("Unrecognized effect filter.")
        }
    }

    private fun decodeRequirement(entry: JSONObject, index: Int, key: Long): ItemRequirement {
        require(!entry.has("upgrade_sum")) {
            "upgrade_sum is no longer supported; use level_sum"
        }
        val item = (entry.opt("item") as? String)?.let(ItemCatalog::findById)
        val kind = (entry.opt("kind") as? String)
            ?.let { name -> ItemKind.entries.firstOrNull { it.name.lowercase() == name } }
            ?: item?.kind
            ?: throw IllegalArgumentException("Requirement ${index + 1} has no category.")
        var tier = 0
        var tierMatch = TierMatch.ANY
        (entry.opt("tier") as? JSONObject)?.let { tierValue ->
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
            }
        }
        var upgrade = 0
        var upgradeMatch = UpgradeMatch.ANY
        when (val upgradeValue = entry.opt("upgrade")) {
            is Number -> {
                upgrade = upgradeValue.toInt()
                upgradeMatch = UpgradeMatch.EXACT
            }
            is JSONObject -> when {
                upgradeValue.has("exact") -> {
                    upgrade = upgradeValue.getInt("exact")
                    upgradeMatch = UpgradeMatch.EXACT
                }
                upgradeValue.has("at_least") -> {
                    upgrade = upgradeValue.getInt("at_least")
                    upgradeMatch = UpgradeMatch.AT_LEAST
                }
            }
            else -> {}
        }
        val source = (entry.opt("source") as? String)?.let { name ->
            ScoutItemSource.entries.firstOrNull { it.name.lowercase() == name }
        }
        return ItemRequirement(
            key = key,
            item = item,
            upgrade = upgrade,
            effect = decodeEffect(entry.opt("effect"), kind),
            kind = kind,
            tier = tier,
            tierMatch = tierMatch,
            upgradeMatch = upgradeMatch,
            source = source,
            identityGroup = if (entry.has("identity_group")) entry.getInt("identity_group") else null,
            maximumDepth = if (entry.has("max_depth")) entry.getInt("max_depth") else null,
            requireUncursed = entry.optBoolean("uncursed"),
            levelSum = entry.optJSONObject("level_sum")?.let {
                LevelSum(group = it.getInt("group"), atLeast = it.getInt("at_least"))
            },
        )
    }

    /** The effect shorthand for every non-curse effect of the item's family. */
    private const val ANY_ENCHANTMENT = "any_enchantment"
}

/** The editor-facing view of a runnable request, which the document mapping is written against. */
fun SearchRequest.toPresetQuery() = PresetQuery(
    requirements = requirements,
    maximumDepth = maximumDepth,
    requireBlacksmith = requireBlacksmith,
    excludeBlacksmithRewards = excludeBlacksmithRewards,
    wandmakerQuest = wandmakerQuest,
    fastMode = fastMode,
    challenges = challenges,
)
