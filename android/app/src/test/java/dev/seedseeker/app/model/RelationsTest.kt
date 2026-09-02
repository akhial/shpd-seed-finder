// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import org.json.JSONObject
import dev.seedseeker.app.catalog.PackagedCatalog
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The board's edits, mirroring the web design's `relations.test.ts` case for
 * case: both platforms must land on the same canonical document for the same
 * gesture, or a query would change meaning when it crosses a share link.
 */
class RelationsTest {
    init { PackagedCatalog.install() }

    private val might = ItemCatalog.findById("ring_might")
    private val fireblast = ItemCatalog.findById("wand_fireblast")
    private val sword = ItemCatalog.findById("sword")

    private fun ring(key: Long, upgrade: Int = 2) =
        ItemRequirement(key = key, item = might, upgrade = upgrade)

    private fun anyWand(key: Long, upgrade: Int = 3) =
        ItemRequirement(key = key, item = null, kind = ItemKind.WAND, upgrade = upgrade)

    private fun bareWand(key: Long) = ItemRequirement(
        key = key,
        item = null,
        kind = ItemKind.WAND,
        upgrade = 0,
        upgradeMatch = UpgradeMatch.ANY,
    )

    // --- either/or clusters ---

    @Test
    fun droppingAChipOnAnotherMakesOneSlotPlacedAfterTheTarget() {
        val start = listOf(ring(1), ItemRequirement(2, sword, 1), anyWand(3))
        val joined = start.joinAlternatives(source = 0, target = 2)
        // The dragged chip moves next to its target and both share a group.
        assertEquals(listOf(2L, 3L, 1L), joined.map { it.key })
        assertEquals(listOf(null, 1, 1), joined.map { it.alternativeGroup })
        assertEquals(2, joined.slotCount())
        assertEquals(2, joined.boardItems().size)
    }

    @Test
    fun joiningAClusterDropsACombinedLevelAndLeavingAPairDissolvesIt() {
        val start = listOf(ring(1))
        val pair = start.setStackCount(start.boardItems().single(), count = 2)
        val stacked = pair.setStackTotal(pair.boardItems().single(), total = 4)
        assertEquals(2, stacked.count { it.levelSum != null })
        // An alternative may not count a level, so the join drops the total.
        val joined = stacked.joinAlternatives(source = 0, target = 1)
        assertNull(joined.firstOrNull { it.levelSum != null })
        // Pulling one out of a pair leaves nobody to be an alternative of.
        val detached = joined.detach(0)
        assertEquals(listOf(null, null), detached.map { it.alternativeGroup })
    }

    // --- stacks ---

    @Test
    fun aConcreteStackEncodesAsPlainRepeatsWithNoIdentityGroup() {
        val start = listOf(ring(1))
        val grown = start.setStackCount(start.boardItems().single(), count = 3)
        assertEquals(3, grown.size)
        // Just "three of this ring": no identity label is needed to say it.
        assertEquals(listOf(null, null, null), grown.map { it.identityGroup })
        assertEquals(listOf(might, might, might), grown.map { it.item })
        // The copies name the same item and nothing else.
        assertEquals(
            listOf(UpgradeMatch.EXACT, UpgradeMatch.ANY, UpgradeMatch.ANY),
            grown.map { it.upgradeMatch },
        )
        // And the board folds them back into one chip asking for three.
        val item = grown.boardItems().single()
        assertEquals(3, item.stackCount)
        assertEquals(1, grown.boardCount())
    }

    @Test
    fun aNarrowedWeaponStackGrowsByFamilyWideCopies() {
        val start = listOf(
            ItemRequirement(
                key = 1,
                item = null,
                upgrade = 5,
                kind = ItemKind.MELEE_WEAPON,
                tier = 4,
                tierMatch = TierMatch.EXACT,
                upgradeMatch = UpgradeMatch.EXACT,
            ),
        )
        val grown = start.setStackCount(start.boardItems().single(), count = 3)
        assertEquals(listOf(ItemKind.MELEE_WEAPON, ItemKind.WEAPON, ItemKind.WEAPON), grown.map { it.kind })
        assertEquals(listOf(false, true, true), grown.map { it.isBare })
        assertNull(grown.validationProblem())
        assertEquals(3, grown.boardItems().single().stackCount)
    }

