// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.BuildConfig
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.Challenge
import dev.seedseeker.app.model.ResumeHint
import dev.seedseeker.app.model.SearchBatch
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.SearchState
import dev.seedseeker.app.model.SearchStatus
import dev.seedseeker.app.model.ScoutAccessibility
import dev.seedseeker.app.model.ScoutItem
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.ScoutQuest
import dev.seedseeker.app.model.ScoutQuestGiver
import dev.seedseeker.app.model.ScoutQuestVariant
import dev.seedseeker.app.model.ScoutWorld
import dev.seedseeker.app.model.SeedResult
import dev.seedseeker.app.catalog.ItemCatalog
import org.json.JSONObject
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.nio.charset.StandardCharsets
import kotlin.math.min

/** A deliberately small boundary shared by the Compose UI, demo engine, and Rust JNI adapter. */
interface NativeSeedFinder {
    fun startSearch(request: SearchRequest): NativeSearchSession
    fun startResumedSearch(request: SearchRequest, resumeFrom: Long, scanLen: Long): NativeSearchSession
    fun filterSeeds(request: SearchRequest, seeds: List<String>): List<String>
    fun scoutSeed(seed: String, challenges: Int = 0): ScoutWorld

    /**
     * Which items of the world [scoutSeed] returns for the same seed and challenge mask explain
     * [request]'s requirements, as indices into that world's item list, or null when this engine's
     * scouted world is not the engine's own (the demo engine's is fabricated, so engine marks
     * would index a different list). Requirements claim distinct items and the marks are a largest
     * satisfiable selection, so a partially matching query marks only the items it could explain.
     * The selection is the engine's `scout_matches`; frontends never re-derive it.
     */
    fun scoutMatches(seed: String, challenges: Int, request: SearchRequest): Set<Int>?

    /**
     * Whether [candidate] never widens [base]: an identical floor limit, challenge set and fast
     * mode, world conditions (blacksmith flags, Wandmaker quest) at least as strict as the base's,
     * and every base requirement covered by a distinct candidate requirement
     * at least as strict — equal or strengthened (a named item, a tightened bound)
     * (UI list keys are not part of the wire query, so re-keying is invisible here). Only such a
     * query may reuse the base run's results and finish by rescanning the seeds it never reached,
     * which is what makes filter-and-resume sound; per docs/search-semantics.md the engine owns
     * this predicate and frontends call it rather than re-derive it.
     *
     * An unchanged query qualifies on purpose: filtering then keeps every seed and the resumed
     * scan simply continues the base run, which is what a second Search tap after a cancel must
     * do. Only an explicit Clear starts over.
     */
    fun queryContinues(candidate: SearchRequest, base: SearchRequest): Boolean
}

interface NativeSearchSession : AutoCloseable {
    fun poll(maxResults: Int = 32): SearchBatch
    fun status(): SearchStatus
    fun resumeHint(): ResumeHint
    fun cancel()
    override fun close()
}

object NativeSeedFinderFactory {
    fun create(): NativeSeedFinder = if (BuildConfig.USE_DEMO_ENGINE) {
        DemoNativeSeedFinder()
    } else {
        JniNativeSeedFinder()
    }
}

/**
 * Non-Rust search engine for previews and debug APKs. It follows the same session lifecycle as
 * JNI and emits deterministic sample seeds so every UI state can be exercised. Only searching is
 * stubbed: wire codecs such as the share-link format always go through [JniBindings], whose
 * library every APK packages.
 */
class DemoNativeSeedFinder : NativeSeedFinder {
    override fun startSearch(request: SearchRequest): NativeSearchSession = DemoSession(request)

    override fun startResumedSearch(
        request: SearchRequest,
        resumeFrom: Long,
        scanLen: Long,
    ): NativeSearchSession {
        require(resumeFrom >= 0 && scanLen > 0) { "Resume window must be non-empty" }
        // Emit the odd-indexed samples so a refine visibly appends seeds the demo filter dropped.
        return DemoSession(
            request,
            seeds = SAMPLE_SEEDS.filterIndexed { index, _ -> index % 2 == 1 },
            durationMs = RESUMED_DEMO_DURATION_MS,
            totalSeeds = scanLen,
        )
    }

