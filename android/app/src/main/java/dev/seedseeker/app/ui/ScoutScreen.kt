// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

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
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.outlined.Place
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LoadingIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.engine.SeedCode
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutAccessibility
import dev.seedseeker.app.model.ScoutItem
import dev.seedseeker.app.model.ScoutWorld

@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun ScoutScreen(
    seedInput: String,
    result: ScoutWorld?,
    isScouting: Boolean,
    error: String?,
    requirements: List<ItemRequirement>,
    maximumDepth: Int,
    excludeBlacksmithRewards: Boolean,
    onSeedChange: (String) -> Unit,
    onScout: () -> Unit,
    onChallenges: () -> Unit,
    onAbout: () -> Unit,
    bottomBar: @Composable () -> Unit,
) {
    val seedIsReady = SeedCode.isCanonical(seedInput)
    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        topBar = {
            TopAppBar(
                title = { Text("Scout") },
                actions = {
                    IconButton(onClick = onChallenges) {
                        Icon(Icons.Filled.Settings, contentDescription = "Challenges")
                    }
                    IconButton(onClick = onAbout) {
                        Icon(Icons.Filled.Info, contentDescription = "About and licenses")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
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
                    SeedInputCard(
                        seedInput = seedInput,
                        seedIsReady = seedIsReady,
                        isScouting = isScouting,
                        error = error,
                        onSeedChange = onSeedChange,
                        onScout = onScout,
                    )
                }

                if (result == null && !isScouting) {
                    item {
                        Card(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(top = 20.dp),
                            shape = MaterialTheme.shapes.large,
                            colors = CardDefaults.cardColors(
                                containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                            ),
                        ) {
                            Column(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(24.dp),
                                horizontalAlignment = Alignment.CenterHorizontally,
                            ) {
                                Icon(
                                    Icons.Outlined.Place,
                                    contentDescription = null,
                                    modifier = Modifier.size(44.dp),
                                    tint = MaterialTheme.colorScheme.primary,
                                )
                                Spacer(Modifier.height(14.dp))
                                Text(
                                    "Enter a seed or tap a search result to list its items through floor 24.",
                                    textAlign = TextAlign.Center,
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                }

                result?.let { world ->
                    val matchIndices = scoutMatchIndices(
                        items = world.items,
                        requirements = requirements,
                        maximumDepth = maximumDepth,
                        excludeBlacksmithRewards = excludeBlacksmithRewards,
                    )
                    item {
                        ScoutSummaryCard(
                            world = world,
                            matchCount = matchIndices.size,
                            showMatchCount = requirements.isNotEmpty(),
                            modifier = Modifier.padding(top = 22.dp),
                        )
                    }

                    world.items.withIndex()
                        .groupBy { it.value.depth }
                        .toSortedMap()
                        .forEach { (depth, floorItems) ->
                            item(key = "floor-$depth") {
                                FloorHeading(
                                    depth = depth,
                                    itemCount = floorItems.size,
                                    modifier = Modifier.padding(top = 20.dp, bottom = 10.dp),
                                )
                            }
                            floorItems.forEach { indexedItem ->
                                val scoutItem = indexedItem.value
                                item(key = "scout-$depth-${indexedItem.index}-${scoutItem.item.id}") {
                                    ScoutItemCard(
                                        scoutItem = scoutItem,
                                        matches = indexedItem.index in matchIndices,
                                        modifier = Modifier.padding(bottom = 8.dp),
                                    )
                                }
                            }
                        }
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
private fun SeedInputCard(
    seedInput: String,
    seedIsReady: Boolean,
    isScouting: Boolean,
    error: String?,
    onSeedChange: (String) -> Unit,
    onScout: () -> Unit,
) {
    var fieldValue by remember {
        mutableStateOf(
            TextFieldValue(seedInput, selection = TextRange(seedInput.length)),
        )
    }
    LaunchedEffect(seedInput) {
        if (seedInput != fieldValue.text) {
            fieldValue = TextFieldValue(seedInput, selection = TextRange(seedInput.length))
        }
    }

    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Column(Modifier.padding(18.dp)) {
            OutlinedTextField(
                value = fieldValue,
                onValueChange = {
                    val formattedValue = formatSeedFieldValue(it)
                    fieldValue = formattedValue
                    onSeedChange(formattedValue.text)
                },
                enabled = !isScouting,
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Seed") },
                placeholder = { Text("ABC-DEF-GHI") },
                singleLine = true,
                shape = MaterialTheme.shapes.medium,
                textStyle = MaterialTheme.typography.titleLarge.copy(
                    fontFamily = FontFamily.Monospace,
                    letterSpacing = 1.2.sp,
                ),
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.Characters,
                    keyboardType = KeyboardType.Ascii,
                    imeAction = ImeAction.Search,
                ),
                keyboardActions = KeyboardActions(
                    onSearch = { if (seedIsReady && !isScouting) onScout() },
                ),
            )
            Spacer(Modifier.height(12.dp))
            Button(
                onClick = onScout,
                enabled = seedIsReady && !isScouting,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(52.dp),
                shapes = ButtonDefaults.shapes(),
            ) {
                if (isScouting) {
                    LoadingIndicator(modifier = Modifier.size(28.dp))
                    Spacer(Modifier.width(10.dp))
                    Text("Generating world…")
                } else {
                    Text("Scout seed")
                }
            }
            error?.let {
                Spacer(Modifier.height(10.dp))
                Text(
                    it,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
    }
}

/** Keeps the logical cursor position when canonical grouping inserts or removes hyphens. */
internal fun formatSeedFieldValue(input: TextFieldValue): TextFieldValue {
    val formatted = SeedCode.formatInput(input.text)
    if (formatted == input.text) return input

    fun remapOffset(offset: Int): Int = SeedCode
        .formatInput(input.text.take(offset))
        .length
        .coerceAtMost(formatted.length)

    return TextFieldValue(
        text = formatted,
        selection = TextRange(
            remapOffset(input.selection.start),
            remapOffset(input.selection.end),
        ),
    )
}

@Composable
private fun ScoutSummaryCard(
    world: ScoutWorld,
    matchCount: Int,
    showMatchCount: Boolean,
    modifier: Modifier = Modifier,
) {
    val clipboard = LocalClipboardManager.current
    val floors = world.items.map(ScoutItem::depth).distinct().size
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
    ) {
        Column(Modifier.padding(18.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    world.seed,
                    modifier = Modifier.weight(1f),
                    style = MaterialTheme.typography.headlineSmall,
                    fontFamily = FontFamily.Monospace,
                    color = MaterialTheme.colorScheme.tertiary,
                )
                TextButton(onClick = { clipboard.setText(AnnotatedString(world.seed)) }) {
                    Text("Copy")
                }
            }
            Spacer(Modifier.height(10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                StatusPill("${world.items.size} items")
                StatusPill("$floors floors")
                if (showMatchCount) {
                    StatusPill(
                        text = if (matchCount == 1) "1 match" else "$matchCount matches",
                        container = if (matchCount > 0) {
                            MaterialTheme.colorScheme.primaryContainer
                        } else {
                            MaterialTheme.colorScheme.surfaceContainerHighest
                        },
                        content = if (matchCount > 0) {
                            MaterialTheme.colorScheme.onPrimaryContainer
                        } else {
                            MaterialTheme.colorScheme.onSurfaceVariant
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun FloorHeading(depth: Int, itemCount: Int, modifier: Modifier = Modifier) {
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            "FLOOR $depth",
            style = MaterialTheme.typography.labelLarge,
            letterSpacing = 1.1.sp,
            color = MaterialTheme.colorScheme.primary,
        )
        Spacer(Modifier.width(8.dp))
        Text(
            floorRegion(depth),
            modifier = Modifier.weight(1f),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            if (itemCount == 1) "1 item" else "$itemCount items",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun ScoutItemCard(
    scoutItem: ScoutItem,
    matches: Boolean,
    modifier: Modifier = Modifier,
) {
    val effectIsCurse = scoutItem.effect != null &&
        ItemCatalog.cursesFor(scoutItem.item.kind).contains(scoutItem.effect)
    val accessibilityLabel = when (scoutItem.accessibility) {
        ScoutAccessibility.Independent -> null
        is ScoutAccessibility.Choice ->
            "Choice group ${scoutItem.accessibility.group + 1} · option ${scoutItem.accessibility.option + 1}"
        is ScoutAccessibility.Scenarios ->
            "Route group ${scoutItem.accessibility.group + 1} · access changes with room choices"
    }

    Card(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = if (matches) {
                MaterialTheme.colorScheme.surfaceContainerHighest
            } else {
                MaterialTheme.colorScheme.surfaceContainerLow
            },
        ),
    ) {
        Row(
            modifier = Modifier.padding(14.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SpriteTile(item = scoutItem.item, modifierName = scoutItem.effect, tileSize = 56)
            Spacer(Modifier.width(14.dp))
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        scoutItem.item.name,
                        style = MaterialTheme.typography.titleMedium,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    if (scoutItem.cursed) {
                        Spacer(Modifier.width(8.dp))
                        Surface(
                            shape = MaterialTheme.shapes.small,
                            color = MaterialTheme.colorScheme.errorContainer,
                        ) {
                            Text(
                                "Cursed",
                                modifier = Modifier.padding(horizontal = 7.dp, vertical = 2.dp),
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onErrorContainer,
                            )
                        }
                    }
                }
                scoutItem.effect?.let { effect ->
                    Text(
                        effect,
                        style = MaterialTheme.typography.bodySmall,
                        color = if (effectIsCurse) {
                            MaterialTheme.colorScheme.error
                        } else {
                            MaterialTheme.colorScheme.tertiary
                        },
                    )
                }
                Text(
                    scoutItem.source.label,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                accessibilityLabel?.let {
                    Spacer(Modifier.height(2.dp))
                    Text(
                        it,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.tertiary,
                    )
                }
            }
            Spacer(Modifier.width(10.dp))
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                if (scoutItem.upgrade != 0) {
                    StatusPill(
                        text = "+${scoutItem.upgrade}",
                        container = MaterialTheme.colorScheme.secondaryContainer,
                        content = MaterialTheme.colorScheme.onSecondaryContainer,
                    )
                }
                if (matches) {
                    Surface(
                        shape = MaterialTheme.shapes.large,
                        color = MaterialTheme.colorScheme.primary,
                    ) {
                        Row(
                            modifier = Modifier.padding(horizontal = 9.dp, vertical = 4.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Icon(
                                Icons.Filled.Check,
                                contentDescription = null,
                                modifier = Modifier.size(13.dp),
                                tint = MaterialTheme.colorScheme.onPrimary,
                            )
                            Spacer(Modifier.width(4.dp))
                            Text(
                                "Match",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onPrimary,
                            )
                        }
                    }
                }
            }
        }
    }
}
