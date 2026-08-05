// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.seedseeker.app.model.FLOOR_LIMIT_OPTIONS
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.QueryPreset
import dev.seedseeker.app.model.SearchState
import dev.seedseeker.app.model.SearchStatus
import dev.seedseeker.app.model.SeedResult
import dev.seedseeker.app.model.WandmakerQuest
import dev.seedseeker.app.model.floorLimitIndex
import kotlin.math.roundToInt

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FinderScreen(
    requirements: List<ItemRequirement>,
    maximumDepth: Int,
    requireBlacksmith: Boolean,
    excludeBlacksmithRewards: Boolean,
    wandmakerQuest: WandmakerQuest?,
    fastMode: Boolean,
    challenges: Int,
    presets: List<QueryPreset>,
    results: List<SeedResult>,
    /** The run's full collection size; `results` lists at most the display cap. */
    foundCount: Int,
    status: SearchStatus?,
    seedsPerSecond: Double,
    elapsedSeconds: Long,
    isSearching: Boolean,
    refinePhase: RefinePhase?,
    error: String?,
    snackbarHostState: SnackbarHostState,
    onAbout: () -> Unit,
    onChallenges: () -> Unit,
    onApplyPreset: (QueryPreset) -> Unit,
    onSavePreset: (String) -> Unit,
    onDeletePreset: (QueryPreset) -> Unit,
    onAdd: () -> Unit,
    onEdit: (ItemRequirement) -> Unit,
    onRemove: (ItemRequirement) -> Unit,
    onMaximumDepthChange: (Int) -> Unit,
    onRequireBlacksmithChange: (Boolean) -> Unit,
    onExcludeBlacksmithRewardsChange: (Boolean) -> Unit,
    onWandmakerQuestChange: (WandmakerQuest?) -> Unit,
    onFastModeChange: (Boolean) -> Unit,
    onSearch: () -> Unit,
    onCancel: () -> Unit,
    canExportResults: Boolean,
    canClearResults: Boolean,
    importNotice: String?,
    onExportResults: () -> Unit,
    onImportResults: () -> Unit,
    onClearResults: () -> Unit,
    onScoutSeed: (String) -> Unit,
    bottomBar: @Composable () -> Unit,
) {
    var showPresets by remember { mutableStateOf(false) }
    var showOverflowMenu by remember { mutableStateOf(false) }
    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        snackbarHost = { SnackbarHost(snackbarHostState) },
        topBar = {
            TopAppBar(
                title = { Text("Seed Seeker") },
                actions = {
                    TextButton(onClick = { showPresets = true }, enabled = !isSearching) {
                        Text("Presets")
                    }
                    IconButton(onClick = onChallenges) {
                        Icon(Icons.Filled.Settings, contentDescription = "Challenges")
                    }
                    IconButton(onClick = onAbout) {
                        Icon(Icons.Filled.Info, contentDescription = "About and licenses")
                    }
                    Box {
                        IconButton(onClick = { showOverflowMenu = true }) {
                            Icon(Icons.Filled.MoreVert, contentDescription = "More options")
                        }
                        DropdownMenu(
                            expanded = showOverflowMenu,
                            onDismissRequest = { showOverflowMenu = false },
                        ) {
                            DropdownMenuItem(
                                text = { Text("Import results…") },
                                enabled = !isSearching,
                                onClick = {
                                    showOverflowMenu = false
                                    onImportResults()
                                },
                            )
                            DropdownMenuItem(
                                text = { Text("Export results…") },
                                enabled = !isSearching && canExportResults,
                                onClick = {
                                    showOverflowMenu = false
                                    onExportResults()
                                },
                            )
                            DropdownMenuItem(
                                text = { Text("Clear results") },
                                enabled = !isSearching && canClearResults,
                                onClick = {
                                    showOverflowMenu = false
                                    onClearResults()
                                },
                            )
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                ),
            )
        },
        bottomBar = {
            Column {
                SearchActionBar(
                    requirementCount = requirements.size,
                    status = status,
                    seedsPerSecond = seedsPerSecond,
                    elapsedSeconds = elapsedSeconds,
                    isSearching = isSearching,
                    onSearch = onSearch,
                    onCancel = onCancel,
                )
                bottomBar()
            }
        },
    ) { scaffoldPadding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(scaffoldPadding),
            contentAlignment = Alignment.TopCenter,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxHeight()
                    .fillMaxWidth()
                    .widthIn(max = 680.dp),
            ) {
                QueryHeader(
                    requirements = requirements,
                    maximumDepth = maximumDepth,
                    requireBlacksmith = requireBlacksmith,
                    excludeBlacksmithRewards = excludeBlacksmithRewards,
                    wandmakerQuest = wandmakerQuest,
                    fastMode = fastMode,
                    challenges = challenges,
                    results = results,
                    foundCount = foundCount,
                    status = status,
                    isSearching = isSearching,
                    refinePhase = refinePhase,
                    error = error,
                    onAdd = onAdd,
                    onEdit = onEdit,
                    onRemove = onRemove,
                    onMaximumDepthChange = onMaximumDepthChange,
                    onRequireBlacksmithChange = onRequireBlacksmithChange,
                    onExcludeBlacksmithRewardsChange = onExcludeBlacksmithRewardsChange,
                    onWandmakerQuestChange = onWandmakerQuestChange,
                    onFastModeChange = onFastModeChange,
                    onChallenges = onChallenges,
                )
                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                LazyColumn(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxWidth(),
                    contentPadding = PaddingValues(start = 16.dp, top = 10.dp, end = 16.dp, bottom = 12.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    importNotice?.let { notice ->
                        item {
                            Text(
                                notice,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(vertical = 4.dp),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                    if (results.isEmpty()) {
                        item {
                            Text(
                                when {
                                    isSearching -> "0 matches yet."
                                    status?.state == SearchState.COMPLETED -> "0 matches."
                                    else -> "No results — run a search."
                                },
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(vertical = 12.dp),
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    } else {
                        items(results, key = { it.seed }) { result ->
                            ResultRow(result = result, onScout = { onScoutSeed(result.seed) })
                        }
                    }
                }
            }
        }
    }

    if (showPresets) {
        PresetsDialog(
            presets = presets,
            onApplyPreset = onApplyPreset,
            onSavePreset = onSavePreset,
            onDeletePreset = onDeletePreset,
            onDismiss = { showPresets = false },
        )
    }
}

@Composable
private fun QueryHeader(
    requirements: List<ItemRequirement>,
    maximumDepth: Int,
    requireBlacksmith: Boolean,
    excludeBlacksmithRewards: Boolean,
    wandmakerQuest: WandmakerQuest?,
    fastMode: Boolean,
    challenges: Int,
    results: List<SeedResult>,
    foundCount: Int,
    status: SearchStatus?,
    isSearching: Boolean,
    refinePhase: RefinePhase?,
    error: String?,
    onAdd: () -> Unit,
    onEdit: (ItemRequirement) -> Unit,
    onRemove: (ItemRequirement) -> Unit,
    onMaximumDepthChange: (Int) -> Unit,
    onRequireBlacksmithChange: (Boolean) -> Unit,
    onExcludeBlacksmithRewardsChange: (Boolean) -> Unit,
    onWandmakerQuestChange: (WandmakerQuest?) -> Unit,
    onFastModeChange: (Boolean) -> Unit,
    onChallenges: () -> Unit,
) {
    Column(Modifier.padding(horizontal = 16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                "Requirements (${requirements.size})",
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = onAdd, enabled = !isSearching) {
                Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(4.dp))
                Text("Add")
            }
        }
        if (requirements.isEmpty()) {
            Text(
                "None — add at least one.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(bottom = 4.dp),
            )
        } else {
            LazyColumn(
                modifier = Modifier.heightIn(max = 280.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                items(requirements, key = { it.key }) { requirement ->
                    RequirementRow(
                        requirement = requirement,
                        enabled = !isSearching,
                        onEdit = { onEdit(requirement) },
                        onRemove = { onRemove(requirement) },
                    )
                }
            }
        }
        Spacer(Modifier.height(4.dp))
        ScopeSection(
            maximumDepth = maximumDepth,
            requireBlacksmith = requireBlacksmith,
            excludeBlacksmithRewards = excludeBlacksmithRewards,
            wandmakerQuest = wandmakerQuest,
            fastMode = fastMode,
            challenges = challenges,
            enabled = !isSearching,
            onMaximumDepthChange = onMaximumDepthChange,
            onRequireBlacksmithChange = onRequireBlacksmithChange,
            onExcludeBlacksmithRewardsChange = onExcludeBlacksmithRewardsChange,
            onWandmakerQuestChange = onWandmakerQuestChange,
            onFastModeChange = onFastModeChange,
            onChallenges = onChallenges,
        )
        Text(
            resultsHeaderText(
                resultCount = foundCount,
                state = status?.state,
                isSearching = isSearching,
                refinePhase = refinePhase,
            ),
            style = MaterialTheme.typography.titleSmall,
        )
        if (error != null) {
            Text(
                error,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
            )
        }
        Spacer(Modifier.height(6.dp))
    }
}

@Composable
private fun RequirementRow(
    requirement: ItemRequirement,
    enabled: Boolean,
    onEdit: () -> Unit,
    onRemove: () -> Unit,
) {
    Surface(
        onClick = onEdit,
        enabled = enabled,
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier.padding(start = 10.dp, top = 6.dp, end = 2.dp, bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SpriteTile(
                item = requirement.item,
                glow = ItemGlows.forEffect(requirement.modifier),
                tileSize = 40,
            )
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    requirement.title,
                    style = MaterialTheme.typography.bodyLarge,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                val detail = requirementDetailLine(requirement)
                if (detail.isNotEmpty()) {
                    Text(
                        detail,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            IconButton(onClick = onRemove, enabled = enabled) {
                Icon(
                    Icons.Filled.Close,
                    contentDescription = "Remove requirement",
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun ScopeSection(
    maximumDepth: Int,
    requireBlacksmith: Boolean,
    excludeBlacksmithRewards: Boolean,
    wandmakerQuest: WandmakerQuest?,
    fastMode: Boolean,
    challenges: Int,
    enabled: Boolean,
    onMaximumDepthChange: (Int) -> Unit,
    onRequireBlacksmithChange: (Boolean) -> Unit,
    onExcludeBlacksmithRewardsChange: (Boolean) -> Unit,
    onWandmakerQuestChange: (WandmakerQuest?) -> Unit,
    onFastModeChange: (Boolean) -> Unit,
    onChallenges: () -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Column {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable { expanded = !expanded }
                .padding(vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Scope", style = MaterialTheme.typography.titleSmall)
            Spacer(Modifier.width(8.dp))
            Text(
                scopeSummaryText(
                    maximumDepth = maximumDepth,
                    requireBlacksmith = requireBlacksmith,
                    excludeBlacksmithRewards = excludeBlacksmithRewards,
                    wandmakerQuest = wandmakerQuest,
                    fastMode = fastMode,
                    challenges = challenges,
                ),
                modifier = Modifier.weight(1f),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Icon(
                if (expanded) Icons.Filled.KeyboardArrowUp else Icons.Filled.KeyboardArrowDown,
                contentDescription = if (expanded) "Collapse scope" else "Expand scope",
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (expanded) {
            Column {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        "Max floor",
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.weight(1f),
                    )
                    Text(
                        "$maximumDepth",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
                // Indexes into FLOOR_LIMIT_OPTIONS so empty boss floors (5, 10, 15) are not offered.
                Slider(
                    value = floorLimitIndex(maximumDepth).toFloat(),
                    onValueChange = {
                        val index = it.roundToInt().coerceIn(0, FLOOR_LIMIT_OPTIONS.lastIndex)
                        onMaximumDepthChange(FLOOR_LIMIT_OPTIONS[index])
                    },
                    valueRange = 0f..FLOOR_LIMIT_OPTIONS.lastIndex.toFloat(),
                    steps = FLOOR_LIMIT_OPTIONS.size - 2,
                    enabled = enabled,
                    modifier = Modifier.semantics { stateDescription = "Floor $maximumDepth" },
                )
                WandmakerQuestRow(
                    quest = wandmakerQuest,
                    enabled = enabled,
                    onQuestChange = onWandmakerQuestChange,
                )
                SwitchRow(
                    label = "Blacksmith reachable",
                    supporting = null,
                    checked = requireBlacksmith,
                    onCheckedChange = onRequireBlacksmithChange,
                    enabled = enabled && maximumDepth < 14,
                )
                SwitchRow(
                    label = "Exclude smith rewards",
                    supporting = "Items may not come from the 2,000-favor Smith trade.",
                    checked = excludeBlacksmithRewards,
                    onCheckedChange = onExcludeBlacksmithRewardsChange,
                    enabled = enabled,
                )
                SwitchRow(
                    label = "Fast mode",
                    supporting = "+3 gear matches quest rewards only; skips rare Crypt " +
                        "and Sacrificial-fire seeds. Found seeds are always genuine.",
                    checked = fastMode,
                    onCheckedChange = onFastModeChange,
                    enabled = enabled,
                )
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable(onClick = onChallenges)
                        .padding(vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        "Challenges: ${Integer.bitCount(challenges)}",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.weight(1f),
                    )
                    Icon(
                        Icons.AutoMirrored.Filled.KeyboardArrowRight,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary,
                    )
                }
            }
        }
    }
}

/**
 * Which Wandmaker quest a run must roll. It sits above the blacksmith
 * switches because only this giver's item is worth choosing: corpse dust, an
 * elemental ember, or a rotberry seed can be used in the dungeon instead of
 * being handed in.
 */
@Composable
private fun WandmakerQuestRow(
    quest: WandmakerQuest?,
    enabled: Boolean,
    onQuestChange: (WandmakerQuest?) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(enabled = enabled) { expanded = true }
                .padding(vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                "Wandmaker quest",
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.weight(1f),
            )
            Text(
                quest?.label ?: "Any",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.primary,
            )
            Icon(
                Icons.Filled.KeyboardArrowDown,
                contentDescription = "Choose the Wandmaker quest",
                tint = MaterialTheme.colorScheme.primary,
            )
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            DropdownMenuItem(
                text = { Text("Any") },
                onClick = {
                    expanded = false
                    onQuestChange(null)
                },
            )
            WandmakerQuest.entries.forEach { option ->
                DropdownMenuItem(
                    text = { Text(option.label) },
                    onClick = {
                        expanded = false
                        onQuestChange(option)
                    },
                )
            }
        }
    }
}

@Composable
private fun ResultRow(result: SeedResult, onScout: () -> Unit) {
    val clipboard = LocalClipboardManager.current
    Surface(
        onClick = onScout,
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier.padding(start = 14.dp, top = 2.dp, end = 2.dp, bottom = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                result.seed,
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Bold,
                fontSize = 18.sp,
                letterSpacing = 1.sp,
                color = MaterialTheme.colorScheme.tertiary,
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = { clipboard.setText(AnnotatedString(result.seed)) }) {
                Text("Copy")
            }
            Icon(
                Icons.AutoMirrored.Filled.KeyboardArrowRight,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
private fun SearchActionBar(
    requirementCount: Int,
    status: SearchStatus?,
    seedsPerSecond: Double,
    elapsedSeconds: Long,
    isSearching: Boolean,
    onSearch: () -> Unit,
    onCancel: () -> Unit,
) {
    Surface(color = MaterialTheme.colorScheme.surfaceContainer) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 10.dp),
        ) {
            if (isSearching) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(
                            "${formatSeedRate(seedsPerSecond)} seeds/s · " +
                                "${formatElapsedTime(elapsedSeconds)} · " +
                                "${compactCount(status?.scannedSeeds ?: 0L)} scanned",
                            style = MaterialTheme.typography.labelLarge,
                        )
                        Text(
                            searchEstimateText(status, seedsPerSecond),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Spacer(Modifier.width(10.dp))
                    OutlinedButton(onClick = onCancel, shapes = ButtonDefaults.shapes()) {
                        Text("Cancel")
                    }
                }
            } else {
                Button(
                    onClick = onSearch,
                    enabled = requirementCount > 0,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(48.dp),
                    shapes = ButtonDefaults.shapes(),
                ) {
                    Text("Search", style = MaterialTheme.typography.titleMedium)
                }
            }
        }
    }
}

@Composable
private fun SwitchRow(
    label: String,
    supporting: String?,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    enabled: Boolean,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.bodyMedium)
            if (supporting != null) {
                Text(
                    supporting,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        Switch(checked = checked, onCheckedChange = onCheckedChange, enabled = enabled)
    }
}

@Composable
private fun PresetsDialog(
    presets: List<QueryPreset>,
    onApplyPreset: (QueryPreset) -> Unit,
    onSavePreset: (String) -> Unit,
    onDeletePreset: (QueryPreset) -> Unit,
    onDismiss: () -> Unit,
) {
    var presetName by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Presets") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                presets.forEach { preset ->
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        TextButton(
                            onClick = {
                                onApplyPreset(preset)
                                onDismiss()
                            },
                            modifier = Modifier.weight(1f),
                            contentPadding = PaddingValues(horizontal = 4.dp),
                        ) {
                            Text(
                                preset.name,
                                modifier = Modifier.fillMaxWidth(),
                                textAlign = TextAlign.Start,
                            )
                        }
                        if (!preset.isBuiltIn) {
                            TextButton(onClick = { onDeletePreset(preset) }) { Text("Delete") }
                        }
                    }
                }
                OutlinedTextField(
                    value = presetName,
                    onValueChange = { presetName = it },
                    label = { Text("New preset name") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                Button(
                    onClick = {
                        onSavePreset(presetName)
                        presetName = ""
                    },
                    enabled = presetName.isNotBlank(),
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Save current query") }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text("Done") }
        },
    )
}
