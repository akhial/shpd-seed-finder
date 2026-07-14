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

@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun RequirementSheet(
    editing: ItemRequirement?,
    onDismiss: () -> Unit,
    onSave: (CatalogItem?, ItemKind, TierMatch, Int, UpgradeMatch, Int, String?, ScoutItemSource?, Int?, Int?) -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val identity = editing?.key ?: -1L
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
    var upgrade by remember(identity) { mutableStateOf(editing?.upgrade ?: 1) }
    var tierMatch by remember(identity) { mutableStateOf(editing?.tierMatch ?: TierMatch.ANY) }
    var tier by remember(identity) { mutableStateOf(editing?.tier?.takeIf { it >= 2 } ?: 2) }
    var tierMenuExpanded by remember(identity) { mutableStateOf(false) }
    var modifierName by remember(identity) { mutableStateOf(editing?.modifier) }
    var modifierMenuExpanded by remember(identity) { mutableStateOf(false) }
    var source by remember(identity) { mutableStateOf(editing?.source) }
    var sourceMenuExpanded by remember(identity) { mutableStateOf(false) }
    var identityGroup by remember(identity) { mutableStateOf(editing?.identityGroup) }
    var maximumDepth by remember(identity) { mutableStateOf(editing?.maximumDepth) }

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

            if (selectedItem == null && kind in setOf(ItemKind.WEAPON, ItemKind.ARMOR)) {
                Row(
                    modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
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
                    Column(
                        modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                        ) {
                            Text(
                                when (tierMatch) {
                                    TierMatch.EXACT -> "Exact tier"
                                    TierMatch.AT_LEAST -> "Minimum tier"
                                    TierMatch.AT_MOST -> "Maximum tier"
                                    TierMatch.ANY -> "Tier"
                                },
                                style = MaterialTheme.typography.labelLarge,
                            )
                            Text(
                                when (tierMatch) {
                                    TierMatch.AT_LEAST -> "Tier $tier or higher"
                                    TierMatch.AT_MOST -> "Tier $tier or lower"
                                    else -> "Tier $tier"
                                },
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
                        modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
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
                Text("Floor limit", style = MaterialTheme.typography.titleMedium)
                Text(
                    "Stop considering this requirement after its selected floor. This can reject seeds earlier.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(8.dp))
                FilterChip(
                    selected = maximumDepth == null,
                    onClick = { maximumDepth = null },
                    label = { Text("Use search limit") },
                )
                if (maximumDepth != null) {
                    Text("Within the first $maximumDepth floors", style = MaterialTheme.typography.bodyMedium)
                    Slider(
                        value = maximumDepth!!.toFloat(),
                        onValueChange = { maximumDepth = it.toInt() },
                        valueRange = 1f..24f,
                        steps = 22,
                    )
                } else {
                    TextButton(onClick = { maximumDepth = 5 }) { Text("Set item-specific limit") }
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
                    tierMatch = tierMatch,
                    tier = tier,
                    upgradeMatch = upgradeMatch,
                    upgrade = upgrade,
                    modifierName = modifierName,
                    source = source,
                    identityGroup = identityGroup,
                    maximumDepth = maximumDepth,
                )
                Spacer(Modifier.height(14.dp))
                Button(
                    onClick = {
                        onSave(selectedItem, kind, tierMatch, if (tierMatch == TierMatch.ANY) 0 else tier,
                            upgradeMatch, upgrade, modifierName, source, identityGroup, maximumDepth)
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
    tierMatch: TierMatch,
    tier: Int,
    upgradeMatch: UpgradeMatch,
    upgrade: Int,
    modifierName: String?,
    source: ScoutItemSource?,
    identityGroup: Int?,
    maximumDepth: Int?,
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
                    item?.name ?: when (tierMatch) {
                        TierMatch.ANY -> "Any ${kind.singularLabel}"
                        TierMatch.EXACT -> "Any Tier $tier ${kind.singularLabel}"
                        TierMatch.AT_LEAST -> "Any Tier $tier+ ${kind.singularLabel}"
                        TierMatch.AT_MOST -> "Any Tier $tier or lower ${kind.singularLabel}"
                    },
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
                        maximumDepth?.let { append(" · by floor $it") }
                    },
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}
