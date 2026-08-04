// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class ResultsExportTest {
    private val loadedQuery = PresetQuery(
        requirements = listOf(
            ItemRequirement(
                key = 1,
                item = ItemCatalog.findById("ring_tenacity"),
                upgrade = 4,
                kind = ItemKind.RING,
                upgradeMatch = UpgradeMatch.EXACT,
                source = ScoutItemSource.IMP_REWARD,
            ),
            ItemRequirement(
                key = 2,
                item = null,
                upgrade = 2,
                kind = ItemKind.WAND,
                upgradeMatch = UpgradeMatch.AT_LEAST,
                identityGroup = 1,
                maximumDepth = 9,
                requireUncursed = true,
            ),
        ),
        maximumDepth = 12,
        requireBlacksmith = true,
        challenges = Challenge.NO_HERBALISM.bit,
    )

    /**
     * The canonical frozen fixture, read straight from the Rust core's test
     * data so this codec can never silently drift from it. It still carries
     * the `"format_version": 1` older releases wrote: files exported by an
     * older release must always stay readable; never edit the fixture.
     * Gradle runs unit tests with the module directory as the working
     * directory, so the repository root is two levels up.
     */
    private val version1Fixture: String by lazy {
        val fixture = java.io.File(
            "../../crates/seedfinder-core/tests/fixtures/results-export-v1.json",
        )
        check(fixture.exists()) { "canonical fixture not found at ${fixture.absolutePath}" }
        fixture.readText()
    }

    @Test
    fun encodeThenDecodeRoundTripsQueryAndSeeds() {
        val text = ResultsExport.encode(loadedQuery, listOf("AAA-AAA-BUH", "ABC-DEF-GHI"), "0.6.1")
        val imported = ResultsExport.decode(text)
        assertEquals(listOf("AAA-AAA-BUH", "ABC-DEF-GHI"), imported.seeds)
        assertEquals(loadedQuery.maximumDepth, imported.query.maximumDepth)
        assertEquals(loadedQuery.challenges, imported.query.challenges)
        assertEquals(loadedQuery.requireBlacksmith, imported.query.requireBlacksmith)
        // Requirements compare equal except for the session-local row keys.
        assertEquals(
            loadedQuery.requirements.map { it.copy(key = 0) },
            imported.query.requirements.map { it.copy(key = 0) },
        )
    }

    @Test
    fun encodeEmitsTheDocumentedEnvelopeAndMinimalQuery() {
        val document = JSONObject(ResultsExport.encode(loadedQuery, listOf("AAA-AAA-BUH"), "0.6.1"))
        assertEquals("seed-seeker-results", document.getString("format"))
        assertFalse(document.has("format_version"))
        assertEquals("0.6.1", document.getString("app_version"))
        assertEquals("3.3.8", document.getString("shpd_version"))
        assertEquals(1, document.getJSONArray("results").length())
        assertEquals(
            "AAA-AAA-BUH",
            document.getJSONArray("results").getJSONObject(0).getString("seed"),
        )
        val query = document.getJSONObject("query")
        assertEquals(12, query.getInt("max_depth"))
        assertTrue(query.getBoolean("require_blacksmith"))
        assertEquals("barren_land", query.getJSONArray("challenges").getString(0))
        val first = query.getJSONArray("requirements").getJSONObject(0)
        assertEquals("ring", first.getString("kind"))
        assertEquals("ring_tenacity", first.getString("item"))
        assertEquals(4, first.getInt("upgrade"))
        assertEquals("imp_reward", first.getString("source"))
        val second = query.getJSONArray("requirements").getJSONObject(1)
        assertEquals(2, second.getJSONObject("upgrade").getInt("at_least"))
        assertTrue(second.getBoolean("uncursed"))
        assertEquals(1, second.getInt("identity_group"))
        assertEquals(9, second.getInt("max_depth"))
    }

    @Test
    fun version1FixtureAlwaysDecodes() {
        val imported = ResultsExport.decode(version1Fixture)
        assertEquals(listOf("AAA-AAA-BUH", "ABC-DEF-GHI"), imported.seeds)
        assertEquals("3.3.8", imported.shpdVersion)
        assertEquals(12, imported.query.maximumDepth)
        assertEquals(Challenge.NO_HERBALISM.bit, imported.query.challenges)
        assertEquals("ring_tenacity", imported.query.requirements[0].item?.id)
        assertEquals(ItemKind.WAND, imported.query.requirements[1].kind)
        assertEquals(UpgradeMatch.AT_LEAST, imported.query.requirements[1].upgradeMatch)
    }

    /** Another frozen document, pinning the narrowed weapon kinds. */
    private val weaponCategoriesFixture: String by lazy {
        val fixture = java.io.File(
            "../../crates/seedfinder-core/tests/fixtures/results-export-v1-weapon-categories.json",
        )
        check(fixture.exists()) { "weapon-categories fixture not found at ${fixture.absolutePath}" }
        fixture.readText()
    }

    @Test
    fun weaponCategoryFixtureDecodesAndRoundTrips() {
        val imported = ResultsExport.decode(weaponCategoriesFixture)
        assertEquals(
            listOf(ItemKind.THROWN_WEAPON, ItemKind.MELEE_WEAPON, ItemKind.WEAPON),
            imported.query.requirements.map { it.kind },
        )
        assertEquals("sword", imported.query.requirements[1].item?.id)
        assertEquals(listOf("AAA-AAA-ACO"), imported.seeds)

        // Re-encoding must keep the narrowing: widening "thrown_weapon" back
        // to "weapon" would silently change the query's meaning on import.
        val reImported = ResultsExport.decode(
            ResultsExport.encode(imported.query, imported.seeds, "0.6.1"),
        )
        assertEquals(
            imported.query.requirements.map { it.copy(key = 0) },
            reImported.query.requirements.map { it.copy(key = 0) },
        )
    }

    /** The frozen quest fixture: the same shared file, with a quest. */
    private val wandmakerQuestFixture: String by lazy {
        val fixture = java.io.File(
            "../../crates/seedfinder-core/tests/fixtures/results-export-wandmaker-quest.json",
        )
        check(fixture.exists()) { "quest fixture not found at ${fixture.absolutePath}" }
        fixture.readText()
    }

    @Test
    fun wandmakerQuestFixtureCarriesTheQuest() {
        val imported = ResultsExport.decode(wandmakerQuestFixture)
        assertEquals(WandmakerQuest.ROTBERRY, imported.query.wandmakerQuest)
        assertEquals(9, imported.query.maximumDepth)
        assertEquals(listOf("AAA-AAA-BUH", "ABC-DEF-GHI"), imported.seeds)

        // Re-encoding keeps the quest.
        val document = JSONObject(ResultsExport.encode(imported.query, imported.seeds, "0.6.1"))
        assertEquals("rotberry", document.getJSONObject("query").getString("wandmaker_quest"))
    }

    @Test
    fun unknownWandmakerQuestIsRejected() {
        val failure = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decode(
                """
                {"format":"seed-seeker-results",
                 "query":{"requirements":[{"item":"sword"}],"wandmaker_quest":"moon_cheese"},
                 "results":[]}
                """.trimIndent(),
            )
        }
        assertTrue(failure.message!!.contains("moon_cheese"))
    }

    @Test
    fun unknownEnvelopeAndResultFieldsAreIgnored() {
        val imported = ResultsExport.decode(
            """
            {
              "format": "seed-seeker-results",
              "format_version": 1,
              "exported_at": "2031-01-01T00:00:00Z",
              "future_minor_field": {"nested": true},
              "query": {"requirements": [{"item": "sword"}]},
              "results": [{"seed": "AAA-AAA-AAB", "future_note": "still fine"}]
            }
            """.trimIndent(),
        )
        assertEquals(listOf("AAA-AAA-AAB"), imported.seeds)
        assertEquals(24, imported.query.maximumDepth)
    }

    @Test
    fun anyDeclaredFormatVersionIsIgnored() {
        // The number carried no meaning for a reader newer than the file, so
        // it is now just another unknown envelope field.
        for (version in listOf("1", "2", "99", "0", "1.5", "true", "\"1\"", "-1")) {
            val imported = ResultsExport.decode(
                """{"format":"seed-seeker-results","format_version":$version,
                   "query":{"requirements":[{"item":"sword"}]},
                   "results":[{"seed":"AAA-AAA-AAB"}]}""",
            )
            assertEquals(version, listOf("AAA-AAA-AAB"), imported.seeds)
        }
    }

    @Test
    fun foreignAndMalformedFilesAreRejectedClearly() {
        for (text in listOf("not json", "[]", "{}", """{"format":"other"}""")) {
            val failure = assertThrows(IllegalArgumentException::class.java) {
                ResultsExport.decode(text)
            }
            assertTrue(failure.message!!.contains("not a Seed Seeker results file"))
        }
    }

    @Test
    fun unknownQueryContentFailsInsteadOfChangingMeaning() {
        val unknownItem = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decode(
                """
                {"format":"seed-seeker-results",
                 "query":{"requirements":[{"item":"item_from_the_future"}]},"results":[]}
                """.trimIndent(),
            )
        }
        assertTrue(unknownItem.message!!.contains("item_from_the_future"))

        val unknownField = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decode(
                """
                {"format":"seed-seeker-results",
                 "query":{"requirements":[{"item":"sword"}],"wished_luck":7},"results":[]}
                """.trimIndent(),
            )
        }
        assertTrue(unknownField.message!!.contains("wished_luck"))
    }

    @Test
    fun invalidSeedCodesNameTheOffendingResult() {
        val failure = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decode(
                """
                {"format":"seed-seeker-results",
                 "query":{"requirements":[{"item":"sword"}]},
                 "results":[{"seed":"AAA-AAA-AAB"},{"seed":"AAA-AAA-AA0"}]}
                """.trimIndent(),
            )
        }
        assertTrue(failure.message!!.contains("Result 2"))
    }

    @Test
    fun wrongTypedQueryFieldsAreRejectedNotCoerced() {
        val payloads = listOf(
            """{"requirements":[{"item":"sword"}],"max_depth":"12"}""",
            """{"requirements":[{"item":"sword"}],"max_depth":99}""",
            """{"requirements":[{"item":42}]}""",
            """{"requirements":[{"item":""}]}""",
            """{"requirements":[{"item":"sword"}],"challenges":"barren_land"}""",
            """{"requirements":[{"item":"sword","upgrade":true}]}""",
            """{"requirements":[{"item":"sword","uncursed":"yes"}]}""",
            """{"requirements":[{"kind":"RING"}]}""",
        )
        for (query in payloads) {
            assertThrows(query, IllegalArgumentException::class.java) {
                ResultsExport.decode(
                    """{"format":"seed-seeker-results",
                       "query":$query,"results":[]}""",
                )
            }
        }
    }

    @Test
    fun onlyCanonicalSeedCodesAreAccepted() {
        for (seed in listOf("aaa-aaa-aab", "AAAAAAAAB", "AAA AAA AAB", " AAA-AAA-AAB")) {
            val failure = assertThrows(IllegalArgumentException::class.java) {
                ResultsExport.decode(
                    """{"format":"seed-seeker-results",
                       "query":{"requirements":[{"item":"sword"}]},
                       "results":[{"seed":"$seed"}]}""",
                )
            }
            assertTrue("$seed: ${failure.message}", failure.message!!.contains("Result 1"))
        }
    }

    @Test
    fun decodeAcceptsAllCoreTierAndUpgradeForms() {
        val imported = ResultsExport.decode(
            """
            {"format":"seed-seeker-results",
             "query":{"requirements":[
               {"kind":"weapon","tier":"any","upgrade":"any"},
               {"kind":"weapon","tier":{"exact":2},"upgrade":{"exact":3}},
               {"kind":"armor","tier":{"at_least":3},"upgrade":{"at_least":1}},
               {"kind":"armor","tier":{"at_most":4},"effect":"anti-magic"}
             ]},
             "results":[]}
            """.trimIndent(),
        )
        val requirements = imported.query.requirements
        assertEquals(TierMatch.ANY, requirements[0].tierMatch)
        assertEquals(UpgradeMatch.ANY, requirements[0].upgradeMatch)
        assertEquals(TierMatch.EXACT, requirements[1].tierMatch)
        assertEquals(2, requirements[1].tier)
        assertEquals(UpgradeMatch.EXACT, requirements[1].upgradeMatch)
        assertEquals(3, requirements[1].upgrade)
        assertEquals(TierMatch.AT_LEAST, requirements[2].tierMatch)
        assertEquals(UpgradeMatch.AT_LEAST, requirements[2].upgradeMatch)
        assertEquals(TierMatch.AT_MOST, requirements[3].tierMatch)
        // Effect matching is case-insensitive and canonicalizes to the catalog name.
        assertEquals("Anti-Magic", requirements[3].modifier)
    }
}
