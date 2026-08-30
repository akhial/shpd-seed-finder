// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.catalog.PackagedCatalog
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test

/**
 * The local pre-validation that gives the editor its messages. The engine
 * enforces the same rules (QueryDocumentTest pins its refusals); these cases
 * pin the friendlier wording the header shows instead of an inert button.
 */
class SearchRequestValidationTest {
    init { PackagedCatalog.install() }

    private val might = ItemCatalog.findById("ring_might")
    private val sword = ItemCatalog.findById("sword")

    private fun ring(key: Long, atLeast: Int, upgrade: Int? = null, group: Int = 1) = ItemRequirement(
        key = key,
        item = might,
        upgrade = upgrade ?: 0,
        upgradeMatch = if (upgrade == null) UpgradeMatch.ANY else UpgradeMatch.EXACT,
        levelSum = LevelSum(group = group, atLeast = atLeast),
    )

    @Test
    fun anEmptyListAsksForARequirement() {
        assertEquals("Add at least one requirement.", emptyList<ItemRequirement>().validationProblem())
        assertThrows(IllegalArgumentException::class.java) { SearchRequest(emptyList()) }
    }

    @Test
    fun anUnattainableTotalNamesWhatTheItemsCanReach() {
        // Levels, not upgrades: an exact +1 ring counts 2, an unbounded ring
        // counts the ring cap of 4 plus one, so the pair reaches 7.
        val members = listOf(ring(1, atLeast = 8, upgrade = 1), ring(2, atLeast = 8))
        assertEquals(
            "A combined level of 8 needs more items: these 2 can reach 7.",
            members.validationProblem(),
        )
        assertNull(listOf(ring(1, atLeast = 7, upgrade = 1), ring(2, atLeast = 7)).validationProblem())
        val failure = assertThrows(IllegalArgumentException::class.java) { SearchRequest(members) }
        assertEquals("A combined level of 8 needs more items: these 2 can reach 7.", failure.message)

        // A weapon pair uses the weapon cap of 5, so 6 levels each.
        val weapons = listOf(
            ItemRequirement(1, sword, 0, upgradeMatch = UpgradeMatch.ANY, levelSum = LevelSum(2, 13)),
            ItemRequirement(2, sword, 0, upgradeMatch = UpgradeMatch.ANY, levelSum = LevelSum(2, 13)),
        )
        assertEquals(
            "A combined level of 13 needs more items: these 2 can reach 12.",
            weapons.validationProblem(),
        )
    }

    @Test
    fun oneUpgradedItemCanCoverACombinedLevelOnItsOwn() {
        // The reforge case: "+3 strength" is one +2 ring, or a +0 and a +1.
        // Members are optional, so a two-ring group asking for 3 levels is
        // reachable — and a single ring reaching 3 on its own is too.
        assertNull(listOf(ring(1, atLeast = 3), ring(2, atLeast = 3)).validationProblem())
    }

    @Test
    fun membersOfASumGroupMustShareOneTotal() {
        assertEquals(
            "A stack must share one combined level (it has 2 and 3).",
            listOf(ring(1, atLeast = 2), ring(2, atLeast = 3)).validationProblem(),
        )
        // Different groups are independent.
        assertNull(listOf(ring(1, atLeast = 2, group = 1), ring(2, atLeast = 3, group = 2)).validationProblem())
    }

    @Test
    fun onlyOneItemOfAStackMayCarryConstraints() {
        val frost = ItemCatalog.findById("wand_frost")
        val fire = ItemCatalog.findById("wand_fireblast")
        val clash = listOf(
            ItemRequirement(1, frost, 1, identityGroup = 1),
            ItemRequirement(2, fire, 1, identityGroup = 1),
        )
        assertEquals(
            "Only one item of a stack can carry constraints; the extra copies are plain.",
            clash.validationProblem(),
        )
        // The same two as alternatives of one slot are fine: they are one unit,
        // and the stack binds to whichever of them the search assigns.
        assertNull(clash.map { it.copy(alternativeGroup = 1) }.validationProblem())
        // The reforge shape: one constrained anchor plus bare copies of its kind.
        assertNull(
            listOf(
                ItemRequirement(1, frost, 1, identityGroup = 1),
                ItemRequirement(2, null, 0, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.ANY, identityGroup = 1),
            ).validationProblem(),
        )
        assertEquals(
            "The copies of a stack must share its category.",
            listOf(
                ItemRequirement(1, frost, 1, identityGroup = 1),
                ItemRequirement(2, null, 0, kind = ItemKind.RING, upgradeMatch = UpgradeMatch.ANY, identityGroup = 1),
            ).validationProblem(),
        )
    }