    override fun filterSeeds(request: SearchRequest, seeds: List<String>): List<String> =
        seeds.filterIndexed { index, _ -> index % 2 == 0 }

    // A wrong continuation verdict would send a demo search down a refine
    // branch the shipped app would never take, so this is the one answer the
    // demo never stands in for: the engine owns the rule
    // (docs/search-semantics.md), and every APK packages its library.
    override fun queryContinues(candidate: SearchRequest, base: SearchRequest): Boolean =
        JniBindings.queryContinues(QueryCodec.encode(candidate), QueryCodec.encode(base))

    // The demo scout hands back a fabricated world rather than an engine SSC2
    // packet, so the engine's marks — computed over the world this seed really
    // generates — would point at other items entirely. Demo APKs therefore show
    // no marks at all; a Kotlin stand-in matcher would be a second
    // implementation of `scout_matches`, which is what this app no longer has.
    override fun scoutMatches(seed: String, challenges: Int, request: SearchRequest): Set<Int>? = null

    override fun scoutSeed(seed: String, challenges: Int): ScoutWorld {
        require(SeedCode.isCanonical(seed)) { "Seed must use XXX-XXX-XXX format" }
        require(challenges in 0..Challenge.ALL_MASK) { "Challenge mask must be 0..${Challenge.ALL_MASK}" }
        return ScoutWorld(
            seed = seed,
            items = listOf(
                ScoutItem(
                    item = ItemCatalog.weapons.first { it.id == "dagger" },
                    depth = 1,
                    upgrade = 1,
                    effect = "Lucky",
                    cursed = false,
                    secret = true,
                    source = ScoutItemSource.CHEST,
                    accessibility = ScoutAccessibility.Independent,
                ),
                ScoutItem(
                    item = ItemCatalog.armor.first { it.id == "leather_armor" },
                    depth = 3,
                    upgrade = 0,
                    effect = null,
                    cursed = true,
                    source = ScoutItemSource.TOMB,
                    accessibility = ScoutAccessibility.Independent,
                ),
                ScoutItem(
                    item = ItemCatalog.wands.first { it.id == "wand_frost" },
                    depth = 7,
                    upgrade = 2,
                    effect = null,
                    cursed = false,
                    source = ScoutItemSource.WANDMAKER_REWARD,
                    accessibility = ScoutAccessibility.Choice(group = 1, option = 0),
                ),
                ScoutItem(
                    item = ItemCatalog.armor.first { it.id == "plate_armor" },
                    depth = 20,
                    upgrade = 3,
                    effect = "Brimstone",
                    cursed = false,
                    source = ScoutItemSource.SHOP,
                    accessibility = ScoutAccessibility.Independent,
                ),
                ScoutItem(
                    item = ItemCatalog.rings.first { it.id == "ring_haste" },
                    depth = 19,
                    upgrade = 4,
                    effect = null,
                    cursed = true,
                    source = ScoutItemSource.IMP_REWARD,
                    accessibility = ScoutAccessibility.Independent,
                ),
            ),
            quests = listOf(
                ScoutQuest(variant = ScoutQuestVariant.GREAT_CRAB, depth = 4),
                ScoutQuest(variant = ScoutQuestVariant.ROTBERRY, depth = 8),
                ScoutQuest(variant = ScoutQuestVariant.CRYSTAL, depth = 13),
                ScoutQuest(variant = ScoutQuestVariant.MONK, depth = 18),
            ),
        )
    }

