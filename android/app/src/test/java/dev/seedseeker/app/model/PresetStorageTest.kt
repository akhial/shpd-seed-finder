// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import android.content.SharedPreferences
import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.catalog.PackagedCatalog
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The app's own preset schema: additive, so presets saved by earlier
 * releases (single `modifier`, no groups) still load — retired keys such as
 * `fastMode` included — and the new fields round-trip.
 */
class PresetStorageTest {
    init { PackagedCatalog.install() }

    @Test
    fun presetsSavedByOlderReleasesStillLoad() {
        val preferences = MemoryPreferences()
        // "fastMode" is a retired flag kept in this saved preset deliberately:
        // presets written before its removal must still load, the key ignored.
        preferences.edit().putString(
            "user_presets",
            """[{"id":"old","name":"Old preset","query":{
                 "maximumDepth":10,"requireBlacksmith":false,"excludeBlacksmithRewards":false,
                 "wandmakerQuest":null,"fastMode":true,"challenges":0,
                 "requirements":[
                   {"item":"sword","kind":"WEAPON","tier":0,"tierMatch":"ANY","upgrade":2,"upgradeMatch":"EXACT",
                    "modifier":"Lucky","source":null,"identityGroup":null,"maximumDepth":null,"requireUncursed":false},
                   {"item":null,"kind":"ARMOR","tier":0,"tierMatch":"ANY","upgrade":0,"upgradeMatch":"ANY",
                    "modifier":null,"source":null,"identityGroup":2,"maximumDepth":5,"requireUncursed":true}
                 ]}}]""",
        ).apply()

        val loaded = PresetStorage(preferences).load().single()
        assertEquals("Old preset", loaded.name)
        assertEquals(9, loaded.query.maximumDepth)
        val (sword, armor) = loaded.query.requirements
        assertEquals("sword", sword.item?.id)
        assertEquals(EffectFilter.OneOf(listOf("Lucky")), sword.effect)
        assertEquals(EffectFilter.Any, armor.effect)
        assertEquals(2, armor.identityGroup)
        assertEquals(4, armor.maximumDepth)
        assertEquals(null, armor.alternativeGroup)
        assertEquals(null, armor.levelSum)
    }

    @Test
    fun newFieldsRoundTrip() {
        val preferences = MemoryPreferences()
        val storage = PresetStorage(preferences)
        val query = PresetQuery(
            requirements = listOf(
                ItemRequirement(1, ItemCatalog.findById("spear"), 3, alternativeGroup = 1),
                ItemRequirement(
                    2,
                    ItemCatalog.findById("greatshield"),
                    2,
                    effect = EffectFilter.OneOf(listOf("Blocking", "Vampiric")),
                    alternativeGroup = 1,
                ),
                ItemRequirement(3, null, 0, kind = ItemKind.ARMOR, upgradeMatch = UpgradeMatch.ANY, effect = EffectFilter.AnyEnchantment),
                ItemRequirement(4, ItemCatalog.findById("ring_might"), 0, upgradeMatch = UpgradeMatch.ANY, levelSum = LevelSum(2, 4)),
                ItemRequirement(5, ItemCatalog.findById("ring_might"), 0, upgradeMatch = UpgradeMatch.ANY, levelSum = LevelSum(2, 4)),
            ),
        )
        storage.save(listOf(QueryPreset(id = "new", name = "New preset", query = query)))

        val loaded = PresetStorage(preferences).load().single()
        assertEquals(query.requirements, loaded.query.requirements)
    }

    /** Just enough of SharedPreferences for the storage: one string key. */
    private class MemoryPreferences : SharedPreferences {
        private val values = mutableMapOf<String, Any?>()

        override fun getAll(): MutableMap<String, *> = values
        override fun getString(key: String?, defValue: String?): String? = values[key] as? String ?: defValue
        override fun getStringSet(key: String?, defValues: MutableSet<String>?): MutableSet<String>? = defValues
        override fun getInt(key: String?, defValue: Int): Int = values[key] as? Int ?: defValue
        override fun getLong(key: String?, defValue: Long): Long = values[key] as? Long ?: defValue
        override fun getFloat(key: String?, defValue: Float): Float = values[key] as? Float ?: defValue
        override fun getBoolean(key: String?, defValue: Boolean): Boolean = values[key] as? Boolean ?: defValue
        override fun contains(key: String?): Boolean = values.containsKey(key)
        override fun registerOnSharedPreferenceChangeListener(
            listener: SharedPreferences.OnSharedPreferenceChangeListener?,
        ) = Unit
        override fun unregisterOnSharedPreferenceChangeListener(
            listener: SharedPreferences.OnSharedPreferenceChangeListener?,
        ) = Unit

        override fun edit(): SharedPreferences.Editor = object : SharedPreferences.Editor {
            private val pending = mutableMapOf<String, Any?>()
            override fun putString(key: String?, value: String?) = apply { pending[key!!] = value }
            override fun putStringSet(key: String?, values: MutableSet<String>?) = apply { pending[key!!] = values }
            override fun putInt(key: String?, value: Int) = apply { pending[key!!] = value }
            override fun putLong(key: String?, value: Long) = apply { pending[key!!] = value }
            override fun putFloat(key: String?, value: Float) = apply { pending[key!!] = value }
            override fun putBoolean(key: String?, value: Boolean) = apply { pending[key!!] = value }
            override fun remove(key: String?) = apply { pending[key!!] = null }
            override fun clear() = apply { values.clear() }
            override fun commit(): Boolean {
                apply()
                return true
            }
            override fun apply() {
                values.putAll(pending)
            }
        }
    }
}
