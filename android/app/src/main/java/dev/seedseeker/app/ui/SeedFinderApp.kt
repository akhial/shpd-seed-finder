// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import androidx.activity.compose.PredictiveBackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Place
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.outlined.Place
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import dev.seedseeker.app.BuildConfig
import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.engine.NativeSearchSession
import dev.seedseeker.app.engine.NativeSeedFinder
import dev.seedseeker.app.engine.SeedCode
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.Challenge
import dev.seedseeker.app.model.BuiltInPresets
import dev.seedseeker.app.model.PresetQuery
import dev.seedseeker.app.model.PresetStorage
import dev.seedseeker.app.model.QueryPreset
import dev.seedseeker.app.model.ResultsExport
import dev.seedseeker.app.model.ScoutWorld
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.SearchState
import dev.seedseeker.app.model.SearchStatus
import dev.seedseeker.app.model.SeedResult
import dev.seedseeker.app.update.UpdateChecker
import dev.seedseeker.app.update.UpdateInfo
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
private const val UPDATE_LAST_CHECK_KEY = "update_last_check"
private const val UPDATE_SKIPPED_KEY = "update_skipped_version"
private const val UPDATE_CHECK_INTERVAL_MILLIS = 24L * 60 * 60 * 1000

private const val MAX_IMPORTED_RESULTS = 1_024

/** Reads at most `limit` bytes as UTF-8, failing on larger files. */
private fun readCapped(stream: java.io.InputStream, limit: Int): String {
    val buffer = ByteArray(limit + 1)
    var total = 0
    while (total < buffer.size) {
        val read = stream.read(buffer, total, buffer.size - total)
        if (read < 0) break
        total += read
    }
    require(total <= limit) {
        "This file is too large to be a Seed Seeker results file (2 MiB limit)."
    }
    return String(buffer, 0, total, Charsets.UTF_8)
}

private enum class Destination { FINDER, SCOUT, CHALLENGES, ABOUT }
private data class SearchRun(val id: Long, val request: SearchRequest, val refine: RefineSpec? = null)

private data class ScoutRun(val id: Long, val seed: String, val challenges: Int)

