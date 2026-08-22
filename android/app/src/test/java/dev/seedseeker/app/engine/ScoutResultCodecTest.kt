// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ScoutAccessibility
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.ScoutQuest
import dev.seedseeker.app.model.ScoutQuestGiver
import dev.seedseeker.app.model.ScoutQuestVariant
import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.io.EOFException
import java.nio.charset.StandardCharsets
import org.junit.Assert.assertEquals
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class ScoutResultCodecTest {
    @Test
    fun decodesAllSsc2FieldsWithoutDroppingDuplicateOrZeroUpgradeItems() {
        val packet = scoutPacket(
            item(
                id = "dagger",
                depth = 1,
                upgrade = 0,
                flags = 1,
                effect = "Lucky",
                source = 2,
                accessibility = byteArrayOf(0),
            ),
            item(
                id = "dagger",
                depth = 1,
                upgrade = 2,
                flags = 2,
                effect = null,
                source = 16,
                accessibility = byteArrayOf(1, 0x12, 0x34, 2),
            ),
            item(
                id = "wand_frost",
                depth = 22,
                upgrade = 3,
                flags = 0,
                effect = null,
                source = 14,
                accessibility = ByteArrayOutputStream().use { bytes ->
                    DataOutputStream(bytes).use { output ->
                        output.writeByte(2)
                        output.writeShort(7)
                        output.writeLong(0x8000_0000_0000_0001UL.toLong())
                    }
                    bytes.toByteArray()
                },
            ),
            item(
                id = "ring_sharpshooting",
                depth = 17,
                upgrade = 4,
                flags = 3,
                source = 16,
            ),
            quests = listOf(quest(1, 2, 3), quest(4, 1, 17)),
        )

        val world = ScoutResultCodec.decode(packet)

        assertEquals("AAA-AAA-AAA", world.seed)
        assertEquals(
            listOf(
                ScoutQuest(ScoutQuestVariant.GNOLL_TRICKSTER, 3),
                ScoutQuest(ScoutQuestVariant.MONK, 17),
            ),
            world.quests,
        )
        assertEquals(4, world.items.size)
        assertEquals(
            listOf("dagger", "dagger", "wand_frost", "ring_sharpshooting"),
            world.items.map { it.item.id },
        )
        with(world.items[0]) {
            assertEquals(0, upgrade)
            assertTrue(cursed)
            assertFalse(secret)
            assertEquals("Lucky", effect)
            assertEquals(ScoutItemSource.LOCKED_CHEST, source)
            assertEquals(ScoutAccessibility.Independent, accessibility)
        }
        with(world.items[1]) {
            assertFalse(cursed)
            assertTrue(secret)
            assertEquals(ScoutAccessibility.Choice(0x1234, 2), accessibility)
        }
        with(world.items[2]) {
            assertEquals(ItemKind.WAND, item.kind)
            assertEquals(22, depth)
            assertEquals(ScoutItemSource.WANDMAKER_REWARD, source)
            assertEquals(
                ScoutAccessibility.Scenarios(7, 0x8000_0000_0000_0001UL),
                accessibility,
            )
        }
        with(world.items[3]) {
            assertEquals(ItemKind.RING, item.kind)
            assertEquals(4, upgrade)
            assertTrue(cursed)
            assertTrue(secret)
            assertEquals(ScoutItemSource.IMP_REWARD, source)
        }
    }

    @Test
    fun decodesTheGoldenSsc2QuestBlockBytes() {
        val packet = "SSC2".toByteArray(StandardCharsets.US_ASCII) +
            byteArrayOf(0x0B) +
            "AAA-AAA-AAA".toByteArray(StandardCharsets.UTF_8) +
            byteArrayOf(
                0x04,
                0x01, 0x03, 0x04,
                0x02, 0x03, 0x08,
                0x03, 0x01, 0x0D,
                0x04, 0x02, 0x12,
                0x00, 0x00,
            )

        val world = ScoutResultCodec.decode(packet)

        assertEquals("AAA-AAA-AAA", world.seed)
        assertTrue(world.items.isEmpty())
        assertEquals(
            listOf(
                ScoutQuest(ScoutQuestVariant.GREAT_CRAB, 4),
                ScoutQuest(ScoutQuestVariant.ROTBERRY, 8),
                ScoutQuest(ScoutQuestVariant.CRYSTAL, 13),
                ScoutQuest(ScoutQuestVariant.GOLEM, 18),
            ),
            world.quests,
        )
        assertEquals(
            listOf(
                ScoutQuestGiver.GHOST,
                ScoutQuestGiver.WANDMAKER,
                ScoutQuestGiver.BLACKSMITH,
                ScoutQuestGiver.IMP,
            ),
            world.quests.map { it.giver },
        )
    }

    @Test
    fun rejectsMalformedQuestBlocks() {
        val unknownQuestId = scoutPacket(quests = listOf(quest(5, 1, 4)))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(unknownQuestId)
        }

        val zeroQuestId = scoutPacket(quests = listOf(quest(0, 1, 4)))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(zeroQuestId)
        }

        val unknownGhostVariant = scoutPacket(quests = listOf(quest(1, 4, 3)))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(unknownGhostVariant)
        }

        val unknownImpVariant = scoutPacket(quests = listOf(quest(4, 3, 18)))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(unknownImpVariant)
        }

        val zeroVariant = scoutPacket(quests = listOf(quest(2, 0, 8)))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(zeroVariant)
        }

        // Quest floors are the giver windows the engine publishes, checked once
        // by ScoutQuest itself rather than re-asserted by this decoder.
        val ghostDepthTooDeep = scoutPacket(quests = listOf(quest(1, 1, 5)))
        assertThrows(IllegalArgumentException::class.java) {
            ScoutResultCodec.decode(ghostDepthTooDeep)
        }

        val impDepthTooShallow = scoutPacket(quests = listOf(quest(4, 1, 16)))
        assertThrows(IllegalArgumentException::class.java) {
            ScoutResultCodec.decode(impDepthTooShallow)
        }

        val duplicateQuestIds = scoutPacket(quests = listOf(quest(2, 1, 7), quest(2, 2, 8)))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(duplicateQuestIds)
        }

        val descendingQuestIds = scoutPacket(quests = listOf(quest(3, 1, 13), quest(1, 1, 2)))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(descendingQuestIds)
        }

        val tooManyQuests = scoutPacket(
            quests = listOf(quest(1, 1, 2), quest(2, 1, 7), quest(3, 1, 12), quest(4, 1, 17)),
            questCount = 5,
        )
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(tooManyQuests)
        }
    }

    @Test
    fun rejectsReservedFlagsUnknownCodesZeroScenarioMasksAndTrailingBytes() {
        val badMagic = scoutPacket().also { it[0] = 'X'.code.toByte() }
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(badMagic)
        }

        val unknownItem = scoutPacket(item(id = "not_in_catalog"))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(unknownItem)
        }

        val impossibleWandEffect = scoutPacket(item(id = "wand_frost", effect = "Lucky"))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(impossibleWandEffect)
        }

        val reservedFlags = scoutPacket(item(flags = 4))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(reservedFlags)
        }

        val unknownSource = scoutPacket(item(source = 17))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(unknownSource)
        }

        val unknownAccessibility = scoutPacket(item(accessibility = byteArrayOf(3)))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(unknownAccessibility)
        }

        val zeroMask = scoutPacket(
            item(accessibility = byteArrayOf(2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0)),
        )
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(zeroMask)
        }

        val invalidChoice = scoutPacket(item(accessibility = byteArrayOf(1, 0, 1, 64)))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(invalidChoice)
        }

        val invalidDepth = scoutPacket(item(depth = 25))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(invalidDepth)
        }

        val invalidUpgrade = scoutPacket(item(upgrade = 4))
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(invalidUpgrade)
        }

        val trailing = scoutPacket(item()) + 0x55.toByte()
        assertThrows(IllegalStateException::class.java) {
            ScoutResultCodec.decode(trailing)
        }

        val truncated = scoutPacket(item()).dropLast(1).toByteArray()
        assertThrows(EOFException::class.java) {
            ScoutResultCodec.decode(truncated)
        }
    }

    @Test
    fun formatsTypedOrPastedSeedAndRequiresCanonicalInputAtTheJniBoundary() {
        assertEquals("ABC-DEF-GHI", SeedCode.formatInput("ab c-def_gHi!!!"))
        assertEquals("ABC-DEF-GHI", SeedCode.formatInput("abcdefghijkl"))
        assertTrue(SeedCode.isCanonical("ABC-DEF-GHI"))

        val bindings = ScoutBindings(scoutPacket())
        val world = JniNativeSeedFinder(bindings).scoutSeed("AAA-AAA-AAA")
        assertArrayEquals(
            byteArrayOf('S'.code.toByte(), 'S'.code.toByte(), 'Q'.code.toByte(), '2'.code.toByte(), 0, 0) +
                "AAA-AAA-AAA".toByteArray(StandardCharsets.UTF_8),
            bindings.scoutRequest,
        )
        assertEquals("AAA-AAA-AAA", world.seed)
        assertThrows(IllegalArgumentException::class.java) {
            JniNativeSeedFinder(bindings).scoutSeed("abc-def-ghi")
        }
        assertThrows(IllegalStateException::class.java) {
            JniNativeSeedFinder(bindings).scoutSeed("ABC-DEF-GHI")
        }
    }

    @Test
    fun scoutRequestEncodesChallengeMaskLittleEndian() {
        assertArrayEquals(
            byteArrayOf('S'.code.toByte(), 'S'.code.toByte(), 'Q'.code.toByte(), '2'.code.toByte(), 1, 1) +
                "AAA-AAA-AAA".toByteArray(StandardCharsets.UTF_8),
            ScoutRequestCodec.encode("AAA-AAA-AAA", 257),
        )
        assertThrows(IllegalArgumentException::class.java) {
            ScoutRequestCodec.encode("AAA-AAA-AAA", 512)
        }
    }

    private fun scoutPacket(
        vararg items: ByteArray,
        quests: List<IntArray> = emptyList(),
        questCount: Int = quests.size,
    ): ByteArray = ByteArrayOutputStream().use { bytes ->
        DataOutputStream(bytes).use { output ->
            output.writeBytes("SSC2")
            writeByteString(output, "AAA-AAA-AAA")
            output.writeByte(questCount)
            quests.forEach { (quest, variant, depth) ->
                output.writeByte(quest)
                output.writeByte(variant)
                output.writeByte(depth)
            }
            output.writeShort(items.size)
            items.forEach { output.write(it) }
        }
        bytes.toByteArray()
    }

    private fun quest(quest: Int, variant: Int, depth: Int) = intArrayOf(quest, variant, depth)

    private fun item(
        id: String = "dagger",
        depth: Int = 1,
        upgrade: Int = 1,
        flags: Int = 0,
        effect: String? = null,
        source: Int = 0,
        accessibility: ByteArray = byteArrayOf(0),
    ): ByteArray = ByteArrayOutputStream().use { bytes ->
        DataOutputStream(bytes).use { output ->
            writeShortString(output, id)
            output.writeByte(depth)
            output.writeByte(upgrade)
            output.writeByte(flags)
            writeShortString(output, effect.orEmpty())
            output.writeByte(source)
            output.write(accessibility)
        }
        bytes.toByteArray()
    }

    private fun writeByteString(output: DataOutputStream, text: String) {
        val encoded = text.toByteArray(StandardCharsets.UTF_8)
        output.writeByte(encoded.size)
        output.write(encoded)
    }

    private fun writeShortString(output: DataOutputStream, text: String) {
        val encoded = text.toByteArray(StandardCharsets.UTF_8)
        output.writeShort(encoded.size)
        output.write(encoded)
    }

    private class ScoutBindings(private val response: ByteArray) : NativeBindings {
        var scoutRequest = byteArrayOf()

        override fun scoutSeed(request: ByteArray): ByteArray {
            scoutRequest = request.copyOf()
            return response.copyOf()
        }

        override fun startSearch(request: ByteArray): Long = error("not used")
        override fun startResumedSearch(request: ByteArray, resumeFrom: Long, scanLen: Long): Long =
            error("not used")
        override fun poll(handle: Long, maxResults: Int): ByteArray = error("not used")
        override fun status(handle: Long): LongArray = error("not used")
        override fun resumeHint(handle: Long): LongArray = error("not used")
        override fun cancel(handle: Long) = error("not used")
        override fun close(handle: Long) = error("not used")
        override fun scoutMatches(request: ByteArray, query: ByteArray): ByteArray =
            error("not used")
        override fun filterSeeds(request: ByteArray, seeds: LongArray): ByteArray = error("not used")
        override fun queryContinues(candidate: ByteArray, base: ByteArray): Boolean =
            error("not used")
        override fun decideStart(
            candidate: ByteArray,
            target: ByteArray?,
            targetSetEmpty: Boolean,
            targetHasUncoveredSeeds: Boolean,
            detachedBase: ByteArray?,
        ): ByteArray = error("not used")
    }
}