    private class DemoSession(
        private val request: SearchRequest,
        private val seeds: List<String> = SAMPLE_SEEDS,
        private val durationMs: Long = DEMO_DURATION_MS,
        private val totalSeeds: Long = TOTAL_SEEDS,
    ) : NativeSearchSession {
        private val startedAt = System.nanoTime()
        private var emitted = 0
        private var cancelled = false
        private var closed = false

        override fun poll(maxResults: Int): SearchBatch = synchronized(this) {
            check(!closed) { "Search session is closed" }
            if (cancelled || maxResults <= 0) return SearchBatch(emptyList())

            val available = min(seeds.size, (elapsedMillis() / 620L).toInt())
            val end = min(available, emitted + maxResults)
            val newResults = seeds.subList(emitted, end).map { seed ->
                SeedResult(seed, request.requirements.size)
            }
            emitted = end
            SearchBatch(newResults)
        }

        override fun status(): SearchStatus = synchronized(this) {
            check(!closed) { "Search session is closed" }
            val elapsed = elapsedMillis()
            val state = when {
                cancelled -> SearchState.CANCELLED
                elapsed >= durationMs -> SearchState.COMPLETED
                else -> SearchState.RUNNING
            }
            SearchStatus(
                state = state,
                scannedSeeds = min(totalSeeds, elapsed * DEMO_SEEDS_PER_MS),
                totalSeeds = totalSeeds,
                matchProbability = DEMO_MATCH_PROBABILITY,
            )
        }

        override fun resumeHint(): ResumeHint = synchronized(this) {
            check(!closed) { "Search session is closed" }
            // A cancelled demo run always claims a small leftover window so refine resumes.
            ResumeHint(
                position = DEMO_RESUME_POSITION,
                remaining = if (cancelled) DEMO_RESUME_REMAINING else 0L,
            )
        }

        override fun cancel() = synchronized(this) {
            if (!closed) cancelled = true
        }

        override fun close() = synchronized(this) {
            closed = true
        }

        private fun elapsedMillis() = (System.nanoTime() - startedAt) / 1_000_000L
    }

    private companion object {
        const val TOTAL_SEEDS = 5_429_503_678_976L // 26^9, rendered as XXX-XXX-XXX.
        const val DEMO_DURATION_MS = 4_250L
        const val RESUMED_DEMO_DURATION_MS = 1_500L
        const val DEMO_SEEDS_PER_MS = 1_277_530_277L
        const val DEMO_MATCH_PROBABILITY = 7.857_777_777_777_78e-9
        const val DEMO_RESUME_POSITION = 2_714_751_839_488L // Halfway through the seed space.
        const val DEMO_RESUME_REMAINING = 250_000_000L
        val SAMPLE_SEEDS = listOf(
            "QHP-YZK-NGV",
            "WDX-KMF-RTA",
            "LCJ-PVU-XBE",
            "ZSN-FQH-MKO",
            "ABR-TYW-JEP",
        )
    }
}

