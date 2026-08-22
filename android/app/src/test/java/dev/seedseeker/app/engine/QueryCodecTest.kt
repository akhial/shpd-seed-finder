// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.catalog.PackagedCatalog
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.UpgradeMatch
import dev.seedseeker.app.model.TierMatch
import dev.seedseeker.app.model.WandmakerQuest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class QueryCodecTest {
    init { PackagedCatalog.install() }

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
                24, 0, 0, 0, 0, 0, 1,
                0, 0, 0, // weapon, any item
                1, 5, // exact tier 5
                0, 0, // any upgrade
                0, 0, // no modifier
                0, 0, 0, // any source, no identity group, no requirement floor limit
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
        assertEquals(4, meleePacket[11].toInt())

        val shuriken = ItemCatalog.thrownWeapons.first { it.id == "shuriken" }
        val thrown = ItemRequirement(
            key = 2,
            item = shuriken,
            upgrade = 0,
            kind = ItemKind.THROWN_WEAPON,
            upgradeMatch = UpgradeMatch.ANY,
        )
        val thrownPacket = QueryCodec.encode(SearchRequest(listOf(thrown)))
        assertEquals(5, thrownPacket[11].toInt())
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
        assertArrayEquals(byteArrayOf(3, 4), packet.copyOfRange(14, 16))
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
    fun encodesStableSsf8PacketWithExactUpgradeAndFloorLimit() {
        val sword = ItemCatalog.weapons.first { it.id == "sword" }
        val request = SearchRequest(
            listOf(ItemRequirement(key = 9, item = sword, upgrade = 2, modifier = "Lucky", maximumDepth = 5)),
        )

        assertArrayEquals(
            byteArrayOf(
                0x53, 0x53, 0x46, 0x38, // SSF8
                0x18, 0x00, // floor 24, no world flags
                0x00, 0x00, // no challenges, little-endian
                0x00, // any Wandmaker quest
                0x00, 0x01, // one requirement
                0x00, // weapon
                0x00, 0x05, 0x73, 0x77, 0x6F, 0x72, 0x64, // sword
                0x00, 0x00, // any tier
                0x01, // exact predicate
                0x02, // exactly +2
                0x00, 0x05, 0x4C, 0x75, 0x63, 0x6B, 0x79, // Lucky
                0x00, 0x00, 0x05, // any source, no identity group, by floor 5
                0x00, // curse state does not matter
            ),
            QueryCodec.encode(request),
        )
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
                0,
                0, 1,
                3,
                0, 18,
            ) + "ring_sharpshooting".encodeToByteArray() + byteArrayOf(0, 0, 1, 4, 0, 0, 0, 0, 0, 0),
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
                0x00,
                0x00, 0x01,
                0x00,
                0x00, 0x05, 0x73, 0x77, 0x6F, 0x72, 0x64,
                0x00, 0x00,
                0x01,
                0x03,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
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
                0x00,
                0x00, 0x01,
                0x00,
                0x00, 0x05, 0x73, 0x77, 0x6F, 0x72, 0x64,
                0x00, 0x00,
                0x01, 0x02,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
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
                14, 1, 0, 0, 0, 0, 4,
                2, 0, 0, 0, 0, 1, 3, 0, 0, 15, 1, 0, 0,
                2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1, 0, 0,
                2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1, 0, 0,
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0,
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
    fun encodesTheWandmakerQuestByteBetweenChallengesAndRequirements() {
        val sword = ItemCatalog.weapons.first { it.id == "sword" }
        val requirement = ItemRequirement(key = 1, item = sword, upgrade = 2)
        for (quest in WandmakerQuest.entries) {
            val packet = QueryCodec.encode(
                SearchRequest(requirements = listOf(requirement), wandmakerQuest = quest),
            )
            assertEquals(quest.wireId, packet[8].toInt())
        }
        assertEquals(0, QueryCodec.encode(SearchRequest(listOf(requirement)))[8].toInt())

        assertEquals("Rotberry", WandmakerQuest.ROTBERRY.label)
        assertEquals(WandmakerQuest.ELEMENTAL_EMBERS, WandmakerQuest.named("elemental_embers"))
        assertEquals(null, WandmakerQuest.named("elemental embers"))
    }

    @Test
    fun uncursedRequirementRejectsCurseModifier() {
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(
                key = 1,
                item = ItemCatalog.weapons.first(),
                upgrade = 1,
                modifier = "Displacing",
                requireUncursed = true,
            )
        }
    }
}
