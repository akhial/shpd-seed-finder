// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialExpressiveTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MotionScheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// Brand palette mirrored from the web front-end's design tokens (see
// `web/src/designs/one/styles.css` `:root`), which use Shattered Pixel Dungeon's
// official colours: dark slate-blue surfaces, the game's yellow for seeds and
// headings, its accent green for primary actions, teal for enchantments and a
// muted red for curses.
val SpdYellow = Color(0xFFFFFF55)
val SpdAccent = Color(0xFF56BD52)
val SpdTeal = Color(0xFF58C2B4)
val SpdGreen = Color(0xFF6EC98F)
val SpdWarn = Color(0xFFE0A83C)
val SpdDanger = Color(0xFFD96C5F)

/** The game's upgrade green, used for `+N` badges. */
val SpdUpgrade = Color(0xFF83FC64)

/** Curse text/badge tint, lighter than [SpdDanger] so it reads on a dark fill. */
val SpdCurse = Color(0xFFF2958A)

// Region accents, mirrored from `web/src/lib/region.ts`.
val RegionSewers = Color(0xFF7FE2B8)
val RegionPrison = Color(0xFF8FB7E8)
val RegionCaves = Color(0xFFD8A26B)
val RegionCity = Color(0xFFC9A6E8)
val RegionHalls = Color(0xFFE88F8F)

private val SeedSeekerColors = darkColorScheme(
    primary = SpdAccent,
    onPrimary = Color(0xFF08210A),
    primaryContainer = Color(0xFF1F4423),
    onPrimaryContainer = Color(0xFFBCEDB8),
    inversePrimary = Color(0xFF2F7A2C),
    secondary = SpdTeal,
    onSecondary = Color(0xFF002F2B),
    secondaryContainer = Color(0xFF16403C),
    onSecondaryContainer = Color(0xFFA8E7DF),
    tertiary = SpdYellow,
    onTertiary = Color(0xFF2C2C00),
    tertiaryContainer = Color(0xFF474710),
    onTertiaryContainer = Color(0xFFFFFFBA),
    background = Color(0xFF1B1D23),
    onBackground = Color(0xFFEAEAEA),
    surface = Color(0xFF1B1D23),
    onSurface = Color(0xFFEAEAEA),
    surfaceVariant = Color(0xFF333846),
    onSurfaceVariant = Color(0xFFA2A6B0),
    surfaceContainerLowest = Color(0xFF14161B),
    surfaceContainerLow = Color(0xFF1F2229),
    surfaceContainer = Color(0xFF23262F),
    surfaceContainerHigh = Color(0xFF262A33),
    surfaceContainerHighest = Color(0xFF2E323D),
    surfaceTint = SpdAccent,
    inverseSurface = Color(0xFFEAEAEA),
    inverseOnSurface = Color(0xFF262A33),
    outline = Color(0xFF6F7480),
    outlineVariant = Color(0xFF3F4553),
    error = SpdDanger,
    onError = Color(0xFF3A0C07),
    errorContainer = Color(0xFF4A1C17),
    onErrorContainer = SpdCurse,
    scrim = Color(0xFF000000),
)

private val SeedSeekerShapes = Shapes(
    extraSmall = RoundedCornerShape(8.dp),
    small = RoundedCornerShape(12.dp),
    medium = RoundedCornerShape(16.dp),
    large = RoundedCornerShape(24.dp),
    extraLarge = RoundedCornerShape(32.dp),
)

private val SeedSeekerTypography
    @Composable get() = MaterialTheme.typography.copy(
        displaySmall = TextStyle(
            fontFamily = FontFamily.SansSerif,
            fontWeight = FontWeight.ExtraBold,
            fontSize = 34.sp,
            lineHeight = 40.sp,
            letterSpacing = (-0.8).sp,
        ),
        headlineLarge = TextStyle(
            fontFamily = FontFamily.SansSerif,
            fontWeight = FontWeight.Bold,
            fontSize = 32.sp,
            lineHeight = 38.sp,
            letterSpacing = (-0.6).sp,
        ),
        headlineMedium = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Bold),
        headlineSmall = MaterialTheme.typography.headlineSmall.copy(fontWeight = FontWeight.Bold),
        titleLarge = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.SemiBold),
        titleMedium = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.SemiBold),
        labelLarge = MaterialTheme.typography.labelLarge.copy(fontWeight = FontWeight.SemiBold),
    )

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun SeedSeekerTheme(content: @Composable () -> Unit) {
    MaterialExpressiveTheme(
        colorScheme = SeedSeekerColors,
        motionScheme = MotionScheme.expressive(),
        shapes = SeedSeekerShapes,
        typography = SeedSeekerTypography,
        content = content,
    )
}
