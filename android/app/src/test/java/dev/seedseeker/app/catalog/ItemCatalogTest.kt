// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.catalog

import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.WeaponClass
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The loaded catalog against the asset it is loaded from — the same
 * third-party extraction every other frontend reads — so a stale or reordered
 * asset fails here instead of showing the wrong sprite for an item id the
 * engine sends. Gradle runs unit tests with the module directory as the
 * working directory, which is where [PackagedCatalog] finds the asset for
 * [ItemCatalog] too.
 */
class ItemCatalogTest {
    init { PackagedCatalog.install() }

    private val asset: JSONObject by lazy {
        val file = java.io.File(
            PackagedCatalog.directory,
            "third_party/shattered-pixel-dungeon/catalog-v4.0.0.json",
        )
        check(file.exists()) { "catalog asset not found at ${file.absolutePath}" }
        JSONObject(file.readText())
    }

    private val entries: List<JSONObject> by lazy {
        val list = asset.getJSONArray("entries")
        List(list.length()) { list.getJSONObject(it) }
    }

    private fun modifierList(key: String): List<String> {
        val list: JSONArray = asset.getJSONObject("modifiers").getJSONArray(key)
        return List(list.length()) { list.getString(it) }
    }

    @Test
    fun everyAssetEntryIsLoadedWithItsIdNameSpriteAndTier() {
        assertEquals(116, entries.size)
        assertEquals(entries.size, ItemCatalog.all.size)
        for (entry in entries) {
            val item = ItemCatalog.findById(entry.getString("id"))
            assertEquals("missing ${entry.getString("id")}", entry.getString("id"), item?.id)
            checkNotNull(item)
            assertEquals(item.id, entry.getString("name"), item.name)
            assertEquals(item.id, entry.getInt("sprite"), item.spriteIndex)
            assertEquals(item.id, if (entry.has("tier")) entry.getInt("tier") else null, item.tier)
            assertEquals(
                item.id,
                if (entry.has("typeIcon")) entry.getInt("typeIcon") else null,
                item.typeIconIndex,
            )
        }
    }

    @Test
    fun assetTypesAndWeaponClassesBecomeCatalogKinds() {
        val expected = entries.groupBy { it.getString("type") }
        assertEquals(expected.getValue("weapon").size, ItemCatalog.weapons.size)
        assertEquals(expected.getValue("armor").size, ItemCatalog.armor.size)
        assertEquals(expected.getValue("wand").size, ItemCatalog.wands.size)
        assertEquals(expected.getValue("ring").size, ItemCatalog.rings.size)
        assertEquals(11, ItemCatalog.artifacts.size)
        assertEquals(ItemCatalog.artifacts, ItemCatalog.forKind(ItemKind.ARTIFACT))

        val melee = entries.filter { it.optString("class") == "melee" }.map { it.getString("id") }
        val thrown = entries.filter { it.optString("class") == "thrown" }.map { it.getString("id") }
        assertEquals(melee, ItemCatalog.meleeWeapons.map { it.id })
        assertEquals(thrown, ItemCatalog.thrownWeapons.map { it.id })
        assertEquals(ItemCatalog.meleeWeapons + ItemCatalog.thrownWeapons, ItemCatalog.weapons)
        assertTrue(ItemCatalog.meleeWeapons.all { it.weaponClass == WeaponClass.MELEE })
        assertTrue(ItemCatalog.thrownWeapons.all { it.weaponClass == WeaponClass.THROWN })
        assertTrue(
            ItemCatalog.all
                .filterNot { it.kind == ItemKind.WEAPON }
                .all { it.weaponClass == null },
        )
        // The crossbow is wielded; every dart and "throwing" item is thrown.
        assertEquals(WeaponClass.MELEE, ItemCatalog.findById("crossbow")?.weaponClass)
        assertEquals(WeaponClass.THROWN, ItemCatalog.findById("shuriken")?.weaponClass)
        assertEquals(WeaponClass.THROWN, ItemCatalog.findById("poison_dart")?.weaponClass)
        assertEquals(ItemCatalog.meleeWeapons, ItemCatalog.forKind(ItemKind.MELEE_WEAPON))
        assertEquals(ItemCatalog.thrownWeapons, ItemCatalog.forKind(ItemKind.THROWN_WEAPON))
    }

    @Test
    fun modifierListsAreTheAssetsFour() {
        assertEquals(modifierList("weaponEnchantments"), ItemCatalog.enchantments)
        assertEquals(modifierList("weaponCurses"), ItemCatalog.weaponCurses)
        assertEquals(modifierList("armorGlyphs"), ItemCatalog.glyphs)
        assertEquals(modifierList("armorCurses"), ItemCatalog.armorCurses)
        assertEquals(
            ItemCatalog.enchantments + ItemCatalog.weaponCurses,
            ItemCatalog.modifiersFor(ItemKind.WEAPON),
        )
        assertEquals(ItemCatalog.modifiersFor(ItemKind.WEAPON), ItemCatalog.modifiersFor(ItemKind.THROWN_WEAPON))
        assertEquals(
            ItemCatalog.glyphs + ItemCatalog.armorCurses,
            ItemCatalog.modifiersFor(ItemKind.ARMOR),
        )
        assertEquals(ItemCatalog.weaponCurses, ItemCatalog.cursesFor(ItemKind.MELEE_WEAPON))
        assertEquals(emptyList<String>(), ItemCatalog.modifiersFor(ItemKind.WAND))
    }

    @Test
    fun nonGeneratedEquipmentIsNotSearchable() {
        val spriteIndices = ItemCatalog.all.map { it.spriteIndex }.toSet()
        assertFalse("Mages Staff has zero generator weight", 101 in spriteIndices)
        assertFalse("Spirit Bow is hero equipment, not generated loot", 144 in spriteIndices)
        assertFalse("Plain darts have zero Generator weight", 160 in spriteIndices)
        assertFalse("Hero/class armors are not generated equipment", 181 in spriteIndices)
        assertTrue(ItemCatalog.all.none { it.id.contains("pickaxe") })
    }

    @Test
    fun idsAreUniqueAndSpritesAreCanonicalConstants() {
        assertEquals(ItemCatalog.all.size, ItemCatalog.all.map { it.id }.toSet().size)
        assertEquals(
            (96..100).toSet() +
                (104..109).toSet() +
                (112..117).toSet() +
                (120..126).toSet() +
                (128..134).toSet() +
                (145..159).toSet() +
                (161..172).toSet(),
            ItemCatalog.weapons.map { it.spriteIndex }.toSet(),
        )
        assertEquals((176..180).toList(), ItemCatalog.armor.map { it.spriteIndex })
        assertEquals((208..220).toList(), ItemCatalog.wands.map { it.spriteIndex })
        assertEquals((224..235).toList(), ItemCatalog.rings.map { it.spriteIndex })
        assertEquals((0..11).toList(), ItemCatalog.rings.map { it.typeIconIndex })
        assertTrue(ItemCatalog.all.filterNot { it.kind == ItemKind.RING }.all { it.typeIconIndex == null })
    }
}
