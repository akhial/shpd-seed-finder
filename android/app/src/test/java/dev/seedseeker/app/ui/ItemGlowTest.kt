// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import androidx.compose.ui.graphics.Color
import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.ItemKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Parity checks against `web/src/lib/glow.ts`, which mirrors `ItemSprite.Glowing`. */
class ItemGlowTest {
    private val black = Color(0xFF000000)

    @Test
    fun `weapon enchantments glow their upstream colour and period`() {
        assertEquals(Glow(Color(0xFFFF4400), 1f), ItemGlows.forEffect("Blazing"))
        assertEquals(Glow(Color(0xFF00FF00), 1f), ItemGlows.forEffect("Lucky"))
        // Shocking is the one enchantment with a faster-than-default pulse.
        assertEquals(Glow(Color(0xFFFFFFFF), 0.5f), ItemGlows.forEffect("Shocking"))
    }

    @Test
    fun `armor glyphs glow their upstream colour and period`() {
        assertEquals(Glow(Color(0xFFFF4400), 1f), ItemGlows.forEffect("Brimstone"))
        assertEquals(Glow(Color(0xFFFFFFFF), 0.6f), ItemGlows.forEffect("Potential"))
        // The hyphenated wire name must match the catalog spelling exactly.
        assertEquals(Glow(Color(0xFF88EEFF), 1f), ItemGlows.forEffect("Anti-Magic"))
    }

    @Test
    fun `every catalog enchantment and glyph has its own glow`() {
        for (effect in ItemCatalog.enchantments + ItemCatalog.glyphs) {
            val glow = requireNotNull(ItemGlows.forEffect(effect)) { "$effect has no glow" }
            // Grim is the only beneficial effect that legitimately glows black.
            if (effect != "Grim") {
                assertNotEquals("$effect must not fall through to the curse glow", black, glow.color)
            }
        }
    }

    @Test
    fun `curses fall through to the black glow`() {
        for (curse in ItemCatalog.cursesFor(ItemKind.WEAPON) + ItemCatalog.cursesFor(ItemKind.ARMOR)) {
            assertEquals("$curse must glow black", Glow(black, 1f), ItemGlows.forEffect(curse))
        }
        assertNull(ItemGlows.forEffect(null))
    }

    @Test
    fun `a beneficial enchantment wins over a curse`() {
        assertEquals(
            Glow(Color(0xFFFFFF00), 1f),
            ItemGlows.forItem(effect = "Kinetic", cursed = true),
        )
        assertEquals(Glow(black, 1f), ItemGlows.forItem(effect = "Wayward", cursed = true))
        assertEquals(Glow(black, 1f), ItemGlows.forItem(effect = null, cursed = true))
        assertNull(ItemGlows.forItem(effect = null, cursed = false))
    }
}
