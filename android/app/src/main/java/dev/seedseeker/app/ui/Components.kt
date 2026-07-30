// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import dev.seedseeker.app.R
import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.CatalogItem
import dev.seedseeker.app.model.EffectRequirement
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutAccessibility
import dev.seedseeker.app.model.ScoutItem
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.SearchStatus
import dev.seedseeker.app.model.TierMatch
import dev.seedseeker.app.model.UpgradeMatch
import dev.seedseeker.app.ui.theme.RegionCaves
import dev.seedseeker.app.ui.theme.RegionCity
import dev.seedseeker.app.ui.theme.RegionHalls
import dev.seedseeker.app.ui.theme.RegionPrison
import dev.seedseeker.app.ui.theme.RegionSewers
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

/**
 * The app's brand glyph: the launcher icon itself, so the About screen and the
 * home screen cannot drift apart.
 *
 * Built from the adaptive icon's own two layers rather than @mipmap/ic_launcher,
 * which resolves to an AdaptiveIconDrawable that Compose's painterResource
 * cannot load. Those layers are authored on a 108dp viewport of which launchers
 * only ever show the middle 72dp, so the content is scaled by 108/72 and
 * clipped to reproduce the crop a user sees on their home screen.
 */
@Composable
fun BrandMark(modifier: Modifier = Modifier) {
    Box(modifier.clip(MaterialTheme.shapes.extraLarge)) {
        for (layer in intArrayOf(R.drawable.ic_launcher_background, R.drawable.ic_launcher_foreground)) {
            Image(
                painter = painterResource(layer),
                contentDescription = null,
                modifier = Modifier
                    .matchParentSize()
                    .scale(ADAPTIVE_ICON_VIEWPORT / ADAPTIVE_ICON_VISIBLE),
            )
        }
    }
}

private const val ADAPTIVE_ICON_VIEWPORT = 108f
private const val ADAPTIVE_ICON_VISIBLE = 72f

/**
 * 16×16 sprite from the upstream atlas, drawn with nearest-neighbour scaling.
 *
 * The art is anchored to the top-left of its atlas cell, so [LocalItemAtlas]
 * holds a copy whose cells were re-centred on their alpha bounding box at decode
 * time (see `centerSpriteCells`), keeping small items like rings and darts
 * centred at the same pixel scale the web front-end renders them at.
 *
 * A [glow] paints the sprite's own opaque pixels with the enchantment or curse
 * colour at the shared pulse clock's current blend factor — the same masked
 * tint the web uses, reproducing upstream's `texel*(1-v) + glow*v` shader with
 * no bloom or halo outside the silhouette.
 */
