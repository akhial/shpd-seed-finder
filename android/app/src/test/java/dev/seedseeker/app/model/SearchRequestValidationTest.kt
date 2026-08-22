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
        upgradeSum = UpgradeSum(group = group, atLeast = atLeast),
    )

    @Test
    fun anEmptyListAsksForARequirement() {
        assertEquals("Add at least one requirement.", emptyList<ItemRequirement>().validationProblem())
        assertThrows(IllegalArgumentException::class.java) { SearchRequest(emptyList()) }
    }

    @Test
    fun anUnattainableTotalNamesTheGroupAndWhatItsItemsCanCarry() {
        // Two rings: an exact +1 counts as 1, an unbounded ring as the ring cap of 4.
        val members = listOf(ring(1, atLeast = 6, upgrade = 1), ring(2, atLeast = 6))
        assertEquals(
            "Combined upgrade group A needs +6 but its items can carry at most +5.",
            members.validationProblem(),
        )
        assertNull(listOf(ring(1, atLeast = 5, upgrade = 1), ring(2, atLeast = 5)).validationProblem())
        val failure = assertThrows(IllegalArgumentException::class.java) { SearchRequest(members) }
        assertEquals("Combined upgrade group A needs +6 but its items can carry at most +5.", failure.message)

        // A weapon group uses the default cap of 3 and its own letter.
        val weapons = listOf(
            ItemRequirement(1, sword, 0, upgradeMatch = UpgradeMatch.ANY, upgradeSum = UpgradeSum(2, 7)),
            ItemRequirement(2, sword, 0, upgradeMatch = UpgradeMatch.ANY, upgradeSum = UpgradeSum(2, 7)),
        )
        assertEquals(
            "Combined upgrade group B needs +7 but its items can carry at most +6.",
            weapons.validationProblem(),
        )
    }

    @Test
    fun membersOfASumGroupMustShareOneTotal() {
        assertEquals(
            "Combined upgrade group A must use one total (it has +2 and +3).",
            listOf(ring(1, atLeast = 2), ring(2, atLeast = 3)).validationProblem(),
        )
        // Different groups are independent.
        assertNull(listOf(ring(1, atLeast = 2, group = 1), ring(2, atLeast = 3, group = 2)).validationProblem())
    }

    @Test
    fun sameItemGroupsMayDisagreeOnlyBetweenAlternativesOfOneSlot() {
        val frost = ItemCatalog.findById("wand_frost")
        val fire = ItemCatalog.findById("wand_fireblast")
        val clash = listOf(
            ItemRequirement(1, frost, 1, identityGroup = 1),
            ItemRequirement(2, fire, 1, identityGroup = 1),
        )
        assertEquals("Same-item group A mixes different items or categories.", clash.validationProblem())
        // The same two as alternatives of one slot are fine: only one is ever assigned.
        assertNull(clash.map { it.copy(alternativeGroup = 1) }.validationProblem())
        // A wildcard agrees with a named item of its category, not with another category.
        assertNull(
            listOf(
                ItemRequirement(1, frost, 1, identityGroup = 1),
                ItemRequirement(2, null, 0, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.ANY, identityGroup = 1),
            ).validationProblem(),
        )
        assertEquals(
            "Same-item group A mixes different items or categories.",
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
        // Rings reach +4; nothing else does.
        assertEquals(4, ItemRequirement(1, might, 4).upgrade)
        assertThrows(IllegalArgumentException::class.java) { ItemRequirement(1, ItemCatalog.findById("wand_frost"), 4) }
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
