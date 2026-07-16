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
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.CatalogItem
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.TierMatch
import dev.seedseeker.app.model.UpgradeMatch
import java.util.Locale
import kotlin.math.roundToInt

private enum class SheetStep { ITEM, DETAILS }

@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun RequirementSheet(
    editing: ItemRequirement?,
    onDismiss: () -> Unit,
    onSave: (CatalogItem?, ItemKind, TierMatch, Int, UpgradeMatch, Int, String?, ScoutItemSource?, Int?, Int?, Boolean) -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val identity = editing?.key ?: -1L
    var step by remember(identity) {
        mutableStateOf(if (editing == null) SheetStep.ITEM else SheetStep.DETAILS)
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
    var modifierName by remember(identity) { mutableStateOf(editing?.modifier) }
    var modifierMenuExpanded by remember(identity) { mutableStateOf(false) }
    var source by remember(identity) { mutableStateOf(editing?.source) }
    var sourceMenuExpanded by remember(identity) { mutableStateOf(false) }
    var identityGroup by remember(identity) { mutableStateOf(editing?.identityGroup) }
    var maximumDepth by remember(identity) { mutableStateOf(editing?.maximumDepth) }
    var requireUncursed by remember(identity) { mutableStateOf(editing?.requireUncursed ?: false) }

    fun clampUpgrade(match: UpgradeMatch, forKind: ItemKind) {
        upgrade = normalizedUpgrade(upgrade, match, forKind)
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
                    if (editing == null) "Add requirement" else "Edit requirement",
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
                        ItemKind.entries.forEach { entry ->
                            ToggleButton(
                                checked = kind == entry,
                                onCheckedChange = { checked ->
                                    if (checked && kind != entry) {
                                        kind = entry
                                        selectedItem = ItemCatalog.forKind(entry).first { it.tier != 1 }
                                        tierMatch = TierMatch.ANY
                                        tier = 2
                                        modifierName = null
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
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        FilterChip(
                            selected = selectedItem == null,
                            onClick = { selectedItem = null },
                            label = { Text("Any ${kind.label.lowercase(Locale.ROOT)}") },
                        )
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
                        if (selectedItem == null && kind in setOf(ItemKind.WEAPON, ItemKind.ARMOR)) {
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
                                                tier = tier.coerceIn(3, 4)
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
                                        valueRange = 2f..5f,
                                        steps = 2,
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
                                        (3..4).forEach { option ->
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
                            if (kind == ItemKind.RING) {
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
                            ExposedDropdownMenuBox(
                                expanded = modifierMenuExpanded,
                                onExpandedChange = { modifierMenuExpanded = it },
                            ) {
                                OutlinedTextField(
                                    value = modifierName ?: "Any / none required",
                                    onValueChange = { },
                                    readOnly = true,
                                    singleLine = true,
                                    shape = MaterialTheme.shapes.medium,
                                    label = { Text(modifierLabel) },
                                    trailingIcon = {
                                        ExposedDropdownMenuDefaults.TrailingIcon(expanded = modifierMenuExpanded)
                                    },
                                    modifier = Modifier
                                        .menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable, enabled = true)
                                        .fillMaxWidth(),
                                )
                                ExposedDropdownMenu(
                                    expanded = modifierMenuExpanded,
                                    onDismissRequest = { modifierMenuExpanded = false },
                                ) {
                                    DropdownMenuItem(
                                        text = { Text("Any / none required") },
                                        onClick = {
                                            modifierName = null
                                            modifierMenuExpanded = false
                                        },
                                    )
                                    Text(
                                        if (kind == ItemKind.WEAPON) "ENCHANTMENTS" else "GLYPHS",
                                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                                        style = MaterialTheme.typography.labelSmall,
                                        letterSpacing = 1.sp,
                                        color = MaterialTheme.colorScheme.primary,
                                    )
                                    val regularModifiers = if (kind == ItemKind.WEAPON) {
                                        ItemCatalog.enchantments
                                    } else {
                                        ItemCatalog.glyphs
                                    }
                                    regularModifiers.forEach { option ->
                                        DropdownMenuItem(
                                            text = { Text(option) },
                                            onClick = {
                                                modifierName = option
                                                modifierMenuExpanded = false
                                            },
                                        )
                                    }
                                    if (!requireUncursed) {
                                        HorizontalDivider(
                                            modifier = Modifier.padding(vertical = 5.dp),
                                            color = MaterialTheme.colorScheme.outlineVariant,
                                        )
                                        Text(
                                            "CURSES",
                                            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                                            style = MaterialTheme.typography.labelSmall,
                                            letterSpacing = 1.sp,
                                            color = MaterialTheme.colorScheme.error,
                                        )
                                        ItemCatalog.cursesFor(kind).forEach { option ->
                                            DropdownMenuItem(
                                                text = { Text(option) },
                                                onClick = {
                                                    modifierName = option
                                                    modifierMenuExpanded = false
                                                },
                                            )
                                        }
                                    }
                                }
                            }
                        }

                        Spacer(Modifier.height(10.dp))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Checkbox(
                                checked = requireUncursed,
                                onCheckedChange = { checked ->
                                    if (checked && modifierName in ItemCatalog.cursesFor(kind)) {
                                        modifierName = null
                                    }
                                    requireUncursed = checked
                                },
                            )
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
                        Slider(
                            value = (maximumDepth ?: 0).toFloat(),
                            onValueChange = { maximumDepth = it.roundToInt().takeIf { depth -> depth > 0 } },
                            valueRange = 0f..24f,
                            steps = 23,
                        )

                        Spacer(Modifier.height(18.dp))
                        Text("Same-item group", style = MaterialTheme.typography.titleSmall)
                        Text(
                            "Requirements sharing a letter must resolve to the same item type, using distinct copies.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Spacer(Modifier.height(8.dp))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(3.dp),
                        ) {
                            ToggleButton(
                                checked = identityGroup == null,
                                onCheckedChange = { if (it) identityGroup = null },
                                modifier = Modifier.weight(1f),
                                colors = ToggleButtonDefaults.toggleButtonColors(
                                    containerColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                                ),
                                contentPadding = PaddingValues(horizontal = 4.dp, vertical = 10.dp),
                            ) {
                                Text("None", maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                            (1..4).forEach { group ->
                                ToggleButton(
                                    checked = identityGroup == group,
                                    onCheckedChange = { if (it) identityGroup = group },
                                    modifier = Modifier.weight(1f),
                                    colors = ToggleButtonDefaults.toggleButtonColors(
                                        containerColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                                    ),
                                    contentPadding = PaddingValues(horizontal = 4.dp, vertical = 10.dp),
                                ) {
                                    Text(('A'.code + group - 1).toChar().toString())
                                }
                            }
                        }
                        Spacer(Modifier.height(14.dp))
                    }

                    Column(Modifier.padding(horizontal = 20.dp)) {
                        RequirementPreview(
                            item = selectedItem,
                            kind = kind,
                            tierMatch = tierMatch,
                            tier = tier,
                            upgradeMatch = upgradeMatch,
                            upgrade = upgrade,
                            modifierName = modifierName,
                            source = source,
                            identityGroup = identityGroup,
                            maximumDepth = maximumDepth,
                            requireUncursed = requireUncursed,
                        )
                        Spacer(Modifier.height(10.dp))
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            OutlinedButton(
                                onClick = { step = SheetStep.ITEM },
                                modifier = Modifier.height(52.dp),
                                shapes = ButtonDefaults.shapes(),
                            ) {
                                Text("Back")
                            }
                            Button(
                                onClick = {
                                    onSave(selectedItem, kind, tierMatch, if (tierMatch == TierMatch.ANY) 0 else tier,
                                        upgradeMatch, upgrade, modifierName, source, identityGroup, maximumDepth,
                                        requireUncursed)
                                },
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

@Composable
private fun RequirementPreview(
    item: CatalogItem?,
    kind: ItemKind,
    tierMatch: TierMatch,
    tier: Int,
    upgradeMatch: UpgradeMatch,
    upgrade: Int,
    modifierName: String?,
    source: ScoutItemSource?,
    identityGroup: Int?,
    maximumDepth: Int?,
    requireUncursed: Boolean,
) {
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
            SpriteTile(item = item, modifierName = modifierName, tileSize = 44)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    item?.name ?: when (tierMatch) {
                        TierMatch.ANY -> "Any ${kind.singularLabel}"
                        TierMatch.EXACT -> "Any Tier $tier ${kind.singularLabel}"
                        TierMatch.AT_LEAST -> "Any Tier $tier+ ${kind.singularLabel}"
                        TierMatch.AT_MOST -> "Any Tier $tier or lower ${kind.singularLabel}"
                    },
                    style = MaterialTheme.typography.titleSmall,
                )
                Text(
                    buildString {
                        append(
                            when (upgradeMatch) {
                                UpgradeMatch.ANY -> "Any upgrade"
                                UpgradeMatch.EXACT -> "+$upgrade exactly"
                                UpgradeMatch.AT_LEAST -> "+$upgrade or higher"
                            },
                        )
                        modifierName?.let { append(" · $it") }
                        if (requireUncursed) append(" · uncursed")
                        source?.let { append(" · ${it.label}") }
                        identityGroup?.let {
                            append(" · group ${('A'.code + it - 1).toChar()}")
                        }
                        maximumDepth?.let { append(" · by floor $it") }
                    },
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}
