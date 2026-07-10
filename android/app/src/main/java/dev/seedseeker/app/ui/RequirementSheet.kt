// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
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
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
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
import dev.seedseeker.app.model.UpgradeMatch
import java.util.Locale

@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun RequirementSheet(
    editing: ItemRequirement?,
    onDismiss: () -> Unit,
    onSave: (CatalogItem?, ItemKind, UpgradeMatch, Int, String?, ScoutItemSource?, Int?) -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val identity = editing?.key ?: -1L
    var kind by remember(identity) { mutableStateOf(editing?.kind ?: ItemKind.WEAPON) }
    var selectedItem by remember(identity) {
        mutableStateOf<CatalogItem?>(
            if (editing == null) ItemCatalog.forKind(kind).first() else editing.item,
        )
    }
    var upgradeMatch by remember(identity) { mutableStateOf(editing?.upgradeMatch ?: UpgradeMatch.EXACT) }
    var upgrade by remember(identity) { mutableStateOf(editing?.upgrade ?: 1) }
    var modifierName by remember(identity) { mutableStateOf(editing?.modifier) }
    var modifierMenuExpanded by remember(identity) { mutableStateOf(false) }
    var source by remember(identity) { mutableStateOf(editing?.source) }
    var sourceMenuExpanded by remember(identity) { mutableStateOf(false) }
    var identityGroup by remember(identity) { mutableStateOf(editing?.identityGroup) }

    fun clampUpgrade(match: UpgradeMatch, forKind: ItemKind) {
        upgrade = when (match) {
            UpgradeMatch.ANY -> 0
            UpgradeMatch.EXACT -> upgrade.coerceIn(1, forKind.maximumSearchUpgrade)
            UpgradeMatch.AT_LEAST -> upgrade.coerceIn(0, forKind.maximumSearchUpgrade)
        }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = MaterialTheme.colorScheme.surfaceContainer,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .padding(bottom = 16.dp),
        ) {
            Row(
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    if (editing == null) "Add requirement" else "Edit requirement",
                    style = MaterialTheme.typography.headlineSmall,
                    modifier = Modifier.weight(1f),
                )
                TextButton(onClick = onDismiss) { Text("Close") }
            }

            // Category — connected toggle-button group.
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp, vertical = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(3.dp),
            ) {
                ItemKind.entries.forEach { entry ->
                    ToggleButton(
                        checked = kind == entry,
                        onCheckedChange = { checked ->
                            if (checked && kind != entry) {
                                kind = entry
                                selectedItem = ItemCatalog.forKind(entry).first()
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
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                FilterChip(
                    selected = selectedItem == null,
                    onClick = { selectedItem = null },
                    label = { Text("Any ${kind.label.lowercase(Locale.ROOT)}") },
                )
                Spacer(Modifier.width(10.dp))
                Text(
                    "Or pick one exact item below.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.weight(1f),
                )
            }

            LazyVerticalGrid(
                columns = GridCells.Adaptive(92.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 180.dp, max = 280.dp),
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items(ItemCatalog.forKind(kind), key = { it.id }) { item ->
                    ItemTile(
                        item = item,
                        selected = selectedItem?.id == item.id,
                        onClick = { selectedItem = item },
                    )
                }
            }

            Column(
                modifier = Modifier
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 20.dp),
            ) {
                Text("Upgrade", style = MaterialTheme.typography.titleMedium)
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
                if (upgradeMatch != UpgradeMatch.ANY) {
                    Spacer(Modifier.height(8.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        val values = if (upgradeMatch == UpgradeMatch.EXACT) {
                            1..kind.maximumSearchUpgrade
                        } else {
                            0..kind.maximumSearchUpgrade
                        }
                        values.forEach { value ->
                            FilterChip(
                                selected = upgrade == value,
                                onClick = { upgrade = value },
                                label = { Text("+$value") },
                            )
                        }
                    }
                }

                val modifierLabel = kind.modifierLabel
                if (modifierLabel != null) {
                    Spacer(Modifier.height(18.dp))
                    Text(modifierLabel, style = MaterialTheme.typography.titleMedium)
                    Spacer(Modifier.height(8.dp))
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
                            label = { Text("$modifierLabel requirement") },
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

                Spacer(Modifier.height(18.dp))
                Text("Source", style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.height(8.dp))
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
                        label = { Text("Where it must come from") },
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
                Text("Same-item group", style = MaterialTheme.typography.titleMedium)
                Text(
                    "Requirements sharing a letter must resolve to the exact same item type, using distinct copies.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(
                        selected = identityGroup == null,
                        onClick = { identityGroup = null },
                        label = { Text("None") },
                    )
                    (1..4).forEach { group ->
                        FilterChip(
                            selected = identityGroup == group,
                            onClick = { identityGroup = group },
                            label = { Text(('A'.code + group - 1).toChar().toString()) },
                        )
                    }
                }

                Spacer(Modifier.height(18.dp))
                RequirementPreview(
                    item = selectedItem,
                    kind = kind,
                    upgradeMatch = upgradeMatch,
                    upgrade = upgrade,
                    modifierName = modifierName,
                    source = source,
                    identityGroup = identityGroup,
                )
                Spacer(Modifier.height(14.dp))
                Button(
                    onClick = {
                        onSave(selectedItem, kind, upgradeMatch, upgrade, modifierName, source, identityGroup)
                    },
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(56.dp),
                    shapes = ButtonDefaults.shapes(),
                ) {
                    Text(
                        if (editing == null) "Add to search" else "Save changes",
                        style = MaterialTheme.typography.titleMedium,
                    )
                }
            }
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

@Composable
private fun RequirementPreview(
    item: CatalogItem?,
    kind: ItemKind,
    upgradeMatch: UpgradeMatch,
    upgrade: Int,
    modifierName: String?,
    source: ScoutItemSource?,
    identityGroup: Int?,
) {
    Surface(
        shape = MaterialTheme.shapes.large,
        color = MaterialTheme.colorScheme.surfaceContainerHighest,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(14.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SpriteTile(item = item, modifierName = modifierName, tileSize = 52)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    item?.name ?: "Any ${kind.label.lowercase(Locale.ROOT)}",
                    style = MaterialTheme.typography.titleMedium,
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
                        source?.let { append(" · ${it.label}") }
                        identityGroup?.let {
                            append(" · group ${('A'.code + it - 1).toChar()}")
                        }
                    },
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}
