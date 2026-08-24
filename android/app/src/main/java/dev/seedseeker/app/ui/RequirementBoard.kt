// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.seedseeker.app.model.BoardItem
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.TierMatch
import dev.seedseeker.app.model.UpgradeMatch
import dev.seedseeker.app.model.boardItems
import dev.seedseeker.app.model.detach
import dev.seedseeker.app.model.joinAlternatives

/**
 * The requirement board: every chip is one item to find, and the two ways
 * chips relate are shown rather than described.
 *
 * - Dragging a chip onto another makes them *either/or* alternatives of one
 *   slot — they share a capsule with a small "or" between them. Dragging a
 *   chip out of its capsule pulls it back out on its own.
 * - A chip asking for several items of the same kind carries a `×N` badge, and
 *   one asking for a combined level carries `Σ≥N`. Both are properties of a
 *   chip rather than relationships between chips, so both are set in the
 *   editor a tap opens — never by a drag.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun RequirementBoard(
    requirements: List<ItemRequirement>,
    enabled: Boolean,
    onChange: (List<ItemRequirement>) -> Unit,
    onEdit: (BoardItem) -> Unit,
    onRemove: (BoardItem) -> Unit,
    onAdd: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val items = remember(requirements) { requirements.boardItems() }
    val haptics = LocalHapticFeedback.current
    // Live layout handles, so a drop can name the chip it landed on. Bounds are
    // resolved against the board only at hit-test time, which keeps them right
    // through wrapping and scrolling.
    val placements = remember { mutableStateMapOf<Int, LayoutCoordinates>() }
    var board by remember { mutableStateOf<LayoutCoordinates?>(null) }
    var dragging by remember { mutableStateOf<Int?>(null) }
    var dragPosition by remember { mutableStateOf(Offset.Zero) }

    fun rectOf(index: Int): Rect? {
        val root = board?.takeIf { it.isAttached } ?: return null
        val chip = placements[index]?.takeIf { it.isAttached } ?: return null
        return root.localBoundingBoxOf(chip)
    }

    /** The chip under the pointer, if it is not the dragged chip itself. */
    fun targetAt(position: Offset, source: Int): Int? = placements.keys
        .filter { it != source }
        .firstOrNull { rectOf(it)?.contains(position) == true }

    val hovered = dragging?.let { targetAt(dragPosition, it) }

    Box(modifier = modifier.onGloballyPositioned { board = it }) {
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            items.forEach { item ->
                key(item.key) {
                    BoardEntry(
                        requirements = requirements,
                        item = item,
                        enabled = enabled,
                        draggingIndex = dragging,
                        hoveredIndex = hovered,
                        onPlaced = { index, coordinates -> placements[index] = coordinates },
                        onEdit = { onEdit(item) },
                        onRemove = { onRemove(item) },
                        onDragStart = { index, offset ->
                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                            dragging = index
                            dragPosition = (rectOf(index)?.topLeft ?: Offset.Zero) + offset
                        },
                        onDrag = { delta -> dragPosition += delta },
                        onDragEnd = {
                            val source = dragging
                            dragging = null
                            if (source == null) return@BoardEntry
                            val target = targetAt(dragPosition, source)
                            when {
                                target != null -> onChange(requirements.joinAlternatives(source, target))
                                // Dropped on open board: leave the capsule.
                                requirements[source].alternativeGroup != null ->
                                    onChange(requirements.detach(source))
                            }
                        },
                        onDragCancel = { dragging = null },
                    )
                }
            }
            AddChip(enabled = enabled, onClick = onAdd)
        }
    }
}

