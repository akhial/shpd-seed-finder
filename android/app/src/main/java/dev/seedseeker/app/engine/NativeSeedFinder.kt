// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.BuildConfig
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.Challenge
import dev.seedseeker.app.model.SearchBatch
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.SearchState
import dev.seedseeker.app.model.SearchStatus
import dev.seedseeker.app.model.ScoutAccessibility
import dev.seedseeker.app.model.ScoutItem
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.ScoutWorld
import dev.seedseeker.app.model.SeedResult
import dev.seedseeker.app.model.TierMatch
import dev.seedseeker.app.catalog.ItemCatalog
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.nio.charset.StandardCharsets
import java.util.Locale
import kotlin.math.min

/** A deliberately small boundary shared by the Compose UI, demo engine, and Rust JNI adapter. */
interface NativeSeedFinder {
    fun startSearch(request: SearchRequest): NativeSearchSession
    fun scoutSeed(seed: String, challenges: Int = 0): ScoutWorld
}

interface NativeSearchSession : AutoCloseable {
    fun poll(maxResults: Int = 32): SearchBatch
    fun status(): SearchStatus
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
 * Non-Rust implementation for previews and debug APKs. It follows the same session lifecycle as
 * JNI and emits deterministic sample seeds so every UI state can be exercised without an `.so`.
 */
class DemoNativeSeedFinder : NativeSeedFinder {
    override fun startSearch(request: SearchRequest): NativeSearchSession = DemoSession(request)

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
        )
    }

    private class DemoSession(private val request: SearchRequest) : NativeSearchSession {
        private val startedAt = System.nanoTime()
        private var emitted = 0
        private var cancelled = false
        private var closed = false

        override fun poll(maxResults: Int): SearchBatch = synchronized(this) {
            check(!closed) { "Search session is closed" }
            if (cancelled || maxResults <= 0) return SearchBatch(emptyList())

            val available = min(SAMPLE_SEEDS.size, (elapsedMillis() / 620L).toInt())
            val end = min(available, emitted + maxResults)
            val newResults = SAMPLE_SEEDS.subList(emitted, end).map { seed ->
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
                elapsed >= DEMO_DURATION_MS -> SearchState.COMPLETED
                else -> SearchState.RUNNING
            }
            SearchStatus(
                state = state,
                scannedSeeds = min(TOTAL_SEEDS, elapsed * DEMO_SEEDS_PER_MS),
                totalSeeds = TOTAL_SEEDS,
                matchProbability = DEMO_MATCH_PROBABILITY,
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
        const val DEMO_SEEDS_PER_MS = 1_277_530_277L
        const val DEMO_MATCH_PROBABILITY = 7.857_777_777_777_78e-9
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
 *
 * Search requests always use `SSF5`: magic, maxDepth:u8, flags:u8, challenges:u16 little-endian,
 * requirementCount:u16 big-endian, followed by repeated
 * kind:u8, optionalItemId:utf8_u16, tierMode:u8, tierValue:u8, upgradeMode:u8,
 * upgradeValue:u8, modifier:utf8_u16,
 * optionalSource:u8, sameItemGroup:u8, requirementMaxDepth:u8 (0 uses the request limit).
 * Flag bit 0 requires an accessible blacksmith; bit 1 enables the lossy fast search mode
 * (quest-only +3 weapon/armor sources); flag bit 2
 * prevents Blacksmith "Smith" rewards from satisfying item requirements.
 * Result packet `SSR1`: magic[4], count:u16, then
 * repeated seedLength:u8, seed:ASCII. State codes are 0 running, 1 complete, 2 cancelled,
 * 3 failed. A non-zero handle is required. Scout requests use `SSQ2`, a little-endian challenge
 * mask, then the canonical UTF-8 seed. Scout packet `SSC1` contains the echoed canonical seed
 * followed by catalog ID, depth, upgrade, curse, effect, source, and accessibility for every item.
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

    override fun startSearch(request: SearchRequest): NativeSearchSession {
        val handle = bindings.startSearch(QueryCodec.encode(request))
        check(handle != 0L) { "Native seed finder returned an invalid handle" }
        return JniSession(handle, request.requirements.size, bindings)
    }

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
    fun poll(handle: Long, maxResults: Int): ByteArray
    fun status(handle: Long): LongArray
    fun cancel(handle: Long)
    fun close(handle: Long)
    fun scoutSeed(request: ByteArray): ByteArray
}

/** Exact class and static method names are retained by ProGuard for Rust's exported JNI symbols. */
object JniBindings {
    init {
        System.loadLibrary("shpd_seedfinder")
    }

    @JvmStatic external fun startSearch(request: ByteArray): Long
    @JvmStatic external fun poll(handle: Long, maxResults: Int): ByteArray
    @JvmStatic external fun status(handle: Long): LongArray
    @JvmStatic external fun cancel(handle: Long)
    @JvmStatic external fun close(handle: Long)
    @JvmStatic external fun scoutSeed(request: ByteArray): ByteArray
}

private object JniBindingsAdapter : NativeBindings {
    override fun startSearch(request: ByteArray) = JniBindings.startSearch(request)
    override fun poll(handle: Long, maxResults: Int) = JniBindings.poll(handle, maxResults)
    override fun status(handle: Long) = JniBindings.status(handle)
    override fun cancel(handle: Long) = JniBindings.cancel(handle)
    override fun close(handle: Long) = JniBindings.close(handle)
    override fun scoutSeed(request: ByteArray) = JniBindings.scoutSeed(request)
}

object SeedCode {
    private val PATTERN = Regex("[A-Z]{3}-[A-Z]{3}-[A-Z]{3}")

    /** Makes typing and pasting forgiving while always producing canonical grouping. */
    fun formatInput(input: String): String {
        val letters = input
            .uppercase(Locale.US)
            .filter { it in 'A'..'Z' }
            .take(9)
        return letters.chunked(3).joinToString("-")
    }

    fun isCanonical(seed: String): Boolean = PATTERN.matches(seed)
}

object QueryCodec {
    fun encode(request: SearchRequest): ByteArray = ByteArrayOutputStream().use { bytes ->
        DataOutputStream(bytes).use { output ->
            output.write("SSF5".toByteArray(StandardCharsets.US_ASCII))
            output.writeByte(request.maximumDepth)
            output.writeByte(
                (if (request.requireBlacksmith) 1 else 0) or
                    (if (request.fastMode) 2 else 0) or
                    (if (request.excludeBlacksmithRewards) 4 else 0),
            )
            output.writeByte(request.challenges and 0xff)
            output.writeByte(request.challenges ushr 8)
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
    private val SEED_PATTERN = Regex("[A-Z]{3}-[A-Z]{3}-[A-Z]{3}")

    fun decode(packet: ByteArray, requirementCount: Int): List<SeedResult> =
        DataInputStream(ByteArrayInputStream(packet)).use { input ->
            val magic = ByteArray(4).also(input::readFully)
            check(magic.contentEquals(MAGIC)) { "Unexpected native result packet" }
            val count = input.readUnsignedShort()
            List(count) {
                val length = input.readUnsignedByte()
                val bytes = ByteArray(length).also(input::readFully)
                val seed = bytes.toString(StandardCharsets.US_ASCII)
                check(SEED_PATTERN.matches(seed)) { "Malformed seed from native engine" }
                SeedResult(seed, requirementCount)
            }.also {
                check(input.available() == 0) { "Trailing bytes in native result packet" }
            }
        }
}

object ScoutResultCodec {
    private val MAGIC = byteArrayOf('S'.code.toByte(), 'S'.code.toByte(), 'C'.code.toByte(), '1'.code.toByte())
    private val SEED_PATTERN = Regex("[A-Z]{3}-[A-Z]{3}-[A-Z]{3}")

    fun decode(packet: ByteArray): ScoutWorld =
        DataInputStream(ByteArrayInputStream(packet)).use { input ->
            val magic = ByteArray(4).also(input::readFully)
            check(magic.contentEquals(MAGIC)) { "Unexpected native scout packet" }

            val seed = readAscii(input, input.readUnsignedByte())
            check(SEED_PATTERN.matches(seed)) { "Malformed seed from native scout" }
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
                check(flags and 0xFE == 0) { "Unknown scout item flags $flags" }
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
                    source = source,
                    accessibility = accessibility,
                )
            }
            check(input.available() == 0) { "Trailing bytes in native scout packet" }
            ScoutWorld(seed, items)
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