/**
 * Production adapter. The Rust library owns all worker threads and every handle it creates.
 * `close()` is mandatory and idempotence should also be enforced by Rust.
 *
 * JNI contract (all integers are signed JVM primitives; packets use unsigned big-endian fields):
 *
 * 1. `startSearch(requestBytes) -> handle` creates a search or throws.
 * 2. `poll(handle, maxResults) -> resultBytes` drains, never blocks for new results.
 * 3. `status(handle) -> long[5]` returns state, scanned, total, error, and probability bits.
 * 4. `cancel(handle)` is cooperative and safe to repeat.
 * 5. `close(handle)` joins/releases native resources and is safe after any terminal state.
 * 6. `scoutSeed(requestBytes) -> scoutBytes` generates one canonical seed through depth 24.
 * 7. `startResumedSearch(requestBytes, resumeFrom, scanLen) -> handle` scans `scanLen` seeds
 *    from `resumeFrom`, wrapping at 26^9, with the same lifecycle as `startSearch`.
 * 8. `resumeHint(handle) -> long[2]` returns where and how much a follow-up traversal must
 *    scan to finish this session's coverage; exact once the session has stopped.
 * 9. `filterSeeds(requestBytes, seedValues) -> resultBytes` re-verifies numeric seed values
 *    against the full query and returns the survivors, in input order, as a result packet.
 * 10. `queryContinues(candidateBytes, baseBytes) -> boolean` reports whether the candidate query
 *    may reuse a run of the base query, throwing for an undecodable packet.
 *
 * Search requests always use `SSF8`: magic, maxDepth:u8, flags:u8, challenges:u16 little-endian,
 * wandmakerQuest:u8 (0 any, else the 1-based variant), requirementCount:u16 big-endian,
 * followed by repeated
 * kind:u8, optionalItemId:utf8_u16, tierMode:u8, tierValue:u8, upgradeMode:u8,
 * upgradeValue:u8, modifier:utf8_u16,
 * optionalSource:u8, sameItemGroup:u8, requirementMaxDepth:u8 (0 uses the request limit),
 * requirementFlags:u8 (bit 0 requires an uncursed item).
 * Flag bit 0 requires an accessible blacksmith; bit 1 enables the lossy fast search mode
 * (quest-only +3 weapon/armor sources); flag bit 2
 * prevents Blacksmith "Smith" rewards from satisfying item requirements.
 * Result packet `SSR1`: magic[4], count:u16, then
 * repeated seedLength:u8, seed:ASCII. State codes are 0 running, 1 complete, 2 cancelled,
 * 3 failed. A non-zero handle is required. Scout requests use `SSQ2`, a little-endian challenge
 * mask, then the canonical UTF-8 seed. Scout packet `SSC2` contains the echoed canonical seed,
 * then a quest block — questCount:u8 (0..4) of strictly ascending {quest:u8, variant:u8,
 * depth:u8} records, where quest 1..4 is ghost/wandmaker/blacksmith/imp, variants are 1-based
 * per giver, and depth must sit in the giver's floor range (2..4, 7..9, 12..14, 17..19) — and
 * finally catalog ID, depth, upgrade, flags (bit 0 cursed, bit 1 hidden in a secret room),
 * effect, source, and accessibility for every item.
 */
class JniNativeSeedFinder(
    private val bindings: NativeBindings = JniBindingsAdapter,
) : NativeSeedFinder {
    override fun scoutSeed(seed: String, challenges: Int): ScoutWorld {
        require(SeedCode.isCanonical(seed)) { "Seed must use XXX-XXX-XXX format" }
        require(challenges in 0..Challenge.ALL_MASK) { "Challenge mask must be 0..${Challenge.ALL_MASK}" }
        val world = ScoutResultCodec.decode(
            bindings.scoutSeed(ScoutRequestCodec.encode(seed, challenges)),
        )
        check(world.seed == seed) { "Native scout returned ${world.seed} for requested seed $seed" }
        return world
    }

    /** Asks the engine, which scouts the same world again and marks it. */
    override fun scoutMatches(seed: String, challenges: Int, request: SearchRequest): Set<Int> =
        ScoutMatchCodec.decode(
            bindings.scoutMatches(
                ScoutRequestCodec.encode(seed, challenges),
                QueryCodec.encode(request),
            ),
        )

    override fun startSearch(request: SearchRequest): NativeSearchSession {
        val handle = bindings.startSearch(QueryCodec.encode(request))
        check(handle != 0L) { "Native seed finder returned an invalid handle" }
        return JniSession(handle, request.requirements.size, bindings)
    }

    override fun startResumedSearch(
        request: SearchRequest,
        resumeFrom: Long,
        scanLen: Long,
    ): NativeSearchSession {
        val handle = bindings.startResumedSearch(QueryCodec.encode(request), resumeFrom, scanLen)
        check(handle != 0L) { "Native seed finder returned an invalid handle" }
        return JniSession(handle, request.requirements.size, bindings)
    }

    override fun filterSeeds(request: SearchRequest, seeds: List<String>): List<String> {
        val values = LongArray(seeds.size) { SeedCode.value(seeds[it]) }
        val packet = bindings.filterSeeds(QueryCodec.encode(request), values)
        return ResultCodec.decode(packet, request.requirements.size).map { it.seed }
    }

    /** Asks the engine, so the refine soundness rule has exactly one implementation. */
    override fun queryContinues(candidate: SearchRequest, base: SearchRequest): Boolean =
        bindings.queryContinues(QueryCodec.encode(candidate), QueryCodec.encode(base))

    private class JniSession(
        private val handle: Long,
        private val requirementCount: Int,
        private val bindings: NativeBindings,
    ) : NativeSearchSession {
        private var closed = false

        override fun poll(maxResults: Int): SearchBatch = synchronized(this) {
            check(!closed) { "Search session is closed" }
            require(maxResults in 1..1024) { "maxResults must be 1..1024" }
            SearchBatch(ResultCodec.decode(bindings.poll(handle, maxResults), requirementCount))
        }

        override fun status(): SearchStatus = synchronized(this) {
            check(!closed) { "Search session is closed" }
            val values = bindings.status(handle)
            check(values.size == 5) { "Native status must contain five values" }
            SearchStatus(
                state = when (values[0]) {
                    0L -> SearchState.RUNNING
                    1L -> SearchState.COMPLETED
                    2L -> SearchState.CANCELLED
                    3L -> SearchState.FAILED
                    else -> error("Unknown native search state ${values[0]}")
                },
                scannedSeeds = values[1].coerceAtLeast(0),
                totalSeeds = values[2].coerceAtLeast(0),
                errorCode = values[3],
                matchProbability = Double.fromBits(values[4]).coerceIn(0.0, 1.0),
            )
        }

        override fun resumeHint(): ResumeHint = synchronized(this) {
            check(!closed) { "Search session is closed" }
            val values = bindings.resumeHint(handle)
            check(values.size == 2) { "Native resume hint must contain two values" }
            ResumeHint(
                position = values[0].coerceAtLeast(0),
                remaining = values[1].coerceAtLeast(0),
            )
        }

        override fun cancel() = synchronized(this) {
            if (!closed) bindings.cancel(handle)
        }

        override fun close() = synchronized(this) {
            if (!closed) {
                closed = true
                bindings.close(handle)
            }
        }
    }
}