@Composable
fun SeedFinderApp(engine: NativeSeedFinder, fakeLatestVersion: String? = null) {
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
    val presetStorage = remember(preferences) { PresetStorage(preferences) }

    var destination by remember { mutableStateOf(Destination.FINDER) }
    var aboutReturnDestination by remember { mutableStateOf(Destination.FINDER) }
    var challengesReturnDestination by remember { mutableStateOf(Destination.FINDER) }
    var requirements by remember {
        mutableStateOf(
            listOf(
                ItemRequirement(1, ItemCatalog.wands.first { it.id == "wand_fireblast" }, 3),
            ),
        )
    }
    var nextRequirementKey by remember { mutableLongStateOf(2L) }
    var userPresets by remember { mutableStateOf(presetStorage.load()) }
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
    var lastFinishedRun by remember { mutableStateOf<FinishedRun?>(null) }
    // Null unless a refine run is in flight; distinguishes its filter phase from the resumed scan.
    var refinePhase by remember { mutableStateOf<RefinePhase?>(null) }
    var isSearching by remember { mutableStateOf(false) }
    var searchError by remember { mutableStateOf<String?>(null) }
    val snackbarHostState = remember { SnackbarHostState() }
    var scoutInput by remember { mutableStateOf("") }
    var scoutResult by remember { mutableStateOf<ScoutWorld?>(null) }
    var scoutRun by remember { mutableStateOf<ScoutRun?>(null) }
    var nextScoutRunId by remember { mutableLongStateOf(1L) }
    var isScouting by remember { mutableStateOf(false) }
    var scoutError by remember { mutableStateOf<String?>(null) }
    var availableUpdate by remember { mutableStateOf<UpdateInfo?>(null) }
    // Survives the activity recreation a document picker can trigger.
    var pendingExport by rememberSaveable { mutableStateOf<String?>(null) }
    var transferError by remember { mutableStateOf<String?>(null) }
    var importNotice by remember { mutableStateOf<String?>(null) }
    // The query that produced the current results, snapshotted at search
    // start (or import) so an export never reflects later editor changes.
    var searchedQuery by remember { mutableStateOf<PresetQuery?>(null) }

    val exportLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/json"),
    ) { uri ->
        val contents = pendingExport
        pendingExport = null
        if (uri != null && contents != null) {
            scope.launch {
                withContext(Dispatchers.IO) {
                    runCatching {
                        context.contentResolver.openOutputStream(uri, "wt")
                            ?.use { it.write(contents.toByteArray()) }
                            ?: error("Could not open the selected file.")
                    }
                }.onFailure { failure ->
                    transferError = "Export failed: ${failure.message}"
                }
            }
        }
    }
    val importLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val outcome = withContext(Dispatchers.IO) {
                runCatching {
                    val text = context.contentResolver.openInputStream(uri)?.use { stream ->
                        readCapped(stream, ResultsExport.MAX_FILE_BYTES)
                    } ?: error("Could not read the selected file.")
                    ResultsExport.decode(text)
                }
            }
            outcome.onSuccess { imported ->
                // A search may have started while the picker was open or the
                // file was being read.
                if (isSearching) {
                    transferError = "Stop the search before importing results."
                    return@onSuccess
                }
                requirements = imported.query.requirements.map { it.copy(key = nextRequirementKey++) }
                maximumDepth = imported.query.maximumDepth
                requireBlacksmith = imported.query.requireBlacksmith
                excludeBlacksmithRewards = imported.query.excludeBlacksmithRewards
                fastMode = imported.query.fastMode
                challenges = imported.query.challenges
                preferences.edit().putInt(CHALLENGES_KEY, challenges).apply()
                // Deduplicate then cap, the shared import rule on every platform.
                val kept = LinkedHashSet<String>()
                for (seed in imported.seeds) {
                    if (kept.size == MAX_IMPORTED_RESULTS) break
                    kept.add(seed)
                }
                val dropped = imported.seeds.size - kept.size
                results = kept.map { SeedResult(it, imported.query.requirements.size) }
                searchedQuery = imported.query
                // Imported results carry no traversal state, so the previous
                // search's refine base no longer describes the listed seeds.
                lastFinishedRun = null
                searchStatus = null
                searchError = null
                importNotice = buildString {
                    append("Imported ${kept.size} seed${if (kept.size == 1) "" else "s"} from file")
                    if (dropped > 0) {
                        append(" · $dropped duplicate or over-limit entr${if (dropped == 1) "y" else "ies"} dropped")
                    }
                    val fileVersion = imported.shpdVersion
                    if (fileVersion != null && fileVersion != ResultsExport.SHPD_VERSION) {
                        append(
                            " · made for Shattered Pixel Dungeon v$fileVersion; this app targets " +
                                "v${ResultsExport.SHPD_VERSION}, so seeds may generate differently",
                        )
                    }
                }
            }.onFailure { failure ->
                transferError = failure.message ?: "The results file could not be imported."
            }
        }
    }

    LaunchedEffect(Unit) {
        val now = System.currentTimeMillis()
        val lastCheck = preferences.getLong(UPDATE_LAST_CHECK_KEY, 0L)
        if (fakeLatestVersion == null && now - lastCheck < UPDATE_CHECK_INTERVAL_MILLIS) {
            return@LaunchedEffect
        }
        preferences.edit().putLong(UPDATE_LAST_CHECK_KEY, now).apply()
        val update = withContext(Dispatchers.IO) {
            UpdateChecker.check(BuildConfig.VERSION_NAME, fakeLatestVersion)
        }
        if (update != null && update.version != preferences.getString(UPDATE_SKIPPED_KEY, null)) {
            availableUpdate = update
        }
    }

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
        searchStatus = null
        // Set together with isSearching so the header never reads one without the other.
        refinePhase = if (currentRun.refine != null) RefinePhase.FILTERING else null
        searchSeedsPerSecond = 0.0
        searchElapsedSeconds = 0L
        // Local to the effect so the limit snackbar fires once per run, never per recomposition.
        var resultLimitNotified = false
        fun notifyIfResultLimitReached() {
            if (resultLimitNotified || results.size < 1_024) return
            resultLimitNotified = true
            // Launched on the app scope so the queued snackbar never suspends the search loop.
            scope.launch { snackbarHostState.showSnackbar("Result limit reached (1,024 seeds).") }
        }

        val searchStartedAt = System.nanoTime()
        var previousScannedSeeds = 0L
        var previousStatusTime = System.nanoTime()

        var session: NativeSearchSession? = null
        try {
            val refine = currentRun.refine
            if (refine == null) {
                results = emptyList()
            } else {
                // Re-verify the seeds already on screen against the narrowed query, then
                // rescan only the window the base run never reached.
                val kept = withContext(Dispatchers.Default) {
                    engine.filterSeeds(currentRun.request, refine.keepSeeds.map { it.seed })
                }
                results = kept.map { SeedResult(it, currentRun.request.requirements.size) }
                // From here on the listed results match the refined request, so
                // that is what an export must claim. A cancelled filter phase
                // leaves the previous results — and their snapshot — untouched.
                searchedQuery = PresetQuery(
                    requirements = currentRun.request.requirements,
                    maximumDepth = currentRun.request.maximumDepth,
                    requireBlacksmith = currentRun.request.requireBlacksmith,
                    excludeBlacksmithRewards = currentRun.request.excludeBlacksmithRewards,
                    fastMode = currentRun.request.fastMode,
                    challenges = currentRun.request.challenges,
                )
                scope.launch {
                    snackbarHostState.showSnackbar(
                        "Kept ${kept.size} of ${refine.keepSeeds.size} previous seeds.",
                    )
                }
                notifyIfResultLimitReached()
                if (refine.remaining == 0L) {
                    searchStatus = SearchStatus(SearchState.COMPLETED, 0, 0)
                    lastFinishedRun = FinishedRun(currentRun.request, refine.resumeFrom, 0, results)
                    return@LaunchedEffect
                }
            }

            // The kept seeds are re-verified; what follows is an ordinary scan of the
            // window the base run never reached, so the header stops saying "refining".
            if (refine != null) refinePhase = RefinePhase.SCANNING

            val openedSession = withContext(Dispatchers.Default) {
                if (refine == null) {
                    engine.startSearch(currentRun.request)
                } else {
                    engine.startResumedSearch(currentRun.request, refine.resumeFrom, refine.remaining)
                }
            }
            session = openedSession
            activeSession = openedSession

            val seenSeeds = results.mapTo(mutableSetOf()) { it.seed }
            while (true) {
                val (batch, status) = withContext(Dispatchers.Default) {
                    openedSession.poll(24) to openedSession.status()
                }
                // The results list keys a LazyColumn by seed, so drop seeds the filter kept.
                val newResults = batch.results.filter { seenSeeds.add(it.seed) }
                if (newResults.isNotEmpty()) {
                    results = results + newResults
                    notifyIfResultLimitReached()
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
                    lastFinishedRun = null
                }
                if (status.state != SearchState.RUNNING) {
                    if (status.state != SearchState.FAILED) {
                        // The hint is only exact once the session has stopped, and it must be
                        // read before the finally block closes the handle.
                        val hint = withContext(Dispatchers.Default) { openedSession.resumeHint() }
                        lastFinishedRun =
                            FinishedRun(currentRun.request, hint.position, hint.remaining, results)
                    }
                    break
                }
                delay(90)
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (failure: Throwable) {
            searchError = failure.message ?: "The native search engine could not start."
            searchStatus = SearchStatus(SearchState.FAILED, 0, 0, -1)
            lastFinishedRun = null
        } finally {
            activeSession = null
            isSearching = false
            refinePhase = null
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

    // Null while the query is not runnable (for example, no requirements yet).
    val currentRequest = runCatching {
        SearchRequest(
            requirements = requirements,
            maximumDepth = maximumDepth,
            challenges = challenges,
            requireBlacksmith = requireBlacksmith,
            excludeBlacksmithRewards = excludeBlacksmithRewards,
            fastMode = fastMode,
        )
    }.getOrNull()
    // Anything a Clear would actually erase: listed seeds, the refine base, or the
    // status/notice lines the results area still shows.
    val canClearResults = results.isNotEmpty() || lastFinishedRun != null ||
        searchStatus != null || searchError != null || importNotice != null

    val navBar: @Composable () -> Unit = {
        SeedSeekerNavBar(
            current = destination,
            onSelect = { destination = it },
        )
    }

    val resultSeeds = remember(results) { results.map { it.seed } }
    // Anchor for result navigation: the in-flight request's seed while
    // scouting, otherwise the seed of the rendered manifest. Editing the
    // seed field does not move the anchor until a scout actually runs.
    val scoutedSeed = if (isScouting) scoutRun?.seed else scoutResult?.seed

    CompositionLocalProvider(
        LocalItemAtlas provides atlas,
        LocalItemIconAtlas provides itemIcons,
        // One clock drives every enchantment/curse pulse in the app.
        LocalGlowPulse provides rememberGlowPulse(),
    ) {
        when (destination) {
            Destination.FINDER -> FinderScreen(
                requirements = requirements,
                maximumDepth = maximumDepth,
                requireBlacksmith = requireBlacksmith,
                excludeBlacksmithRewards = excludeBlacksmithRewards,
                fastMode = fastMode,
                challenges = challenges,
                presets = BuiltInPresets.all + userPresets,
                results = results,
                status = searchStatus,
                seedsPerSecond = searchSeedsPerSecond,
                elapsedSeconds = searchElapsedSeconds,
                isSearching = isSearching,
                refinePhase = refinePhase,
                error = searchError,
                snackbarHostState = snackbarHostState,
                onAbout = {
                    aboutReturnDestination = Destination.FINDER
                    destination = Destination.ABOUT
                },
                onChallenges = {
                    challengesReturnDestination = Destination.FINDER
                    destination = Destination.CHALLENGES
                },
                onApplyPreset = { preset ->
                    requirements = preset.query.requirements.map { it.copy(key = nextRequirementKey++) }
                    maximumDepth = preset.query.maximumDepth
                    requireBlacksmith = preset.query.requireBlacksmith
                    excludeBlacksmithRewards = preset.query.excludeBlacksmithRewards
                    fastMode = preset.query.fastMode
                    challenges = preset.query.challenges
                    preferences.edit().putInt(CHALLENGES_KEY, challenges).apply()
                },
                onSavePreset = { name ->
                    val cleanName = name.trim()
                    if (cleanName.isNotEmpty()) {
                        val query = PresetQuery(
                            requirements = requirements,
                            maximumDepth = maximumDepth,
                            requireBlacksmith = requireBlacksmith,
                            excludeBlacksmithRewards = excludeBlacksmithRewards,
                            fastMode = fastMode,
                            challenges = challenges,
                        )
                        val existing = userPresets.indexOfFirst { it.name.equals(cleanName, ignoreCase = true) }
                        userPresets = if (existing >= 0) {
                            userPresets.toMutableList().also { it[existing] = it[existing].copy(query = query) }
                        } else {
                            userPresets + QueryPreset(name = cleanName, query = query)
                        }
                        presetStorage.save(userPresets)
                    }
                },
                onDeletePreset = { preset ->
                    userPresets = userPresets.filterNot { it.id == preset.id }
                    presetStorage.save(userPresets)
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
                    if (currentRequest != null) {
                        importNotice = null
                        // Refining is implicit: a query that narrows — or leaves unchanged —
                        // the last finished run reuses its seeds and resumes where it stopped.
                        val refine = refinePlanFor(currentRequest, lastFinishedRun)
                        if (refine == null) {
                            searchedQuery = PresetQuery(
                                requirements = requirements,
                                maximumDepth = maximumDepth,
                                requireBlacksmith = requireBlacksmith,
                                excludeBlacksmithRewards = excludeBlacksmithRewards,
                                fastMode = fastMode,
                                challenges = challenges,
                            )
                        }
                        // A refine only claims the narrowed query once its filter phase has
                        // actually rewritten the results, so the snapshot is set there.
                        run = SearchRun(nextRunId++, currentRequest, refine)
                    }
                },
                onCancel = {
                    val session = activeSession
                    if (session != null) {
                        scope.launch(Dispatchers.Default) { session.cancel() }
                    } else if (isSearching) {
                        // The refine filter phase has no native session yet, so cancel the
                        // driver coroutine itself. The previous results and lastFinishedRun
                        // are untouched, so the refine can be retried.
                        run = null
                    }
                },
                canExportResults = searchedQuery != null && results.isNotEmpty(),
                canClearResults = canClearResults,
                importNotice = importNotice,
                onClearResults = {
                    // Drops the refine base too, so the next search is always a fresh scan.
                    run = null
                    results = emptyList()
                    lastFinishedRun = null
                    refinePhase = null
                    searchStatus = null
                    searchError = null
                    searchedQuery = null
                    importNotice = null
                },
                onExportResults = {
                    // Export the query snapshot that produced the results,
                    // never the live editor state.
                    val query = searchedQuery
                    if (query == null || results.isEmpty()) {
                        transferError = "Run a search first — there are no results to export yet."
                    } else {
                        runCatching {
                            ResultsExport.encode(query, results.map { it.seed }, BuildConfig.VERSION_NAME)
                        }.onSuccess { contents ->
                            pendingExport = contents
                            exportLauncher.launch(ResultsExport.SUGGESTED_FILE_NAME)
                        }.onFailure { failure ->
                            transferError = "Export failed: ${failure.message}"
                        }
                    }
                },
                onImportResults = {
                    importLauncher.launch(
                        arrayOf("application/json", "text/plain", "application/octet-stream"),
                    )
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
                resultSeeds = resultSeeds,
                scoutedSeed = scoutedSeed,
                onScoutSeed = ::scoutSeed,
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

        transferError?.let { message ->
            AlertDialog(
                onDismissRequest = { transferError = null },
                title = { Text("Results file") },
                text = { Text(message) },
                confirmButton = {
                    TextButton(onClick = { transferError = null }) { Text("OK") }
                },
            )
        }

        availableUpdate?.let { update ->
            val uriHandler = LocalUriHandler.current
            AlertDialog(
                onDismissRequest = { availableUpdate = null },
                title = { Text("Update available") },
                text = {
                    Text(
                        "Seed Seeker ${update.version} is available on GitHub. " +
                            "You have ${BuildConfig.VERSION_NAME}.",
                    )
                },
                confirmButton = {
                    TextButton(
                        onClick = {
                            availableUpdate = null
                            runCatching { uriHandler.openUri(update.url) }
                        },
                    ) { Text("Download") }
                },
                dismissButton = {
                    TextButton(
                        onClick = {
                            preferences.edit().putString(UPDATE_SKIPPED_KEY, update.version).apply()
                            availableUpdate = null
                        },
                    ) { Text("Skip") }
                    TextButton(onClick = { availableUpdate = null }) { Text("Not now") }
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
                selectedIconColor = MaterialTheme.colorScheme.onPrimaryContainer,
                selectedTextColor = MaterialTheme.colorScheme.onSurface,
                indicatorColor = MaterialTheme.colorScheme.primaryContainer,
            ),
        )
        NavigationBarItem(
            selected = current == Destination.SCOUT,
            onClick = { onSelect(Destination.SCOUT) },
            icon = {
                Icon(
                    if (current == Destination.SCOUT) Icons.Filled.Place else Icons.Outlined.Place,
                    contentDescription = null,
                )
            },
            label = { Text("Scout") },
            colors = NavigationBarItemDefaults.colors(
                selectedIconColor = MaterialTheme.colorScheme.onPrimaryContainer,
                selectedTextColor = MaterialTheme.colorScheme.onSurface,
                indicatorColor = MaterialTheme.colorScheme.primaryContainer,
            ),
        )
    }
}
