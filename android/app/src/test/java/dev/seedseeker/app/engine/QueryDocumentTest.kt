// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.catalog.PackagedCatalog
import dev.seedseeker.app.model.EffectFilter
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutItemSource
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.TierMatch
import dev.seedseeker.app.model.UpgradeMatch
import dev.seedseeker.app.model.LevelSum
import dev.seedseeker.app.model.WandmakerQuest
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The one query encoder: every query-taking engine call sends the canonical
 * JSON query document (docs/results-export-format.md). The golden cases pin
 * the writer rules every platform shares — omitted defaults, the bare-number
 * upgrade, one `any_of` per alternative group, the effect forms — and the
 * engine-backed cases hand the bytes to the real library through
 * `JniBindings.queryContinues` / `scoutMatches`, which decode them with the
 * same codec the search uses.
 */
class QueryDocumentTest {
    init { PackagedCatalog.install() }

    private val sword = ItemCatalog.weapons.first { it.id == "sword" }

    @Test
    fun aMinimalRequirementWritesOnlyItsKind() {
        val requirement = ItemRequirement(
            key = 1,
            item = null,
            upgrade = 0,
            kind = ItemKind.WEAPON,
            tier = 5,
            tierMatch = TierMatch.EXACT,
            upgradeMatch = UpgradeMatch.ANY,
        )
        assertDocument(
            """{"requirements":[{"kind":"weapon","tier":{"exact":5}}]}""",
            SearchRequest(listOf(requirement)),
        )
        assertDocument(
            """{"requirements":[{"kind":"melee_weapon"}]}""",
            SearchRequest(listOf(requirement.copy(kind = ItemKind.MELEE_WEAPON, tier = 0, tierMatch = TierMatch.ANY))),
        )
    }

    @Test
    fun scopeFlagsAndRequirementFieldsUseTheDocumentedNames() {
        val request = SearchRequest(
            requirements = listOf(
                ItemRequirement(
                    key = 9,
                    item = sword,
                    upgrade = 2,
                    effect = EffectFilter.named("Lucky"),
                    source = ScoutItemSource.SACRIFICIAL_FIRE,
                    identityGroup = 2,
                    maximumDepth = 5,
                    requireUncursed = true,
                ),
                ItemRequirement(
                    key = 10,
                    item = null,
                    kind = ItemKind.RING,
                    upgrade = 3,
                    upgradeMatch = UpgradeMatch.AT_LEAST,
                ),
            ),
            maximumDepth = 14,
            requireBlacksmith = true,
            excludeBlacksmithRewards = true,
            fastMode = true,
            wandmakerQuest = WandmakerQuest.ROTBERRY,
            challenges = 257,
        )
        assertDocument(
            """{"requirements":[
                 {"kind":"weapon","item":"sword","upgrade":2,"effect":"Lucky","uncursed":true,
                  "source":"sacrificial_fire","identity_group":2,"max_depth":5},
                 {"kind":"ring","upgrade":{"at_least":3}}],
               "max_depth":14,"require_blacksmith":true,"exclude_blacksmith_rewards":true,
               "wandmaker_quest":"rotberry","fast_mode":true,
               "challenges":["on_diet","badder_bosses"]}""",
            request,
        )
    }

    @Test
    fun anAlternativeGroupIsOneAnyOfEntryAtItsFirstMembersPosition() {
        val request = SearchRequest(
            listOf(
                ItemRequirement(key = 1, item = ItemCatalog.findById("spear"), upgrade = 3, alternativeGroup = 7),
                ItemRequirement(key = 2, item = ItemCatalog.findById("ring_haste"), upgrade = 1),
                ItemRequirement(
                    key = 3,
                    item = ItemCatalog.findById("shuriken"),
                    upgrade = 2,
                    kind = ItemKind.THROWN_WEAPON,
                    alternativeGroup = 7,
                ),
                // A group of one is a plain requirement.
                ItemRequirement(key = 4, item = sword, upgrade = 1, alternativeGroup = 8),
            ),
        )
        assertDocument(
            """{"requirements":[
                 {"any_of":[{"kind":"weapon","item":"spear","upgrade":3},
                            {"kind":"thrown_weapon","item":"shuriken","upgrade":2}]},
                 {"kind":"ring","item":"ring_haste","upgrade":1},
                 {"kind":"weapon","item":"sword","upgrade":1}]}""",
            request,
        )
    }