interface NativeBindings {
    fun startSearch(request: ByteArray): Long
    fun startResumedSearch(request: ByteArray, resumeFrom: Long, scanLen: Long): Long
    fun poll(handle: Long, maxResults: Int): ByteArray
    fun status(handle: Long): LongArray
    fun resumeHint(handle: Long): LongArray
    fun cancel(handle: Long)
    fun close(handle: Long)
    fun scoutSeed(request: ByteArray): ByteArray
    fun scoutMatches(request: ByteArray, query: ByteArray): ByteArray
    fun filterSeeds(request: ByteArray, seeds: LongArray): ByteArray
    fun queryContinues(candidate: ByteArray, base: ByteArray): Boolean
}

/** Exact class and static method names are retained by ProGuard for Rust's exported JNI symbols. */
object JniBindings {
    init {
        System.loadLibrary("shpd_seedfinder")
    }

    @JvmStatic external fun startSearch(request: ByteArray): Long
    @JvmStatic external fun startResumedSearch(request: ByteArray, resumeFrom: Long, scanLen: Long): Long
    @JvmStatic external fun poll(handle: Long, maxResults: Int): ByteArray
    @JvmStatic external fun status(handle: Long): LongArray
    @JvmStatic external fun resumeHint(handle: Long): LongArray
    @JvmStatic external fun cancel(handle: Long)
    @JvmStatic external fun close(handle: Long)
    @JvmStatic external fun scoutSeed(request: ByteArray): ByteArray
    @JvmStatic external fun scoutMatches(request: ByteArray, query: ByteArray): ByteArray
    @JvmStatic external fun filterSeeds(request: ByteArray, seeds: LongArray): ByteArray
    @JvmStatic external fun queryContinues(candidate: ByteArray, base: ByteArray): Boolean

    // Share-link codec (docs/share-link-format.md): UTF-8 in, UTF-8 out.
    // Unlike the search entry points above, these also run in debug APKs,
    // which package the library solely for them.
    @JvmStatic external fun shareEncode(queryDocument: ByteArray): ByteArray
    @JvmStatic external fun shareDecode(text: ByteArray): ByteArray
    @JvmStatic external fun shareExtract(text: ByteArray): ByteArray?

