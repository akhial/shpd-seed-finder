// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.SearchState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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

    private class RecordingBindings : NativeBindings {
        var request = byteArrayOf()
        var statusPacket = longArrayOf(1, 123, 456, 0, 0.125.toBits())
        var cancelCalls = 0
        var closeCalls = 0

        override fun startSearch(request: ByteArray): Long {
            this.request = request.copyOf()
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
            '1'.code.toByte(),
            11,
        ) + "AAA-AAA-AAA".encodeToByteArray() + byteArrayOf(0, 0)
    }
}
