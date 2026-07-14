// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.seedseeker.app.model.CatalogItem
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutAccessibility
import dev.seedseeker.app.model.ScoutItem
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.SearchStatus
import dev.seedseeker.app.model.UpgradeMatch
import dev.seedseeker.app.ui.theme.Amber
import dev.seedseeker.app.ui.theme.Mint
import java.util.Locale
import kotlin.math.floor
import kotlin.math.log10
import kotlin.math.pow
import kotlin.math.roundToInt

val LocalItemAtlas = staticCompositionLocalOf<ImageBitmap?> { null }
val LocalItemIconAtlas = staticCompositionLocalOf<ImageBitmap?> { null }

private const val ITEM_SPRITE_SIZE = 16
private const val ITEM_ATLAS_COLUMNS = 16
private const val ITEM_ICON_SIZE = 8
private const val ITEM_ICON_COLUMNS = 16
private val RingTypeIconSizes = listOf(
    IntSize(7, 7), // Accuracy
    IntSize(7, 7), // Arcana
    IntSize(7, 7), // Elements
    IntSize(7, 5), // Energy
    IntSize(7, 7), // Evasion
    IntSize(5, 6), // Force
    IntSize(7, 6), // Furor
    IntSize(6, 6), // Haste
    IntSize(7, 7), // Might
    IntSize(7, 7), // Sharpshooting
    IntSize(6, 6), // Tenacity
    IntSize(7, 6), // Wealth
)

/** Original compass mark used as the app's brand glyph. */
@Composable
fun CompassMark(modifier: Modifier = Modifier) {
    Canvas(modifier) {
        val stroke = size.minDimension * 0.07f
        drawCircle(Mint, style = Stroke(stroke))
        drawLine(
            color = Amber,
            start = Offset(size.width * 0.32f, size.height * 0.72f),
            end = Offset(size.width * 0.67f, size.height * 0.28f),
            strokeWidth = stroke * 1.25f,
            cap = StrokeCap.Round,
        )
        drawCircle(Color.White, radius = stroke * 0.8f, center = center)
    }
}

/** 16×16 sprite from the upstream atlas, drawn with nearest-neighbour scaling. */
@Composable
fun ItemSprite(
    item: CatalogItem,
    modifierName: String? = null,
    modifier: Modifier = Modifier,
) {
    val atlas = LocalItemAtlas.current
    val iconAtlas = LocalItemIconAtlas.current
    val placeholderColor = MaterialTheme.colorScheme.outline
    Canvas(
        modifier = modifier.semantics { contentDescription = item.name },
    ) {
        if (modifierName != null) {
            val glow = if (item.kind == ItemKind.ARMOR) Mint else Amber
            drawCircle(glow, radius = size.minDimension * 0.46f, alpha = 0.23f)
            drawCircle(glow, radius = size.minDimension * 0.34f, alpha = 0.16f)
        }
        if (atlas != null) {
            drawImage(
                image = atlas,
                srcOffset = IntOffset(
                    x = (item.spriteIndex % ITEM_ATLAS_COLUMNS) * ITEM_SPRITE_SIZE,
                    y = (item.spriteIndex / ITEM_ATLAS_COLUMNS) * ITEM_SPRITE_SIZE,
                ),
                srcSize = IntSize(ITEM_SPRITE_SIZE, ITEM_SPRITE_SIZE),
                dstOffset = IntOffset.Zero,
                dstSize = IntSize(size.width.toInt(), size.height.toInt()),
                filterQuality = FilterQuality.None,
            )
        } else {
            drawCircle(placeholderColor, radius = size.minDimension * 0.28f)
        }

        val typeIconIndex = item.typeIconIndex
        if (iconAtlas != null && typeIconIndex != null) {
            val iconSize = RingTypeIconSizes[typeIconIndex]
            val scale = size.minDimension / ITEM_SPRITE_SIZE
            val destinationSize = IntSize(
                (iconSize.width * scale).roundToInt(),
                (iconSize.height * scale).roundToInt(),
            )
            drawImage(
                image = iconAtlas,
                srcOffset = IntOffset(
                    x = (typeIconIndex % ITEM_ICON_COLUMNS) * ITEM_ICON_SIZE,
                    y = (typeIconIndex / ITEM_ICON_COLUMNS) * ITEM_ICON_SIZE,
                ),
                srcSize = iconSize,
                dstOffset = IntOffset(
                    x = ((size.width + size.minDimension) / 2).roundToInt() - destinationSize.width,
                    y = ((size.height - size.minDimension) / 2).roundToInt(),
                ),
                dstSize = destinationSize,
                filterQuality = FilterQuality.None,
            )
        }
    }
}