    // Seed-code text handling: the as-you-type masker and the parser, which
    // answers `{"code", "value"}` (UTF-8 in, UTF-8 out).
    @JvmStatic external fun formatSeedCode(input: ByteArray): ByteArray
    @JvmStatic external fun parseSeedCode(input: ByteArray): ByteArray

    // Results-file codec (docs/results-export-format.md): UTF-8 in, UTF-8 out.
    // `resultsDecode` applies the shared dedupe-and-cap and the 2 MiB import
    // cap itself, and reports what it dropped.
    @JvmStatic external fun resultsEncode(request: ByteArray): ByteArray
    @JvmStatic external fun resultsDecode(contents: ByteArray): ByteArray

    /** The engine's own constants as UTF-8 JSON; see [EngineInfo]. */
    @JvmStatic external fun engineInfo(): ByteArray
}

private object JniBindingsAdapter : NativeBindings {
    override fun startSearch(request: ByteArray) = JniBindings.startSearch(request)
    override fun startResumedSearch(request: ByteArray, resumeFrom: Long, scanLen: Long) =
        JniBindings.startResumedSearch(request, resumeFrom, scanLen)
    override fun poll(handle: Long, maxResults: Int) = JniBindings.poll(handle, maxResults)
    override fun status(handle: Long) = JniBindings.status(handle)
    override fun resumeHint(handle: Long) = JniBindings.resumeHint(handle)
    override fun cancel(handle: Long) = JniBindings.cancel(handle)
    override fun close(handle: Long) = JniBindings.close(handle)
    override fun scoutSeed(request: ByteArray) = JniBindings.scoutSeed(request)
    override fun scoutMatches(request: ByteArray, query: ByteArray) =
        JniBindings.scoutMatches(request, query)
    override fun filterSeeds(request: ByteArray, seeds: LongArray) =
        JniBindings.filterSeeds(request, seeds)
    override fun queryContinues(candidate: ByteArray, base: ByteArray) =
        JniBindings.queryContinues(candidate, base)
}

/**
 * Seed-code text handling, all of it the engine's: the as-you-type masker is
 * `seed::format_input` and the parser is `DungeonSeed::from_code`, reached
 * through [JniBindings]. Nothing here re-derives the game's own rules — not the
 * canonical grouping, not the base-26 value, and not the ASCII-filter-then-
 * uppercase order a Kotlin `uppercase(Locale)` used to get wrong for alphabets
 * such as Turkish.
 */
object SeedCode {
    /** Makes typing and pasting forgiving while always producing canonical grouping. */
    fun formatInput(input: String): String =
        String(JniBindings.formatSeedCode(input.toByteArray()), StandardCharsets.UTF_8)

    fun isCanonical(seed: String): Boolean = parse(seed)?.code == seed

    /** Numeric value of a canonical seed, as the game reads it. */
    fun value(seed: String): Long {
        val parsed = parse(seed)
        require(parsed != null && parsed.code == seed) { "Seed must use XXX-XXX-XXX format" }
        return parsed.value
    }

    private data class Parsed(val code: String, val value: Long)

    /** The engine's reading of [text], or null when it is not a seed code at all. */
    private fun parse(text: String): Parsed? = runCatching {
        JSONObject(String(JniBindings.parseSeedCode(text.toByteArray()), StandardCharsets.UTF_8))
    }.getOrNull()?.let { Parsed(it.getString("code"), it.getLong("value")) }
}

