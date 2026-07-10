// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.UpgradeMatch
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class QueryCodecTest {
    @Test
    fun encodesStableSsf2PacketWithExactUpgrade() {
        val sword = ItemCatalog.weapons.first { it.id == "sword" }
        val request = SearchRequest(
            listOf(ItemRequirement(key = 9, item = sword, upgrade = 2, modifier = "Lucky")),
        )

        assertArrayEquals(
            byteArrayOf(
                0x53, 0x53, 0x46, 0x32, // SSF2
                0x18, 0x00, // floor 24, no world flags
                0x00, 0x01, // one requirement
                0x00, // weapon
                0x00, 0x05, 0x73, 0x77, 0x6F, 0x72, 0x64, // sword
                0x01, // exact predicate
                0x02, // exactly +2
                0x00, 0x05, 0x4C, 0x75, 0x63, 0x6B, 0x79, // Lucky
                0x00, 0x00, // any source, no identity group
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
                0x53, 0x53, 0x46, 0x32,
                24, 0,
                0, 1,
                3,
                0, 18,
            ) + "ring_sharpshooting".encodeToByteArray() + byteArrayOf(1, 4, 0, 0, 0, 0),
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
                0x53, 0x53, 0x46, 0x32,
                0x18, 0x02, // floor 24, fast-mode flag
                0x00, 0x01,
                0x00,
                0x00, 0x05, 0x73, 0x77, 0x6F, 0x72, 0x64,
                0x01,
                0x03,
                0x00, 0x00,
                0x00, 0x00,
            ),
            QueryCodec.encode(request),
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
                'S'.code.toByte(), 'S'.code.toByte(), 'F'.code.toByte(), '2'.code.toByte(),
                14, 1, 0, 4,
                2, 0, 0, 1, 3, 0, 0, 15, 1,
                2, 0, 0, 2, 0, 0, 0, 0, 1,
                2, 0, 0, 2, 0, 0, 0, 0, 1,
                2, 0, 0, 1, 1, 0, 0, 0, 0,
            ),
            packet,
        )
    }
}
