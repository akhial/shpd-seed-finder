// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.catalog.PackagedCatalog
import dev.seedseeker.app.engine.DemoNativeSeedFinder
import dev.seedseeker.app.engine.JniNativeSeedFinder
import dev.seedseeker.app.engine.ScoutMatches
import dev.seedseeker.app.model.EffectFilter
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.SearchLimits
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.UpgradeMatch
import dev.seedseeker.app.model.LevelSum
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
    init { PackagedCatalog.install() }

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
        // satisfied: they are the Wandmaker's two reward options.
        val exclusive = matchesFor(
            wand("wand_warding", key = 1),
            wand("wand_blast_wave", key = 2),
        )
        assertEquals(1, exclusive.size)

        // A requirement outside that group is claimed alongside it.
        val compatible = matchesFor(
            wand("wand_warding", key = 1),
            wand("wand_corrosion", key = 2),
        )
        assertEquals(2, compatible.size)
        assertEquals(
            setOf("wand_warding", "wand_corrosion"),
            compatible.mapTo(mutableSetOf()) { world.items[it].item.id },
        )
    }

    @Test
    fun uncursedRequirementRejectsCursedCopies() {
        // This world holds two Fireblast wands: a cursed one in a floor-2 chest
        // and a clean one among the Imp vault's floor-17 treasure.
        val fireblasts = world.items.withIndex()
            .filter { it.value.item.id == "wand_fireblast" }
        assertEquals(listOf(true, false), fireblasts.map { it.value.cursed })

        val marks = matchesFor(uncursedFireblast(maximumDepth = null))
        assertEquals(1, marks.size)
        assertEquals(fireblasts.last().index, marks.single())

        // Limited to the floors that hold only the cursed copy, nothing matches.
        assertEquals(emptySet<Int>(), matchesFor(uncursedFireblast(maximumDepth = 2)))
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
    fun anAlternativeGroupIsOneSlotAnyMemberSatisfies() {
        // Both Wandmaker options in one "any of these" slot: one item explains
        // the single slot, and the counts say so.
        val marks = marksFor(
            wand("wand_corrosion", key = 1).copy(alternativeGroup = 1),
            wand("wand_warding", key = 2).copy(alternativeGroup = 1),
        )
        assertEquals(1, marks.items.size)
        assertEquals(1, marks.matchedSlots)
        assertEquals(1, marks.totalSlots)

        // A member the world lacks does not stop the other from serving the slot.
        val partial = marksFor(
            ItemRequirement(
                key = 1,
                item = ItemCatalog.findById("ring_tenacity"),
                upgrade = 4,
                upgradeMatch = UpgradeMatch.EXACT,
                alternativeGroup = 1,
            ),
            wand("wand_fireblast", key = 2).copy(alternativeGroup = 1),
            wand("wand_corrosion", key = 3),
        )
        assertEquals(2, partial.items.size)
        assertEquals(2, partial.matchedSlots)
        assertEquals(2, partial.totalSlots)
    }

    @Test
    fun effectSetsAndAnyEnchantmentMatchTheScoutedEffects() {
        // The Thorns mail armor satisfies a set naming Thorns among others.
        val thornsArmors = world.items.withIndex()
            .filter { it.value.item.id == "mail_armor" && it.value.effect == "Thorns" }
            .map { it.index }
        val set = matchesFor(
            ItemRequirement(
                key = 1,
                item = ItemCatalog.findById("mail_armor"),
                upgrade = 0,
                upgradeMatch = UpgradeMatch.ANY,
                effect = EffectFilter.OneOf(listOf("Potential", "Thorns", "Brimstone")),
            ),
        )
        assertEquals(1, set.size)
        assertTrue("$set", set.single() in thornsArmors)

        // "Any enchantment" matches every glyphed armor but no plain one.
        val glyphed = world.items.withIndex()
            .filter { it.value.item.kind == ItemKind.ARMOR && it.value.effect != null && !it.value.effect.let { effect -> effect in ItemCatalog.armorCurses } }
            .map { it.index }
        val any = matchesFor(
            ItemRequirement(
                key = 1,
                item = null,
                kind = ItemKind.ARMOR,
                upgrade = 0,
                upgradeMatch = UpgradeMatch.ANY,
                effect = EffectFilter.AnyEnchantment,
            ),
        )
        assertEquals(1, any.size)
        assertTrue("$any", any.single() in glyphed)
    }

    @Test
    fun aCombinedLevelGroupMarksAllOrNothing() {
        // Two rings whose levels — each item's upgrade plus one — add up to the
        // total. The group is one scout condition, and its members are optional,
        // so one upgraded ring can cover a small total alone. Raising the total
        // past what this world can reach marks nothing at all rather than the
        // rings that fell short.
        fun anyRing(key: Long, atLeast: Int) = ItemRequirement(
            key = key,
            item = null,
            kind = ItemKind.RING,
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            levelSum = LevelSum(group = 1, atLeast = atLeast),
        )
        val capacity = SearchLimits.ringStackCapacity(2)
        var reachable = 0
        for (total in 1..capacity) {
            val marks = marksFor(anyRing(1, total), anyRing(2, total))
            assertEquals(1, marks.totalSlots)
            if (marks.items.isEmpty()) {
                assertEquals(0, marks.matchedSlots)
                break
            }
            assertEquals(1, marks.matchedSlots)
            assertTrue("$marks", marks.items.size in 1..2)
            assertTrue(marks.items.all { world.items[it].item.kind == ItemKind.RING })
            assertTrue("$marks", marks.items.sumOf { world.items[it].upgrade + 1 } >= total)
            reachable = total
        }
        assertTrue("no ring total is reachable in $SEED", reachable >= 1)
        assertTrue("every total up to $capacity was reachable", reachable < capacity)
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
        marksFor(*requirements).items

    private fun marksFor(vararg requirements: ItemRequirement): ScoutMatches =
        checkNotNull(engine.scoutMatches(SEED, 0, SearchRequest(requirements.toList())))

    private fun wand(id: String, key: Long) = ItemRequirement(
        key = key,
        item = ItemCatalog.findById(id),
        upgrade = 0,
        upgradeMatch = UpgradeMatch.ANY,
    )

    private fun uncursedFireblast(maximumDepth: Int?) = ItemRequirement(
        key = 1,
        item = ItemCatalog.findById("wand_fireblast"),
        upgrade = 0,
        upgradeMatch = UpgradeMatch.ANY,
        maximumDepth = maximumDepth,
        requireUncursed = true,
    )

    private companion object {
        /** A pinned seed; its depth-24 manifest is deterministic. */
        const val SEED = "AAA-AAA-BUH"
    }
}