/** A chip, or the capsule holding an either/or cluster's chips. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun BoardEntry(
    requirements: List<ItemRequirement>,
    item: BoardItem,
    enabled: Boolean,
    draggingIndex: Int?,
    hoveredIndex: Int?,
    onPlaced: (Int, LayoutCoordinates) -> Unit,
    onEdit: () -> Unit,
    onRemove: () -> Unit,
    onDragStart: (Int, Offset) -> Unit,
    onDrag: (Offset) -> Unit,
    onDragEnd: () -> Unit,
    onDragCancel: () -> Unit,
) {
    // A lone chip carries its own stack badges; a cluster's belong to the
    // capsule, since the stack binds to whichever member the search picks.
    val onCapsule = item.cluster != null
    val chip: @Composable (Int) -> Unit = { index ->
        RequirementChip(
            requirement = requirements[index],
            stackCount = if (!onCapsule && index == item.anchor) item.stackCount else 1,
            total = if (!onCapsule && index == item.anchor) item.total else null,
            enabled = enabled,
            dimmed = draggingIndex == index,
            highlighted = hoveredIndex == index,
            onPlaced = { coordinates -> onPlaced(index, coordinates) },
            onClick = onEdit,
            onRemove = onRemove,
            onDragStart = { offset -> onDragStart(index, offset) },
            onDrag = onDrag,
            onDragEnd = onDragEnd,
            onDragCancel = onDragCancel,
        )
    }
    if (item.cluster == null) {
        chip(item.anchor)
        return
    }
    Surface(
        shape = RoundedCornerShape(18.dp),
        color = Color.Transparent,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.tertiary.copy(alpha = 0.7f)),
    ) {
        FlowRow(
            modifier = Modifier.padding(horizontal = 6.dp, vertical = 5.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            item.members.forEachIndexed { position, index ->
                // The connector and the chip it introduces travel together, so
                // a wrap never leaves a dangling "or" at the end of a row.
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (position > 0) {
                        Text(
                            "or",
                            modifier = Modifier.padding(end = 4.dp),
                            style = MaterialTheme.typography.labelSmall,
                            fontFamily = FontFamily.Monospace,
                            color = MaterialTheme.colorScheme.tertiary,
                        )
                    }
                    chip(index)
                }
            }
            if (item.stackCount > 1 || item.total != null) {
                Row(
                    modifier = Modifier.align(Alignment.CenterVertically),
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    if (item.stackCount > 1) ChipTag("×${item.stackCount}", accent = true)
                    item.total?.let { ChipTag("Σ≥$it", accent = true) }
                }
            }
        }
    }
}

/**
 * One item to find: its sprite, its name, and the qualifiers that narrow it.
 * A tap edits it, a long press picks it up to drop on another chip, and the
 * trailing ✕ deletes it.
 */
