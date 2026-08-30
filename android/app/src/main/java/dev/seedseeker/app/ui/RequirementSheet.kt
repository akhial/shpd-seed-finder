// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.ToggleButton
import androidx.compose.material3.ToggleButtonDefaults
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.CatalogItem
import dev.seedseeker.app.model.EffectFilter
import dev.seedseeker.app.model.FLOOR_LIMIT_OPTIONS
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.SearchLimits
import dev.seedseeker.app.model.TierMatch
import dev.seedseeker.app.model.UpgradeMatch
import dev.seedseeker.app.model.floorLimitIndex
import java.util.Locale
import kotlin.math.roundToInt

private enum class SheetStep { ITEM, DETAILS }

/** The three shapes an effect filter takes in the editor. */
private enum class EffectMode(val label: String) {
    ANY("Any"),
    ANY_ENCHANTMENT("Any enchantment"),
    SPECIFIC("Specific…"),
}

/**
 * The requirement editor. It edits one chip — the anchor of its stack — plus
 * the shape of that stack: [editingCount] items of the same kind, reaching
 * [editingTotal] combined levels when a total is set, its extra copies kept to
 * [editingCopyDepth]'s floor. [onSave] hands the finished chip and stack shape
 * back — with [editing]'s key and alternative group, or key 0 for a new chip —
 * for the caller to place through `applyEdit`.
 *
 * [startWithItemPicker] opens an existing chip on the item step, which is what
 * a freshly forked alternative wants: the copy is meant to become a different
 * item.
 *
 * [onRemove], given for an existing chip, takes it off the board from here: the
 * board's chips flow across lines, so a tap is the sure way to reach one, and
 * the editor a tap opens is where its removal lives besides the drop zone.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun RequirementSheet(
    editing: ItemRequirement?,
    editingCount: Int = 1,
    editingTotal: Int? = null,
    editingCopyDepth: Int? = null,
    startWithItemPicker: Boolean = false,
    onDismiss: () -> Unit,
    onSave: (requirement: ItemRequirement, count: Int, total: Int?, copyDepth: Int?) -> Unit,
    onRemove: (() -> Unit)? = null,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val identity = editing?.key ?: -1L
    var step by remember(identity) {
        mutableStateOf(if (editing == null || startWithItemPicker) SheetStep.ITEM else SheetStep.DETAILS)
    }
    var kind by remember(identity) { mutableStateOf(editing?.kind ?: ItemKind.WEAPON) }
    var selectedItem by remember(identity) {
        mutableStateOf<CatalogItem?>(
            if (editing == null) {
                ItemCatalog.forKind(kind).first { it.tier != 1 }
            } else {
                editing.item
            },
        )
    }
    var upgradeMatch by remember(identity) { mutableStateOf(editing?.upgradeMatch ?: UpgradeMatch.EXACT) }
    var upgrade by remember(identity) {
        val initialMatch = editing?.upgradeMatch ?: UpgradeMatch.EXACT
        val initialKind = editing?.kind ?: ItemKind.WEAPON
        mutableStateOf(normalizedUpgrade(editing?.upgrade ?: 1, initialMatch, initialKind))
    }
    var upgradeMenuExpanded by remember(identity) { mutableStateOf(false) }
    var tierMatch by remember(identity) { mutableStateOf(editing?.tierMatch ?: TierMatch.ANY) }
    var tier by remember(identity) { mutableStateOf(editing?.tier?.takeIf { it >= 2 } ?: 2) }
    var tierMenuExpanded by remember(identity) { mutableStateOf(false) }
    var effectMode by remember(identity) {
        mutableStateOf(
            when (editing?.effect) {
                null, EffectFilter.Any -> EffectMode.ANY
                EffectFilter.AnyEnchantment -> EffectMode.ANY_ENCHANTMENT
                is EffectFilter.OneOf -> EffectMode.SPECIFIC
            },
        )
    }
    var selectedEffects by remember(identity) {
        mutableStateOf((editing?.effect as? EffectFilter.OneOf)?.names?.toSet() ?: emptySet())
    }
    var source by remember(identity) { mutableStateOf(editing?.source) }
    var sourceMenuExpanded by remember(identity) { mutableStateOf(false) }
    var maximumDepth by remember(identity) { mutableStateOf(editing?.maximumDepth) }
    var requireUncursed by remember(identity) { mutableStateOf(editing?.requireUncursed ?: false) }
    // The stack this chip anchors: how many items of its kind to find, and the
    // combined level they must reach together (null when it just wants copies).
    var stackCount by remember(identity) { mutableStateOf(editingCount.coerceIn(1, SearchLimits.STACK_MAX)) }
    var stackTotal by remember(identity) { mutableStateOf(editingTotal) }
    // The chip's own floor limit bounds the one item it describes; this one
    // bounds the copies behind it, which carry no constraints of their own.
    var copyDepth by remember(identity) { mutableStateOf(editingCopyDepth) }

    // A member of an either/or cluster leaves the stack to the cluster itself.
    val inAlternativeGroup = editing?.alternativeGroup != null
    val draftMaximumUpgrade = if (upgradeMatch == UpgradeMatch.EXACT) upgrade else kind.maximumSearchUpgrade
    // Every item of a stack that counts levels is a copy of the anchor, and each
    // contributes its upgrade plus one.
    val levelCapacity = (draftMaximumUpgrade + 1) * stackCount

    fun clampUpgrade(match: UpgradeMatch, forKind: ItemKind) {
        upgrade = normalizedUpgrade(upgrade, match, forKind)
    }

    fun resetEffects() {
        effectMode = EffectMode.ANY
        selectedEffects = emptySet()
    }

    val draft: Result<ItemRequirement> = runCatching {
        ItemRequirement(
            key = editing?.key ?: 0L,
            item = selectedItem,
            upgrade = upgrade,
            effect = when (effectMode) {
                EffectMode.ANY -> EffectFilter.Any
                EffectMode.ANY_ENCHANTMENT -> EffectFilter.AnyEnchantment
                EffectMode.SPECIFIC -> EffectFilter.of(selectedEffects, kind)
            },
            kind = kind,
            tier = if (tierMatch == TierMatch.ANY) 0 else tier,
            tierMatch = tierMatch,
            upgradeMatch = upgradeMatch,
            source = source,
            identityGroup = editing?.identityGroup,
            maximumDepth = maximumDepth,
            requireUncursed = requireUncursed,
            alternativeGroup = editing?.alternativeGroup,
            levelSum = editing?.levelSum,
        )
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        sheetGesturesEnabled = false,
        dragHandle = null,
        containerColor = MaterialTheme.colorScheme.surfaceContainer,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .fillMaxHeight(0.94f)
                .navigationBarsPadding()
                .padding(bottom = 16.dp),
        ) {
            Row(
                modifier = Modifier.padding(start = 20.dp, top = 12.dp, end = 20.dp, bottom = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    when {
                        editing == null -> "Add requirement"
                        inAlternativeGroup -> "Edit alternative"
                        else -> "Edit requirement"
                    },
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    if (step == SheetStep.ITEM) "1/2 · Item" else "2/2 · Details",
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                TextButton(onClick = onDismiss) { Text("Close") }
            }

            when (step) {
                SheetStep.ITEM -> {
                    // Category — connected toggle-button group (fixed chrome).
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 20.dp, vertical = 8.dp),
                        horizontalArrangement = Arrangement.spacedBy(3.dp),
                    ) {
                        listOf(ItemKind.WEAPON, ItemKind.ARMOR, ItemKind.WAND, ItemKind.RING)
                            .forEach { entry ->
                                ToggleButton(
                                    checked = kind.family == entry,
                                    onCheckedChange = { checked ->
                                        if (checked && kind.family != entry) {
                                            kind = entry
                                            selectedItem = ItemCatalog.forKind(entry).first { it.tier != 1 }
                                            tierMatch = TierMatch.ANY
                                            tier = 2
                                            resetEffects()
                                            clampUpgrade(upgradeMatch, entry)
                                        }
                                    },
                                    modifier = Modifier.weight(1f),
                                    colors = ToggleButtonDefaults.toggleButtonColors(
                                        containerColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                                    ),
                                    contentPadding = PaddingValues(horizontal = 4.dp, vertical = 10.dp),
                                ) {
                                    Text(entry.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                }
                            }
                    }

                    Row(
                        modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        FilterChip(
                            selected = selectedItem == null,
                            onClick = { selectedItem = null },
                            label = { Text("Any ${kind.label.lowercase(Locale.ROOT)}") },
                        )
                        if (kind.family == ItemKind.WEAPON) {
                            listOf(
                                ItemKind.WEAPON to "All",
                                ItemKind.MELEE_WEAPON to "Melee",
                                ItemKind.THROWN_WEAPON to "Thrown",
                            ).forEach { (weaponKind, label) ->
                                FilterChip(
                                    selected = kind == weaponKind,
                                    onClick = {
                                        if (kind != weaponKind) {
                                            kind = weaponKind
                                            if (selectedItem?.let(weaponKind::accepts) == false) {
                                                selectedItem = ItemCatalog.forKind(weaponKind)
                                                    .first { it.tier != 1 }
                                            }
                                        }
                                    },
                                    label = { Text(label) },
                                )
                            }
                        }
                    }

                    // Item picker — the only scrollable region on this step.
                    LazyVerticalGrid(
                        columns = GridCells.Adaptive(92.dp),
                        modifier = Modifier
                            .fillMaxWidth()
                            .weight(1f),
                        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        items(ItemCatalog.forKind(kind).filter { it.tier != 1 }, key = { it.id }) { item ->
                            ItemTile(
                                item = item,
                                selected = selectedItem?.id == item.id,
                                onClick = {
                                    selectedItem = item
                                    tierMatch = TierMatch.ANY
                                },
                            )
                        }
                    }

                    Button(
                        onClick = { step = SheetStep.DETAILS },
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 20.dp)
                            .padding(top = 10.dp)
                            .height(52.dp),
                        shapes = ButtonDefaults.shapes(),
                    ) {
                        Text("Next", style = MaterialTheme.typography.titleMedium)
                    }
                }

                SheetStep.DETAILS -> {
                    // Details — a single scrollable column.
                    Column(
                        modifier = Modifier
                            .weight(1f)
                            .verticalScroll(rememberScrollState())
                            .padding(horizontal = 20.dp),
                    ) {
                        if (selectedItem == null && kind.family in setOf(ItemKind.WEAPON, ItemKind.ARMOR)) {
                            Text("Tier", style = MaterialTheme.typography.titleSmall)
                            Spacer(Modifier.height(8.dp))
                            Row(
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                TierMatch.entries.forEach { match ->
                                    FilterChip(
                                        selected = tierMatch == match,
                                        onClick = {
                                            tierMatch = match
                                            if (match in setOf(TierMatch.AT_LEAST, TierMatch.AT_MOST)) {
                                                tier = tier.coerceIn(SearchLimits.BOUNDED_TIERS.first, SearchLimits.BOUNDED_TIERS.last)
                                            }
                                            tierMenuExpanded = false
                                        },
                                        label = { Text(match.label) },
                                    )
                                }
                            }
                            if (tierMatch == TierMatch.EXACT) {
                                Column(Modifier.padding(vertical = 4.dp)) {
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        horizontalArrangement = Arrangement.SpaceBetween,
                                    ) {
                                        Text("Exact tier", style = MaterialTheme.typography.labelLarge)
                                        Text(
                                            "Tier $tier",
                                            style = MaterialTheme.typography.labelLarge,
                                            color = MaterialTheme.colorScheme.primary,
                                        )
                                    }
                                    Slider(
                                        value = tier.toFloat(),
                                        onValueChange = { tier = it.roundToInt() },
                                        valueRange = SearchLimits.EXACT_TIERS.first.toFloat()..
                                            SearchLimits.EXACT_TIERS.last.toFloat(),
                                        steps = SearchLimits.EXACT_TIERS.count() - 2,
                                    )
                                }
                            } else if (tierMatch in setOf(TierMatch.AT_LEAST, TierMatch.AT_MOST)) {
                                ExposedDropdownMenuBox(
                                    expanded = tierMenuExpanded,
                                    onExpandedChange = { tierMenuExpanded = it },
                                    modifier = Modifier.padding(vertical = 8.dp),
                                ) {
                                    OutlinedTextField(
                                        value = if (tierMatch == TierMatch.AT_LEAST) {
                                            "Tier $tier or higher"
                                        } else {
                                            "Tier $tier or lower"
                                        },
                                        onValueChange = { },
                                        readOnly = true,
                                        singleLine = true,
                                        label = {
                                            Text(if (tierMatch == TierMatch.AT_LEAST) "Minimum tier" else "Maximum tier")
                                        },
                                        trailingIcon = {
                                            ExposedDropdownMenuDefaults.TrailingIcon(expanded = tierMenuExpanded)
                                        },
                                        modifier = Modifier
                                            .menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable, enabled = true)
                                            .fillMaxWidth(),
                                    )
                                    ExposedDropdownMenu(
                                        expanded = tierMenuExpanded,
                                        onDismissRequest = { tierMenuExpanded = false },
                                    ) {
                                        SearchLimits.BOUNDED_TIERS.forEach { option ->
                                            DropdownMenuItem(
                                                text = { Text("Tier $option") },
                                                onClick = {
                                                    tier = option
                                                    tierMenuExpanded = false
                                                },
                                            )
                                        }
                                    }
                                }
                            }
                            Spacer(Modifier.height(18.dp))
                        }

                        Text("Upgrade", style = MaterialTheme.typography.titleSmall)
                        Spacer(Modifier.height(8.dp))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(3.dp),
                        ) {
                            UpgradeMatch.entries.forEach { match ->
                                ToggleButton(
                                    checked = upgradeMatch == match,
                                    onCheckedChange = { checked ->
                                        if (checked) {
                                            upgradeMatch = match
                                            clampUpgrade(match, kind)
                                            upgradeMenuExpanded = false
                                        }
                                    },
                                    modifier = Modifier.weight(1f),
                                    colors = ToggleButtonDefaults.toggleButtonColors(
                                        containerColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                                    ),
                                    contentPadding = PaddingValues(horizontal = 4.dp, vertical = 10.dp),
                                ) {
                                    Text(match.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                }
                            }
                        }
                        if (upgradeMatch == UpgradeMatch.EXACT) {
                            Spacer(Modifier.height(8.dp))
                            Column {
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                ) {
                                    Text("Level", style = MaterialTheme.typography.labelLarge)
                                    Text(
                                        "+$upgrade",
                                        style = MaterialTheme.typography.labelLarge,
                                        color = MaterialTheme.colorScheme.primary,
                                    )
                                }
                                Slider(
                                    value = upgrade.toFloat(),
                                    onValueChange = { upgrade = it.roundToInt() },
                                    valueRange = 1f..kind.maximumSearchUpgrade.toFloat(),
                                    steps = kind.maximumSearchUpgrade - 2,
                                )
                            }
                        } else if (upgradeMatch == UpgradeMatch.AT_LEAST) {
                            Spacer(Modifier.height(8.dp))
                            // A slider needs three stops to beat a dropdown, and
                            // since v4.0.0 raised the ceilings every family has
                            // them (+1..+4 for weapons, +1..+3 for the rest).
                            if (kind.maximumSearchUpgrade - 1 >= 3) {
                                Column {
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        horizontalArrangement = Arrangement.SpaceBetween,
                                    ) {
                                        Text("At least", style = MaterialTheme.typography.labelLarge)
                                        Text(
                                            "+$upgrade or higher",
                                            style = MaterialTheme.typography.labelLarge,
                                            color = MaterialTheme.colorScheme.primary,
                                        )
                                    }
                                    Slider(
                                        value = upgrade.toFloat(),
                                        onValueChange = { upgrade = it.roundToInt() },
                                        valueRange = 1f..(kind.maximumSearchUpgrade - 1).toFloat(),
                                        steps = kind.maximumSearchUpgrade - 3,
                                    )
                                }
                            } else {
                                ExposedDropdownMenuBox(
                                    expanded = upgradeMenuExpanded,
                                    onExpandedChange = { upgradeMenuExpanded = it },
                                ) {
                                    OutlinedTextField(
                                        value = "+$upgrade or higher",
                                        onValueChange = { },
                                        readOnly = true,
                                        singleLine = true,
                                        label = { Text("Minimum upgrade") },
                                        trailingIcon = {
                                            ExposedDropdownMenuDefaults.TrailingIcon(expanded = upgradeMenuExpanded)
                                        },
                                        modifier = Modifier
                                            .menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable, enabled = true)
                                            .fillMaxWidth(),
                                    )
                                    ExposedDropdownMenu(
                                        expanded = upgradeMenuExpanded,
                                        onDismissRequest = { upgradeMenuExpanded = false },
                                    ) {
                                        (1..<kind.maximumSearchUpgrade).forEach { option ->
                                            DropdownMenuItem(
                                                text = { Text("+$option or higher") },
                                                onClick = {
                                                    upgrade = option
                                                    upgradeMenuExpanded = false
                                                },
                                            )
                                        }
                                    }
                                }
                            }
                        }

                        val modifierLabel = kind.modifierLabel
                        if (modifierLabel != null) {
                            Spacer(Modifier.height(18.dp))
                            Text(modifierLabel, style = MaterialTheme.typography.titleSmall)
                            Spacer(Modifier.height(8.dp))
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.spacedBy(3.dp),
                            ) {
                                EffectMode.entries.forEach { mode ->
                                    ToggleButton(
                                        checked = effectMode == mode,
                                        onCheckedChange = { checked -> if (checked) effectMode = mode },
                                        modifier = Modifier.weight(1f),
                                        colors = ToggleButtonDefaults.toggleButtonColors(
                                            containerColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                                        ),
                                        contentPadding = PaddingValues(horizontal = 4.dp, vertical = 10.dp),
                                    ) {
                                        Text(
                                            if (mode == EffectMode.ANY_ENCHANTMENT) "Any ${modifierLabel.lowercase()}" else mode.label,
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis,
                                        )
                                    }
                                }
                            }
                            if (effectMode == EffectMode.SPECIFIC) {
                                Spacer(Modifier.height(8.dp))
                                EffectGrid(
                                    heading = if (kind.family == ItemKind.WEAPON) "ENCHANTMENTS" else "GLYPHS",
                                    names = ItemCatalog.enchantmentsFor(kind),
                                    selected = selectedEffects,
                                    headingColor = MaterialTheme.colorScheme.primary,
                                    onToggle = { name, checked ->
                                        selectedEffects = if (checked) selectedEffects + name else selectedEffects - name
                                    },
                                )
                                // Curses are hidden (and deselected) while the item must be uncursed.
                                if (!requireUncursed) {
                                    Spacer(Modifier.height(6.dp))
                                    EffectGrid(
                                        heading = "CURSES",
                                        names = ItemCatalog.cursesFor(kind),
                                        selected = selectedEffects,
                                        headingColor = MaterialTheme.colorScheme.error,
                                        onToggle = { name, checked ->
                                            selectedEffects = if (checked) selectedEffects + name else selectedEffects - name
                                        },
                                    )
                                }
                                if (selectedEffects.isEmpty()) {
                                    Text(
                                        "Nothing picked yet — any ${modifierLabel.lowercase()} is accepted until you do.",
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                            }
                        }

                        Spacer(Modifier.height(10.dp))
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .toggleable(
                                    value = requireUncursed,
                                    role = Role.Checkbox,
                                    onValueChange = { checked ->
                                        if (checked) {
                                            selectedEffects = selectedEffects - ItemCatalog.cursesFor(kind).toSet()
                                        }
                                        requireUncursed = checked
                                    },
                                ),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Checkbox(checked = requireUncursed, onCheckedChange = null)
                            Text("Require uncursed", style = MaterialTheme.typography.bodyMedium)
                        }

                        Spacer(Modifier.height(10.dp))
                        ExposedDropdownMenuBox(
                            expanded = sourceMenuExpanded,
                            onExpandedChange = { sourceMenuExpanded = it },
                        ) {
                            OutlinedTextField(
                                value = source?.label ?: "Any source",
                                onValueChange = { },
                                readOnly = true,
                                singleLine = true,
                                shape = MaterialTheme.shapes.medium,
                                label = { Text("Source") },
                                trailingIcon = {
                                    ExposedDropdownMenuDefaults.TrailingIcon(expanded = sourceMenuExpanded)
                                },
                                modifier = Modifier
                                    .menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable, enabled = true)
                                    .fillMaxWidth(),
                            )
                            ExposedDropdownMenu(
                                expanded = sourceMenuExpanded,
                                onDismissRequest = { sourceMenuExpanded = false },
                            ) {
                                DropdownMenuItem(
                                    text = { Text("Any source") },
                                    onClick = {
                                        source = null
                                        sourceMenuExpanded = false
                                    },
                                )
                                ScoutItemSource.entries.forEach { option ->
                                    DropdownMenuItem(
                                        text = { Text(option.label) },
                                        onClick = {
                                            source = option
                                            sourceMenuExpanded = false
                                        },
                                    )
                                }
                            }
                        }

                        Spacer(Modifier.height(18.dp))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                        ) {
                            Text("Floor limit", style = MaterialTheme.typography.titleSmall)
                            Text(
                                maximumDepth?.let { "≤ floor $it" } ?: "Search limit",
                                style = MaterialTheme.typography.labelLarge,
                                color = MaterialTheme.colorScheme.primary,
                            )
                        }
                        // Position 0 means "no limit"; the rest index into FLOOR_LIMIT_OPTIONS so
                        // empty boss floors (5, 10, 15) are not offered. Off-list stored values
                        // snap to the nearest option below via floorLimitIndex.
                        Slider(
                            value = (maximumDepth?.let { depth -> floorLimitIndex(depth) + 1 } ?: 0).toFloat(),
                            onValueChange = {
                                val index = it.roundToInt().coerceIn(0, FLOOR_LIMIT_OPTIONS.size)
                                maximumDepth = if (index == 0) null else FLOOR_LIMIT_OPTIONS[index - 1]
                            },
                            valueRange = 0f..FLOOR_LIMIT_OPTIONS.size.toFloat(),
                            steps = FLOOR_LIMIT_OPTIONS.size - 1,
                            modifier = Modifier.semantics {
                                stateDescription = maximumDepth?.let { "Floor $it" } ?: "No limit"
                            },
                        )

                        if (!inAlternativeGroup) {
                            Spacer(Modifier.height(18.dp))
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text("How many", style = MaterialTheme.typography.titleSmall)
                                Spacer(Modifier.weight(1f))
                                Stepper(
                                    value = stackCount,
                                    range = 1..SearchLimits.STACK_MAX,
                                    label = { "×$it" },
                                    onChange = { count ->
                                        stackCount = count
                                        // A shrinking stack cannot keep a total its items
                                        // can no longer reach.
                                        stackTotal = stackTotal?.coerceAtMost((draftMaximumUpgrade + 1) * count)
                                    },
                                )
                            }
                            if (stackCount > 1 && stackTotal == null) {
                                Spacer(Modifier.height(12.dp))
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Column(Modifier.weight(1f)) {
                                        Text(
                                            "Limit the extra copies to a floor",
                                            style = MaterialTheme.typography.titleSmall,
                                        )
                                        Text(
                                            "A floor limit is where an item lies, not what it is, " +
                                                "so the copies keep their own.",
                                            style = MaterialTheme.typography.bodySmall,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        )
                                    }
                                    Switch(
                                        checked = copyDepth != null,
                                        onCheckedChange = { on -> copyDepth = if (on) 4 else null },
                                    )
                                }
                                copyDepth?.let { depth ->
                                    Spacer(Modifier.height(6.dp))
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        horizontalArrangement = Arrangement.SpaceBetween,
                                    ) {
                                        Text(
                                            "Copies within first",
                                            style = MaterialTheme.typography.labelLarge,
                                        )
                                        Text(
                                            "$depth floor${if (depth == 1) "" else "s"}",
                                            style = MaterialTheme.typography.labelLarge,
                                            color = MaterialTheme.colorScheme.primary,
                                        )
                                    }
                                    Slider(
                                        value = floorLimitIndex(depth).toFloat(),
                                        onValueChange = {
                                            val index = it.roundToInt().coerceIn(0, FLOOR_LIMIT_OPTIONS.size - 1)
                                            copyDepth = FLOOR_LIMIT_OPTIONS[index]
                                        },
                                        valueRange = 0f..(FLOOR_LIMIT_OPTIONS.size - 1).toFloat(),
                                        steps = FLOOR_LIMIT_OPTIONS.size - 2,
                                        modifier = Modifier.semantics {
                                            stateDescription = "Copies within floor $depth"
                                        },
                                    )
                                }
                            }
                            if (stackCount > 1 && selectedItem != null) {
                                val total = stackTotal
                                Spacer(Modifier.height(12.dp))
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Column(Modifier.weight(1f)) {
                                        Text("Combined level", style = MaterialTheme.typography.titleSmall)
                                        Text(
                                            "Each item counts its upgrade plus one, and spare " +
                                                "items may go unused.",
                                            style = MaterialTheme.typography.bodySmall,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        )
                                    }
                                    Switch(
                                        checked = total != null,
                                        onCheckedChange = { on ->
                                            stackTotal = if (on) levelCapacity.coerceAtLeast(1) else null
                                        },
                                    )
                                }
                                if (total != null) {
                                    val clamped = total.coerceIn(1, maxOf(1, levelCapacity))
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        horizontalArrangement = Arrangement.SpaceBetween,
                                    ) {
                                        Text("Levels together", style = MaterialTheme.typography.labelLarge)
                                        Text(
                                            "≥ $clamped of $levelCapacity",
                                            style = MaterialTheme.typography.labelLarge,
                                            color = MaterialTheme.colorScheme.primary,
                                        )
                                    }
                                    val sliderMaximum = maxOf(2, levelCapacity)
                                    Slider(
                                        value = clamped.toFloat(),
                                        onValueChange = { stackTotal = it.roundToInt() },
                                        valueRange = 1f..sliderMaximum.toFloat(),
                                        steps = sliderMaximum - 2,
                                        modifier = Modifier.semantics {
                                            stateDescription = "Combined level at least $clamped"
                                        },
                                    )
                                }
                            }
                        }
                        Spacer(Modifier.height(14.dp))
                    }

                    Column(Modifier.padding(horizontal = 20.dp)) {
                        RequirementPreview(draft = draft)
                        Spacer(Modifier.height(10.dp))
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            if (editing != null && onRemove != null) {
                                OutlinedButton(
                                    onClick = onRemove,
                                    modifier = Modifier.height(52.dp),
                                    shapes = ButtonDefaults.shapes(),
                                    colors = ButtonDefaults.outlinedButtonColors(
                                        contentColor = MaterialTheme.colorScheme.error,
                                    ),
                                    contentPadding = PaddingValues(horizontal = 14.dp),
                                ) {
                                    Icon(
                                        Icons.Filled.Delete,
                                        contentDescription = if (inAlternativeGroup) {
                                            "Remove alternative"
                                        } else {
                                            "Remove requirement"
                                        },
                                        modifier = Modifier.size(20.dp),
                                    )
                                }
                            }
                            OutlinedButton(
                                onClick = { step = SheetStep.ITEM },
                                modifier = Modifier.height(52.dp),
                                shapes = ButtonDefaults.shapes(),
                            ) {
                                Text("Back")
                            }
                            Button(
                                onClick = {
                                    draft.getOrNull()?.let {
                                        val total = if (inAlternativeGroup) null else stackTotal
                                        // A cluster's stack is the cluster's, and
                                        // a combined level leaves no lone copies.
                                        val copies = if (inAlternativeGroup || stackCount < 2 || total != null) {
                                            null
                                        } else {
                                            copyDepth
                                        }
                                        onSave(it, stackCount, total, copies)
                                    }
                                },
                                enabled = draft.isSuccess,
                                modifier = Modifier
                                    .weight(1f)
                                    .height(52.dp),
                                shapes = ButtonDefaults.shapes(),
                            ) {
                                Text(
                                    if (editing == null) "Add" else "Save",
                                    style = MaterialTheme.typography.titleMedium,
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

private fun normalizedUpgrade(value: Int, match: UpgradeMatch, kind: ItemKind): Int = when (match) {
    UpgradeMatch.ANY -> 0
    UpgradeMatch.EXACT -> value.coerceIn(1, kind.maximumSearchUpgrade)
    UpgradeMatch.AT_LEAST -> value.coerceIn(1, kind.maximumSearchUpgrade - 1)
}

/** A compact −/+ stepper for the small bounded counts the board deals in. */
@Composable
private fun Stepper(
    value: Int,
    range: IntRange,
    label: (Int) -> String,
    onChange: (Int) -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        IconButton(
            onClick = { onChange(value - 1) },
            enabled = value > range.first,
        ) {
            Text("−", style = MaterialTheme.typography.titleLarge)
        }
        Text(
            label(value),
            style = MaterialTheme.typography.titleMedium,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.widthIn(min = 40.dp),
            textAlign = TextAlign.Center,
        )
        IconButton(
            onClick = { onChange(value + 1) },
            enabled = value < range.last,
        ) {
            Text("+", style = MaterialTheme.typography.titleLarge)
        }
    }
}

