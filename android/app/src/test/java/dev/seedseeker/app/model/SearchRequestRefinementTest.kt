// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SearchRequestRefinementTest {
    private val frost = ItemRequirement(1, ItemCatalog.wands.first { it.id == "wand_frost" }, 2)
    private val fireblast =
        ItemRequirement(2, ItemCatalog.wands.first { it.id == "wand_fireblast" }, 3)
    private val haste = ItemRequirement(3, ItemCatalog.rings.first { it.id == "ring_haste" }, 1)

    @Test
    fun addingARequirementRefinesTheBaseRun() {
        assertTrue(request(frost, fireblast).isRefinementOf(request(frost)))
    }

    @Test
    fun anIdenticalQueryIsNotARefinement() {
        assertFalse(request(frost, fireblast).isRefinementOf(request(frost, fireblast)))
    }

    @Test
    fun removingOrEditingABaseRequirementIsNotARefinement() {
        assertFalse(request(frost).isRefinementOf(request(frost, fireblast)))
        val editedFrost = frost.copy(upgrade = 3)
        assertFalse(request(editedFrost, fireblast).isRefinementOf(request(frost)))
    }

    @Test
    fun anyScopeChangeIsNotARefinement() {
        val base = request(frost)
        assertFalse(request(frost, fireblast, maximumDepth = 12).isRefinementOf(base))
        assertFalse(request(frost, fireblast, challenges = Challenge.DARKNESS.bit).isRefinementOf(base))
        assertFalse(request(frost, fireblast, requireBlacksmith = true).isRefinementOf(base))
        assertFalse(request(frost, fireblast, excludeBlacksmithRewards = true).isRefinementOf(base))
        assertFalse(request(frost, fireblast, fastMode = true).isRefinementOf(base))
    }

    @Test
    fun duplicateRequirementsAreMatchedAsAMultiset() {
        val secondFrost = frost.copy(key = 9)
        assertTrue(request(frost, secondFrost, haste).isRefinementOf(request(frost, secondFrost)))
        assertFalse(request(frost, fireblast, haste).isRefinementOf(request(frost, secondFrost)))
    }

    @Test
    fun requirementKeysAreIgnoredWhenMatching() {
        val rekeyed = frost.copy(key = 77)
        assertTrue(request(rekeyed, haste).isRefinementOf(request(frost)))
        assertFalse(request(rekeyed).isRefinementOf(request(frost)))
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