    @Test
    fun slotsGroupAlternativesAtTheirFirstMembersPosition() {
        val rows = listOf(
            ItemRequirement(1, sword, 1, alternativeGroup = 3),
            ItemRequirement(2, might, 1),
            ItemRequirement(3, sword, 2, alternativeGroup = 3),
            ItemRequirement(4, sword, 3, alternativeGroup = 9),
        )
        assertEquals(listOf(listOf(1L, 3L), listOf(2L), listOf(4L)), rows.slots().map { slot -> slot.map { it.key } })
        assertEquals(3, rows.slotCount())
        assertEquals(3, SearchRequest(rows).slotCount)
    }

    @Test
    fun effectFiltersCanonicalizeToTheSharedForm() {
        assertEquals(EffectFilter.Any, EffectFilter.of(emptyList(), ItemKind.WEAPON))
        assertEquals(EffectFilter.OneOf(listOf("Blazing", "Lucky")), EffectFilter.of(setOf("Lucky", "Blazing"), ItemKind.MELEE_WEAPON))
        assertEquals(EffectFilter.AnyEnchantment, EffectFilter.of(ItemCatalog.enchantments.shuffled(), ItemKind.WEAPON))
        assertEquals(
            EffectFilter.OneOf(ItemCatalog.enchantments + listOf("Annoying")),
            EffectFilter.of(ItemCatalog.enchantments + listOf("Annoying"), ItemKind.WEAPON),
        )
        assertEquals("Blazing", ItemRequirement(1, sword, 1, effect = EffectFilter.named("Blazing")).singleEffect)
        assertNull(ItemRequirement(1, sword, 1, effect = EffectFilter.OneOf(listOf("Blazing", "Lucky"))).singleEffect)
        assertThrows(IllegalArgumentException::class.java) { EffectFilter.OneOf(emptyList()) }
        assertThrows(IllegalArgumentException::class.java) { EffectFilter.OneOf(listOf("Lucky", "Lucky")) }
    }

    /** Model invariants the engine also enforces; pinned here since the SSF8 codec tests that held them are gone. */
    @Test
    fun modelInvariantsAreEnforcedLocally() {
        val anyWeapon = ItemRequirement(1, null, 0, kind = ItemKind.WEAPON, upgradeMatch = UpgradeMatch.ANY)
        assertThrows(IllegalArgumentException::class.java) { SearchRequest(listOf(anyWeapon), challenges = 512) }
        // Tier bounds.
        assertEquals(5, anyWeapon.copy(tier = 5, tierMatch = TierMatch.EXACT).tier)
        assertThrows(IllegalArgumentException::class.java) { anyWeapon.copy(tier = 1, tierMatch = TierMatch.EXACT) }
        assertThrows(IllegalArgumentException::class.java) { anyWeapon.copy(tier = 6, tierMatch = TierMatch.EXACT) }
        assertEquals(4, anyWeapon.copy(tier = 4, tierMatch = TierMatch.AT_MOST).tier)
        assertThrows(IllegalArgumentException::class.java) { anyWeapon.copy(tier = 5, tierMatch = TierMatch.AT_MOST) }
        assertThrows(IllegalArgumentException::class.java) { anyWeapon.copy(tier = 2, tierMatch = TierMatch.AT_MOST) }
        assertThrows(IllegalArgumentException::class.java) { anyWeapon.copy(tier = 2, tierMatch = TierMatch.AT_LEAST) }
        assertThrows(IllegalArgumentException::class.java) { anyWeapon.copy(tier = 5, tierMatch = TierMatch.AT_LEAST) }
        // A narrowed weapon kind rejects the other class.
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(1, sword, 0, kind = ItemKind.THROWN_WEAPON, upgradeMatch = UpgradeMatch.ANY)
        }
        assertEquals("Any melee weapon", anyWeapon.copy(kind = ItemKind.MELEE_WEAPON).title)
        // Weapons reach the vault's +5; every other family stops at +4.
        assertEquals(5, ItemRequirement(1, sword, 5).upgrade)
        assertEquals(4, ItemRequirement(1, might, 4).upgrade)
        assertThrows(IllegalArgumentException::class.java) { ItemRequirement(1, sword, 6) }
        assertThrows(IllegalArgumentException::class.java) { ItemRequirement(1, ItemCatalog.findById("wand_frost"), 5) }
        // Uncursed with a curses-only set.
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(1, sword, 1, effect = EffectFilter.named("Displacing"), requireUncursed = true)
        }
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(1, sword, 1, effect = EffectFilter.OneOf(listOf("Annoying", "Sacrificial")), requireUncursed = true)
        }
        // Quest names are the exact snake_case document names.
        assertEquals(WandmakerQuest.ELEMENTAL_EMBERS, WandmakerQuest.named("elemental_embers"))
        assertNull(WandmakerQuest.named("elemental embers"))
        assertEquals("Rotberry", WandmakerQuest.ROTBERRY.label)
    }
}
