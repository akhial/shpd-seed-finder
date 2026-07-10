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

// Brand palette: deep mossy greens with a mint primary and amber tertiary,
// mirroring the macOS app's dungeon-lantern look.
val Mint = Color(0xFF7FE2B8)
val Amber = Color(0xFFFFC96B)

private val SeedSeekerColors = darkColorScheme(
    primary = Mint,
    onPrimary = Color(0xFF00382A),
    primaryContainer = Color(0xFF1C5140),
    onPrimaryContainer = Color(0xFFA9F2D3),
    inversePrimary = Color(0xFF1F6B52),
    secondary = Color(0xFFB2CCBE),
    onSecondary = Color(0xFF1E352B),
    secondaryContainer = Color(0xFF354C41),
    onSecondaryContainer = Color(0xFFCEE9DA),
    tertiary = Amber,
    onTertiary = Color(0xFF422C00),
    tertiaryContainer = Color(0xFF5F4100),
    onTertiaryContainer = Color(0xFFFFDEA8),
    background = Color(0xFF0B1110),
    onBackground = Color(0xFFE2E9E4),
    surface = Color(0xFF0B1110),
    onSurface = Color(0xFFE2E9E4),
    surfaceVariant = Color(0xFF243230),
    onSurfaceVariant = Color(0xFFA9B8B0),
    surfaceContainerLowest = Color(0xFF060B0A),
    surfaceContainerLow = Color(0xFF101817),
    surfaceContainer = Color(0xFF141D1B),
    surfaceContainerHigh = Color(0xFF1B2624),
    surfaceContainerHighest = Color(0xFF25312E),
    surfaceTint = Mint,
    inverseSurface = Color(0xFFE2E9E4),
    inverseOnSurface = Color(0xFF1B2624),
    outline = Color(0xFF55655F),
    outlineVariant = Color(0xFF2E3B37),
    error = Color(0xFFFFB4A9),
    onError = Color(0xFF690005),
    errorContainer = Color(0xFF93000A),
    onErrorContainer = Color(0xFFFFDAD5),
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
