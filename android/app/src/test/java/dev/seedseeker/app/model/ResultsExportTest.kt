// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import org.json.JSONObject
import org.junit.Assert.assertEquals
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
     * The canonical frozen version-1 fixture, read straight from the Rust
     * core's test data so this codec can never silently drift from it. Files
     * exported today must always stay readable; never edit the fixture.
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
        assertEquals(1, document.getInt("format_version"))
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

    /** The narrowed weapon kinds are additive within format version 1. */
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
    fun futureFormatVersionsFailWithAnUpdateMessage() {
        val failure = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decode(
                """{"format":"seed-seeker-results","format_version":2,"query":{"requirements":[]},"results":[]}""",
            )
        }
        assertTrue(failure.message!!.contains("format version 2"))
        assertTrue(failure.message!!.contains("Update Seed Seeker"))
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
                {"format":"seed-seeker-results","format_version":1,
                 "query":{"requirements":[{"item":"item_from_the_future"}]},"results":[]}
                """.trimIndent(),
            )
        }
        assertTrue(unknownItem.message!!.contains("item_from_the_future"))

        val unknownField = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decode(
                """
                {"format":"seed-seeker-results","format_version":1,
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
                {"format":"seed-seeker-results","format_version":1,
                 "query":{"requirements":[{"item":"sword"}]},
                 "results":[{"seed":"AAA-AAA-AAB"},{"seed":"AAA-AAA-AA0"}]}
                """.trimIndent(),
            )
        }
        assertTrue(failure.message!!.contains("Result 2"))
    }

    @Test
    fun formatVersionMustBeAPositiveInteger() {
        for (version in listOf("0", "1.5", "true", "\"1\"", "-1")) {
            val failure = assertThrows(IllegalArgumentException::class.java) {
                ResultsExport.decode(
                    """{"format":"seed-seeker-results","format_version":$version,
                       "query":{"requirements":[{"item":"sword"}]},"results":[]}""",
                )
            }
            assertTrue("$version: ${failure.message}", failure.message!!.contains("format version"))
        }
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
                    """{"format":"seed-seeker-results","format_version":1,
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
                    """{"format":"seed-seeker-results","format_version":1,
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
            {"format":"seed-seeker-results","format_version":1,
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
        assertEquals(EffectRequirement.OneOf(listOf("Anti-Magic")), requirements[3].effect)
    }

    @Test
    fun anyOfGroupsRoundTripWithFreshSequentialIds() {
        val query = PresetQuery(
            requirements = listOf(
                ItemRequirement(
                    key = 1,
                    item = ItemCatalog.findById("spear"),
                    upgrade = 3,
                    upgradeMatch = UpgradeMatch.EXACT,
                    alternativeGroup = 7,
                ),
                ItemRequirement(
                    key = 2,
                    item = ItemCatalog.findById("shuriken"),
                    upgrade = 2,
                    upgradeMatch = UpgradeMatch.EXACT,
                    alternativeGroup = 7,
                ),
                ItemRequirement(3, null, 0, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.ANY),
            ),
        )
        val text = ResultsExport.encode(query, listOf("AAA-AAA-BUH"), "0.6.1")
        // The group serializes as one any_of entry at its first member's
        // position, holding the members in requirement order.
        val entries = JSONObject(text).getJSONObject("query").getJSONArray("requirements")
        assertEquals(2, entries.length())
        val anyOf = entries.getJSONObject(0).getJSONArray("any_of")
        assertEquals(2, anyOf.length())
        assertEquals("spear", anyOf.getJSONObject(0).getString("item"))
        assertEquals("shuriken", anyOf.getJSONObject(1).getString("item"))

        val imported = ResultsExport.decode(text).query
        assertEquals(listOf(1, 1, null), imported.requirements.map { it.alternativeGroup })
        assertEquals(
            query.requirements.map { it.copy(key = 0, alternativeGroup = null) },
            imported.requirements.map { it.copy(key = 0, alternativeGroup = null) },
        )
    }

    @Test
    fun singleMemberAlternativeGroupsCollapseToPlainRequirements() {
        val query = PresetQuery(
            requirements = listOf(
                ItemRequirement(1, ItemCatalog.findById("sword"), 1, alternativeGroup = 3),
            ),
        )
        val text = ResultsExport.encode(query, listOf("AAA-AAA-BUH"), "0.6.1")
        val entries = JSONObject(text).getJSONObject("query").getJSONArray("requirements")
        assertTrue(!entries.getJSONObject(0).has("any_of"))
        // A one-member "any_of" also decodes as no group at all.
        val imported = ResultsExport.decode(
            """{"format":"seed-seeker-results","format_version":1,
               "query":{"requirements":[{"any_of":[{"item":"sword","upgrade":1}]}]},
               "results":[]}""",
        )
        assertEquals(listOf<Int?>(null), imported.query.requirements.map { it.alternativeGroup })
    }

    @Test
    fun effectSetsAndAnyEnchantmentRoundTrip() {
        val query = PresetQuery(
            requirements = listOf(
                ItemRequirement(
                    key = 1,
                    item = null,
                    upgrade = 0,
                    kind = ItemKind.WEAPON,
                    upgradeMatch = UpgradeMatch.ANY,
                    effect = EffectRequirement.OneOf(listOf("Blazing", "Vampiric", "Grim")),
                ),
                ItemRequirement(
                    key = 2,
                    item = null,
                    upgrade = 0,
                    kind = ItemKind.ARMOR,
                    upgradeMatch = UpgradeMatch.ANY,
                    effect = EffectRequirement.AnyEnchantment,
                ),
                ItemRequirement(
                    key = 3,
                    item = null,
                    upgrade = 0,
                    kind = ItemKind.WEAPON,
                    upgradeMatch = UpgradeMatch.ANY,
                    effect = EffectRequirement.OneOf(listOf("Lucky")),
                ),
            ),
        )
        val text = ResultsExport.encode(query, listOf("AAA-AAA-BUH"), "0.6.1")
        val entries = JSONObject(text).getJSONObject("query").getJSONArray("requirements")
        assertEquals(3, entries.getJSONObject(0).getJSONArray("effect").length())
        assertEquals("any_enchantment", entries.getJSONObject(1).getString("effect"))
        assertEquals("Lucky", entries.getJSONObject(2).getString("effect"))
        val imported = ResultsExport.decode(text).query
        assertEquals(
            query.requirements.map { it.copy(key = 0) },
            imported.requirements.map { it.copy(key = 0) },
        )
    }

    @Test
    fun fullNonCurseEffectSetsUseAndDecodeTheAnyEnchantmentShorthand() {
        val query = PresetQuery(
            requirements = listOf(
                ItemRequirement(
                    key = 1,
                    item = null,
                    upgrade = 0,
                    kind = ItemKind.WEAPON,
                    upgradeMatch = UpgradeMatch.ANY,
                    effect = EffectRequirement.OneOf(ItemCatalog.enchantments),
                ),
            ),
        )
        val text = ResultsExport.encode(query, listOf("AAA-AAA-BUH"), "0.6.1")
        val entry = JSONObject(text).getJSONObject("query").getJSONArray("requirements")
            .getJSONObject(0)
        assertEquals("any_enchantment", entry.getString("effect"))
        assertEquals(
            EffectRequirement.AnyEnchantment,
            ResultsExport.decode(text).query.requirements.single().effect,
        )
    }

    @Test
    fun effectNamesMatchCaseInsensitivelyAndBadOnesAreRejected() {
        val imported = ResultsExport.decode(
            """{"format":"seed-seeker-results","format_version":1,
               "query":{"requirements":[{"kind":"thrown_weapon","effect":["blazing","PROJECTING"]}]},
               "results":[]}""",
        )
        assertEquals(
            EffectRequirement.OneOf(listOf("Blazing", "Projecting")),
            imported.query.requirements.single().effect,
        )
        for (payload in listOf(
            """{"requirements":[{"kind":"weapon","effect":[]}]}""",
            """{"requirements":[{"kind":"weapon","effect":"Thorns"}]}""",
            """{"requirements":[{"kind":"weapon","effect":["Blazing","Thorns"]}]}""",
            """{"requirements":[{"kind":"ring","effect":"any_enchantment"}]}""",
            """{"requirements":[{"kind":"weapon","effect":7}]}""",
        )) {
            assertThrows(payload, IllegalArgumentException::class.java) {
                ResultsExport.decode(
                    """{"format":"seed-seeker-results","format_version":1,
                       "query":$payload,"results":[]}""",
                )
            }
        }
    }

    @Test
    fun upgradeSumsRoundTrip() {
        val query = PresetQuery(
            requirements = listOf(
                ItemRequirement(
                    key = 1,
                    item = ItemCatalog.findById("ring_might"),
                    upgrade = 0,
                    upgradeMatch = UpgradeMatch.ANY,
                    identityGroup = 1,
                    upgradeSumGroup = 2,
                    upgradeSumTotal = 5,
                ),
                ItemRequirement(
                    key = 2,
                    item = ItemCatalog.findById("ring_might"),
                    upgrade = 0,
                    upgradeMatch = UpgradeMatch.ANY,
                    identityGroup = 1,
                    upgradeSumGroup = 2,
                    upgradeSumTotal = 5,
                ),
            ),
        )
        val text = ResultsExport.encode(query, listOf("AAA-AAA-BUH"), "0.6.1")
        val entry = JSONObject(text).getJSONObject("query").getJSONArray("requirements")
            .getJSONObject(0).getJSONObject("upgrade_sum")
        assertEquals(2, entry.getInt("group"))
        assertEquals(5, entry.getInt("at_least"))
        val imported = ResultsExport.decode(text).query
        assertEquals(
            query.requirements.map { it.copy(key = 0) },
            imported.requirements.map { it.copy(key = 0) },
        )
    }

    @Test
    fun upgradeSumInsideAnyOfIsRejected() {
        val failure = assertThrows(IllegalArgumentException::class.java) {
            ResultsExport.decode(
                """{"format":"seed-seeker-results","format_version":1,
                   "query":{"requirements":[{"any_of":[
                     {"item":"ring_might","upgrade_sum":{"group":1,"at_least":2}},
                     {"item":"ring_haste"}
                   ]}]},
                   "results":[]}""",
            )
        }
        assertTrue(failure.message!!.contains("any_of"))
    }

    @Test
    fun malformedGroupsAndSumsAreRejected() {
        val payloads = listOf(
            // Empty and non-object any_of members, unknown sibling fields.
            """{"requirements":[{"any_of":[]}]}""",
            """{"requirements":[{"any_of":["sword"]}]}""",
            """{"requirements":[{"any_of":[{"item":"sword"}],"extra":1}]}""",
            // Nested groups are not representable.
            """{"requirements":[{"any_of":[{"any_of":[{"item":"sword"}]}]}]}""",
            // Unknown or missing upgrade_sum fields, out-of-range values.
            """{"requirements":[{"item":"ring_might","upgrade_sum":{"group":1}}]}""",
            """{"requirements":[{"item":"ring_might","upgrade_sum":{"group":1,"at_least":2,"bonus":3}}]}""",
            """{"requirements":[{"item":"ring_might","upgrade_sum":7}]}""",
            """{"requirements":[{"item":"ring_might","upgrade_sum":{"group":0,"at_least":2}}]}""",
            """{"requirements":[{"item":"ring_might","upgrade_sum":{"group":1,"at_least":9}}]}""",
            // Members of one group must agree on the total.
            """{"requirements":[
                {"item":"ring_might","upgrade_sum":{"group":1,"at_least":2}},
                {"item":"ring_might","upgrade_sum":{"group":1,"at_least":3}}
            ]}""",
        )
        for (payload in payloads) {
            assertThrows(payload, IllegalArgumentException::class.java) {
                ResultsExport.decode(
                    """{"format":"seed-seeker-results","format_version":1,
                       "query":$payload,"results":[]}""",
                )
            }
        }
    }

    @Test
    fun narrowedWeaponKindsRoundTripWithEffectsAndTiers() {
        val query = PresetQuery(
            requirements = listOf(
                ItemRequirement(
                    key = 1,
                    item = null,
                    upgrade = 0,
                    kind = ItemKind.MELEE_WEAPON,
                    tier = 4,
                    tierMatch = TierMatch.AT_LEAST,
                    upgradeMatch = UpgradeMatch.ANY,
                    effect = EffectRequirement.OneOf(listOf("Grim")),
                ),
                ItemRequirement(
                    key = 2,
                    item = null,
                    upgrade = 1,
                    kind = ItemKind.THROWN_WEAPON,
                    upgradeMatch = UpgradeMatch.AT_LEAST,
                    effect = EffectRequirement.AnyEnchantment,
                ),
            ),
        )
        val text = ResultsExport.encode(query, listOf("AAA-AAA-BUH"), "0.6.1")
        val entries = JSONObject(text).getJSONObject("query").getJSONArray("requirements")
        assertEquals("melee_weapon", entries.getJSONObject(0).getString("kind"))
        assertEquals("thrown_weapon", entries.getJSONObject(1).getString("kind"))
        val imported = ResultsExport.decode(text).query
        assertEquals(
            query.requirements.map { it.copy(key = 0) },
            imported.requirements.map { it.copy(key = 0) },
        )
    }
}