@Composable
private fun RequirementChip(
    requirement: ItemRequirement,
    stackCount: Int,
    total: Int?,
    enabled: Boolean,
    dimmed: Boolean,
    highlighted: Boolean,
    onPlaced: (LayoutCoordinates) -> Unit,
    onClick: () -> Unit,
    onRemove: () -> Unit,
    onDragStart: (Offset) -> Unit,
    onDrag: (Offset) -> Unit,
    onDragEnd: () -> Unit,
    onDragCancel: () -> Unit,
) {
    val outline = when {
        highlighted -> MaterialTheme.colorScheme.tertiary
        else -> MaterialTheme.colorScheme.outlineVariant
    }
    Surface(
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        border = BorderStroke(if (highlighted) 2.dp else 1.dp, outline),
        modifier = Modifier
            .alpha(if (dimmed) 0.4f else 1f)
            .onGloballyPositioned(onPlaced)
            .pointerInput(enabled) {
                if (!enabled) return@pointerInput
                detectDragGesturesAfterLongPress(
                    onDragStart = onDragStart,
                    onDrag = { change, delta ->
                        change.consume()
                        onDrag(delta)
                    },
                    onDragEnd = onDragEnd,
                    onDragCancel = onDragCancel,
                )
            }
            .clickable(enabled = enabled, onClick = onClick)
            .semantics { contentDescription = chipDescription(requirement, stackCount, total) },
    ) {
        Row(
            modifier = Modifier.padding(start = 6.dp, top = 4.dp, end = 2.dp, bottom = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SpriteTile(
                item = requirement.item,
                glow = ItemGlows.forEffect(requirement.singleEffect),
                tileSize = 26,
            )
            Spacer(Modifier.width(6.dp))
            Text(
                chipTitle(requirement),
                style = MaterialTheme.typography.labelLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            chipTags(requirement, stackCount, total).forEach { tag ->
                Spacer(Modifier.width(4.dp))
                ChipTag(text = tag.text, accent = tag.accent)
            }
            Text(
                "✕",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier
                    .clickable(enabled = enabled, onClick = onRemove)
                    .semantics { contentDescription = "Remove ${requirement.title}" }
                    .padding(horizontal = 6.dp, vertical = 2.dp),
            )
        }
    }
}

/** The trailing "+" chip that opens the editor on a new requirement. */
@Composable
private fun AddChip(enabled: Boolean, onClick: () -> Unit) {
    Surface(
        shape = RoundedCornerShape(16.dp),
        color = Color.Transparent,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        modifier = Modifier
            .clickable(enabled = enabled, onClick = onClick)
            .semantics { contentDescription = "Add requirement" },
    ) {
        Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) {
            Text("+", style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
        }
    }
}

/** A qualifier badge; [accent] marks the ones that describe a stack. */
private data class ChipTagSpec(val text: String, val accent: Boolean)

@Composable
private fun ChipTag(text: String, accent: Boolean) {
    Surface(
        shape = RoundedCornerShape(7.dp),
        color = if (accent) {
            MaterialTheme.colorScheme.tertiaryContainer
        } else {
            MaterialTheme.colorScheme.surfaceContainerHighest
        },
    ) {
        Text(
            text,
            modifier = Modifier.padding(horizontal = 5.dp, vertical = 1.dp),
            style = MaterialTheme.typography.labelSmall,
            fontFamily = FontFamily.Monospace,
            color = if (accent) {
                MaterialTheme.colorScheme.onTertiaryContainer
            } else {
                MaterialTheme.colorScheme.onSurfaceVariant
            },
        )
    }
}

/** The chip's name: the item, or the wildcard it stands for, without its tier. */
private fun chipTitle(requirement: ItemRequirement): String =
    requirement.item?.name ?: "Any ${requirement.kind.singularLabel}"

/** The badges after the name: what narrows the item, then what the stack asks for. */
private fun chipTags(requirement: ItemRequirement, stackCount: Int, total: Int?): List<ChipTagSpec> =
    buildList {
        when (requirement.tierMatch) {
            TierMatch.ANY -> Unit
            TierMatch.EXACT -> add(ChipTagSpec("T${requirement.tier}", accent = false))
            TierMatch.AT_LEAST -> add(ChipTagSpec("T${requirement.tier}+", accent = false))
            TierMatch.AT_MOST -> add(ChipTagSpec("T≤${requirement.tier}", accent = false))
        }
        when (requirement.upgradeMatch) {
            UpgradeMatch.ANY -> Unit
            UpgradeMatch.EXACT -> add(ChipTagSpec("+${requirement.upgrade}", accent = false))
            UpgradeMatch.AT_LEAST -> add(ChipTagSpec("≥+${requirement.upgrade}", accent = false))
        }
        requirement.effectLabel?.let { add(ChipTagSpec(it, accent = false)) }
        if (requirement.requireUncursed) add(ChipTagSpec("✓", accent = false))
        requirement.source?.let { add(ChipTagSpec(it.label, accent = false)) }
        requirement.maximumDepth?.let { add(ChipTagSpec("F≤$it", accent = false)) }
        if (stackCount > 1) add(ChipTagSpec("×$stackCount", accent = true))
        total?.let { add(ChipTagSpec("Σ≥$it", accent = true)) }
    }

/** What a screen reader says for a chip: its name and every badge, spelled out. */
private fun chipDescription(requirement: ItemRequirement, stackCount: Int, total: Int?): String =
    buildList {
        add(chipTitle(requirement))
        if (stackCount > 1) add("$stackCount of them")
        total?.let { add("reaching $it levels together") }
        val detail = requirementDetailLine(requirement)
        if (detail.isNotEmpty()) add(detail)
    }.joinToString(", ")
