// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import android.content.SharedPreferences
import dev.seedseeker.app.catalog.ItemCatalog
import org.json.JSONArray
import org.json.JSONObject

private const val USER_PRESETS_KEY = "user_presets"

class PresetStorage(private val preferences: SharedPreferences) {
    fun load(): List<QueryPreset> = runCatching {
        val values = JSONArray(preferences.getString(USER_PRESETS_KEY, "[]") ?: "[]")
        buildList {
            for (index in 0 until values.length()) {
                decodePreset(values.getJSONObject(index))?.let(::add)
            }
        }
    }.getOrDefault(emptyList())

    fun save(presets: List<QueryPreset>) {
        val values = JSONArray()
        presets.filterNot(QueryPreset::isBuiltIn).forEach { values.put(encodePreset(it)) }
        preferences.edit().putString(USER_PRESETS_KEY, values.toString()).apply()
    }

    private fun encodePreset(preset: QueryPreset) = JSONObject().apply {
        put("id", preset.id)
        put("name", preset.name)
        put("query", encodeQuery(preset.query))
    }

    private fun encodeQuery(query: PresetQuery) = JSONObject().apply {
        put("maximumDepth", query.maximumDepth)
        put("requireBlacksmith", query.requireBlacksmith)
        put("excludeBlacksmithRewards", query.excludeBlacksmithRewards)
        put("fastMode", query.fastMode)
        put("challenges", query.challenges)
        put("requirements", JSONArray().apply {
            query.requirements.forEach { requirement ->
                put(JSONObject().apply {
                    put("item", requirement.item?.id ?: JSONObject.NULL)
                    put("kind", requirement.kind.name)
                    put("tier", requirement.tier)
                    put("tierMatch", requirement.tierMatch.name)
                    put("upgrade", requirement.upgrade)
                    put("upgradeMatch", requirement.upgradeMatch.name)
                    put("modifier", requirement.modifier ?: JSONObject.NULL)
                    put("source", requirement.source?.name ?: JSONObject.NULL)
                    put("identityGroup", requirement.identityGroup ?: JSONObject.NULL)
                    put("maximumDepth", requirement.maximumDepth ?: JSONObject.NULL)
                    put("requireUncursed", requirement.requireUncursed)
                })
            }
        })
    }

    private fun decodePreset(value: JSONObject): QueryPreset? = runCatching {
        val name = value.getString("name").trim()
        require(name.isNotEmpty())
        QueryPreset(value.getString("id"), name, decodeQuery(value.getJSONObject("query")))
    }.getOrNull()

    private fun decodeQuery(value: JSONObject): PresetQuery {
        val maximumDepth = value.getInt("maximumDepth")
        val challenges = value.optInt("challenges", 0)
        require(maximumDepth in 1..24 && challenges in 0..Challenge.ALL_MASK)
        val encodedRequirements = value.getJSONArray("requirements")
        val requirements = buildList {
            for (index in 0 until encodedRequirements.length()) {
                val encoded = encodedRequirements.getJSONObject(index)
                val item = encoded.stringOrNull("item")?.let { id ->
                    requireNotNull(ItemCatalog.findById(id))
                }
                add(
                    ItemRequirement(
                        key = index.toLong() + 1,
                        item = item,
                        upgrade = encoded.getInt("upgrade"),
                        modifier = encoded.stringOrNull("modifier"),
                        kind = ItemKind.valueOf(encoded.getString("kind")),
                        tier = encoded.optInt("tier", 0),
                        tierMatch = TierMatch.valueOf(encoded.optString("tierMatch", TierMatch.ANY.name)),
                        upgradeMatch = UpgradeMatch.valueOf(encoded.getString("upgradeMatch")),
                        source = encoded.stringOrNull("source")?.let(ScoutItemSource::valueOf),
                        identityGroup = encoded.optInt("identityGroup").takeIf { !encoded.isNull("identityGroup") },
                        maximumDepth = encoded.optInt("maximumDepth").takeIf { !encoded.isNull("maximumDepth") },
                        requireUncursed = encoded.optBoolean("requireUncursed", false),
                    ),
                )
            }
        }
        return PresetQuery(
            requirements = requirements,
            maximumDepth = maximumDepth,
            requireBlacksmith = value.optBoolean("requireBlacksmith"),
            excludeBlacksmithRewards = value.optBoolean("excludeBlacksmithRewards"),
            fastMode = value.optBoolean("fastMode"),
            challenges = challenges,
        )
    }

    private fun JSONObject.stringOrNull(key: String): String? =
        if (isNull(key)) null else getString(key).takeIf(String::isNotEmpty)
}
