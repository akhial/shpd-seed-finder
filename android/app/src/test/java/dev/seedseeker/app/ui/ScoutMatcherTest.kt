// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.engine.DemoNativeSeedFinder
import dev.seedseeker.app.engine.JniNativeSeedFinder
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.UpgradeMatch
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Which scouted items explain which requirements, asserted against the engine that owns the
 * selection. These cases call the real `JniBindings.scoutMatches` through the host build of the
 * Rust library (see QueryContinuationTest for how Gradle provides it), over the world the engine
 * really generates for [SEED] — the marks index that world's own item list, so every case scouts
 * it first and checks what the marked items are.
 */
class ScoutMatcherTest {
    private val engine = JniNativeSeedFinder()
    private val world = engine.scoutSeed(SEED, 0)

    @Test
    fun selectsOnlyOneMutuallyExclusiveReward() {
        // The Wandmaker hands out one of two wands; a requirement either wand
        // satisfies still marks a single item, because only one is obtainable.
        val wandmakerWands = world.items.withIndex()
            .filter { it.value.source == ScoutItemSource.WANDMAKER_REWARD }
            .map { it.index }
        assertEquals(2, wandmakerWands.size)

        val marks = matchesFor(
            ItemRequirement(
                key = 1,
                item = null,
                upgrade = 0,
                kind = ItemKind.WAND,
                upgradeMatch = UpgradeMatch.ANY,
                source = ScoutItemSource.WANDMAKER_REWARD,
            ),
        )
        assertEquals(1, marks.size)
        assertTrue("$marks", marks.single() in wandmakerWands)
    }

    @Test
    fun exclusiveBranchesCannotSatisfyTwoRequirementsAtOnce() {
        // Both wands exist in the manifest, but naming both cannot be
        // satisfied: they are two options of the same reward.
        val exclusive = matchesFor(
            wand("wand_corrosion", key = 1),
            wand("wand_warding", key = 2),
        )
        assertEquals(1, exclusive.size)

        // A requirement outside that group is claimed alongside it.
        val compatible = matchesFor(
            wand("wand_corrosion", key = 1),
            wand("wand_fireblast", key = 2),
        )
        assertEquals(2, compatible.size)
        assertEquals(
            setOf("wand_corrosion", "wand_fireblast"),
            compatible.mapTo(mutableSetOf()) { world.items[it].item.id },
        )
    }

    @Test
    fun uncursedRequirementRejectsCursedCopies() {
        // This world holds two +2 Potential mail armors: a cursed one on floor
        // 14 and a clean one on floor 19.
        val potentialArmors = world.items.withIndex()
            .filter { it.value.item.id == "mail_armor" && it.value.effect == "Potential" }
        assertEquals(listOf(true, false), potentialArmors.map { it.value.cursed })

        val marks = matchesFor(uncursedPotentialArmor(maximumDepth = null))
        assertEquals(1, marks.size)
        assertEquals(potentialArmors.last().index, marks.single())

        // Limited to the floors that hold only the cursed copy, nothing matches.
        assertEquals(emptySet<Int>(), matchesFor(uncursedPotentialArmor(maximumDepth = 14)))
    }

    @Test
    fun anUnsatisfiableRequirementLeavesTheOthersMarked() {
        // The marks are a largest satisfiable selection, so a query that only
        // partly matches still explains the part it can.
        val marks = matchesFor(
            ItemRequirement(
                key = 1,
                item = ItemCatalog.findById("ring_tenacity"),
                upgrade = 0,
                upgradeMatch = UpgradeMatch.ANY,
            ),
            ItemRequirement(
                key = 2,
                item = ItemCatalog.findById("ring_tenacity"),
                upgrade = 4,
                upgradeMatch = UpgradeMatch.EXACT,
            ),
        )
        assertEquals(1, marks.size)
        assertEquals("ring_tenacity", world.items[marks.single()].item.id)
    }

    @Test
    fun theDemoEngineMarksNothing() {
        // Its scouted world is fabricated rather than an engine packet, so
        // there is no world for the engine's marks to index — and no Kotlin
        // matcher stands in for them.
        assertNull(
            DemoNativeSeedFinder().scoutMatches(
                SEED,
                0,
                SearchRequest(listOf(wand("wand_corrosion", key = 1))),
            ),
        )
    }

    private fun matchesFor(vararg requirements: ItemRequirement): Set<Int> =
        checkNotNull(engine.scoutMatches(SEED, 0, SearchRequest(requirements.toList())))

    private fun wand(id: String, key: Long) = ItemRequirement(
        key = key,
        item = ItemCatalog.findById(id),
        upgrade = 0,
        upgradeMatch = UpgradeMatch.ANY,
    )

    private fun uncursedPotentialArmor(maximumDepth: Int?) = ItemRequirement(
        key = 1,
        item = ItemCatalog.findById("mail_armor"),
        upgrade = 2,
        modifier = "Potential",
        upgradeMatch = UpgradeMatch.EXACT,
        maximumDepth = maximumDepth,
        requireUncursed = true,
    )

    private companion object {
        /** A pinned seed; its depth-24 manifest is deterministic. */
        const val SEED = "AAA-AAA-BUH"
    }
}
