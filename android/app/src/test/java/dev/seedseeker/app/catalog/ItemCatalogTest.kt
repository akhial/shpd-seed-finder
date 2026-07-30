// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.catalog

import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.WeaponClass
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ItemCatalogTest {
    @Test
    fun generatedEquipmentUsesCanonicalSpriteConstants() {
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

    @Test
    fun weaponsSplitIntoMeleeAndThrownClasses() {
        assertEquals(31, ItemCatalog.meleeWeapons.size)
        assertEquals(27, ItemCatalog.thrownWeapons.size)
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
        assertEquals(ItemCatalog.modifiersFor(ItemKind.WEAPON), ItemCatalog.modifiersFor(ItemKind.THROWN_WEAPON))
        assertEquals(ItemCatalog.cursesFor(ItemKind.WEAPON), ItemCatalog.cursesFor(ItemKind.MELEE_WEAPON))
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
    fun idsAndModifierNamesAreStableAndUnique() {
        assertEquals(ItemCatalog.all.size, ItemCatalog.all.map { it.id }.toSet().size)
        assertEquals(13, ItemCatalog.enchantments.size)
        assertEquals(8, ItemCatalog.weaponCurses.size)
        assertEquals(13, ItemCatalog.glyphs.size)
        assertEquals(8, ItemCatalog.armorCurses.size)
        assertEquals("gloves", ItemCatalog.weapons.first { it.spriteIndex == 98 }.id)
        assertEquals("gauntlet", ItemCatalog.weapons.first { it.spriteIndex == 133 }.id)
        assertEquals("rot_dart", ItemCatalog.weapons.first { it.spriteIndex == 161 }.id)
        assertEquals("blinding_dart", ItemCatalog.weapons.first { it.spriteIndex == 172 }.id)
        assertEquals("ring_accuracy", ItemCatalog.rings.first().id)
        assertEquals("ring_wealth", ItemCatalog.rings.last().id)
    }
}
