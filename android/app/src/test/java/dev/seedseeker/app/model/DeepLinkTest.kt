// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.catalog.PackagedCatalog
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The share-link codec, exercised against the canonical Rust implementation:
 * every call reaches `crates/seedfinder-core/src/deep_link.rs` through the
 * host build of the JNI library (Gradle's `buildHostJni` task puts it on
 * `java.library.path`), so these tests cover the exact codec shipped in every
 * APK plus the JSON query-document mapping this app layers on the boundary.
 * The bit-stream format, its frozen code tables, and crafted-payload
 * rejections are pinned by the Rust core's own tests.
 */
class DeepLinkTest {
    init { PackagedCatalog.install() }

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

    private val pinnedLink = "https://shpd-seed-seeker.web.app/#q=MAGWhMAA"

    /**
     * The cross-platform pinned vector: a known code must decode identically
     * forever on every platform. This mirrors the Rust core's
     * `codes_are_stable` test.
     */
    @Test
    fun pinnedCodesAreStable() {
        assertEquals(pinnedLink, DeepLink.encodeLink(pinnedQuery))
        assertEquals(pinnedQuery.normalized(), DeepLink.decode("MAGWhMAA").normalized())
        assertEquals(pinnedQuery.normalized(), DeepLink.decode(pinnedLink).normalized())
        // The same query expressed as the canonical JSON query document.
        val document = """
            {"format":"seed-seeker-results","format_version":1,
             "query":{"requirements":[{"item":"wand_fireblast","kind":"wand","upgrade":{"at_least":3}}]},
             "results":[]}
        """.trimIndent()
        assertEquals(pinnedLink, DeepLink.encodeLink(ResultsExport.decode(document).query))
    }