/** A two-column checkbox grid of effect names under a small heading. */
@Composable
private fun EffectGrid(
    heading: String,
    names: List<String>,
    selected: Set<String>,
    headingColor: Color,
    onToggle: (String, Boolean) -> Unit,
) {
    Text(
        heading,
        modifier = Modifier.padding(vertical = 4.dp),
        style = MaterialTheme.typography.labelSmall,
        letterSpacing = 1.sp,
        color = headingColor,
    )
    names.chunked(2).forEach { pair ->
        Row(Modifier.fillMaxWidth()) {
            pair.forEach { name ->
                val checked = name in selected
                Row(
                    modifier = Modifier
                        .weight(1f)
                        .toggleable(
                            value = checked,
                            role = Role.Checkbox,
                            onValueChange = { onToggle(name, it) },
                        )
                        .padding(vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Checkbox(checked = checked, onCheckedChange = null)
                    Text(
                        name,
                        style = MaterialTheme.typography.bodyMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            if (pair.size == 1) Spacer(Modifier.weight(1f))
        }
    }
}

@Composable
private fun ItemTile(item: CatalogItem, selected: Boolean, onClick: () -> Unit) {
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .height(118.dp)
            .selectable(selected = selected, onClick = onClick),
        shape = MaterialTheme.shapes.medium,
        color = if (selected) {
            MaterialTheme.colorScheme.primaryContainer
        } else {
            MaterialTheme.colorScheme.surfaceContainerHigh
        },
        border = if (selected) {
            BorderStroke(1.dp, MaterialTheme.colorScheme.primary)
        } else {
            null
        },
    ) {
        Column(
            modifier = Modifier.padding(horizontal = 7.dp, vertical = 9.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            ItemSprite(item, modifier = Modifier.size(42.dp))
            Spacer(Modifier.height(5.dp))
            Text(
                item.name,
                style = MaterialTheme.typography.labelSmall,
                textAlign = TextAlign.Center,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            item.tier?.let {
                Text(
                    "Tier $it",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** The row as it will appear in the list, or why it cannot be saved yet. */
@Composable
private fun RequirementPreview(draft: Result<ItemRequirement>) {
    Surface(
        shape = MaterialTheme.shapes.large,
        color = MaterialTheme.colorScheme.surfaceContainerHighest,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val requirement = draft.getOrNull()
            SpriteTile(
                item = requirement?.item,
                glows = requirement?.effect?.let(ItemGlows::forFilter).orEmpty(),
                tileSize = 44,
            )
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                if (requirement == null) {
                    Text(
                        draft.exceptionOrNull()?.message ?: "This requirement cannot be saved.",
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodySmall,
                    )
                } else {
                    Text(requirement.title, style = MaterialTheme.typography.titleSmall)
                    Text(
                        requirement.description.replace(" • ", " · "),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
        }
    }
}