    @Test
    fun effectsWriteABareNameAListInCatalogOrderOrTheAnyEnchantmentShorthand() {
        fun armor(effect: EffectFilter, uncursed: Boolean = false) = SearchRequest(
            listOf(
                ItemRequirement(
                    key = 1,
                    item = null,
                    kind = ItemKind.ARMOR,
                    upgrade = 0,
                    upgradeMatch = UpgradeMatch.ANY,
                    effect = effect,
                    requireUncursed = uncursed,
                ),
            ),
        )
        assertDocument("""{"requirements":[{"kind":"armor"}]}""", armor(EffectFilter.Any))
        assertDocument(
            """{"requirements":[{"kind":"armor","effect":"Thorns"}]}""",
            armor(EffectFilter.OneOf(listOf("Thorns"))),
        )
        // Members are written in catalog order whatever order they were picked in.
        assertDocument(
            """{"requirements":[{"kind":"armor","effect":["Anti-Magic","Thorns","Stench"]}]}""",
            armor(EffectFilter.OneOf(listOf("Thorns", "Stench", "Anti-Magic"))),
        )
        assertDocument(
            """{"requirements":[{"kind":"armor","effect":"any_enchantment","uncursed":true}]}""",
            armor(EffectFilter.AnyEnchantment, uncursed = true),
        )
        // Picking every glyph is the shorthand too.
        assertDocument(
            """{"requirements":[{"kind":"armor","effect":"any_enchantment"}]}""",
            armor(EffectFilter.OneOf(ItemCatalog.glyphs)),
        )
        // A mixed set with uncursed is fine: only its good members can match.
        assertDocument(
            """{"requirements":[{"kind":"armor","effect":["Thorns","Stench"],"uncursed":true}]}""",
            armor(EffectFilter.OneOf(listOf("Stench", "Thorns")), uncursed = true),
        )
    }

    @Test
    fun combinedLevelGroupsWriteGroupAndTotal() {
        val might = ItemCatalog.findById("ring_might")
        val request = SearchRequest(
            listOf(
                ItemRequirement(
                    key = 1,
                    item = might,
                    upgrade = 0,
                    upgradeMatch = UpgradeMatch.ANY,
                    maximumDepth = 4,
                    levelSum = LevelSum(group = 1, atLeast = 4),
                ),
                ItemRequirement(
                    key = 2,
                    item = might,
                    upgrade = 0,
                    upgradeMatch = UpgradeMatch.ANY,
                    maximumDepth = 4,
                    levelSum = LevelSum(group = 1, atLeast = 4),
                ),
            ),
        )
        assertDocument(
            """{"requirements":[
                 {"kind":"ring","item":"ring_might","max_depth":4,
                  "level_sum":{"group":1,"at_least":4}},
                 {"kind":"ring","item":"ring_might","max_depth":4,
                  "level_sum":{"group":1,"at_least":4}}]}""",
            request,
        )
    }

    /** The engine decodes what this app writes: every new structure survives the real codec. */
    @Test
    fun theEngineAcceptsEveryDocumentTheAppWrites() {
        val full = SearchRequest(
            listOf(
                ItemRequirement(key = 1, item = ItemCatalog.findById("spear"), upgrade = 3, alternativeGroup = 1),
                ItemRequirement(
                    key = 2,
                    item = ItemCatalog.findById("greatshield"),
                    upgrade = 2,
                    effect = EffectFilter.OneOf(listOf("Blocking", "Projecting", "Vampiric")),
                    alternativeGroup = 1,
                ),
                ItemRequirement(
                    key = 3,
                    item = null,
                    kind = ItemKind.ARMOR,
                    upgrade = 0,
                    upgradeMatch = UpgradeMatch.ANY,
                    effect = EffectFilter.AnyEnchantment,
                    requireUncursed = true,
                ),
                ItemRequirement(
                    key = 4,
                    item = ItemCatalog.findById("ring_might"),
                    upgrade = 0,
                    upgradeMatch = UpgradeMatch.ANY,
                    levelSum = LevelSum(group = 1, atLeast = 4),
                ),
                ItemRequirement(
                    key = 5,
                    item = ItemCatalog.findById("ring_might"),
                    upgrade = 0,
                    upgradeMatch = UpgradeMatch.ANY,
                    levelSum = LevelSum(group = 1, atLeast = 4),
                ),
            ),
        )
        val bytes = QueryDocument.encode(full)
        assertTrue(bytes.first() == '{'.code.toByte())
        // An identical query always continues itself; the engine had to decode
        // both documents — groups, sums and effect sets included — to say so.
        assertTrue(JniBindings.queryContinues(bytes, bytes))
        val marks = JSONObject(
            String(JniBindings.scoutMatches(ScoutRequestCodec.encode("AAA-AAA-BUH", 0), bytes), Charsets.UTF_8),
        )
        // Three conditions: the alternative group counts once, and so does the
        // whole combined-level group — its members stand or fall together.
        assertEquals(3, marks.getInt("totalRequirements"))
    }

