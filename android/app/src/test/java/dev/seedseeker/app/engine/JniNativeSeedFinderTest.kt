// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.SearchState
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class JniNativeSeedFinderTest {
    @Test
    fun sessionBridgesPacketsStatusCancellationAndIdempotentClose() {
        val bindings = RecordingBindings()
        val finder = JniNativeSeedFinder(bindings)
        val request = SearchRequest(
            listOf(
                ItemRequirement(
                    key = 1,
                    item = ItemCatalog.wands.first { it.id == "wand_frost" },
                    upgrade = 2,
                    modifier = null,
                ),
            ),
        )

        val session = finder.startSearch(request)
        assertTrue(bindings.request.contentEquals(QueryCodec.encode(request)))
        assertEquals("AAA-AAA-AAA", session.poll(24).results.single().seed)
        assertEquals(1, session.poll(24).results.single().matchedRequirements)

        val status = session.status()
        assertEquals(SearchState.COMPLETED, status.state)
        assertEquals(123, status.scannedSeeds)
        assertEquals(456, status.totalSeeds)
        assertEquals(0, status.errorCode)
        assertEquals(0.125, status.matchProbability, 0.0)

        session.cancel()
        session.close()
        session.close()
        session.cancel()
        assertEquals(1, bindings.cancelCalls)
        assertEquals(1, bindings.closeCalls)
    }

    @Test
    fun allNativeStateCodesAreMappedWithoutLosingTheErrorCode() {
        val bindings = RecordingBindings()
        val finder = JniNativeSeedFinder(bindings)
        val request = SearchRequest(
            listOf(ItemRequirement(1, ItemCatalog.armor.first(), 1, null)),
        )
        val expected = listOf(
            0L to SearchState.RUNNING,
            1L to SearchState.COMPLETED,
            2L to SearchState.CANCELLED,
            3L to SearchState.FAILED,
        )

        for ((native, kotlin) in expected) {
            bindings.statusPacket = longArrayOf(native, 7, 9, 41, 0.25.toBits())
            val session = finder.startSearch(request)
            val status = session.status()
            assertEquals(kotlin, status.state)
            assertEquals(41, status.errorCode)
            assertFalse(status.scannedSeeds < 0 || status.totalSeeds < 0)
            session.close()
        }
    }

    @Test
    fun resumedSearchPassesTheWindowThroughAndReturnsAWorkingSession() {
        val bindings = RecordingBindings()
        val finder = JniNativeSeedFinder(bindings)
        val request = SearchRequest(
            listOf(ItemRequirement(1, ItemCatalog.rings.first(), 2, null)),
        )

        val session = finder.startResumedSearch(request, resumeFrom = 5_000L, scanLen = 77L)
        assertTrue(bindings.resumedRequest.contentEquals(QueryCodec.encode(request)))
        assertEquals(5_000L, bindings.resumedFrom)
        assertEquals(77L, bindings.resumedScanLen)
        assertEquals("AAA-AAA-AAA", session.poll(24).results.single().seed)
        assertEquals(SearchState.COMPLETED, session.status().state)
        session.close()
    }

    @Test
    fun resumeHintMapsAndCoercesTheNativePair() {
        val bindings = RecordingBindings()
        val finder = JniNativeSeedFinder(bindings)
        val request = SearchRequest(
            listOf(ItemRequirement(1, ItemCatalog.wands.first(), 1, null)),
        )

        val session = finder.startSearch(request)
        bindings.resumeHintPacket = longArrayOf(1_000L, 2_000L)
        assertEquals(1_000L, session.resumeHint().position)
        assertEquals(2_000L, session.resumeHint().remaining)

        bindings.resumeHintPacket = longArrayOf(-5L, -9L)
        assertEquals(0L, session.resumeHint().position)
        assertEquals(0L, session.resumeHint().remaining)
        session.close()
    }

    @Test
    fun filterSeedsEncodesTheQueryWithNumericSeedsAndDecodesTheSurvivors() {
        val bindings = RecordingBindings()
        val finder = JniNativeSeedFinder(bindings)
        val request = SearchRequest(
            listOf(ItemRequirement(1, ItemCatalog.armor.first(), 3, null)),
        )

        val kept = finder.filterSeeds(request, listOf("AAA-AAA-AAB", "AAA-AAA-BAA", "ZZZ-ZZZ-ZZZ"))
        assertTrue(bindings.filterRequest.contentEquals(QueryCodec.encode(request)))
        assertArrayEquals(longArrayOf(1L, 676L, 5_429_503_678_975L), bindings.filterValues)
        assertEquals(listOf("AAA-AAA-AAB"), kept)
    }

    @Test
    fun scoutMatchesSendsTheScoutRequestWithTheQueryAndReadsBackTheMarks() {
        // The selection itself is the engine's (ScoutMatcherTest asserts it against the real
        // library); this only pins the two packets the adapter sends and the envelope it reads.
        val bindings = RecordingBindings()
        val finder = JniNativeSeedFinder(bindings)
        val request = SearchRequest(listOf(ItemRequirement(1, ItemCatalog.wands.first(), 1, null)))

        assertEquals(setOf(1, 3), finder.scoutMatches("AAA-AAA-AAB", 6, request))
        assertTrue(
            bindings.scoutMatchRequest.contentEquals(ScoutRequestCodec.encode("AAA-AAA-AAB", 6)),
        )
        assertTrue(bindings.scoutMatchQuery.contentEquals(QueryCodec.encode(request)))
    }

    @Test
    fun queryContinuesHandsBothQueriesToTheEngineAsRequestPackets() {
        // The verdict itself is the engine's (QueryContinuationTest asserts it against the real
        // library); this only pins the two packets the adapter sends and which side is which.
        val bindings = RecordingBindings()
        val finder = JniNativeSeedFinder(bindings)
        val base = SearchRequest(listOf(ItemRequirement(1, ItemCatalog.rings.first(), 2, null)))
        val candidate = base.copy(
            requirements = base.requirements + ItemRequirement(2, ItemCatalog.armor.first(), 1, null),
        )

        assertTrue(finder.queryContinues(candidate, base))
        assertTrue(bindings.continuesCandidate.contentEquals(QueryCodec.encode(candidate)))
        assertTrue(bindings.continuesBase.contentEquals(QueryCodec.encode(base)))
    }

    @Test
    fun decideStartPassesTheSessionStateThroughAndReturnsTheEnginesName() {
        // The decision itself is the engine's (RefinePlanTest asserts it against the real
        // library); this pins which packet is which and that absent queries travel as null.
        val bindings = RecordingBindings()
        val finder = JniNativeSeedFinder(bindings)
        val candidate = SearchRequest(listOf(ItemRequirement(1, ItemCatalog.wands.first(), 1, null)))
        val target = SearchRequest(listOf(ItemRequirement(2, ItemCatalog.rings.first(), 2, null)))

        assertEquals("target-filter", finder.decideStart(candidate, target, false, true, null))
        assertTrue(bindings.decideStartCandidate.contentEquals(QueryCodec.encode(candidate)))
        assertTrue(bindings.decideStartTarget!!.contentEquals(QueryCodec.encode(target)))
        assertNull(bindings.decideStartDetachedBase)
        assertArrayEquals(booleanArrayOf(false, true), bindings.decideStartFlags)

        finder.decideStart(candidate, null, true, false, target)
        assertNull(bindings.decideStartTarget)
        assertTrue(bindings.decideStartDetachedBase!!.contentEquals(QueryCodec.encode(target)))
    }

    private class RecordingBindings : NativeBindings {
        var request = byteArrayOf()
        var statusPacket = longArrayOf(1, 123, 456, 0, 0.125.toBits())
        var resumeHintPacket = longArrayOf(0, 0)
        var resumedRequest = byteArrayOf()
        var resumedFrom = -1L
        var resumedScanLen = -1L
        var filterRequest = byteArrayOf()
        var filterValues = longArrayOf()
        var scoutMatchRequest = byteArrayOf()
        var scoutMatchQuery = byteArrayOf()
        var decideStartCandidate = byteArrayOf()
        var decideStartTarget: ByteArray? = byteArrayOf()
        var decideStartDetachedBase: ByteArray? = byteArrayOf()
        var decideStartFlags = booleanArrayOf()
        var continuesCandidate = byteArrayOf()
        var continuesBase = byteArrayOf()
        var cancelCalls = 0
        var closeCalls = 0

        override fun startSearch(request: ByteArray): Long {
            this.request = request.copyOf()
            return 42
        }

        override fun startResumedSearch(request: ByteArray, resumeFrom: Long, scanLen: Long): Long {
            resumedRequest = request.copyOf()
            resumedFrom = resumeFrom
            resumedScanLen = scanLen
            return 42
        }

        override fun poll(handle: Long, maxResults: Int): ByteArray {
            assertEquals(42, handle)
            assertEquals(24, maxResults)
            return byteArrayOf(
                'S'.code.toByte(),
                'S'.code.toByte(),
                'R'.code.toByte(),
                '1'.code.toByte(),
                0,
                1,
                11,
            ) + "AAA-AAA-AAA".encodeToByteArray()
        }

        override fun status(handle: Long): LongArray {
            assertEquals(42, handle)
            return statusPacket.copyOf()
        }

        override fun resumeHint(handle: Long): LongArray {
            assertEquals(42, handle)
            return resumeHintPacket.copyOf()
        }

        override fun cancel(handle: Long) {
            assertEquals(42, handle)
            cancelCalls++
        }

        override fun close(handle: Long) {
            assertEquals(42, handle)
            closeCalls++
        }

        override fun scoutSeed(request: ByteArray): ByteArray = byteArrayOf(
            'S'.code.toByte(),
            'S'.code.toByte(),
            'C'.code.toByte(),
            '2'.code.toByte(),
            11,
        ) + "AAA-AAA-AAA".encodeToByteArray() + byteArrayOf(0, 0, 0)

        override fun scoutMatches(request: ByteArray, query: ByteArray): ByteArray {
            scoutMatchRequest = request.copyOf()
            scoutMatchQuery = query.copyOf()
            return """{"matched":[1,3],"matched_requirements":2,"total_requirements":2}"""
                .encodeToByteArray()
        }

        override fun queryContinues(candidate: ByteArray, base: ByteArray): Boolean {
            continuesCandidate = candidate.copyOf()
            continuesBase = base.copyOf()
            return true
        }

        override fun decideStart(
            candidate: ByteArray,
            target: ByteArray?,
            targetSetEmpty: Boolean,
            targetHasUncoveredSeeds: Boolean,
            detachedBase: ByteArray?,
        ): ByteArray {
            decideStartCandidate = candidate.copyOf()
            decideStartTarget = target?.copyOf()
            decideStartDetachedBase = detachedBase?.copyOf()
            decideStartFlags = booleanArrayOf(targetSetEmpty, targetHasUncoveredSeeds)
            return "target-filter".encodeToByteArray()
        }

        override fun filterSeeds(request: ByteArray, seeds: LongArray): ByteArray {
            filterRequest = request.copyOf()
            filterValues = seeds.copyOf()
            return byteArrayOf(
                'S'.code.toByte(),
                'S'.code.toByte(),
                'R'.code.toByte(),
                '1'.code.toByte(),
                0,
                1,
                11,
            ) + "AAA-AAA-AAB".encodeToByteArray()
        }
    }
}