    @Test
    fun roundTripsAFullyLoadedQuery() {
        val query = PresetQuery(
            requirements = listOf(
                ItemRequirement(
                    key = 1,
                    item = ItemCatalog.findById("war_scythe"),
                    upgrade = 2,
                    effect = EffectFilter.named("Grim"),
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
            challenges = Challenge.NO_FOOD.bit or Challenge.STRONGER_BOSSES.bit,
        )
        assertRoundTrips(query)
    }

    @Test
    fun roundTripsEveryWandmakerFilter() {
        for (variant in WandmakerQuest.entries) {
            assertRoundTrips(pinnedQuery.copy(wandmakerQuest = variant))
        }
        // An unfiltered query spends one bit and decodes back to "any".
        assertNull(DeepLink.decode(DeepLink.encodeLink(pinnedQuery)).wandmakerQuest)
    }

    /**
     * Every catalog item, effect, source, and challenge must survive the trip
     * through the JSON boundary and the Rust codec — this is what catches a
     * drift between the app's catalog names and the core's stable ids.
     */
    @Test
    fun roundTripsEveryItemEffectSourceAndChallenge() {
        for (item in ItemCatalog.all) {
            assertRoundTrips(minimal(ItemRequirement(1, item, 0, upgradeMatch = UpgradeMatch.ANY)))
        }
        for (kind in listOf(ItemKind.WEAPON, ItemKind.ARMOR)) {
            for (effect in ItemCatalog.modifiersFor(kind)) {
                assertRoundTrips(minimal(wildcard(kind).copy(effect = EffectFilter.named(effect))))
            }
        }
        for (source in ScoutItemSource.entries) {
            assertRoundTrips(minimal(wildcard(ItemKind.WAND).copy(source = source)))
        }
        for (challenge in Challenge.entries) {
            assertRoundTrips(minimal(wildcard(ItemKind.RING)).copy(challenges = challenge.bit))
        }
    }

    /**
     * The new structures travel as version-2 codes; a plain query keeps
     * writing the identical version-1 code it always did.
     */
    @Test
    fun roundTripsAlternativeGroupsEffectSetsAndCombinedLevelGroups() {
        val query = PresetQuery(
            requirements = listOf(
                ItemRequirement(
                    key = 1,
                    item = ItemCatalog.findById("spear"),
                    upgrade = 3,
                    alternativeGroup = 1,
                ),
                ItemRequirement(
                    key = 2,
                    item = ItemCatalog.findById("shuriken"),
                    upgrade = 2,
                    kind = ItemKind.THROWN_WEAPON,
                    effect = EffectFilter.OneOf(listOf("Blocking", "Projecting")),
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
                    levelSum = LevelSum(group = 2, atLeast = 4),
                ),
                ItemRequirement(
                    key = 5,
                    item = ItemCatalog.findById("ring_might"),
                    upgrade = 0,
                    upgradeMatch = UpgradeMatch.ANY,
                    levelSum = LevelSum(group = 2, atLeast = 4),
                ),
            ),
        )
        assertRoundTrips(query)
        assertEquals(pinnedLink, DeepLink.encodeLink(pinnedQuery))
    }

    /**
     * The v4.0.0 enchantments and curses sit above the 24 effect codes a
     * version-2 mask can carry, so a set naming one makes the codec write a
     * version-3 link. The app never names a version — it only has to keep
     * round-tripping what the wider mask now holds.
     */
    @Test
    fun roundTripsEffectSetsNeedingTheVersionThreeMask() {
        assertRoundTrips(
            minimal(
                wildcard(ItemKind.WEAPON).copy(
                    effect = EffectFilter.OneOf(listOf("Crystal", "Wondrous")),
                ),
            ),
        )
        // Every effect the catalog lists survives being paired with another.
        for (effect in ItemCatalog.modifiersFor(ItemKind.WEAPON)) {
            assertRoundTrips(
                minimal(
                    wildcard(ItemKind.WEAPON)
                        .copy(effect = EffectFilter.of(listOf("Blazing", effect), ItemKind.WEAPON)),
                ),
            )
        }
        // A plain query is untouched by the wider mask and keeps its version-1 code.
        assertEquals(pinnedLink, DeepLink.encodeLink(pinnedQuery))
    }

    @Test
    fun refusesToEncodeInvalidQueries() {
        val empty = assertThrows(IllegalArgumentException::class.java) {
            DeepLink.encodeLink(PresetQuery(requirements = emptyList()))
        }
        assertTrue(empty.message!!.contains("at least one requirement"))
        val tooMany = PresetQuery(
            requirements = List(64) { wildcard(ItemKind.WAND, key = it + 1L) },
        )
        val failure = assertThrows(IllegalArgumentException::class.java) {
            DeepLink.encodeLink(tooMany)
        }
        assertTrue(failure.message!!.contains("63"))
        assertThrows(IllegalArgumentException::class.java) {
            DeepLink.encodeLink(minimal(wildcard(ItemKind.WAND)).copy(maximumDepth = 0))
        }
    }

    @Test
    fun rejectsMalformedLinks() {
        assertDecodeFails("no share code", "")
        assertDecodeFails("no share code", "https://example.com/")
        assertDecodeFails("not part of a share link", "!!!")
        assertDecodeFails("truncated", "A")
        assertDecodeFails("truncated", "MAGWhM")
        assertDecodeFails("trailing data", "MAGWhMAAAAAA")
        // Unsupported future version (bits 0100 in the top nibble).
        val versioned = assertThrows(IllegalArgumentException::class.java) {
            DeepLink.decode("QAAA")
        }
        assertTrue(versioned.message!!.contains("version 4"))
        assertTrue(versioned.message!!.contains("different"))
    }

    @Test
    fun extractsCodesFromEverySupportedLinkForm() {
        val link = DeepLink.encodeLink(pinnedQuery)
        val code = link.substringAfter("#q=")
        assertEquals(code, DeepLink.extractCode(link))
        assertEquals(pinnedQuery.normalized(), DeepLink.decode("  $link  ").normalized())
        assertEquals(code, DeepLink.extractCode("https://example.com/?utm=1&q=$code#top"))
        assertEquals(code, DeepLink.extractCode("seedseeker://q/$code"))
        assertEquals(code, DeepLink.extractCode(code))
        assertNull(DeepLink.extractCode(""))
        assertNull(DeepLink.extractCode("https://example.com/"))
    }

    private fun minimal(requirement: ItemRequirement) = PresetQuery(requirements = listOf(requirement))

    private fun wildcard(kind: ItemKind, key: Long = 1) =
        ItemRequirement(key, null, 0, kind = kind, upgradeMatch = UpgradeMatch.ANY)

    /** Requirement row keys are session-local and never travel in a link. */
    private fun PresetQuery.normalized() =
        copy(requirements = requirements.map { it.copy(key = 0) })

    private fun assertRoundTrips(query: PresetQuery) {
        assertEquals(query.normalized(), DeepLink.decode(DeepLink.encodeLink(query)).normalized())
    }

    private fun assertDecodeFails(fragment: String, text: String) {
        val failure = assertThrows(IllegalArgumentException::class.java) { DeepLink.decode(text) }
        assertTrue("\"$text\": ${failure.message}", failure.message!!.contains(fragment))
    }
}
