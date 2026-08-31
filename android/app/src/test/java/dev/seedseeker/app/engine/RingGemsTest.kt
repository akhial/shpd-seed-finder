// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.catalog.PackagedCatalog
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.RingGems
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Test

/**
 * Which colour a scouted ring is drawn in, asserted against the engine that
 * owns the shuffle. The table comes from the run's own `SSC3` scout packet
 * through the host build of the Rust library (see [QueryContinuationTest] for
 * how Gradle provides it), so a drift between the engine's shuffle and what the
 * app draws fails here instead of shipping twelve fixed ring colours.
 */
class RingGemsTest {
    init { PackagedCatalog.install() }

    private val haste = ItemCatalog.rings.first { it.id == "ring_haste" }

    @Test
    fun aScoutedWorldCarriesTheTableItsOwnRunRolled() {
        // Haste is ring class 7 and this run draws it as gem 11, a diamond;
        // the catalog cell alone would draw gem 7, a sapphire.
        assertEquals(SEED_GEMS, gemsFor(SEED))
        assertEquals(11, gemsFor(SEED).ordinals[7])
        assertNotEquals(RingGems.CATALOG, gemsFor(SEED))
    }

    @Test
    fun challengesDoNotMoveTheTable() {
        // The gems are drawn in `Dungeon.init()`, before any challenge is read.
        assertEquals(SEED_GEMS, gemsFor(SEED, challenges = 1))
        assertEquals(SEED_GEMS, gemsFor(SEED, challenges = 511))
    }

    @Test
    fun aRingTakesTheRunsGemWhileItsGlyphStaysTheClasses() {
        // 224 is `ItemSpriteSheet.RINGS`; the glyph is the class's own cell in
        // item_icons.png and no run changes it.
        assertEquals(224 + 11, SEED_GEMS.spriteIndexFor(haste))
        assertEquals(7, haste.typeIconIndex)
        assertEquals(224 + 7, haste.spriteIndex)
    }

    @Test
    fun aSurfaceWithNoRunDrawsTheCatalogCell() {
        for (ring in ItemCatalog.rings) {
            assertEquals(ring.spriteIndex, RingGems.CATALOG.spriteIndexFor(ring))
        }
    }

    @Test
    fun everyRingStillGetsADistinctCellFromTheGemBlock() {
        val cells = ItemCatalog.rings.map { SEED_GEMS.spriteIndexFor(it) }
        assertEquals((224..235).toSet(), cells.toSet())
    }

    @Test
    fun onlyRingsMove() {
        for (item in ItemCatalog.all.filterNot { it.kind == ItemKind.RING }) {
            assertEquals(item.spriteIndex, SEED_GEMS.spriteIndexFor(item))
        }
    }

    @Test
    fun aTableMustBeAPermutationOfTheTwelveGems() {
        assertThrows(IllegalArgumentException::class.java) { RingGems(listOf(0, 1, 2)) }
        assertThrows(IllegalArgumentException::class.java) {
            RingGems(List(12) { 0 })
        }
        assertThrows(IllegalArgumentException::class.java) {
            RingGems((0 until 11).toList() + 12)
        }
    }

    @Test
    fun theEngineRejectsARequestItCannotRead() {
        // No run, so no table: the call that would carry the gems refuses a
        // request that names no seed.
        assertThrows(IllegalArgumentException::class.java) {
            JniBindings.scoutSeed(byteArrayOf(1, 2, 3))
        }
    }

    /** The gems ride in the scouted world, so asking for one is asking for them. */
    private fun gemsFor(seed: String, challenges: Int = 0): RingGems =
        JniNativeSeedFinder().scoutSeed(seed, challenges).ringGems

    private companion object {
        const val SEED = "YKH-LGJ-WDQ"

        /** What the Java oracle gives this seed, in ring-class order. */
        val SEED_GEMS = RingGems(listOf(7, 8, 3, 5, 4, 6, 2, 11, 10, 1, 0, 9))
    }
}