    @Test
    fun aWildcardStackEncodesAsBareCopiesSharingAnIdentityGroup() {
        val start = listOf(anyWand(1))
        val grown = start.setStackCount(start.boardItems().single(), count = 3)
        assertEquals(3, grown.size)
        // The copies must resolve to the *same* wand as the anchor — the
        // blacksmith's reforge fodder — which only a shared label can say.
        assertEquals(listOf(1, 1, 1), grown.map { it.identityGroup })
        assertEquals(listOf(false, true, true), grown.map { it.isBare })
        assertNull(grown.validationProblem())
        assertEquals(3, grown.boardItems().single().stackCount)
    }

    @Test
    fun anEitherOrClusterCanAnchorAStack() {
        val cluster = listOf(anyWand(1), ItemRequirement(2, fireblast, 3)).joinAlternatives(0, 1)
        val grown = cluster.setStackCount(cluster.boardItems().single(), count = 3)
        // Every member of the cluster carries the label, so the stack binds to
        // whichever alternative the search picks.
        assertEquals(listOf(1, 1, 1, 1), grown.map { it.identityGroup })
        assertEquals(2, grown.count { it.alternativeGroup != null })
        assertNull(grown.validationProblem())
        val item = grown.boardItems().single()
        assertEquals(2, item.members.size)
        assertEquals(3, item.stackCount)
    }

    @Test
    fun aPlainRepeatStackTradesItsCopiesForLabelsWhenItJoinsACluster() {
        val start = listOf(ring(1))
        val stacked = start.setStackCount(start.boardItems().single(), count = 2) +
            ItemRequirement(9, ItemCatalog.findById("ring_energy"), 1)
        val joined = stacked.joinAlternatives(source = 0, target = 2)
        // The repeat could no longer be told apart from a second slot, so it
        // becomes a labelled bare copy instead and the stack survives.
        assertNull(joined.validationProblem())
        assertEquals(1, joined.boardCount())
        assertEquals(2, joined.boardItems().single().stackCount)
    }

    @Test
    fun aLabelledStackLetsGoOfItsCopiesInACrossCategoryCluster() {
        // Dragging a ×3 "any weapon" onto a wand: the copies named weapons, and
        // "weapon or wand" is not a kind anything can copy, so they go rather
        // than leaving a query the engine refuses.
        val start = listOf(anyWand(1).copy(kind = ItemKind.WEAPON, item = null))
        val stacked = start.setStackCount(start.boardItems().single(), count = 3) +
            ItemRequirement(9, fireblast, 3)
        assertEquals(3, stacked.count { it.identityGroup != null })
        val joined = stacked.joinAlternatives(source = 0, target = 3)
        assertNull(joined.validationProblem())
        assertNull(joined.firstOrNull { it.identityGroup != null })
        val item = joined.boardItems().single()
        assertEquals(2, item.members.size)
        assertEquals(1, item.stackCount)
    }

    @Test
    fun aStackDoesNotFollowItsChipIntoAClusterOfAnotherCategory() {
        // A copy has to name the kind it copies, and "ring or wand" names none,
        // so the second ring stays the standalone chip it already encodes as
        // rather than becoming an impossible copy.
        val start = listOf(ring(1))
        val stacked = start.setStackCount(start.boardItems().single(), count = 2) + anyWand(9)
        val joined = stacked.joinAlternatives(source = 0, target = 2)
        assertNull(joined.validationProblem())
        assertNull(joined.firstOrNull { it.identityGroup != null })
        assertEquals(2, joined.boardCount())
    }

    @Test
    fun aClusterSpanningTwoCategoriesCannotGrowAStack() {
        val mixed = listOf(anyWand(1), ItemRequirement(2, sword, 3)).joinAlternatives(0, 1)
        val cluster = mixed.boardItems().single()
        assertEquals(2, cluster.members.size)
        assertFalse(mixed.canStack(cluster))
        assertSame(mixed, mixed.setStackCount(cluster, count = 2))
        // A cluster of one category is still free to stack.
        val wands = listOf(anyWand(1), ItemRequirement(2, fireblast, 3)).joinAlternatives(0, 1)
        assertTrue(wands.canStack(wands.boardItems().single()))
    }