@Composable
fun ItemSprite(
    item: CatalogItem,
    glow: Glow? = null,
    modifier: Modifier = Modifier,
) {
    val atlas = LocalItemAtlas.current
    val iconAtlas = LocalItemIconAtlas.current
    val pulse = LocalGlowPulse.current
    val placeholderColor = MaterialTheme.colorScheme.outline
    Canvas(
        modifier = modifier.semantics { contentDescription = item.name },
    ) {
        if (atlas != null) {
            val srcOffset = IntOffset(
                x = (item.spriteIndex % ITEM_ATLAS_COLUMNS) * ITEM_SPRITE_SIZE,
                y = (item.spriteIndex / ITEM_ATLAS_COLUMNS) * ITEM_SPRITE_SIZE,
            )
            val srcSize = IntSize(ITEM_SPRITE_SIZE, ITEM_SPRITE_SIZE)
            val dstSize = IntSize(size.width.toInt(), size.height.toInt())
            drawImage(
                image = atlas,
                srcOffset = srcOffset,
                srcSize = srcSize,
                dstOffset = IntOffset.Zero,
                dstSize = dstSize,
                filterQuality = FilterQuality.None,
            )
            if (glow != null) {
                // Reading the clock here keeps the pulse in the draw phase, so a
                // frame never recomposes a scout row.
                drawImage(
                    image = atlas,
                    srcOffset = srcOffset,
                    srcSize = srcSize,
                    dstOffset = IntOffset.Zero,
                    dstSize = dstSize,
                    colorFilter = ColorFilter.tint(
                        color = glow.color.copy(alpha = pulse.alphaFor(glow.period)),
                        blendMode = BlendMode.SrcIn,
                    ),
                    filterQuality = FilterQuality.None,
                )
            }
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

/**
 * Sprite inside a soft tonal tile; falls back to a "?" for wildcard
 * requirements. Used by the requirement editor and its pickers — scout rows show
 * bare sprites on the row background, as the web does.
 */
@Composable
fun SpriteTile(
    item: CatalogItem?,
    glow: Glow? = null,
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
                    glow = glow,
                    modifier = Modifier.size((tileSize * 3 / 4).dp),
                )
            }
        }
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

/** Condensed constraint list shown under a requirement's title; empty when unconstrained. */
fun requirementDetailLine(requirement: ItemRequirement): String = buildList {
    when (requirement.upgradeMatch) {
        UpgradeMatch.ANY -> Unit
        UpgradeMatch.EXACT -> add("+${requirement.upgrade}")
        UpgradeMatch.AT_LEAST -> add("≥+${requirement.upgrade}")
    }
    requirement.effectSummary?.let { add(it) }
    if (requirement.requireUncursed) add("uncursed")
    requirement.source?.let { add(it.label) }
    requirement.identityGroup?.let { add("grp ${('A' + it - 1)}") }
    requirement.upgradeSumTotal?.let {
        add("combined +$it total (group ${('A' + (requirement.upgradeSumGroup ?: 1) - 1)})")
    }
    requirement.maximumDepth?.let { add("≤ floor $it") }
}.joinToString(" · ")

/** One-line summary of the search scope, listing only active constraints. */
fun scopeSummaryText(
    maximumDepth: Int,
    requireBlacksmith: Boolean,
    excludeBlacksmithRewards: Boolean,
    fastMode: Boolean,
    challenges: Int,
): String = buildList {
    add("≤ floor $maximumDepth")
    if (requireBlacksmith) add("smith")
    if (excludeBlacksmithRewards) add("no smith rewards")
    if (fastMode) add("fast")
    when (val count = Integer.bitCount(challenges)) {
        0 -> Unit
        1 -> add("1 challenge")
        else -> add("$count challenges")
    }
}.joinToString(" · ")

/** Region names shown next to floor numbers, as in the macOS manifest. */
fun floorRegion(depth: Int): String = when {
    depth < 6 -> "Sewers"
    depth < 11 -> "Prison"
    depth < 16 -> "Caves"
    depth < 21 -> "Dwarven City"
    else -> "Demon Halls"
}

/** Region accent for a floor, mirrored from the web's `regionForDepth`. */
fun floorRegionColor(depth: Int): Color = when {
    depth < 6 -> RegionSewers
    depth < 11 -> RegionPrison
    depth < 16 -> RegionCaves
    depth < 21 -> RegionCity
    else -> RegionHalls
}

/** Highest upgrade any generated item can carry (+4 rings from the Imp). */
private const val MAX_ITEM_UPGRADE = 4

/**
 * Selects the distinct, jointly obtainable scout items that satisfy as many
 * query slots as possible, mirroring the engine's `best_match_indices`: an
 * alternative group is one slot served by any single member, and a
 * combined-upgrade group only counts — and is only highlighted — when every
 * member is assigned and the upgrade total is met.
 */
internal fun scoutMatchIndices(
    items: List<ScoutItem>,
    requirements: List<ItemRequirement>,
    maximumDepth: Int = 24,
    excludeBlacksmithRewards: Boolean = false,
): Set<Int> {
    if (requirements.isEmpty()) return emptySet()

    fun matchesEffect(item: ScoutItem, requirement: ItemRequirement): Boolean =
        when (val wanted = requirement.effect) {
            EffectRequirement.Any -> true
            EffectRequirement.AnyEnchantment ->
                item.effect != null && item.effect !in ItemCatalog.cursesFor(requirement.kind)
            is EffectRequirement.OneOf -> item.effect in wanted.effects
        }

    fun matches(item: ScoutItem, requirement: ItemRequirement): Boolean =
        item.depth <= maximumDepth &&
            item.depth <= (requirement.maximumDepth ?: maximumDepth) &&
            (!excludeBlacksmithRewards || item.source != ScoutItemSource.BLACKSMITH_REWARD) &&
            requirement.kind.accepts(item.item) &&
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
            matchesEffect(item, requirement) &&
            (!requirement.requireUncursed || !item.cursed) &&
            (requirement.source == null || requirement.source == item.source)

    // One slot per alternative group (candidates from all members) or plain requirement.
    val slotOfGroup = mutableMapOf<Int, Int>()
    val slots = mutableListOf<MutableList<Pair<Int, ItemRequirement>>>()
    val sumGroupSizes = mutableMapOf<Int, Int>()
    val sumGroupTotals = mutableMapOf<Int, Int>()
    for (requirement in requirements) {
        val slot = requirement.alternativeGroup?.let { group ->
            slotOfGroup.getOrPut(group) {
                slots.add(mutableListOf())
                slots.lastIndex
            }
        } ?: run {
            slots.add(mutableListOf())
            slots.lastIndex
        }
        requirement.upgradeSumGroup?.let { group ->
            sumGroupSizes[group] = (sumGroupSizes[group] ?: 0) + 1
            sumGroupTotals[group] = requirement.upgradeSumTotal ?: 0
        }
        items.indices
            .filter { matches(items[it], requirement) }
            .forEach { slots[slot].add(it to requirement) }
    }
    // Fail early by assigning the most constrained slot first.
    slots.sortBy { it.size }

    val used = BooleanArray(items.size)
    val selected = mutableListOf<Pair<Int, Int?>>() // item index, sum group served
    var best = emptySet<Int>()
    val scenarios = mutableMapOf<Int, ULong>()
    val identities = mutableMapOf<Int, String>()
    val sums = mutableMapOf<Int, Pair<Int, Int>>() // assigned member count, upgrade total

    fun visit(slot: Int) {
        if (slot == slots.size) {
            // Items serving an incomplete or short combined-upgrade group do
            // not count and are not highlighted.
            val failed = sums.filterKeys { group ->
                val (assigned, total) = sums.getValue(group)
                assigned < (sumGroupSizes[group] ?: 0) || total < (sumGroupTotals[group] ?: 0)
            }.keys
            val counted = selected
                .filter { (_, sumGroup) -> sumGroup == null || sumGroup !in failed }
                .map { it.first }
            if (counted.size > best.size) best = counted.toSet()
            return
        }
        if (selected.size + slots.size - slot <= best.size) return
        for ((index, requirement) in slots[slot]) {
            if (used[index]) continue
            val item = items[index]
            val identityGroup = requirement.identityGroup
            val previousIdentity = identityGroup?.let { identities[it] }
            if (identityGroup != null && previousIdentity != null && previousIdentity != item.item.id) continue
            if (identityGroup != null) identities[identityGroup] = item.item.id
            fun undoIdentity() {
                if (identityGroup != null) {
                    if (previousIdentity == null) identities.remove(identityGroup)
                    else identities[identityGroup] = previousIdentity
                }
            }

            val constraint = when (val accessibility = item.accessibility) {
                ScoutAccessibility.Independent -> null
                is ScoutAccessibility.Choice -> accessibility.group to (1UL shl accessibility.option)
                is ScoutAccessibility.Scenarios -> accessibility.group to accessibility.mask
            }
            val previousScenarios = constraint?.first?.let { scenarios[it] }
            if (constraint != null) {
                val compatible = (previousScenarios ?: ULong.MAX_VALUE) and constraint.second
                if (compatible == 0UL) {
                    undoIdentity()
                    continue
                }
                scenarios[constraint.first] = compatible
            }
            fun undoScenario() {
                if (constraint != null) {
                    if (previousScenarios == null) scenarios.remove(constraint.first)
                    else scenarios[constraint.first] = previousScenarios
                }
            }

            val sumGroup = requirement.upgradeSumGroup
            val previousSum = sumGroup?.let { sums[it] }
            if (sumGroup != null) {
                val assigned = (previousSum?.first ?: 0) + 1
                val total = (previousSum?.second ?: 0) + item.upgrade
                val remaining = ((sumGroupSizes[sumGroup] ?: 0) - assigned).coerceAtLeast(0)
                if (total + remaining * MAX_ITEM_UPGRADE < (sumGroupTotals[sumGroup] ?: 0)) {
                    undoScenario()
                    undoIdentity()
                    continue
                }
                sums[sumGroup] = assigned to total
            }

            used[index] = true
            selected += index to sumGroup
            visit(slot + 1)
            selected.removeAt(selected.lastIndex)
            used[index] = false
            if (sumGroup != null) {
                if (previousSum == null) sums.remove(sumGroup)
                else sums[sumGroup] = previousSum
            }
            undoScenario()
            undoIdentity()
        }
        visit(slot + 1)
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
    val probability = status?.matchProbability ?: 0.0
    if (probability <= 0.0 || seedsPerSecond <= 0.0) {
        return "p estimating… · est —"
    }
    return "p ${formatProbabilityPercent(probability)} · " +
        "est ${formatEstimateDuration(1.0 / probability / seedsPerSecond)}"
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

internal fun formatSeedRate(rate: Double): String = when {
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
