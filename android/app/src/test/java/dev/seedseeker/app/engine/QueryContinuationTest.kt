// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.Challenge
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.UpgradeMatch
import dev.seedseeker.app.model.WandmakerQuest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

/**
 * The refine soundness predicate, asserted against the engine that owns it. These tests call the
 * real `JniBindings.queryContinues` through the host build of the Rust library (Gradle's
 * `buildHostJni` task runs scripts/build-host-native.sh and puts it on `java.library.path`), so a
 * drift between `SearchQuery::continues` and what the app assumes fails here instead of shipping.
 * Both engines answer through that one entry point — the demo engine delegates rather than
 * re-derive the rule — so these cases are the behaviour spec for every APK.
 */
class QueryContinuationTest {
    private val native = JniNativeSeedFinder()

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
    fun strengtheningABaseRequirementContinues() {
        // Naming the ring, or raising an at-least bound, only shrinks the match
        // set, so the base run's coverage still holds — the "specific ring after
        // any-ring runs" refine must resume scanning, not stall on the filter.
        val anyRing = ItemRequirement(
            4, null, 3,
            kind = ItemKind.RING, upgradeMatch = UpgradeMatch.AT_LEAST,
        )
        val namedRing = ItemRequirement(
            5, ItemCatalog.rings.first { it.id == "ring_haste" }, 3,
            upgradeMatch = UpgradeMatch.AT_LEAST,
        )
        assertContinues(true, request(namedRing), request(anyRing))
        assertContinues(true, request(anyRing.copy(upgrade = 4)), request(anyRing))
        // A strengthened requirement covers one base row, never two, and the
        // cover must survive the named ring standing beside a wildcard.
        assertContinues(true, request(namedRing, anyRing.copy(key = 6)), request(anyRing, namedRing))
        assertContinues(false, request(namedRing, frost), request(anyRing, anyRing.copy(key = 6)))
        // Loosening breaks containment: the widened query must rescan.
        assertContinues(false, request(anyRing), request(namedRing))
        assertContinues(false, request(anyRing.copy(upgrade = 2)), request(anyRing))
    }

    @Test
    fun aWidenedScopeDoesNotContinue() {
        val base = request(frost)
        assertContinues(false, request(frost, fireblast, maximumDepth = 12), base)
        assertContinues(false, request(frost, fireblast, challenges = Challenge.DARKNESS.bit), base)
        assertContinues(false, request(frost, fireblast, fastMode = true), base)

        // The blacksmith flags and the quest filter only narrow the match
        // set, so switching one on strengthens the base instead of ending the
        // continuation. Switching it back off — or swapping the quest for
        // another variant — forces a rescan.
        val smith = request(frost, requireBlacksmith = true)
        assertContinues(true, request(frost, fireblast, requireBlacksmith = true), base)
        assertContinues(false, request(frost, fireblast), smith)
        val excluded = request(frost, excludeBlacksmithRewards = true)
        assertContinues(true, request(frost, fireblast, excludeBlacksmithRewards = true), base)
        assertContinues(false, request(frost, fireblast), excluded)
        val quested = request(frost, wandmakerQuest = WandmakerQuest.ROTBERRY)
        assertContinues(true, request(frost, fireblast, wandmakerQuest = WandmakerQuest.ROTBERRY), base)
        assertContinues(true, request(frost, fireblast, wandmakerQuest = WandmakerQuest.ROTBERRY), quested)
        assertContinues(false, request(frost, fireblast), quested)
        assertContinues(false, request(frost, fireblast, wandmakerQuest = WandmakerQuest.CORPSE_DUST), quested)
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
    }

    private fun request(
        vararg requirements: ItemRequirement,
        maximumDepth: Int = 24,
        challenges: Int = 0,
        requireBlacksmith: Boolean = false,
        excludeBlacksmithRewards: Boolean = false,
        wandmakerQuest: WandmakerQuest? = null,
        fastMode: Boolean = false,
    ) = SearchRequest(
        requirements = requirements.toList(),
        maximumDepth = maximumDepth,
        challenges = challenges,
        requireBlacksmith = requireBlacksmith,
        excludeBlacksmithRewards = excludeBlacksmithRewards,
        wandmakerQuest = wandmakerQuest,
        fastMode = fastMode,
    )
}