    /** What the app refuses to build, and what the engine refuses when asked directly. */
    @Test
    fun invalidCombinationsAreRefusedBeforeAndAtTheEngine() {
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(key = 1, item = sword, upgrade = 1, effect = EffectFilter.named("Displacing"), requireUncursed = true)
        }
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(key = 1, item = sword, upgrade = 1, effect = EffectFilter.OneOf(listOf("Thorns")))
        }
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(key = 1, item = null, kind = ItemKind.WAND, upgrade = 1, effect = EffectFilter.AnyEnchantment)
        }
        assertThrows(IllegalArgumentException::class.java) {
            ItemRequirement(key = 1, item = sword, upgrade = 1, alternativeGroup = 1, levelSum = LevelSum(1, 1))
        }
        val valid = QueryDocument.encode(SearchRequest(listOf(ItemRequirement(key = 1, item = sword, upgrade = 1))))
        for (rejected in listOf(
            // A sum inside an alternative group.
            """{"requirements":[{"any_of":[{"item":"ring_might","level_sum":{"group":1,"at_least":2}},{"item":"sword"}]}]}""",
            // Members disagreeing on the total.
            """{"requirements":[{"item":"ring_might","level_sum":{"group":1,"at_least":2}},{"item":"ring_might","level_sum":{"group":1,"at_least":3}}]}""",
            // An unattainable total.
            """{"requirements":[{"item":"ring_might","level_sum":{"group":1,"at_least":9}}]}""",
            // A sum outside rings.
            """{"requirements":[{"item":"sword","level_sum":{"group":1,"at_least":2}}]}""",
            // A total past what a world generates: only one ring passes +2,
            // so three rings stop at 11, not 15.
            """{"requirements":[{"item":"ring_might","level_sum":{"group":1,"at_least":12}},""" +
                """{"item":"ring_might","level_sum":{"group":1,"at_least":12}},""" +
                """{"item":"ring_might","level_sum":{"group":1,"at_least":12}}]}""",
            // A curses-only set on an uncursed item.
            """{"requirements":[{"kind":"weapon","uncursed":true,"effect":["Annoying","Sacrificial"]}]}""",
            // Nested groups.
            """{"requirements":[{"any_of":[{"any_of":[{"item":"sword"}]}]}]}""",
        )) {
            assertThrows(rejected, IllegalArgumentException::class.java) {
                JniBindings.queryContinues(rejected.encodeToByteArray(), valid)
            }
        }
    }

    private fun assertDocument(expected: String, request: SearchRequest) {
        val actual = JSONObject(String(QueryDocument.encode(request), Charsets.UTF_8))
        assertEquals(canonical(JSONObject(expected)), canonical(actual))
    }

    /** JSON with keys sorted, so object key order (which org.json does not keep) cannot fail a comparison. */
    private fun canonical(value: Any?): String = when (value) {
        is JSONObject -> value.keys().asSequence().sorted()
            .joinToString(",", "{", "}") { key -> "\"$key\":${canonical(value.get(key))}" }
        is JSONArray -> (0 until value.length()).joinToString(",", "[", "]") { canonical(value.get(it)) }
        is String -> "\"$value\""
        else -> value.toString()
    }
}
