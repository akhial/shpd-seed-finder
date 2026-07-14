// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.engine.NativeSearchSession
import dev.seedseeker.app.engine.NativeSeedFinder
import dev.seedseeker.app.engine.SeedCode
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.Challenge
import dev.seedseeker.app.model.ScoutWorld
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.SearchState
import dev.seedseeker.app.model.SearchStatus
import dev.seedseeker.app.model.SeedResult
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private const val ATLAS_PATH = "third_party/shattered-pixel-dungeon/items.png"
private const val ITEM_ICONS_PATH = "third_party/shattered-pixel-dungeon/item_icons.png"
private const val SETTINGS_PREFERENCES = "seed_seeker_settings"
private const val CHALLENGES_KEY = "challenges_mask"

private enum class Destination { FINDER, SCOUT, CHALLENGES, ABOUT }
private data class SearchRun(val id: Long, val request: SearchRequest)
private data class ScoutRun(val id: Long, val seed: String, val challenges: Int)

@Composable
fun SeedFinderApp(engine: NativeSeedFinder) {
    val context = LocalContext.current
    val atlas = remember(context) {
        runCatching {
            context.assets.open(ATLAS_PATH).use(BitmapFactory::decodeStream)
                ?.centerSpriteCells()
                ?.asImageBitmap()
        }.getOrNull()
    }
    val itemIcons = remember(context) {
        runCatching {
            context.assets.open(ITEM_ICONS_PATH).use(BitmapFactory::decodeStream)?.asImageBitmap()
        }.getOrNull()
    }
    val scope = rememberCoroutineScope()
    val preferences = remember(context) {
        context.getSharedPreferences(SETTINGS_PREFERENCES, Context.MODE_PRIVATE)
    }

    var destination by remember { mutableStateOf(Destination.FINDER) }
    var aboutReturnDestination by remember { mutableStateOf(Destination.FINDER) }
    var challengesReturnDestination by remember { mutableStateOf(Destination.FINDER) }
    var requirements by remember {
        mutableStateOf(
            listOf(
                ItemRequirement(1, ItemCatalog.weapons.first { it.id == "sword" }, 2, "Lucky"),
                ItemRequirement(2, ItemCatalog.armor.first { it.id == "plate_armor" }, 1, "Brimstone"),
            ),
        )
    }
    var nextRequirementKey by remember { mutableLongStateOf(3L) }
    var maximumDepth by remember { mutableStateOf(24) }
    var requireBlacksmith by remember { mutableStateOf(false) }
    var excludeBlacksmithRewards by remember { mutableStateOf(false) }
    var fastMode by remember { mutableStateOf(false) }
    var challenges by remember {
        mutableStateOf(
            preferences.getInt(CHALLENGES_KEY, 0).takeIf { it in 0..Challenge.ALL_MASK } ?: 0,
        )
    }
    var editingRequirement by remember { mutableStateOf<ItemRequirement?>(null) }
    var showRequirementSheet by remember { mutableStateOf(false) }
    var results by remember { mutableStateOf(emptyList<SeedResult>()) }
    var searchStatus by remember { mutableStateOf<SearchStatus?>(null) }
    var searchSeedsPerSecond by remember { mutableStateOf(0.0) }
    var searchElapsedSeconds by remember { mutableLongStateOf(0L) }
    var activeSession by remember { mutableStateOf<NativeSearchSession?>(null) }
    var run by remember { mutableStateOf<SearchRun?>(null) }
    var nextRunId by remember { mutableLongStateOf(1L) }
    var isSearching by remember { mutableStateOf(false) }
    var searchError by remember { mutableStateOf<String?>(null) }
    var scoutInput by remember { mutableStateOf("") }
    var scoutResult by remember { mutableStateOf<ScoutWorld?>(null) }
    var scoutRun by remember { mutableStateOf<ScoutRun?>(null) }
    var nextScoutRunId by remember { mutableLongStateOf(1L) }
    var isScouting by remember { mutableStateOf(false) }
    var scoutError by remember { mutableStateOf<String?>(null) }

    PredictiveBackHandler(enabled = destination != Destination.FINDER) { progress ->
        progress.collect { }
        destination = when (destination) {
            Destination.ABOUT -> aboutReturnDestination
            Destination.CHALLENGES -> challengesReturnDestination
            else -> Destination.FINDER
        }
    }

    LaunchedEffect(run?.id) {
        val currentRun = run ?: return@LaunchedEffect
        isSearching = true
        searchError = null
        results = emptyList()
        searchStatus = null
        searchSeedsPerSecond = 0.0
        searchElapsedSeconds = 0L

        val searchStartedAt = System.nanoTime()
        var previousScannedSeeds = 0L
        var previousStatusTime = System.nanoTime()

        var session: NativeSearchSession? = null
        try {
            val openedSession = withContext(Dispatchers.Default) {
                engine.startSearch(currentRun.request)
            }
            session = openedSession
            activeSession = openedSession

            while (true) {
                val (batch, status) = withContext(Dispatchers.Default) {
                    openedSession.poll(24) to openedSession.status()
                }
                if (batch.results.isNotEmpty()) {
                    results = results + batch.results
                }
                val statusTime = System.nanoTime()
                searchElapsedSeconds = (statusTime - searchStartedAt) / 1_000_000_000L
                val elapsedSeconds = (statusTime - previousStatusTime) / 1_000_000_000.0
                if (elapsedSeconds > 0.0 && status.scannedSeeds > previousScannedSeeds) {
                    val instantRate = (status.scannedSeeds - previousScannedSeeds) / elapsedSeconds
                    searchSeedsPerSecond = if (searchSeedsPerSecond == 0.0) {
                        instantRate
                    } else {
                        searchSeedsPerSecond * 0.7 + instantRate * 0.3
                    }
                }
                previousScannedSeeds = status.scannedSeeds
                previousStatusTime = statusTime
                searchStatus = status
                if (status.state == SearchState.FAILED) {
                    searchError = when (status.errorCode) {
                        2_001L -> "A native world-generation worker stopped unexpectedly."
                        else -> "The native search stopped with error ${status.errorCode}."
                    }
                }
                if (status.state != SearchState.RUNNING) break
                delay(90)
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (failure: Throwable) {
            searchError = failure.message ?: "The native search engine could not start."
            searchStatus = SearchStatus(SearchState.FAILED, 0, 0, -1)
        } finally {
            activeSession = null
            isSearching = false
            session?.let {
                withContext(NonCancellable + Dispatchers.Default) { it.close() }
            }
        }
    }

    LaunchedEffect(scoutRun?.id) {
        val currentRun = scoutRun ?: return@LaunchedEffect
        isScouting = true
        scoutError = null
        scoutResult = null
        try {
            scoutResult = withContext(Dispatchers.Default) {
                engine.scoutSeed(currentRun.seed, currentRun.challenges)
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (failure: Throwable) {
            scoutError = failure.message ?: "The native scout could not generate this seed."
        } finally {
            isScouting = false
        }
    }

    fun scoutSeed(seed: String) {
        val formatted = SeedCode.formatInput(seed)
        scoutInput = formatted
        scoutError = null
        destination = Destination.SCOUT
        if (SeedCode.isCanonical(formatted)) {
            scoutRun = ScoutRun(nextScoutRunId++, formatted, challenges)
        }
    }

    val navBar: @Composable () -> Unit = {
        SeedSeekerNavBar(
            current = destination,
            onSelect = { destination = it },
        )
    }

    CompositionLocalProvider(
        LocalItemAtlas provides atlas,
        LocalItemIconAtlas provides itemIcons,
    ) {
        when (destination) {
            Destination.FINDER -> FinderScreen(
                requirements = requirements,
                maximumDepth = maximumDepth,
                requireBlacksmith = requireBlacksmith,
                excludeBlacksmithRewards = excludeBlacksmithRewards,
                fastMode = fastMode,
                challenges = challenges,
                results = results,
                status = searchStatus,
                seedsPerSecond = searchSeedsPerSecond,
                elapsedSeconds = searchElapsedSeconds,
                isSearching = isSearching,
                error = searchError,
                onAbout = {
                    aboutReturnDestination = Destination.FINDER
                    destination = Destination.ABOUT
                },
                onChallenges = {
                    challengesReturnDestination = Destination.FINDER
                    destination = Destination.CHALLENGES
                },
                onAdd = {
                    editingRequirement = null
                    showRequirementSheet = true
                },
                onEdit = {
                    editingRequirement = it
                    showRequirementSheet = true
                },
                onRemove = { requirement ->
                    requirements = requirements.filterNot { it.key == requirement.key }
                },
                onMaximumDepthChange = { maximumDepth = it },
                onRequireBlacksmithChange = { requireBlacksmith = it },
                onExcludeBlacksmithRewardsChange = { excludeBlacksmithRewards = it },
                onFastModeChange = { fastMode = it },
                onSearch = {
                    if (requirements.isNotEmpty()) {
                        run = SearchRun(
                            nextRunId++,
                            SearchRequest(
                                requirements = requirements,
                                maximumDepth = maximumDepth,
                                challenges = challenges,
                                requireBlacksmith = requireBlacksmith,
                                excludeBlacksmithRewards = excludeBlacksmithRewards,
                                fastMode = fastMode,
                            ),
                        )
                    }
                },
                onCancel = {
                    val session = activeSession
                    if (session != null) {
                        scope.launch(Dispatchers.Default) { session.cancel() }
                    }
                },
                onScoutSeed = ::scoutSeed,
                bottomBar = navBar,
            )

            Destination.SCOUT -> ScoutScreen(
                seedInput = scoutInput,
                result = scoutResult,
                isScouting = isScouting,
                error = scoutError,
                requirements = requirements,
                maximumDepth = maximumDepth,
                excludeBlacksmithRewards = excludeBlacksmithRewards,
                onSeedChange = {
                    val formatted = SeedCode.formatInput(it)
                    scoutInput = formatted
                    if (formatted != scoutResult?.seed) scoutResult = null
                    scoutError = null
                },
                onScout = {
                    if (SeedCode.isCanonical(scoutInput)) {
                        scoutRun = ScoutRun(nextScoutRunId++, scoutInput, challenges)
                    }
                },
                onChallenges = {
                    challengesReturnDestination = Destination.SCOUT
                    destination = Destination.CHALLENGES
                },
                onAbout = {
                    aboutReturnDestination = Destination.SCOUT
                    destination = Destination.ABOUT
                },
                bottomBar = navBar,
            )

            Destination.CHALLENGES -> ChallengesScreen(
                challenges = challenges,
                enabled = !isSearching && !isScouting,
                onChallengeChange = { challenge, checked ->
                    val updatedChallenges = if (checked) {
                        challenges or challenge.bit
                    } else {
                        challenges and challenge.bit.inv()
                    }
                    challenges = updatedChallenges
                    scoutResult = null
                    preferences.edit().putInt(CHALLENGES_KEY, updatedChallenges).apply()
                },
                onBack = { destination = challengesReturnDestination },
            )

            Destination.ABOUT -> AboutScreen(onBack = { destination = aboutReturnDestination })
        }

        if (showRequirementSheet) {
            RequirementSheet(
                editing = editingRequirement,
                onDismiss = { showRequirementSheet = false },
                onSave = { item, kind, tierMatch, tier, upgradeMatch, upgrade, modifier, source, identityGroup, itemMaximumDepth, requireUncursed ->
                    val existing = editingRequirement
                    if (existing == null) {
                        requirements = requirements + ItemRequirement(
                            key = nextRequirementKey++,
                            item = item,
                            upgrade = upgrade,
                            modifier = modifier,
                            kind = kind,
                            tier = tier,
                            tierMatch = tierMatch,
                            upgradeMatch = upgradeMatch,
                            source = source,
                            identityGroup = identityGroup,
                            maximumDepth = itemMaximumDepth,
                            requireUncursed = requireUncursed,
                        )
                    } else {
                        requirements = requirements.map {
                            if (it.key == existing.key) {
                                existing.copy(
                                    item = item,
                                    upgrade = upgrade,
                                    modifier = modifier,
                                    kind = kind,
                                    tier = tier,
                                    tierMatch = tierMatch,
                                    upgradeMatch = upgradeMatch,
                                    source = source,
                                    identityGroup = identityGroup,
                                    maximumDepth = itemMaximumDepth,
                                    requireUncursed = requireUncursed,
                                )
                            } else {
                                it
                            }
                        }
                    }
                    showRequirementSheet = false
                },
            )
        }
    }
}

/**
 * The upstream atlas packs each item against the top-left of its 16 px cell. Recenter the
 * non-transparent pixels while retaining every sprite's original size and pixel-art scaling.
 */
private fun Bitmap.centerSpriteCells(cellSize: Int = 16): Bitmap {
    require(width % cellSize == 0 && height % cellSize == 0)

    val sourcePixels = IntArray(width * height)
    getPixels(sourcePixels, 0, width, 0, 0, width, height)

    val centered = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888).also {
        it.density = density
    }
    val canvas = Canvas(centered)
    val paint = Paint().apply { isFilterBitmap = false }

    for (cellY in 0 until height step cellSize) {
        for (cellX in 0 until width step cellSize) {
            var minX = cellSize
            var minY = cellSize
            var maxX = -1
            var maxY = -1

            for (y in 0 until cellSize) {
                for (x in 0 until cellSize) {
                    if (sourcePixels[(cellY + y) * width + cellX + x] ushr 24 != 0) {
                        minX = minOf(minX, x)
                        minY = minOf(minY, y)
                        maxX = maxOf(maxX, x)
                        maxY = maxOf(maxY, y)
                    }
                }
            }

            if (maxX < 0) continue

            val offsetX = (cellSize - 1 - minX - maxX) / 2
            val offsetY = (cellSize - 1 - minY - maxY) / 2
            val source = Rect(cellX, cellY, cellX + cellSize, cellY + cellSize)
            val destination = Rect(
                cellX + offsetX,
                cellY + offsetY,
                cellX + cellSize + offsetX,
                cellY + cellSize + offsetY,
            )
            canvas.drawBitmap(this, source, destination, paint)
        }
    }

    return centered
}

@Composable
private fun SeedSeekerNavBar(
    current: Destination,
    onSelect: (Destination) -> Unit,
) {
    NavigationBar(containerColor = MaterialTheme.colorScheme.surfaceContainer) {
        NavigationBarItem(
            selected = current == Destination.FINDER,
            onClick = { onSelect(Destination.FINDER) },
            icon = {
                Icon(
                    if (current == Destination.FINDER) Icons.Filled.Search else Icons.Outlined.Search,
                    contentDescription = null,
                )
            },
            label = { Text("Finder") },
            colors = NavigationBarItemDefaults.colors(
                indicatorColor = MaterialTheme.colorScheme.primaryContainer,
            ),
        )
        NavigationBarItem(
            selected = current == Destination.SCOUT,
            onClick = { onSelect(Destination.SCOUT) },
            icon = { CompassMark(Modifier.size(22.dp)) },
            label = { Text("Scout") },
            colors = NavigationBarItemDefaults.colors(
                indicatorColor = MaterialTheme.colorScheme.primaryContainer,
            ),
        )
    }
}
