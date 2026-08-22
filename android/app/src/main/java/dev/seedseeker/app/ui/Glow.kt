// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import android.content.Context
import android.provider.Settings
import androidx.compose.animation.core.withInfiniteAnimationFrameMillis
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.LongState
import androidx.compose.runtime.Stable
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.ItemKind
import kotlin.math.abs

/**
 * An enchantment / curse glow: the colour the sprite blends toward at the pulse
 * peak, plus the seconds it takes to reach that peak. The glow fades back out
 * over the same span, so one full cycle lasts `2 × period`.
 */
data class Glow(val color: Color, val period: Float)

/**
 * Enchantment / glyph glow colours and pulse periods, mirrored 1:1 from
 * Shattered Pixel Dungeon's `ItemSprite.Glowing` definitions — and kept in step
 * with the web front-end's `web/src/lib/glow.ts`, which is keyed by the same
 * wire names the scout emits. Curses are absent from the table because every
 * curse glows black in the game; the catalog names them.
 */
object ItemGlows {
    /** Upstream's default `Glowing(color)` period when none is given (1f). */
    private const val DEFAULT_PERIOD = 1f

    private val enchantments: Map<String, Glow> = mapOf(
        // Weapon enchantments
        "Blazing" to Glow(Color(0xFFFF4400), DEFAULT_PERIOD),
        "Chilling" to Glow(Color(0xFF00FFFF), DEFAULT_PERIOD),
        "Kinetic" to Glow(Color(0xFFFFFF00), DEFAULT_PERIOD),
        "Shocking" to Glow(Color(0xFFFFFFFF), 0.5f),
        "Blocking" to Glow(Color(0xFF0000FF), DEFAULT_PERIOD),
        "Blooming" to Glow(Color(0xFF008800), DEFAULT_PERIOD),
        "Elastic" to Glow(Color(0xFFFF00FF), DEFAULT_PERIOD),
        "Lucky" to Glow(Color(0xFF00FF00), DEFAULT_PERIOD),
        "Projecting" to Glow(Color(0xFF8844CC), DEFAULT_PERIOD),
        "Unstable" to Glow(Color(0xFF999999), DEFAULT_PERIOD),
        "Corrupting" to Glow(Color(0xFF440066), DEFAULT_PERIOD),
        "Grim" to Glow(Color(0xFF000000), DEFAULT_PERIOD),
        "Vampiric" to Glow(Color(0xFF660022), DEFAULT_PERIOD),
        // Armor glyphs
        "Obfuscation" to Glow(Color(0xFF888888), DEFAULT_PERIOD),
        "Swiftness" to Glow(Color(0xFFFFFF00), DEFAULT_PERIOD),
        "Viscosity" to Glow(Color(0xFF8844CC), DEFAULT_PERIOD),
        "Potential" to Glow(Color(0xFFFFFFFF), 0.6f),
        "Brimstone" to Glow(Color(0xFFFF4400), DEFAULT_PERIOD),
        "Stone" to Glow(Color(0xFF222222), DEFAULT_PERIOD),
        "Entanglement" to Glow(Color(0xFF663300), DEFAULT_PERIOD),
        "Repulsion" to Glow(Color(0xFFFFFFFF), DEFAULT_PERIOD),
        "Camouflage" to Glow(Color(0xFF448822), DEFAULT_PERIOD),
        "Flow" to Glow(Color(0xFF0000FF), DEFAULT_PERIOD),
        "Affection" to Glow(Color(0xFFFF4488), DEFAULT_PERIOD),
        "Anti-Magic" to Glow(Color(0xFF88EEFF), DEFAULT_PERIOD),
        "Thorns" to Glow(Color(0xFF660022), DEFAULT_PERIOD),
    )

    /** Every curse glows black in the game, at the default period. */
    private val curse = Glow(Color(0xFF000000), DEFAULT_PERIOD)

    /** The catalog's own curse names, for weapons and armor alike. */
    private val curses: Set<String> by lazy {
        (ItemCatalog.cursesFor(ItemKind.WEAPON) + ItemCatalog.cursesFor(ItemKind.ARMOR)).toSet()
    }

    /**
     * The pulse glow for a scouted item, or null when it carries no enchantment
     * or curse. A beneficial enchantment/glyph wins even on a cursed item
     * (matching `Weapon.glowing()`, which returns the enchantment's colour when
     * one is present — a curse-infused Kinetic weapon still glows yellow);
     * otherwise a cursed item pulses black.
     */
    fun forItem(effect: String?, cursed: Boolean): Glow? =
        effect?.let(::forEffect) ?: if (cursed) curse else null

    /**
     * The pulse glow for a bare effect name (as carried by a requirement), or
     * null when there is none. Enchantments and glyphs pulse their own colour;
     * the catalog's curses pulse black.
     */
    fun forEffect(effect: String?): Glow? {
        if (effect == null) return null
        return enchantments[effect] ?: curse.takeIf { effect in curses }
    }
}

/** Peak blend toward the glow colour, matching the web's `d1-ench-pulse`. */
private const val GLOW_PEAK_ALPHA = 0.6f

/** Frozen blend used when the system asks for reduced motion. */
private const val GLOW_STATIC_ALPHA = 0.3f

/**
 * One shared pulse clock for every glowing sprite on screen. Sprites read the
 * elapsed time inside their draw lambda, so a frame invalidates drawing only —
 * never composition — and dozens of scout rows animate off a single frame
 * subscription instead of one infinite transition each.
 */
@Stable
class GlowPulse internal constructor(
    private val elapsedMillis: LongState,
    private val animated: Boolean,
) {
    /**
     * The blend factor for a glow of this period: linear 0 → 0.6 → 0 across
     * `2 × period` seconds, reproducing upstream's `texel*(1-v) + glow*v` shader
     * exactly as the web animates it. Held at a static 0.3 under reduced motion.
     */
    fun alphaFor(period: Float): Float {
        if (!animated) return GLOW_STATIC_ALPHA
        val cycleMillis = (2_000f * period).toLong().coerceAtLeast(1L)
        val phase = (elapsedMillis.longValue % cycleMillis) / cycleMillis.toFloat()
        return GLOW_PEAK_ALPHA * (1f - abs(2f * phase - 1f))
    }
}

private val StaticGlowPulse = GlowPulse(mutableLongStateOf(0L), animated = false)

/** The pulse clock in scope; a frozen one when no host provided a running clock. */
val LocalGlowPulse = staticCompositionLocalOf { StaticGlowPulse }

/** Starts the app-wide glow clock, or a frozen one when animations are off. */
@Composable
fun rememberGlowPulse(): GlowPulse {
    val context = LocalContext.current
    val animated = remember(context) { animationsEnabled(context) }
    val elapsed = remember { mutableLongStateOf(0L) }
    LaunchedEffect(animated) {
        if (!animated) return@LaunchedEffect
        var origin = -1L
        while (true) {
            withInfiniteAnimationFrameMillis { frameMillis ->
                if (origin < 0L) origin = frameMillis
                elapsed.longValue = frameMillis - origin
            }
        }
    }
    return remember(elapsed, animated) { GlowPulse(elapsed, animated) }
}

/**
 * False when the user has turned animations off — the developer-options animator
 * scale and the "remove animations" accessibility toggle both zero this setting.
 */
private fun animationsEnabled(context: Context): Boolean =
    Settings.Global.getFloat(
        context.contentResolver,
        Settings.Global.ANIMATOR_DURATION_SCALE,
        1f,
    ) != 0f