    @Test
    fun deletingTheAnchorDeletesItsCopiesAndLeavesNoStaleGroups() {
        val start = listOf(anyWand(1))
        val stacked = start.setStackCount(start.boardItems().single(), count = 3) + ring(9)
        val left = stacked.removeItem(stacked.boardItems().first())
        assertEquals(listOf(9L), left.map { it.key })
        assertNull(left.single().identityGroup)
    }

    @Test
    fun ejectingAMemberFromAStackedClusterStripsItsLabel() {
        val cluster = listOf(anyWand(1), ItemRequirement(2, fireblast, 3)).joinAlternatives(0, 1)
        val grown = cluster.setStackCount(cluster.boardItems().single(), count = 2)
        val ejected = grown.detach(grown.indexOfFirst { it.item == fireblast })
        val loose = ejected.single { it.item == fireblast }
        assertNull(loose.alternativeGroup)
        assertNull(loose.identityGroup)
        assertNull(ejected.validationProblem())
    }

    // --- combined levels ---

    @Test
    fun aTotalTurnsTheStackIntoIdenticalOptionalMembers() {
        val start = listOf(ring(1, upgrade = 2))
        val stacked = start.setStackCount(start.boardItems().single(), count = 2)
        val totalled = stacked.setStackTotal(stacked.boardItems().single(), total = 3)
        // "+3 strength": one +2 ring, or a +0 and a +1. Every member is the
        // same open-ended ring, and the engine may leave one unused.
        assertEquals(listOf(3, 3), totalled.map { it.levelSum?.atLeast })
        assertEquals(listOf(UpgradeMatch.ANY, UpgradeMatch.ANY), totalled.map { it.upgradeMatch })
        assertEquals(listOf(null, null), totalled.map { it.identityGroup })
        assertNull(totalled.validationProblem())
        val item = totalled.boardItems().single()
        assertEquals(3, item.total)
        assertEquals(2, item.stackCount)
        // Clearing it returns to "exactly two of the ring".
        val cleared = totalled.setStackTotal(item, total = null)
        assertNull(cleared.firstOrNull { it.levelSum != null })
        assertEquals(2, cleared.boardItems().single().stackCount)
    }

    @Test
    fun onlyARingStackCanCountLevelsTogether() {
        // A wand's power does not scale the way a ring's effect does, so a
        // total is refused and the stack stays plain repeats.
        val start = listOf(ItemRequirement(1, fireblast, 3))
        val stacked = start.setStackCount(start.boardItems().single(), count = 2)
        assertSame(stacked, stacked.setStackTotal(stacked.boardItems().single(), total = 3))
        // A stale non-ring sum from a hand-written document still clears.
        val loaded = listOf(
            ItemRequirement(1, fireblast, 0, upgradeMatch = UpgradeMatch.ANY, levelSum = LevelSum(1, 3)),
            ItemRequirement(2, fireblast, 0, upgradeMatch = UpgradeMatch.ANY, levelSum = LevelSum(1, 3)),
        )
        val cleared = loaded.setStackTotal(loaded.boardItems().single(), total = null)
        assertNull(cleared.firstOrNull { it.levelSum != null })
        assertEquals(2, cleared.boardItems().single().stackCount)
        assertNull(cleared.validationProblem())
    }

    @Test
    fun aLoadedLevelSumDocumentCollapsesBackIntoOneChip() {
        val loaded = listOf(
            ItemRequirement(1, might, 0, upgradeMatch = UpgradeMatch.ANY, levelSum = LevelSum(1, 3)),
            ItemRequirement(2, might, 0, upgradeMatch = UpgradeMatch.ANY, levelSum = LevelSum(1, 3)),
        )
        val item = loaded.boardItems().single()
        assertEquals(2, item.stackCount)
        assertEquals(3, item.total)
        assertEquals(listOf(1), item.extras)
    }

    // --- the editor round trip ---

    @Test
    fun theEditorAppliesCountAndTotalAndRebuildsTheStack() {
        val start = listOf(ring(1, upgrade = 2))
        val edited = start.applyEdit(index = 0, requirement = ring(1, upgrade = 2), count = 3, total = 4)
        assertEquals(3, edited.size)
        assertEquals(listOf(4, 4, 4), edited.map { it.levelSum?.atLeast })
        assertNull(edited.validationProblem())
        // Shrinking back to one item dissolves the group entirely.
        val shrunk = edited.applyEdit(
            index = 0,
            requirement = ring(1, upgrade = 2),
            count = 1,
            total = null,
        )
        assertEquals(1, shrunk.size)
        assertNull(shrunk.single().levelSum)
        assertEquals(UpgradeMatch.EXACT, shrunk.single().upgradeMatch)
    }

