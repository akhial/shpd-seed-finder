// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class DeepLinkTest {
    private val pinnedQuery = PresetQuery(
        requirements = listOf(
            ItemRequirement(
                key = 1,
                item = ItemCatalog.findById("wand_fireblast"),
                upgrade = 3,
                kind = ItemKind.WAND,
                upgradeMatch = UpgradeMatch.AT_LEAST,
            ),
        ),
    )

    /**
     * The cross-platform pinned vector: a known code must decode identically
     * forever on every platform. This mirrors the Rust core's
     * `version_one_codes_are_stable` test.
     */
    @Test
    fun versionOneCodesAreStable() {
        assertEquals("EAGWhMA", DeepLink.encode(pinnedQuery))
        assertEquals("https://shpd-seed-seeker.web.app/#q=EAGWhMA", DeepLink.encodeLink(pinnedQuery))
        assertEquals(pinnedQuery.normalized(), DeepLink.decode("EAGWhMA").normalized())
        assertEquals(
            pinnedQuery.normalized(),
            DeepLink.decodeText("https://shpd-seed-seeker.web.app/#q=EAGWhMA").normalized(),
        )
        // The same query expressed as the canonical JSON query document.
        val document = """
            {"format":"seed-seeker-results","format_version":1,
             "query":{"requirements":[{"item":"wand_fireblast","kind":"wand","upgrade":{"at_least":3}}]},
             "results":[]}
        """.trimIndent()
        assertEquals("EAGWhMA", DeepLink.encode(ResultsExport.decode(document).query))
    }

    @Test
    fun roundTripsAFullyLoadedQuery() {
        val query = PresetQuery(
            requirements = listOf(
                ItemRequirement(
                    key = 1,
                    item = ItemCatalog.findById("war_scythe"),
                    upgrade = 2,
                    modifier = "Grim",
                    kind = ItemKind.MELEE_WEAPON,
                    upgradeMatch = UpgradeMatch.AT_LEAST,
                    source = ScoutItemSource.SACRIFICIAL_FIRE,
                    identityGroup = 4,
                    maximumDepth = 21,
                    requireUncursed = true,
                ),
                ItemRequirement(
                    key = 2,
                    item = null,
                    upgrade = 3,
                    kind = ItemKind.ARMOR,
                    tier = 4,
                    tierMatch = TierMatch.AT_LEAST,
                    upgradeMatch = UpgradeMatch.EXACT,
                ),
                ItemRequirement(
                    key = 3,
                    item = null,
                    upgrade = 0,
                    kind = ItemKind.THROWN_WEAPON,
                    tier = 3,
                    tierMatch = TierMatch.AT_MOST,
                    upgradeMatch = UpgradeMatch.ANY,
                ),
                ItemRequirement(
                    key = 4,
                    item = ItemCatalog.findById("ring_wealth"),
                    upgrade = 4,
                    kind = ItemKind.RING,
                    upgradeMatch = UpgradeMatch.EXACT,
                ),
            ),
            maximumDepth = 19,
            requireBlacksmith = true,
            excludeBlacksmithRewards = true,
            wandmakerQuest = WandmakerQuest.ROTBERRY,
            fastMode = true,
            challenges = Challenge.NO_FOOD.bit or Challenge.STRONGER_BOSSES.bit,
        )
        assertEquals(query.normalized(), DeepLink.decode(DeepLink.encode(query)).normalized())
    }

    @Test
    fun roundTripsEveryWandmakerFilter() {
        for (variant in WandmakerQuest.entries) {
            val query = pinnedQuery.copy(wandmakerQuest = variant)
            assertEquals(query.normalized(), DeepLink.decode(DeepLink.encode(query)).normalized())
        }
        // An unfiltered query spends one bit and decodes back to "any".
        assertNull(DeepLink.decode(DeepLink.encode(pinnedQuery)).wandmakerQuest)
    }

    @Test
    fun roundTripsEveryItemEffectSourceAndChallenge() {
        for (id in DeepLink.ITEM_CODES) {
            val item = checkNotNull(ItemCatalog.findById(id)) { "unknown table item $id" }
            assertRoundTrips(minimal(ItemRequirement(1, item, 0, upgradeMatch = UpgradeMatch.ANY)))
        }
        val effectTables = listOf(
            ItemKind.WEAPON to DeepLink.WEAPON_EFFECT_CODES,
            ItemKind.ARMOR to DeepLink.ARMOR_EFFECT_CODES,
        )
        for ((kind, effects) in effectTables) {
            for (effect in effects) {
                assertRoundTrips(minimal(wildcard(kind).copy(modifier = effect)))
            }
        }
        for (source in DeepLink.SOURCE_CODES) {
            assertRoundTrips(minimal(wildcard(ItemKind.WAND).copy(source = source)))
        }
        for (challenge in Challenge.entries) {
            assertRoundTrips(minimal(wildcard(ItemKind.RING)).copy(challenges = challenge.bit))
        }
    }

    @Test
    fun refusesToEncodeInvalidQueries() {
        assertThrows(IllegalArgumentException::class.java) {
            DeepLink.encode(PresetQuery(requirements = emptyList()))
        }
        val tooMany = PresetQuery(
            requirements = List(64) { wildcard(ItemKind.WAND, key = it + 1L) },
        )
        val failure = assertThrows(IllegalArgumentException::class.java) {
            DeepLink.encode(tooMany)
        }
        assertTrue(failure.message!!.contains("63"))
        assertThrows(IllegalArgumentException::class.java) {
            DeepLink.encode(minimal(wildcard(ItemKind.WAND)).copy(maximumDepth = 0))
        }
    }

    @Test
    fun rejectsMalformedCodes() {
        assertDecodeFails("incomplete", "")
        assertDecodeFails("characters", "!!!")
        assertDecodeFails("incomplete", "A")
        // Unsupported future version (bits 0100 in the top nibble).
        val versioned = assertThrows(IllegalArgumentException::class.java) {
            DeepLink.decode("QAAA")
        }
        assertTrue(versioned.message!!.contains("version 4"))
        assertTrue(versioned.message!!.contains("newer"))
        assertDecodeFails("incomplete", "EAGWhM")
        assertDecodeFails("trailing", "EAGWhMAAAAA")
    }

    @Test
    fun rejectsInvalidPayloads() {
        // Sanity anchor: the raw-bit helper reproduces the pinned vector.
        val pinnedFields = arrayOf(
            *header(count = 1),
            4 to 3, 1 to 1, 52 to 7, 0 to 2, 2 to 2, 3 to 3,
            0 to 1, 0 to 1, 0 to 1, 0 to 1, 0 to 1,
        )
        assertEquals("EAGWhMA", rawCode(*pinnedFields))
        assertDecodeFails("no requirements", rawCode(*header(count = 0)))
        assertDecodeFails("incomplete", rawCode(*header(count = 1)))
        assertDecodeFails("category code 6", rawCode(*header(count = 1), 6 to 3))
        assertDecodeFails("item code 88", rawCode(*header(count = 1), 4 to 3, 1 to 1, 88 to 7))
        assertDecodeFails(
            "upgrade mode 3",
            rawCode(*header(count = 1), 4 to 3, 0 to 1, 0 to 2, 3 to 2),
        )
        assertDecodeFails(
            "effect code 0",
            rawCode(*header(count = 1), 4 to 3, 0 to 1, 0 to 2, 0 to 2, 1 to 1, 0 to 5),
        )
        assertDecodeFails(
            "source code 17",
            rawCode(
                *header(count = 1),
                4 to 3, 0 to 1, 0 to 2, 0 to 2, 0 to 1, 0 to 1, 1 to 1, 17 to 5,
            ),
        )
        assertDecodeFails(
            "identity group 0",
            rawCode(
                *header(count = 1),
                4 to 3, 0 to 1, 0 to 2, 0 to 2, 0 to 1, 0 to 1, 0 to 1, 1 to 1, 0 to 8, 0 to 1,
            ),
        )
        // Group 5 decodes but fails the shared model validation (A..D only).
        assertDecodeFails(
            "group",
            rawCode(
                *header(count = 1),
                4 to 3, 0 to 1, 0 to 2, 0 to 2, 0 to 1, 0 to 1, 0 to 1, 1 to 1, 5 to 8, 0 to 1,
            ),
        )
        // A tier predicate on a wand fails the same model validation.
        assertDecodeFails(
            "Tier",
            rawCode(
                *header(count = 1),
                4 to 3, 0 to 1, 1 to 2, 3 to 3, 0 to 2, 0 to 1, 0 to 1, 0 to 1, 0 to 1, 0 to 1,
            ),
        )
        // Query floor 25 (raw 24) in the header is outside the dungeon.
        assertDecodeFails(
            "outside the dungeon",
            rawCode(1 to 4, 0 to 1, 0 to 1, 0 to 1, 1 to 1, 24 to 5),
        )
        // Nonzero final-byte padding bits are trailing data.
        assertDecodeFails("trailing", rawCode(*pinnedFields, 1 to 1))
    }

    @Test
    fun extractsCodesFromEverySupportedLinkForm() {
        val code = DeepLink.encode(pinnedQuery)
        val link = DeepLink.encodeLink(pinnedQuery)
        assertEquals(code, DeepLink.extractCode(link))
        assertEquals(pinnedQuery.normalized(), DeepLink.decodeText("  $link  ").normalized())
        assertEquals(code, DeepLink.extractCode("https://example.com/?utm=1&q=$code#top"))
        assertEquals(code, DeepLink.extractCode("seedseeker://q/$code"))
        assertEquals(code, DeepLink.extractCode(code))
        assertNull(DeepLink.extractCode(""))
        assertNull(DeepLink.extractCode("https://example.com/"))
        assertThrows(IllegalArgumentException::class.java) {
            DeepLink.decodeText("https://example.com/")
        }
    }

    /**
     * The code tables are part of the persisted link format: entries may be
     * appended, but existing positions must never change. If this test fails,
     * restore the order here and map any new catalog entries to fresh codes
     * at the end of the table. Mirrors the Rust core's
     * `code_tables_are_frozen` test.
     */
    @Test
    fun codeTablesAreFrozen() {
        val expectedItems = listOf(
            "worn_shortsword",
            "cudgel",
            "gloves",
            "rapier",
            "dagger",
            "shortsword",
            "hand_axe",
            "spear",
            "quarterstaff",
            "dirk",
            "sickle",
            "sword",
            "mace",
            "scimitar",
            "round_shield",
            "sai",
            "whip",
            "longsword",
            "battle_axe",
            "flail",
            "runic_blade",
            "assassins_blade",
            "crossbow",
            "katana",
            "greatsword",
            "war_hammer",
            "glaive",
            "greataxe",
            "greatshield",
            "gauntlet",
            "war_scythe",
            "throwing_stone",
            "throwing_knife",
            "throwing_spike",
            "fishing_spear",
            "throwing_club",
            "shuriken",
            "throwing_spear",
            "kunai",
            "bolas",
            "javelin",
            "tomahawk",
            "heavy_boomerang",
            "trident",
            "throwing_hammer",
            "force_cube",
            "cloth_armor",
            "leather_armor",
            "mail_armor",
            "scale_armor",
            "plate_armor",
            "wand_magic_missile",
            "wand_fireblast",
            "wand_frost",
            "wand_lightning",
            "wand_disintegration",
            "wand_prismatic_light",
            "wand_corrosion",
            "wand_living_earth",
            "wand_blast_wave",
            "wand_corruption",
            "wand_warding",
            "wand_regrowth",
            "wand_transfusion",
            "rot_dart",
            "incendiary_dart",
            "adrenaline_dart",
            "healing_dart",
            "chilling_dart",
            "shocking_dart",
            "poison_dart",
            "cleansing_dart",
            "paralytic_dart",
            "holy_dart",
            "displacing_dart",
            "blinding_dart",
            "ring_accuracy",
            "ring_arcana",
            "ring_elements",
            "ring_energy",
            "ring_evasion",
            "ring_force",
            "ring_furor",
            "ring_haste",
            "ring_might",
            "ring_sharpshooting",
            "ring_tenacity",
            "ring_wealth",
        )
        assertEquals(expectedItems, DeepLink.ITEM_CODES)
        // Every catalog item must be representable in a link.
        assertEquals(ItemCatalog.all.map { it.id }.toSet(), DeepLink.ITEM_CODES.toSet())

        val expectedWeaponEffects = listOf(
            "Blazing",
            "Chilling",
            "Kinetic",
            "Shocking",
            "Blocking",
            "Blooming",
            "Elastic",
            "Lucky",
            "Projecting",
            "Unstable",
            "Corrupting",
            "Grim",
            "Vampiric",
            "Annoying",
            "Displacing",
            "Dazzling",
            "Explosive",
            "Sacrificial",
            "Wayward",
            "Polarized",
            "Friendly",
        )
        val expectedArmorEffects = listOf(
            "Obfuscation",
            "Swiftness",
            "Viscosity",
            "Potential",
            "Brimstone",
            "Stone",
            "Entanglement",
            "Repulsion",
            "Camouflage",
            "Flow",
            "Affection",
            "Anti-Magic",
            "Thorns",
            "Anti-Entropy",
            "Corrosion",
            "Displacement",
            "Metabolism",
            "Multiplicity",
            "Stench",
            "Overgrowth",
            "Bulk",
        )
        assertEquals(expectedWeaponEffects, DeepLink.WEAPON_EFFECT_CODES)
        assertEquals(expectedArmorEffects, DeepLink.ARMOR_EFFECT_CODES)
        assertEquals(
            ItemCatalog.modifiersFor(ItemKind.WEAPON).toSet(),
            DeepLink.WEAPON_EFFECT_CODES.toSet(),
        )
        assertEquals(
            ItemCatalog.modifiersFor(ItemKind.ARMOR).toSet(),
            DeepLink.ARMOR_EFFECT_CODES.toSet(),
        )

        val expectedSources = listOf(
            ScoutItemSource.HEAP,
            ScoutItemSource.CHEST,
            ScoutItemSource.LOCKED_CHEST,
            ScoutItemSource.CRYSTAL_CHEST,
            ScoutItemSource.TOMB,
            ScoutItemSource.SKELETON,
            ScoutItemSource.SACRIFICIAL_FIRE,
            ScoutItemSource.MIMIC,
            ScoutItemSource.GOLDEN_MIMIC,
            ScoutItemSource.CRYSTAL_MIMIC,
            ScoutItemSource.STATUE,
            ScoutItemSource.ARMORED_STATUE,
            ScoutItemSource.SHOP,
            ScoutItemSource.GHOST_REWARD,
            ScoutItemSource.WANDMAKER_REWARD,
            ScoutItemSource.BLACKSMITH_REWARD,
            ScoutItemSource.IMP_REWARD,
        )
        assertEquals(expectedSources, DeepLink.SOURCE_CODES)
        assertEquals(ScoutItemSource.entries.toSet(), DeepLink.SOURCE_CODES.toSet())

        // Challenge bits are the upstream mask bits, pinned in mask order.
        assertEquals(List(9) { 1 shl it }, Challenge.entries.map { it.bit })
    }

    private fun minimal(requirement: ItemRequirement) = PresetQuery(requirements = listOf(requirement))

    private fun wildcard(kind: ItemKind, key: Long = 1) =
        ItemRequirement(key, null, 0, kind = kind, upgradeMatch = UpgradeMatch.ANY)

    /** Requirement row keys are session-local and never travel in a link. */
    private fun PresetQuery.normalized() =
        copy(requirements = requirements.map { it.copy(key = 0) })

    private fun assertRoundTrips(query: PresetQuery) {
        assertEquals(query.normalized(), DeepLink.decode(DeepLink.encode(query)).normalized())
    }

    private fun assertDecodeFails(fragment: String, code: String) {
        val failure = assertThrows(IllegalArgumentException::class.java) { DeepLink.decode(code) }
        assertTrue("\"$code\": ${failure.message}", failure.message!!.contains(fragment))
    }

    /** The query header up to and including the requirement count. */
    private fun header(count: Int) =
        arrayOf(1 to 4, 0 to 1, 0 to 1, 0 to 1, 0 to 1, 0 to 1, 0 to 1, count to 6)

    /** Builds a code from explicit `value to widthBits` fields to craft payloads. */
    private fun rawCode(vararg fields: Pair<Int, Int>): String {
        val bits = StringBuilder()
        for ((value, width) in fields) {
            for (offset in width - 1 downTo 0) {
                bits.append((value ushr offset) and 1)
            }
        }
        while (bits.length % 8 != 0) bits.append('0')
        return buildString {
            var index = 0
            while (index < bits.length) {
                val group = bits.substring(index, minOf(index + 6, bits.length)).padEnd(6, '0')
                append(BASE64URL[group.toInt(2)])
                index += 6
            }
        }
    }

    private companion object {
        const val BASE64URL = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
    }
}
