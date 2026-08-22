// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.catalog.PackagedCatalog
import dev.seedseeker.app.engine.EngineInfo
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The results-file format, asserted against the engine that owns it. The codec
 * is `crates/seedfinder-core/src/results_export.rs`, reached through
 * `JniBindings.resultsEncode`/`resultsDecode` (the host build of the Rust
 * library JVM tests load — see QueryContinuationTest), so what these cases pin
 * is the real file format plus this app's mapping between the canonical query
 * document and its own models.
 */
class ResultsExportTest {
    init { PackagedCatalog.install() }

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
     * data so this app can never silently drift from it. It still carries
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
        assertEquals(0, imported.dropped)
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
        assertEquals("0.6.1", document.getString("app_version"))
        assertEquals(EngineInfo.shpdVersion, document.getString("shpd_version"))
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
        assertEquals(EngineInfo.shpdVersion, imported.shpdVersion)
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
    fun decodeReportsWhatDedupeAndCapRemoved() {
        // Dedupe-and-cap is the engine's, and it reports the entries it
        // dropped so the importer can still tell the user.
        val imported = ResultsExport.decode(
            """
            {"format":"seed-seeker-results",
             "query":{"requirements":[{"item":"sword"}]},
             "results":[{"seed":"AAA-AAA-AAB"},{"seed":"AAA-AAA-AAC"},{"seed":"AAA-AAA-AAB"}]}
            """.trimIndent(),
        )
        assertEquals(listOf("AAA-AAA-AAB", "AAA-AAA-AAC"), imported.seeds)
        assertEquals(1, imported.dropped)
    }

    @Test
    fun aMalformedFileSurfacesTheEngineMessage() {
        // Validation belongs to the codec; the app only shows what it says.
        val foreign = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decode("""{"format":"other"}""")
        }
        assertTrue(foreign.message, foreign.message!!.contains("not a Seed Seeker results file"))

        val unknownItem = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decode(
                """
                {"format":"seed-seeker-results",
                 "query":{"requirements":[{"item":"item_from_the_future"}]},"results":[]}
                """.trimIndent(),
            )
        }
        assertTrue(unknownItem.message, unknownItem.message!!.contains("item_from_the_future"))

        val badSeed = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decode(
                """
                {"format":"seed-seeker-results",
                 "query":{"requirements":[{"item":"sword"}]},
                 "results":[{"seed":"AAA-AAA-AAB"},{"seed":"AAA-AAA-AA0"}]}
                """.trimIndent(),
            )
        }
        assertTrue(badSeed.message, badSeed.message!!.contains("result 2"))
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
        assertEquals(EffectFilter.OneOf(listOf("Anti-Magic")), requirements[3].effect)
    }

    @Test
    fun decodeReadsAlternativeGroupsEffectListsAndCombinedUpgradeGroups() {
        val imported = ResultsExport.decode(
            """
            {"format":"seed-seeker-results",
             "query":{"requirements":[
               {"any_of":[{"item":"spear","upgrade":3},
                          {"kind":"thrown_weapon","effect":["projecting","blocking"]}]},
               {"kind":"armor","effect":"ANY_ENCHANTMENT","uncursed":true},
               {"any_of":[{"item":"sword"}]},
               {"item":"ring_might","identity_group":1,"upgrade_sum":{"group":2,"at_least":4}},
               {"item":"ring_might","identity_group":1,"upgrade_sum":{"group":2,"at_least":4}}
             ]},
             "results":[]}
            """.trimIndent(),
        )
        val requirements = imported.query.requirements
        // Groups get fresh sequential ids in document order; a one-member group is a plain row.
        assertEquals(listOf(1, 1, null, null, null, null), requirements.map { it.alternativeGroup })
        assertEquals(5, requirements.slotCount())
        // Effect lists canonicalize to catalog spelling and order.
        assertEquals(EffectFilter.OneOf(listOf("Blocking", "Projecting")), requirements[1].effect)
        assertEquals(ItemKind.THROWN_WEAPON, requirements[1].kind)
        assertEquals(EffectFilter.AnyEnchantment, requirements[2].effect)
        assertEquals(UpgradeSum(group = 2, atLeast = 4), requirements[4].upgradeSum)
        assertEquals(UpgradeSum(group = 2, atLeast = 4), requirements[5].upgradeSum)

        // And the same structures survive a trip back through the engine's writer.
        val reImported = ResultsExport.decode(
            ResultsExport.encode(imported.query, listOf("AAA-AAA-BUH"), "0.6.1"),
        )
        assertEquals(
            requirements.map { it.copy(key = 0) },
            reImported.query.requirements.map { it.copy(key = 0) },
        )
        val document = JSONObject(ResultsExport.encode(imported.query, listOf("AAA-AAA-BUH"), "0.6.1"))
        val entries = document.getJSONObject("query").getJSONArray("requirements")
        assertEquals(5, entries.length())
        assertEquals(2, entries.getJSONObject(0).getJSONArray("any_of").length())
        assertEquals("any_enchantment", entries.getJSONObject(1).getString("effect"))
        assertEquals(4, entries.getJSONObject(3).getJSONObject("upgrade_sum").getInt("at_least"))
    }

    @Test
    fun theEngineRefusesASumInsideAnAlternativeGroup() {
        val failure = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decode(
                """
                {"format":"seed-seeker-results",
                 "query":{"requirements":[{"any_of":[
                   {"item":"ring_might","upgrade_sum":{"group":1,"at_least":2}},
                   {"item":"sword"}]}]},
                 "results":[]}
                """.trimIndent(),
            )
        }
        assertTrue(failure.message, failure.message!!.contains("alternative"))
    }

    @Test
    fun aCategorylessMemberOfAGroupIsReportedAtTheGroupsDocumentPosition() {
        // The app's own message for an entry the core accepted but this side
        // cannot place uses the 1-based document entry, group members included.
        val failure = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decodeQuery(
                JSONObject(
                    """{"requirements":[{"any_of":[{"item":"sword"},{"item":"spear"}]},
                                        {"any_of":[{"item":"sword"},{"tier":{"exact":2}}]}]}""",
                ),
            )
        }
        assertEquals("Requirement 2 has no category.", failure.message)
    }
}