    @Test
    fun aNewChipIsAppendedWithAFreshKey() {
        val start = listOf(ring(1))
        val added = start.applyEdit(index = null, requirement = anyWand(0), count = 1, total = null)
        assertEquals(2, added.size)
        assertEquals(2L, added.last().key)
        assertEquals(2, added.boardCount())
    }

    // --- the copies' floor limits ---

    @Test
    fun theAnchorAndItsCopiesCarryIndependentFloorLimits() {
        // "The +3 ring in the first four floors, a second one by floor nine."
        val edited = emptyList<ItemRequirement>().applyEdit(
            index = null,
            requirement = ring(0, upgrade = 3).copy(maximumDepth = 4),
            count = 2,
            total = null,
            copyDepth = 9,
        )
        assertEquals(listOf(4, 9), edited.map { it.maximumDepth })
        // Still one chip: a repeat carrying only a floor limit is a placement
        // bound, not a second kind of item, so it folds into the stack.
        val item = edited.boardItems().single()
        assertEquals(2, item.stackCount)
        assertEquals(9, edited.copyDepthOf(item))
        assertNull(edited.validationProblem())
        // The document keeps both limits apart across a round trip.
        val decoded = ResultsExport.decodeQuery(ResultsExport.encodeQuery(SearchRequest(edited))).requirements
        assertEquals(listOf(4, 9), decoded.map { it.maximumDepth })
        assertEquals(1, decoded.boardCount())
    }

    @Test
    fun unlimitedCopiesStayUnlimitedWhileTheAnchorIsFloorBound() {
        val edited = emptyList<ItemRequirement>().applyEdit(
            index = null,
            requirement = anyWand(0).copy(maximumDepth = 4),
            count = 2,
            total = null,
            copyDepth = null,
        )
        assertEquals(listOf(4, null), edited.map { it.maximumDepth })
        assertEquals(edited[0].identityGroup, edited[1].identityGroup)
        assertNull(edited.validationProblem())
    }

    @Test
    fun aWildcardStackLimitsItsBareCopiesWithoutConstrainingThemOtherwise() {
        val edited = emptyList<ItemRequirement>().applyEdit(
            index = null,
            requirement = anyWand(0),
            count = 2,
            total = null,
            copyDepth = 9,
        )
        assertTrue(edited.drop(1).all { it.maximumDepth == 9 && it.upgradeMatch == UpgradeMatch.ANY })
        assertNull(edited.validationProblem())
        // Growing the stack from the chip badge keeps the copies' floor.
        val grown = edited.setStackCount(edited.boardItems().single(), count = 3)
        assertEquals(3, grown.size)
        assertTrue(grown.drop(1).all { it.maximumDepth == 9 })
    }

    @Test
    fun editingAwayTheLimitClearsItFromEveryCopy() {
        val limited = emptyList<ItemRequirement>().applyEdit(
            index = null,
            requirement = ring(0),
            count = 3,
            total = null,
            copyDepth = 6,
        )
        assertTrue(limited.drop(1).all { it.maximumDepth == 6 })
        val cleared = limited.applyEdit(index = 0, requirement = limited[0], count = 3, total = null, copyDepth = null)
        assertEquals(listOf(null, null, null), cleared.map { it.maximumDepth })
        assertEquals(1, cleared.boardCount())
    }

    @Test
    fun theCopiesKeepTheirFloorWhenTheStackFollowsItsChipIntoACluster() {
        val stacked = emptyList<ItemRequirement>().applyEdit(
            index = null,
            requirement = ring(0),
            count = 2,
            total = null,
            copyDepth = 7,
        ) + ItemRequirement(9, ItemCatalog.findById("ring_energy"), 1)
        val joined = stacked.joinAlternatives(source = 0, target = 2)
        // The plain repeat became a labelled bare copy, and kept its floor.
        assertEquals(7, joined.single { it.item == null }.maximumDepth)
        assertNull(joined.validationProblem())
    }

    // --- parity with the document the engine reads ---

