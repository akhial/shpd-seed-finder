// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.Challenge
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.SearchRequest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

/**
 * The refine soundness predicate, asserted against the engine that owns it. These tests call the
 * real `JniBindings.queryContinues` through the host build of the Rust library (Gradle's
 * `buildHostJni` task runs scripts/build-host-native.sh and puts it on `java.library.path`), so a
 * drift between `SearchQuery::continues` and what the app assumes fails here instead of shipping.
 *
 * Every case also checks the demo engine's Kotlin stand-in — the answer demo APKs, which carry no
 * `.so`, have to fall back on — so the two never disagree.
 */
class QueryContinuationTest {
    private val native = JniNativeSeedFinder()
    private val demo = DemoNativeSeedFinder()

    private val frost = ItemRequirement(1, ItemCatalog.wands.first { it.id == "wand_frost" }, 2)
    private val fireblast =
        ItemRequirement(2, ItemCatalog.wands.first { it.id == "wand_fireblast" }, 3)
    private val haste = ItemRequirement(3, ItemCatalog.rings.first { it.id == "ring_haste" }, 1)

    @Test
    fun addingARequirementContinuesTheBaseRun() {
        assertContinues(true, request(frost, fireblast), request(frost))
    }

    @Test
    fun anIdenticalQueryContinuesTheBaseRun() {
        // Tapping Search again without touching the query must resume, not start over:
        // filtering keeps every seed and the scan picks up where the base run stopped.
        assertContinues(true, request(frost, fireblast), request(frost, fireblast))
    }

    @Test
    fun removingOrEditingABaseRequirementDoesNotContinue() {
        assertContinues(false, request(frost), request(frost, fireblast))
        val editedFrost = frost.copy(upgrade = 3)
        assertContinues(false, request(editedFrost, fireblast), request(frost))
        // Equal size but a different multiset: swapping or editing in place still runs fresh.
        assertContinues(false, request(fireblast), request(frost))
        assertContinues(false, request(editedFrost), request(frost))
    }

    @Test
    fun anyScopeChangeDoesNotContinue() {
        val base = request(frost)
        assertContinues(false, request(frost, fireblast, maximumDepth = 12), base)
        assertContinues(false, request(frost, fireblast, challenges = Challenge.DARKNESS.bit), base)
        assertContinues(false, request(frost, fireblast, requireBlacksmith = true), base)
        assertContinues(false, request(frost, fireblast, excludeBlacksmithRewards = true), base)
        assertContinues(false, request(frost, fireblast, fastMode = true), base)
    }

    @Test
    fun duplicateRequirementsAreMatchedAsAMultiset() {
        val secondFrost = frost.copy(key = 9)
        assertContinues(true, request(frost, secondFrost, haste), request(frost, secondFrost))
        assertContinues(false, request(frost, fireblast, haste), request(frost, secondFrost))
    }

    @Test
    fun requirementKeysAreIgnoredWhenMatching() {
        val rekeyed = frost.copy(key = 77)
        assertContinues(true, request(rekeyed, haste), request(frost))
        // Same requirement, different UI list key: still the same query, so still eligible.
        assertContinues(true, request(rekeyed), request(frost))
    }

    @Test
    fun anUndecodableRequestPacketIsRejected() {
        val valid = QueryCodec.encode(request(frost))
        assertThrows(IllegalArgumentException::class.java) {
            JniBindings.queryContinues(byteArrayOf(1, 2, 3), valid)
        }
        assertThrows(IllegalArgumentException::class.java) {
            JniBindings.queryContinues(valid, byteArrayOf())
        }
    }

    private fun assertContinues(
        expected: Boolean,
        candidate: SearchRequest,
        base: SearchRequest,
    ) {
        assertEquals(expected, native.queryContinues(candidate, base))
        assertEquals(
            "The demo stand-in disagrees with the engine",
            expected,
            demo.queryContinues(candidate, base),
        )
    }

    private fun request(
        vararg requirements: ItemRequirement,
        maximumDepth: Int = 24,
        challenges: Int = 0,
        requireBlacksmith: Boolean = false,
        excludeBlacksmithRewards: Boolean = false,
        fastMode: Boolean = false,
    ) = SearchRequest(
        requirements = requirements.toList(),
        maximumDepth = maximumDepth,
        challenges = challenges,
        requireBlacksmith = requireBlacksmith,
        excludeBlacksmithRewards = excludeBlacksmithRewards,
        fastMode = fastMode,
    )
}
