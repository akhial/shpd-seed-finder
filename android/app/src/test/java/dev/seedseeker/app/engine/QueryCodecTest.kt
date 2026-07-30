// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.EffectRequirement
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.UpgradeMatch
import dev.seedseeker.app.model.TierMatch
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class QueryCodecTest {
    @Test
    fun tierPredicateUsesSsf8AndEncodesExactTierWithZeroChallengeMask() {
        val requirement = ItemRequirement(
            key = 1,
            item = null,
            upgrade = 0,
            kind = ItemKind.WEAPON,
            tier = 5,
            tierMatch = TierMatch.EXACT,
            upgradeMatch = UpgradeMatch.ANY,
        )

        assertArrayEquals(
            byteArrayOf(
                'S'.code.toByte(), 'S'.code.toByte(), 'F'.code.toByte(), '8'.code.toByte(),
                24, 0, 0, 0, 0, 1,
                0, 0, 0, // weapon, any item
                1, 5, // exact tier 5
                0, 0, // any upgrade
                0, // any effect
                0, 0, 0, // any source, no identity group, no requirement floor limit
                0, // no alternative group
                0, 0, // no combined-upgrade group or total
                0, // curse state does not matter
            ),
            QueryCodec.encode(SearchRequest(listOf(requirement))),
        )
        assertThrows(IllegalArgumentException::class.java) {
            requirement.copy(tier = 1)
        }
    }

    @Test
    fun meleeAndThrownWeaponKindsUseWireIdsFourAndFive() {
        val melee = ItemRequirement(
            key = 1,
            item = null,
            upgrade = 0,
            kind = ItemKind.MELEE_WEAPON,
            upgradeMatch = UpgradeMatch.ANY,
        )
        val meleePacket = QueryCodec.encode(SearchRequest(listOf(melee)))
        assertEquals(4, meleePacket[10].toInt())

        val shuriken = ItemCatalog.thrownWeapons.first { it.id == "shuriken" }
        val thrown = ItemRequirement(
            key = 2,
            item = shuriken,
            upgrade = 0,
            kind = ItemKind.THROWN_WEAPON,
            upgradeMatch = UpgradeMatch.ANY,
        )
        val thrownPacket = QueryCodec.encode(SearchRequest(listOf(thrown)))
        assertEquals(5, thrownPacket[10].toInt())
        assertEquals("Any melee weapon", melee.title)

        // A narrowed kind rejects an item of the other weapon class.
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(
                key = 3,
                item = ItemCatalog.meleeWeapons.first { it.id == "sword" },
                upgrade = 0,
                kind = ItemKind.THROWN_WEAPON,
                upgradeMatch = UpgradeMatch.ANY,
            )
        }
    }

    @Test
    fun encodesAtMostTierPredicate() {
        val requirement = ItemRequirement(
            key = 1,
            item = null,
            upgrade = 0,
            kind = ItemKind.ARMOR,
            tier = 4,
            tierMatch = TierMatch.AT_MOST,
            upgradeMatch = UpgradeMatch.ANY,
        )

        val packet = QueryCodec.encode(SearchRequest(listOf(requirement)))
        assertArrayEquals(byteArrayOf(3, 4), packet.copyOfRange(13, 15))
        assertEquals("Any Tier 4 or lower armor", requirement.title)
        assertThrows(IllegalArgumentException::class.java) {
            requirement.copy(tier = 5)
        }
        assertThrows(IllegalArgumentException::class.java) {
            requirement.copy(tier = 2)
        }
        assertThrows(IllegalArgumentException::class.java) {
            requirement.copy(tier = 2, tierMatch = TierMatch.AT_LEAST)
        }
        assertThrows(IllegalArgumentException::class.java) {
            requirement.copy(tier = 5, tierMatch = TierMatch.AT_LEAST)
        }
    }

    @Test
    fun encodesStableSsf8PacketWithSingleEffectAndFloorLimit() {
        val sword = ItemCatalog.weapons.first { it.id == "sword" }
        val request = SearchRequest(
            listOf(
                ItemRequirement(
                    key = 9,
                    item = sword,
                    upgrade = 2,
                    effect = EffectRequirement.OneOf(listOf("Lucky")),
                    maximumDepth = 5,
                ),
            ),
        )

        assertArrayEquals(
            byteArrayOf(
                0x53, 0x53, 0x46, 0x38, // SSF8
                0x18, 0x00, // floor 24, no world flags
                0x00, 0x00, // no challenges, little-endian
                0x00, 0x01, // one requirement
                0x00, // weapon
                0x00, 0x05, 0x73, 0x77, 0x6F, 0x72, 0x64, // sword
                0x00, 0x00, // any tier
                0x01, // exact predicate
                0x02, // exactly +2
                0x01, 0x01, // one-of effect set with one member
                0x00, 0x05, 0x4C, 0x75, 0x63, 0x6B, 0x79, // Lucky
                0x00, 0x00, 0x05, // any source, no identity group, by floor 5
                0x00, // no alternative group
                0x00, 0x00, // no combined-upgrade group or total
                0x00, // curse state does not matter
            ),
            QueryCodec.encode(request),
        )
    }

    @Test
    fun oneOfEffectSetsPreserveTheirMemberOrder() {
        val requirement = ItemRequirement(
            key = 1,
            item = ItemCatalog.armor.first { it.id == "plate_armor" },
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            effect = EffectRequirement.OneOf(listOf("Thorns", "Anti-Magic")),
        )

        val packet = QueryCodec.encode(SearchRequest(listOf(requirement)))
        val expectedEffects = byteArrayOf(0x01, 0x02) + // one-of, two members
            byteArrayOf(0x00, 0x06) + "Thorns".encodeToByteArray() +
            byteArrayOf(0x00, 0x0A) + "Anti-Magic".encodeToByteArray()
        // Header, kind, item id, tier, and upgrade precede the effect predicate.
        val effectOffset = 10 + 1 + 2 + "plate_armor".length + 2 + 2
        assertArrayEquals(
            expectedEffects,
            packet.copyOfRange(effectOffset, effectOffset + expectedEffects.size),
        )
    }

    @Test
    fun anyEnchantmentExpandsToTheFamilyNonCurseNamesInCatalogOrder() {
        val requirement = ItemRequirement(
            key = 1,
            item = ItemCatalog.weapons.first { it.id == "sword" },
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            effect = EffectRequirement.AnyEnchantment,
        )

        val packet = QueryCodec.encode(SearchRequest(listOf(requirement)))
        var expectedEffects = byteArrayOf(0x01, 13)
        ItemCatalog.enchantments.forEach { name ->
            expectedEffects += byteArrayOf(0, name.length.toByte()) + name.encodeToByteArray()
        }
        val effectOffset = 10 + 1 + 2 + "sword".length + 2 + 2
        assertArrayEquals(
            expectedEffects,
            packet.copyOfRange(effectOffset, effectOffset + expectedEffects.size),
        )
        // The trailer after the effect predicate stays all-defaults.
        assertArrayEquals(
            byteArrayOf(0, 0, 0, 0, 0, 0, 0),
            packet.copyOfRange(effectOffset + expectedEffects.size, packet.size),
        )
    }

    @Test
    fun encodesAlternativeGroupByte() {
        fun weapon(key: Long, id: String) = ItemRequirement(
            key = key,
            item = ItemCatalog.weapons.first { it.id == id },
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            alternativeGroup = 3,
        )

        val packet = QueryCodec.encode(SearchRequest(listOf(weapon(1, "spear"), weapon(2, "sword"))))
        // kind, item id, tier, upgrade, effect mode, source, identity, floor.
        val firstAlternativeOffset = 10 + 1 + 2 + "spear".length + 2 + 2 + 1 + 3
        assertArrayEquals(
            byteArrayOf(3, 0, 0, 0), // group 3, no sum group/total, no flags
            packet.copyOfRange(firstAlternativeOffset, firstAlternativeOffset + 4),
        )
        assertEquals(3, packet[packet.size - 4].toInt())
        assertThrows(IllegalArgumentException::class.java) {
            weapon(3, "sword").copy(alternativeGroup = 0)
        }
        assertThrows(IllegalArgumentException::class.java) {
            weapon(3, "sword").copy(upgradeSumGroup = 1, upgradeSumTotal = 2)
        }
    }

    @Test
    fun encodesCombinedUpgradeGroupAndTotal() {
        fun ring(key: Long) = ItemRequirement(
            key = key,
            item = ItemCatalog.rings.first { it.id == "ring_might" },
            upgrade = 0,
            upgradeMatch = UpgradeMatch.ANY,
            identityGroup = 1,
            upgradeSumGroup = 1,
            upgradeSumTotal = 2,
        )

        val packet = QueryCodec.encode(SearchRequest(listOf(ring(1), ring(2))))
        // Each requirement ends with alt, sumGroup, sumTotal, flags.
        assertArrayEquals(byteArrayOf(0, 1, 2, 0), packet.copyOfRange(packet.size - 4, packet.size))
        assertThrows(IllegalArgumentException::class.java) {
            ring(3).copy(upgradeSumTotal = null)
        }
        assertThrows(IllegalArgumentException::class.java) {
            ring(3).copy(upgradeSumTotal = 9)
        }
        assertThrows(IllegalArgumentException::class.java) {
            ring(3).copy(upgradeSumGroup = 5)
        }
    }

    @Test
    fun ringsAcceptPlusFourWithoutExpandingOtherItemRanges() {
        val ring = ItemCatalog.rings.first { it.id == "ring_sharpshooting" }
        val request = SearchRequest(listOf(ItemRequirement(1, ring, 4)))
        val packet = QueryCodec.encode(request)
        assertArrayEquals(
            byteArrayOf(
                0x53, 0x53, 0x46, 0x38,
                24, 0,
                0, 0,
                0, 1,
                3,
                0, 18,
            ) + "ring_sharpshooting".encodeToByteArray() +
                byteArrayOf(0, 0, 1, 4, 0, 0, 0, 0, 0, 0, 0, 0),
            packet,
        )
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(2, ItemCatalog.wands.first(), 4)
        }
    }

    @Test
    fun fastModeSetsFlagBitOne() {
        val sword = ItemCatalog.weapons.first { it.id == "sword" }
        val request = SearchRequest(
            requirements = listOf(ItemRequirement(key = 9, item = sword, upgrade = 3)),
            fastMode = true,
        )
        assertArrayEquals(
            byteArrayOf(
                0x53, 0x53, 0x46, 0x38,
                0x18, 0x02, // floor 24, fast-mode flag
                0x00, 0x00,
                0x00, 0x01,
                0x00,
                0x00, 0x05, 0x73, 0x77, 0x6F, 0x72, 0x64,
                0x00, 0x00,
                0x01,
                0x03,
                0x00, // any effect
                0x00, 0x00, 0x00, // any source, no identity group, no floor limit
                0x00, 0x00, 0x00, // no alternative or combined-upgrade group
                0x00,
            ),
            QueryCodec.encode(request),
        )
    }

    @Test
    fun excludeBlacksmithRewardsSetsFlagBitTwo() {
        val sword = ItemCatalog.weapons.first { it.id == "sword" }
        val packet = QueryCodec.encode(
            SearchRequest(
                requirements = listOf(ItemRequirement(key = 9, item = sword, upgrade = 2)),
                excludeBlacksmithRewards = true,
            ),
        )

        assertArrayEquals(
            byteArrayOf(
                0x53, 0x53, 0x46, 0x38,
                0x18, 0x04,
                0x00, 0x00,
                0x00, 0x01,
                0x00,
                0x00, 0x05, 0x73, 0x77, 0x6F, 0x72, 0x64,
                0x00, 0x00,
                0x01, 0x02,
                0x00,
                0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
                0x00,
            ),
            packet,
        )
    }

    @Test
    fun generalConstraintsExpressLinkedWandReforgeSetup() {
        fun wand(
            key: Long,
            match: UpgradeMatch,
            upgrade: Int,
            source: ScoutItemSource? = null,
            group: Int? = null,
        ) = ItemRequirement(
            key = key,
            item = null,
            upgrade = upgrade,
            kind = ItemKind.WAND,
            upgradeMatch = match,
            source = source,
            identityGroup = group,
        )
        val request = SearchRequest(
            requirements = listOf(
                wand(1, UpgradeMatch.EXACT, 3, ScoutItemSource.WANDMAKER_REWARD, 1),
                wand(2, UpgradeMatch.AT_LEAST, 0, group = 1),
                wand(3, UpgradeMatch.AT_LEAST, 0, group = 1),
                wand(4, UpgradeMatch.EXACT, 1),
            ),
            maximumDepth = 14,
            requireBlacksmith = true,
        )

        val packet = QueryCodec.encode(request)
        assertArrayEquals(
            byteArrayOf(
                'S'.code.toByte(), 'S'.code.toByte(), 'F'.code.toByte(), '8'.code.toByte(),
                14, 1, 0, 0, 0, 4,
                2, 0, 0, 0, 0, 1, 3, 0, 15, 1, 0, 0, 0, 0, 0,
                2, 0, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0,
                2, 0, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0,
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0,
            ),
            packet,
        )
    }

    @Test
    fun encodesChallengeMaskLittleEndian() {
        val request = SearchRequest(
            requirements = listOf(ItemRequirement(1, ItemCatalog.weapons.first(), 1)),
            challenges = 257,
        )

        val packet = QueryCodec.encode(request)
        assertArrayEquals(byteArrayOf(1, 1), packet.copyOfRange(6, 8))
        assertThrows(IllegalArgumentException::class.java) {
            request.copy(challenges = 512)
        }
    }

    @Test
    fun uncursedRequirementSetsRequirementFlagBitZero() {
        val requirement = ItemRequirement(
            key = 1,
            item = ItemCatalog.rings.first(),
            upgrade = 1,
            requireUncursed = true,
        )

        assertEquals(1, QueryCodec.encode(SearchRequest(listOf(requirement))).last().toInt())
    }

    @Test
    fun uncursedRequirementRejectsCursesOnlyEffectSets() {
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(
                key = 1,
                item = ItemCatalog.weapons.first(),
                upgrade = 1,
                effect = EffectRequirement.OneOf(listOf("Displacing")),
                requireUncursed = true,
            )
        }
        // A mixed set stays valid: curse members simply cannot match.
        ItemRequirement(
            key = 1,
            item = ItemCatalog.weapons.first(),
            upgrade = 1,
            effect = EffectRequirement.OneOf(listOf("Blazing", "Displacing")),
            requireUncursed = true,
        )
    }

    @Test
    fun effectSetsRejectForeignFamiliesAndEffectlessKinds() {
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(
                key = 1,
                item = ItemCatalog.weapons.first(),
                upgrade = 1,
                effect = EffectRequirement.OneOf(listOf("Thorns")),
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(
                key = 1,
                item = ItemCatalog.rings.first(),
                upgrade = 1,
                effect = EffectRequirement.AnyEnchantment,
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(
                key = 1,
                item = ItemCatalog.weapons.first(),
                upgrade = 1,
                effect = EffectRequirement.OneOf(emptyList()),
            )
        }
    }
}