object QueryCodec {
    fun encode(request: SearchRequest): ByteArray = ByteArrayOutputStream().use { bytes ->
        DataOutputStream(bytes).use { output ->
            output.write("SSF8".toByteArray(StandardCharsets.US_ASCII))
            output.writeByte(request.maximumDepth)
            output.writeByte(
                (if (request.requireBlacksmith) 1 else 0) or
                    (if (request.fastMode) 2 else 0) or
                    (if (request.excludeBlacksmithRewards) 4 else 0),
            )
            output.writeByte(request.challenges and 0xff)
            output.writeByte(request.challenges ushr 8)
            output.writeByte(request.wandmakerQuest?.wireId ?: 0)
            output.writeShort(request.requirements.size)
            request.requirements.forEach { requirement -> writeRequirement(output, requirement) }
        }
        bytes.toByteArray()
    }

    private fun writeRequirement(output: DataOutputStream, requirement: ItemRequirement) {
        output.writeByte(requirement.kind.ordinal)
        writeUtf8(output, requirement.item?.id.orEmpty())
        output.writeByte(requirement.tierMatch.ordinal)
        output.writeByte(requirement.tier)
        output.writeByte(requirement.upgradeMatch.ordinal)
        output.writeByte(requirement.upgrade)
        writeUtf8(output, requirement.modifier.orEmpty())
        output.writeByte(requirement.source?.let { it.ordinal + 1 } ?: 0)
        output.writeByte(requirement.identityGroup ?: 0)
        output.writeByte(requirement.maximumDepth ?: 0)
        output.writeByte(if (requirement.requireUncursed) 1 else 0)
    }

    private fun writeUtf8(output: DataOutputStream, text: String) {
        val encoded = text.toByteArray(StandardCharsets.UTF_8)
        require(encoded.size <= 65_535) { "Wire string is too long" }
        output.writeShort(encoded.size)
        output.write(encoded)
    }
}

object ScoutRequestCodec {
    fun encode(seed: String, challenges: Int): ByteArray {
        require(SeedCode.isCanonical(seed)) { "Seed must use XXX-XXX-XXX format" }
        require(challenges in 0..Challenge.ALL_MASK) { "Challenge mask must be 0..${Challenge.ALL_MASK}" }
        return byteArrayOf(
            'S'.code.toByte(),
            'S'.code.toByte(),
            'Q'.code.toByte(),
            '2'.code.toByte(),
            (challenges and 0xff).toByte(),
            (challenges ushr 8).toByte(),
        ) + seed.toByteArray(StandardCharsets.UTF_8)
    }
}

private object ResultCodec {
    private val MAGIC = byteArrayOf('S'.code.toByte(), 'S'.code.toByte(), 'R'.code.toByte(), '1'.code.toByte())

    fun decode(packet: ByteArray, requirementCount: Int): List<SeedResult> =
        DataInputStream(ByteArrayInputStream(packet)).use { input ->
            val magic = ByteArray(4).also(input::readFully)
            check(magic.contentEquals(MAGIC)) { "Unexpected native result packet" }
            val count = input.readUnsignedShort()
            List(count) {
                val length = input.readUnsignedByte()
                val bytes = ByteArray(length).also(input::readFully)
                val seed = bytes.toString(StandardCharsets.US_ASCII)
                check(SeedCode.isCanonical(seed)) { "Malformed seed from native engine" }
                SeedResult(seed, requirementCount)
            }.also {
                check(input.available() == 0) { "Trailing bytes in native result packet" }
            }
        }
}

/** Reads the scout-match envelope `scoutMatches` returns: indices into the `SSC2` item order. */
private object ScoutMatchCodec {
    fun decode(document: ByteArray): Set<Int> {
        val matched = JSONObject(String(document, StandardCharsets.UTF_8)).getJSONArray("matched")
        return buildSet { for (index in 0 until matched.length()) add(matched.getInt(index)) }
    }
}

object ScoutResultCodec {
    private val MAGIC = byteArrayOf('S'.code.toByte(), 'S'.code.toByte(), 'C'.code.toByte(), '2'.code.toByte())