    @Test
    fun everyStackShapeSurvivesTheDocumentRoundTrip() {
        val concrete = listOf(ring(1)).let { it.setStackCount(it.boardItems().single(), 3) }
        val wildcard = listOf(anyWand(1)).let { it.setStackCount(it.boardItems().single(), 2) }
        val totalled = listOf(ring(1)).let {
            val two = it.setStackCount(it.boardItems().single(), 2)
            two.setStackTotal(two.boardItems().single(), 3)
        }
        val cluster = listOf(anyWand(1), ItemRequirement(2, fireblast, 3)).joinAlternatives(0, 1)
        for (shape in listOf(concrete, wildcard, totalled, cluster)) {
            val document = ResultsExport.encodeQuery(SearchRequest(shape))
            val decoded = ResultsExport.decodeQuery(document).requirements
            assertEquals(shape.size, decoded.size)
            // The collapsed board is what the user sees restored.
            assertEquals(shape.boardItems().size, decoded.boardItems().size)
            assertEquals(
                shape.boardItems().map { it.stackCount to it.total },
                decoded.boardItems().map { it.stackCount to it.total },
            )
            assertNull(decoded.validationProblem())
        }
    }

    /**
     * The documents the web board writes for the same four gestures, captured
     * from its encoder. A stack means the same thing on both platforms only if
     * they spell it the same way — key order aside, which `JSONObject` does not
     * preserve and no reader depends on.
     */
    @Test
    fun theFourStackShapesMatchTheWebDocument() {
        fun documentOf(requirements: List<ItemRequirement>) =
            ResultsExport.encodeQuery(SearchRequest(requirements)).toString()

        // Both sides through one writer, so only the structure is compared.
        fun assertEquals(web: String, kotlin: String) =
            org.junit.Assert.assertEquals(JSONObject(web).toString(), JSONObject(kotlin).toString())

        val concrete = listOf(ring(1)).let { it.setStackCount(it.boardItems().single(), 3) }
        assertEquals(
            """{"requirements":[{"kind":"ring","item":"ring_might","upgrade":2},""" +
                """{"kind":"ring","item":"ring_might"},{"kind":"ring","item":"ring_might"}]}""",
            documentOf(concrete),
        )

        val wildcard = listOf(anyWand(1)).let { it.setStackCount(it.boardItems().single(), 3) }
        assertEquals(
            """{"requirements":[{"kind":"wand","upgrade":3,"identity_group":1},""" +
                """{"kind":"wand","identity_group":1},{"kind":"wand","identity_group":1}]}""",
            documentOf(wildcard),
        )

        val totalled = listOf(ring(1)).let {
            val two = it.setStackCount(it.boardItems().single(), 2)
            two.setStackTotal(two.boardItems().single(), 3)
        }
        assertEquals(
            """{"requirements":[{"kind":"ring","item":"ring_might","level_sum":{"group":1,"at_least":3}},""" +
                """{"kind":"ring","item":"ring_might","level_sum":{"group":1,"at_least":3}}]}""",
            documentOf(totalled),
        )

        val cluster = listOf(anyWand(1), ItemRequirement(2, fireblast, 3))
            .joinAlternatives(0, 1)
            .let { it.setStackCount(it.boardItems().single(), 3) }
        assertEquals(
            """{"requirements":[{"any_of":[{"kind":"wand","item":"wand_fireblast","upgrade":3,"identity_group":1},""" +
                """{"kind":"wand","upgrade":3,"identity_group":1}]},""" +
                """{"kind":"wand","identity_group":1},{"kind":"wand","identity_group":1}]}""",
            documentOf(cluster),
        )
    }

    @Test
    fun bareCopiesNeverOutliveTheirAnchorsCategory() {
        // Editing a wildcard stack's anchor into another category must not
        // leave copies of the old one behind: applyEdit rebuilds the stack.
        val start = listOf(anyWand(1))
        val stacked = start.setStackCount(start.boardItems().single(), count = 3)
        val retyped = stacked.applyEdit(
            index = 0,
            requirement = ItemRequirement(1, null, 0, kind = ItemKind.RING, upgradeMatch = UpgradeMatch.ANY),
            count = 3,
            total = null,
        )
        assertEquals(listOf(ItemKind.RING, ItemKind.RING, ItemKind.RING), retyped.map { it.kind })
        assertNull(retyped.validationProblem())
    }
}