/** Sprite inside a soft tonal tile; falls back to a "?" for wildcard requirements. */
@Composable
fun SpriteTile(
    item: CatalogItem?,
    modifierName: String? = null,
    tileSize: Int = 60,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier.size(tileSize.dp),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerLowest,
    ) {
        Box(contentAlignment = Alignment.Center) {
            if (item == null) {
                Text(
                    "?",
                    style = MaterialTheme.typography.headlineMedium,
                    color = MaterialTheme.colorScheme.primary,
                )
            } else {
                ItemSprite(
                    item = item,
                    modifierName = modifierName,
                    modifier = Modifier.size((tileSize * 3 / 4).dp),
                )
            }
        }
    }
}

@Composable
fun SectionHeader(
    eyebrow: String,
    title: String,
    supporting: String? = null,
    modifier: Modifier = Modifier,
    trailing: @Composable () -> Unit = {},
) {
    Row(modifier, verticalAlignment = Alignment.Bottom) {
        Column(Modifier.weight(1f)) {
            Text(
                eyebrow.uppercase(Locale.ROOT),
                style = MaterialTheme.typography.labelSmall,
                letterSpacing = 1.4.sp,
                color = MaterialTheme.colorScheme.primary,
            )
            Spacer(Modifier.height(4.dp))
            Text(title, style = MaterialTheme.typography.headlineSmall)
            if (supporting != null) {
                Spacer(Modifier.height(4.dp))
                Text(
                    supporting,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        trailing()
    }
}

/** Small tonal capsule used for counts and states. */
@Composable
fun StatusPill(
    text: String,
    modifier: Modifier = Modifier,
    container: Color = MaterialTheme.colorScheme.surfaceContainerHighest,
    content: Color = MaterialTheme.colorScheme.onSurfaceVariant,
) {
    Surface(shape = MaterialTheme.shapes.large, color = container, modifier = modifier) {
        Text(
            text,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
            style = MaterialTheme.typography.labelMedium,
            color = content,
        )
    }
}

fun upgradeBadgeLabel(match: UpgradeMatch, upgrade: Int): String = when (match) {
    UpgradeMatch.ANY -> "Any"
    UpgradeMatch.EXACT -> "= +$upgrade"
    UpgradeMatch.AT_LEAST -> "≥ +$upgrade"
}

/** Region names shown next to floor numbers, as in the macOS manifest. */
fun floorRegion(depth: Int): String = when {
    depth < 6 -> "Sewers"
    depth < 11 -> "Prison"
    depth < 16 -> "Caves"
    depth < 21 -> "Dwarven City"
    else -> "Demon Halls"
}

/** Selects one distinct, jointly obtainable scout item per matched requirement. */
internal fun scoutMatchIndices(
    items: List<ScoutItem>,
    requirements: List<ItemRequirement>,
    maximumDepth: Int = 24,
    excludeBlacksmithRewards: Boolean = false,
): Set<Int> {
    fun matches(item: ScoutItem, requirement: ItemRequirement): Boolean =
        item.depth <= maximumDepth &&
            item.depth <= (requirement.maximumDepth ?: maximumDepth) &&
            (!excludeBlacksmithRewards || item.source != ScoutItemSource.BLACKSMITH_REWARD) &&
            requirement.kind == item.item.kind &&
            (requirement.item == null || requirement.item.id == item.item.id) &&
            when (requirement.tierMatch) {
                dev.seedseeker.app.model.TierMatch.ANY -> true
                dev.seedseeker.app.model.TierMatch.EXACT -> item.item.tier == requirement.tier
                dev.seedseeker.app.model.TierMatch.AT_LEAST ->
                    item.item.tier?.let { it >= requirement.tier } == true
                dev.seedseeker.app.model.TierMatch.AT_MOST ->
                    item.item.tier?.let { it <= requirement.tier } == true
            } &&
            when (requirement.upgradeMatch) {
                UpgradeMatch.ANY -> true
                UpgradeMatch.EXACT -> item.upgrade == requirement.upgrade
                UpgradeMatch.AT_LEAST -> item.upgrade >= requirement.upgrade
            } &&
            (requirement.modifier == null || requirement.modifier == item.effect) &&
            (!requirement.requireUncursed || !item.cursed) &&
            (requirement.source == null || requirement.source == item.source)

    val candidates = requirements
        .map { requirement -> requirement to items.indices.filter { matches(items[it], requirement) } }
        .sortedBy { it.second.size }
    val used = mutableSetOf<Int>()
    val selected = mutableSetOf<Int>()
    var best = emptySet<Int>()
    val scenarios = mutableMapOf<Int, ULong>()
    val identities = mutableMapOf<Int, String>()

    fun visit(position: Int) {
        if (position == candidates.size) {
            if (selected.size > best.size) best = selected.toSet()
            return
        }
        if (selected.size + candidates.size - position <= best.size) return
        val (requirement, itemCandidates) = candidates[position]
        for (index in itemCandidates) {
            if (index in used) continue
            val item = items[index]
            val identityGroup = requirement.identityGroup
            val previousIdentity = identityGroup?.let { identities[it] }
            if (identityGroup != null && previousIdentity != null && previousIdentity != item.item.id) continue
            if (identityGroup != null) identities[identityGroup] = item.item.id

            val constraint = when (val accessibility = item.accessibility) {
                ScoutAccessibility.Independent -> null
                is ScoutAccessibility.Choice -> accessibility.group to (1UL shl accessibility.option)
                is ScoutAccessibility.Scenarios -> accessibility.group to accessibility.mask
            }
            val previousScenarios = constraint?.first?.let { scenarios[it] }
            if (constraint != null) {
                val compatible = (previousScenarios ?: ULong.MAX_VALUE) and constraint.second
                if (compatible == 0UL) {
                    if (identityGroup != null) {
                        if (previousIdentity == null) identities.remove(identityGroup)
                        else identities[identityGroup] = previousIdentity
                    }
                    continue
                }
                scenarios[constraint.first] = compatible
            }
            used += index
            selected += index
            visit(position + 1)
            used -= index
            selected -= index
            if (constraint != null) {
                if (previousScenarios == null) scenarios.remove(constraint.first)
                else scenarios[constraint.first] = previousScenarios
            }
            if (identityGroup != null) {
                if (previousIdentity == null) identities.remove(identityGroup)
                else identities[identityGroup] = previousIdentity
            }
        }
        visit(position + 1)
    }
    visit(0)
    return best
}

fun compactCount(value: Long): String = when {
    value >= 1_000_000_000_000L -> String.format(Locale.US, "%.2fT", value / 1_000_000_000_000.0)
    value >= 1_000_000_000L -> String.format(Locale.US, "%.2fB", value / 1_000_000_000.0)
    value >= 1_000_000L -> String.format(Locale.US, "%.1fM", value / 1_000_000.0)
    value >= 1_000L -> String.format(Locale.US, "%.1fK", value / 1_000.0)
    else -> value.toString()
}

internal fun searchEstimateText(status: SearchStatus?, seedsPerSecond: Double): String {
    val rate = formatSeedRate(seedsPerSecond)
    val probability = status?.matchProbability ?: 0.0
    if (probability <= 0.0 || seedsPerSecond <= 0.0) {
        return "Seed match probability: estimating… TTS @ $rate seeds/s: estimating…"
    }
    return "Seed match probability: ${formatProbabilityPercent(probability)} " +
        "TTS @ $rate seeds/s: ${formatEstimateDuration(1.0 / probability / seedsPerSecond)}"
}

private fun formatProbabilityPercent(probability: Double): String {
    val percent = probability * 100.0
    var exponent = floor(log10(percent)).toInt()
    var mantissa = percent / 10.0.pow(exponent)
    if (mantissa >= 9.95) {
        mantissa = 1.0
        exponent += 1
    }
    return String.format(Locale.US, "%.1fx10^%d%%", mantissa, exponent)
}

private fun formatSeedRate(rate: Double): String = when {
    rate <= 0.0 -> "—"
    rate >= 1_000_000.0 -> String.format(Locale.US, "%.1fM", rate / 1_000_000.0)
    rate >= 1_000.0 -> String.format(Locale.US, "%.1fk", rate / 1_000.0)
    else -> String.format(Locale.US, "%.0f", rate)
}

private fun formatEstimateDuration(seconds: Double): String {
    val (value, unit) = when {
        seconds < 60.0 -> seconds to "second"
        seconds < 3_600.0 -> seconds / 60.0 to "minute"
        seconds < 86_400.0 -> seconds / 3_600.0 to "hour"
        else -> seconds / 86_400.0 to "day"
    }
    val plural = if (value >= 0.95 && value < 1.05) "" else "s"
    return String.format(Locale.US, "%.1f %s%s", value, unit, plural)
}

internal fun formatElapsedTime(seconds: Long): String = when {
    seconds < 60 -> "${seconds}s"
    seconds < 3_600 -> "${seconds / 60}m ${seconds % 60}s"
    else -> "${seconds / 3_600}h ${(seconds % 3_600) / 60}m"
}
