// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import androidx.compose.foundation.background
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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LargeFlexibleTopAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.SearchState
import dev.seedseeker.app.model.SearchStatus
import dev.seedseeker.app.model.SeedResult
import java.util.Locale
import kotlin.math.roundToInt

@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun FinderScreen(
    requirements: List<ItemRequirement>,
    maximumDepth: Int,
    requireBlacksmith: Boolean,
    excludeBlacksmithRewards: Boolean,
    fastMode: Boolean,
    results: List<SeedResult>,
    status: SearchStatus?,
    seedsPerSecond: Double,
    elapsedSeconds: Long,
    isSearching: Boolean,
    error: String?,
    onAbout: () -> Unit,
    onAdd: () -> Unit,
    onEdit: (ItemRequirement) -> Unit,
    onRemove: (ItemRequirement) -> Unit,
    onMaximumDepthChange: (Int) -> Unit,
    onRequireBlacksmithChange: (Boolean) -> Unit,
    onExcludeBlacksmithRewardsChange: (Boolean) -> Unit,
    onFastModeChange: (Boolean) -> Unit,
    onSearch: () -> Unit,
    onCancel: () -> Unit,
    onScoutSeed: (String) -> Unit,
    bottomBar: @Composable () -> Unit,
) {
    val scrollBehavior = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()
    Scaffold(
        modifier = Modifier.nestedScroll(scrollBehavior.nestedScrollConnection),
        containerColor = MaterialTheme.colorScheme.background,
        topBar = {
            LargeFlexibleTopAppBar(
                scrollBehavior = scrollBehavior,
                title = { Text("Seed Seeker") },
                subtitle = {
                    Text(
                        "Shattered Pixel Dungeon · unofficial",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                },
                actions = {
                    IconButton(onClick = onAbout) {
                        Icon(Icons.Filled.Info, contentDescription = "About and licenses")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                    scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer,
                ),
            )
        },
        bottomBar = bottomBar,
    ) { scaffoldPadding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(scaffoldPadding),
            contentAlignment = Alignment.TopCenter,
        ) {
            LazyColumn(
                modifier = Modifier
                    .fillMaxHeight()
                    .fillMaxWidth()
                    .widthIn(max = 680.dp),
                contentPadding = PaddingValues(start = 16.dp, top = 4.dp, end = 16.dp, bottom = 24.dp),
            ) {
                item {
                    SectionHeader(
                        eyebrow = "World must contain",
                        title = "Requirements",
                        supporting = "Joined with AND — every result satisfies all of them.",
                        modifier = Modifier.padding(top = 8.dp, bottom = 14.dp),
                        trailing = {
                            StatusPill(
                                text = "${requirements.size}",
                                container = MaterialTheme.colorScheme.primaryContainer,
                                content = MaterialTheme.colorScheme.onPrimaryContainer,
                            )
                        },
                    )
                }

                if (requirements.isEmpty()) {
                    item { EmptyRequirementsCard() }
                } else {
                    requirements.forEachIndexed { index, requirement ->
                        item(key = requirement.key) {
                            if (index > 0) AndConnector()
                            RequirementCard(
                                requirement = requirement,
                                enabled = !isSearching,
                                onEdit = { onEdit(requirement) },
                                onRemove = { onRemove(requirement) },
                            )
                        }
                    }
                }

                item {
                    FilledTonalButton(
                        onClick = onAdd,
                        enabled = !isSearching,
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(top = 14.dp)
                            .height(52.dp),
                        shapes = ButtonDefaults.shapes(),
                    ) {
                        Icon(Icons.Filled.Add, contentDescription = null)
                        Spacer(Modifier.width(8.dp))
                        Text("Add requirement")
                    }
                }

                item {
                    ScopeCard(
                        maximumDepth = maximumDepth,
                        requireBlacksmith = requireBlacksmith,
                        excludeBlacksmithRewards = excludeBlacksmithRewards,
                        fastMode = fastMode,
                        enabled = !isSearching,
                        onMaximumDepthChange = onMaximumDepthChange,
                        onRequireBlacksmithChange = onRequireBlacksmithChange,
                        onExcludeBlacksmithRewardsChange = onExcludeBlacksmithRewardsChange,
                        onFastModeChange = onFastModeChange,
                        modifier = Modifier.padding(top = 20.dp),
                    )
                }

                item {
                    SearchControls(
                        requirementCount = requirements.size,
                        status = status,
                        seedsPerSecond = seedsPerSecond,
                        elapsedSeconds = elapsedSeconds,
                        isSearching = isSearching,
                        error = error,
                        onSearch = onSearch,
                        onCancel = onCancel,
                        modifier = Modifier.padding(top = 20.dp),
                    )
                }

                item {
                    SectionHeader(
                        eyebrow = "Matching worlds",
                        title = "Results",
                        modifier = Modifier.padding(top = 28.dp, bottom = 12.dp),
                        trailing = {
                            val label = when {
                                isSearching -> "Live · ${results.size}"
                                status?.state == SearchState.COMPLETED -> "${results.size} found"
                                else -> "${results.size}"
                            }
                            StatusPill(label)
                        },
                    )
                }

                if (results.isEmpty()) {
                    item { EmptyResultsCard(isSearching = isSearching, status = status) }
                } else {
                    items(results, key = { it.seed }) { result ->
                        ResultCard(
                            result = result,
                            onScout = { onScoutSeed(result.seed) },
                            modifier = Modifier.padding(bottom = 10.dp),
                        )
                    }
                    if (results.size >= 1_024) {
                        item {
                            Text(
                                "Result limit reached (1,024 seeds).",
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(top = 4.dp),
                                textAlign = TextAlign.Center,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun EmptyRequirementsCard() {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Column(
            modifier = Modifier.padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            CompassMark(Modifier.size(44.dp))
            Spacer(Modifier.height(14.dp))
            Text("Describe the loot. Find the world.", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(6.dp))
            Text(
                "Add the weapons, armor, wands, and rings the dungeon must contain — with upgrades, enchantments, and sources.",
                textAlign = TextAlign.Center,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun RequirementCard(
    requirement: ItemRequirement,
    enabled: Boolean,
    onEdit: () -> Unit,
    onRemove: () -> Unit,
) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(enabled = enabled, onClick = onEdit),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
    ) {
        Row(
            modifier = Modifier.padding(start = 16.dp, top = 16.dp, end = 6.dp, bottom = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SpriteTile(item = requirement.item, modifierName = requirement.modifier)
            Spacer(Modifier.width(14.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    requirement.title,
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(3.dp))
                Text(
                    requirement.description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                requirement.item?.tier?.let {
                    Text(
                        "Tier $it · ${requirement.kind.label.lowercase(Locale.ROOT)}",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Spacer(Modifier.width(6.dp))
            StatusPill(
                text = upgradeBadgeLabel(requirement.upgradeMatch, requirement.upgrade),
                container = MaterialTheme.colorScheme.primaryContainer,
                content = MaterialTheme.colorScheme.onPrimaryContainer,
            )
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
private fun AndConnector() {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(36.dp),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .width(1.dp)
                .fillMaxHeight()
                .background(MaterialTheme.colorScheme.outlineVariant),
        )
        Surface(
            shape = MaterialTheme.shapes.large,
            color = MaterialTheme.colorScheme.surfaceContainerHighest,
        ) {
            Text(
                "AND",
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
                style = MaterialTheme.typography.labelSmall,
                letterSpacing = 1.sp,
                color = MaterialTheme.colorScheme.secondary,
            )
        }
    }
}

@Composable
private fun ScopeCard(
    maximumDepth: Int,
    requireBlacksmith: Boolean,
    excludeBlacksmithRewards: Boolean,
    fastMode: Boolean,
    enabled: Boolean,
    onMaximumDepthChange: (Int) -> Unit,
    onRequireBlacksmithChange: (Boolean) -> Unit,
    onExcludeBlacksmithRewardsChange: (Boolean) -> Unit,
    onFastModeChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Column(Modifier.padding(18.dp)) {
            Text("Search scope", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(4.dp))
            Text(
                "Every required item and facility must appear within the selected floors.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(14.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    if (maximumDepth == 1) "First floor only" else "First $maximumDepth floors",
                    modifier = Modifier.weight(1f),
                    style = MaterialTheme.typography.bodyMedium,
                )
                StatusPill(
                    text = "≤ floor $maximumDepth",
                    container = MaterialTheme.colorScheme.secondaryContainer,
                    content = MaterialTheme.colorScheme.onSecondaryContainer,
                )
            }
            Slider(
                value = maximumDepth.toFloat(),
                onValueChange = { onMaximumDepthChange(it.roundToInt()) },
                valueRange = 1f..24f,
                steps = 22,
                enabled = enabled,
            )
            Spacer(Modifier.height(6.dp))
            Text(
                "Blacksmith",
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.secondary,
            )
            Spacer(Modifier.height(6.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Accessible blacksmith", style = MaterialTheme.typography.bodyMedium)
                    Text(
                        "Require the troll blacksmith quest to be reachable.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Switch(
                    checked = requireBlacksmith,
                    onCheckedChange = onRequireBlacksmithChange,
                    enabled = enabled && maximumDepth <= 14,
                )
            }
            Spacer(Modifier.height(6.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Exclude Smith rewards", style = MaterialTheme.typography.bodyMedium)
                    Text(
                        "Required items cannot come from the 2,000-favor Smith choice, " +
                            "leaving favor available for reforging.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Switch(
                    checked = excludeBlacksmithRewards,
                    onCheckedChange = onExcludeBlacksmithRewardsChange,
                    enabled = enabled,
                )
            }
            Spacer(Modifier.height(6.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Fast search", style = MaterialTheme.typography.bodyMedium)
                    Text(
                        "Treat +3 weapons and armor as quest rewards only. Rare Crypt and " +
                            "Sacrificial-fire seeds are skipped; found seeds are always genuine.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Switch(
                    checked = fastMode,
                    onCheckedChange = onFastModeChange,
                    enabled = enabled,
                )
            }
        }
    }
}

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
private fun SearchControls(
    requirementCount: Int,
    status: SearchStatus?,
    seedsPerSecond: Double,
    elapsedSeconds: Long,
    isSearching: Boolean,
    error: String?,
    onSearch: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Column(Modifier.padding(18.dp)) {
            if (isSearching) {
                Text("Searching seeds in order…", style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.height(8.dp))
                Text(
                    searchEstimateText(status, seedsPerSecond),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.primary,
                )
                Text(
                    "Time elapsed: ${formatElapsedTime(elapsedSeconds)}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(16.dp))
                OutlinedButton(
                    onClick = onCancel,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(48.dp),
                    shapes = ButtonDefaults.shapes(),
                ) {
                    Text("Cancel search")
                }
            } else {
                Text(
                    if (requirementCount == 1) "Ready to match 1 requirement" else "Ready to match $requirementCount requirements",
                    style = MaterialTheme.typography.titleMedium,
                )
                error?.let {
                    Spacer(Modifier.height(8.dp))
                    Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                }
                Spacer(Modifier.height(14.dp))
                Button(
                    onClick = onSearch,
                    enabled = requirementCount > 0,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(56.dp),
                    shapes = ButtonDefaults.shapes(),
                ) {
                    Text("Search seeds", style = MaterialTheme.typography.titleMedium)
                }
                when (status?.state) {
                    SearchState.CANCELLED -> StatusFootnote("Search cancelled — results found so far are kept below.")
                    SearchState.COMPLETED -> StatusFootnote("Search completed.")
                    else -> Unit
                }
            }
        }
    }
}

@Composable
private fun StatusFootnote(text: String) {
    Spacer(Modifier.height(10.dp))
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun EmptyResultsCard(isSearching: Boolean, status: SearchStatus?) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Text(
            when {
                isSearching -> "Matches will appear here while the search continues."
                status?.state == SearchState.COMPLETED -> "No worlds matched every requirement. Try widening the search."
                else -> "Run a search to reveal seeds in XXX-XXX-XXX form."
            },
            modifier = Modifier
                .fillMaxWidth()
                .padding(22.dp),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun ResultCard(
    result: SeedResult,
    onScout: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val clipboard = LocalClipboardManager.current
    Card(
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onScout),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
    ) {
        Row(
            modifier = Modifier.padding(start = 18.dp, top = 12.dp, end = 6.dp, bottom = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    result.seed,
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.Bold,
                    fontSize = 21.sp,
                    letterSpacing = 1.1.sp,
                    color = MaterialTheme.colorScheme.tertiary,
                )
                Text(
                    "Matches all ${result.matchedRequirements} — tap to scout",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
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