    fun decode(packet: ByteArray): ScoutWorld =
        DataInputStream(ByteArrayInputStream(packet)).use { input ->
            val magic = ByteArray(4).also(input::readFully)
            check(magic.contentEquals(MAGIC)) { "Unexpected native scout packet" }

            val seed = readAscii(input, input.readUnsignedByte())
            check(SeedCode.isCanonical(seed)) { "Malformed seed from native scout" }
            val questCount = input.readUnsignedByte()
            check(questCount <= ScoutQuestGiver.entries.size) { "Scout quest count must be 0..4" }
            var previousQuestId = 0
            val quests = List(questCount) {
                val questId = input.readUnsignedByte()
                val giver = ScoutQuestGiver.entries.getOrNull(questId - 1)
                    ?: error("Unknown scout quest id $questId")
                check(questId > previousQuestId) { "Scout quest ids must be strictly ascending" }
                previousQuestId = questId
                val variantCode = input.readUnsignedByte()
                val variant = ScoutQuestVariant.variantsFor(giver).getOrNull(variantCode - 1)
                    ?: error("Unknown ${giver.label} quest variant $variantCode")
                val depth = input.readUnsignedByte()
                check(depth in giver.depths) {
                    "${giver.label} quest floor must be in ${giver.depths}"
                }
                ScoutQuest(variant = variant, depth = depth)
            }
            val items = List(input.readUnsignedShort()) {
                val stableId = readUtf8(input, input.readUnsignedShort())
                val catalogItem = checkNotNull(ItemCatalog.findById(stableId)) {
                    "Unknown catalog item '$stableId' in native scout packet"
                }
                val depth = input.readUnsignedByte()
                check(depth in 1..24) { "Scout item depth must be 1..24" }
                val upgrade = input.readUnsignedByte()
                check(upgrade in 0..catalogItem.kind.maximumSearchUpgrade) {
                    "Scout item upgrade must be 0..${catalogItem.kind.maximumSearchUpgrade}"
                }
                val flags = input.readUnsignedByte()
                check(flags and 0xFC == 0) { "Unknown scout item flags $flags" }
                val effect = readUtf8(input, input.readUnsignedShort()).ifEmpty { null }
                effect?.let {
                    check(it in ItemCatalog.modifiersFor(catalogItem.kind)) {
                        "Unknown modifier '$it' for ${catalogItem.id}"
                    }
                }
                val source = ScoutItemSource.entries.getOrNull(input.readUnsignedByte())
                    ?: error("Unknown scout item source")
                val accessibility = when (val tag = input.readUnsignedByte()) {
                    0 -> ScoutAccessibility.Independent
                    1 -> {
                        val group = input.readUnsignedShort()
                        val option = input.readUnsignedByte()
                        check(option < 64) { "Scout choice option must be 0..63" }
                        ScoutAccessibility.Choice(group = group, option = option)
                    }
                    2 -> {
                        val group = input.readUnsignedShort()
                        val mask = input.readLong().toULong()
                        check(mask != 0UL) { "Scout scenario mask must be non-zero" }
                        ScoutAccessibility.Scenarios(group = group, mask = mask)
                    }
                    else -> error("Unknown scout accessibility tag $tag")
                }
                ScoutItem(
                    item = catalogItem,
                    depth = depth,
                    upgrade = upgrade,
                    effect = effect,
                    cursed = flags and 1 != 0,
                    secret = flags and 2 != 0,
                    source = source,
                    accessibility = accessibility,
                )
            }
            check(input.available() == 0) { "Trailing bytes in native scout packet" }
            ScoutWorld(seed = seed, items = items, quests = quests)
        }

    private fun readUtf8(input: DataInputStream, length: Int): String {
        val bytes = ByteArray(length).also(input::readFully)
        val text = bytes.toString(StandardCharsets.UTF_8)
        check(text.toByteArray(StandardCharsets.UTF_8).contentEquals(bytes)) {
            "Malformed UTF-8 in native scout packet"
        }
        return text
    }

    private fun readAscii(input: DataInputStream, length: Int): String {
        val bytes = ByteArray(length).also(input::readFully)
        check(bytes.all { it.toInt() in 0..0x7F }) { "Malformed ASCII in native scout packet" }
        return bytes.toString(StandardCharsets.US_ASCII)
    }
}
