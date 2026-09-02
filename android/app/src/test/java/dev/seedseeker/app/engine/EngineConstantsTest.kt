// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.engine

import dev.seedseeker.app.catalog.ItemCatalog
import dev.seedseeker.app.catalog.PackagedCatalog
import dev.seedseeker.app.model.CatalogItem
import dev.seedseeker.app.model.Challenge
import dev.seedseeker.app.model.EMPTY_BOSS_FLOORS
import dev.seedseeker.app.model.FLOOR_LIMIT_OPTIONS
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.ItemRequirement
import dev.seedseeker.app.model.ScoutQuestGiver
import dev.seedseeker.app.model.SearchLimits
import dev.seedseeker.app.model.SearchRequest
import dev.seedseeker.app.model.TierMatch
import dev.seedseeker.app.model.UpgradeMatch
import dev.seedseeker.app.ui.RESULT_CAP
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The app keeps local copies of the engine's scalar constants so the models
 * need nothing from the native library to validate. This is the one place
 * they meet the engine: every local is asserted against the `engineInfo`
 * document the host-built library publishes, so a change on either side
 * fails here rather than as a sheet offering a query the search refuses.
 */
class EngineConstantsTest {
    init {
        PackagedCatalog.install()
    }

    private val info = JSONObject(String(JniBindings.engineInfo(), Charsets.UTF_8))
    private val limits = info.getJSONObject("limits")
    private val anyWand = ItemRequirement(key = 1, item = null, upgrade = 0, kind = ItemKind.WAND, upgradeMatch = UpgradeMatch.ANY)

    @Test
    fun queryBoundsMatchTheEngine() {
        assertEquals(limits.getInt("maxDepth"), SearchLimits.MAX_DEPTH)
        assertEquals(limits.getInt("exactTierMin")..limits.getInt("exactTierMax"), SearchLimits.EXACT_TIERS)
        assertEquals(limits.getInt("boundedTierMin")..limits.getInt("boundedTierMax"), SearchLimits.BOUNDED_TIERS)
        assertEquals(limits.getInt("identityGroupMax"), SearchLimits.IDENTITY_GROUP_MAX)
        assertEquals(limits.getInt("levelSumGroupMax"), SearchLimits.LEVEL_SUM_GROUP_MAX)
        assertEquals(limits.getInt("maxUpgradeDefault"), SearchLimits.MAX_UPGRADE_DEFAULT)
        assertEquals(limits.getInt("maxUpgradeRing"), SearchLimits.MAX_UPGRADE_RING)
        assertEquals(limits.getInt("maxUpgradeRingStandard"), SearchLimits.MAX_UPGRADE_RING_STANDARD)
        assertEquals(limits.getInt("maxUpgradeWeapon"), SearchLimits.MAX_UPGRADE_WEAPON)
        // The families route to the right maximum, narrowed weapon kinds
        // included: the engine publishes one ceiling per family, and a kind
        // takes its family's — so a melee weapon reaches the weapon's +5.
        val byKind = limits.getJSONObject("maxUpgradeByKind")
        for (kind in ItemKind.entries) {
            assertEquals(kind.label, byKind.getInt(kind.family.name.lowercase()), kind.maximumSearchUpgrade)
        }
        assertEquals(limits.getInt("maxUpgradeAnyTier"), SearchLimits.MAX_UPGRADE_ANY_TIER)
        assertEquals(limits.getInt("extraUpgradeTier"), SearchLimits.EXTRA_UPGRADE_TIER)
        assertEquals(SearchLimits.MAX_DEPTH, SearchRequest(listOf(anyWand)).maximumDepth)
    }

    /**
     * Only a tier-4 weapon is levelled past the shared ceiling, so a
     * requirement that rules that tier out loses the top of its range.
     */
    @Test
    fun topWeaponUpgradeNeedsTheTierThatReachesIt() {
        val ceiling = limits.getInt("maxUpgradeWeapon")
        val capped = limits.getInt("maxUpgradeAnyTier")
        val extraTier = limits.getInt("extraUpgradeTier")
        val maximum = { item: CatalogItem?, match: TierMatch, tier: Int ->
            SearchLimits.maximumUpgrade(ItemKind.WEAPON, item, match, tier)
        }
        assertEquals(ceiling, maximum(null, TierMatch.ANY, 0))
        assertEquals(ceiling, maximum(null, TierMatch.EXACT, extraTier))
        assertEquals(capped, maximum(null, TierMatch.EXACT, 5))
        assertEquals(capped, maximum(null, TierMatch.AT_MOST, 3))
        assertEquals(ceiling, maximum(ItemCatalog.findById("battle_axe"), TierMatch.ANY, 0))
        assertEquals(ceiling, maximum(ItemCatalog.findById("javelin"), TierMatch.ANY, 0))
        assertEquals(capped, maximum(ItemCatalog.findById("sword"), TierMatch.ANY, 0))
        assertEquals(capped, SearchLimits.maximumUpgrade(ItemKind.ARMOR, null, TierMatch.EXACT, extraTier))
    }

    @Test
    fun sessionAndFileLimitsMatchTheEngine() {
        assertEquals(info.getInt("maxResults"), RESULT_CAP)
        assertEquals(info.getLong("totalSeeds"), TOTAL_SEEDS)
        // The import byte cap has no local copy: the app reads it from the
        // engine at runtime (EngineInfo.resultsFileMaxBytes) and the codec
        // applies it itself. Pin that the runtime reader agrees with the
        // document this test reads.
        assertEquals(limits.getInt("resultsFileMaxBytes"), EngineInfo.resultsFileMaxBytes)
        assertEquals(info.getString("shpdVersion"), EngineInfo.shpdVersion)
    }

    /**
     * The worker ceiling is not in the constants document: it is a property of
     * the host, asked for at runtime. The contract the selector relies on is
     * that it is never below one, so a device always has a count to search
     * with even when the engine can see no parallelism at all.
     */
    @Test
    fun availableWorkersOffersAtLeastOneCore() {
        val workers = JniBindings.availableWorkers()
        assertTrue("availableWorkers returned $workers", workers >= 1)
        assertEquals(workers, SearchWorkers.ceiling)
    }

    @Test
    fun emptyBossFloorsMatchTheEngine() {
        val floors = info.getJSONArray("emptyBossFloors")
        assertEquals((0 until floors.length()).map(floors::getInt).toSet(), EMPTY_BOSS_FLOORS)
        assertEquals((1..SearchLimits.MAX_DEPTH).filterNot(EMPTY_BOSS_FLOORS::contains), FLOOR_LIMIT_OPTIONS)
    }

    @Test
    fun questWindowsMatchTheEngine() {
        val windows = info.getJSONObject("questWindows")
        for (giver in ScoutQuestGiver.entries) {
            val window = windows.getJSONArray(giver.name.lowercase())
            assertEquals(2, window.length())
            assertEquals(giver.label, window.getInt(0)..window.getInt(1), giver.depths)
        }
    }

    @Test
    fun challengesMatchTheEngineInMaskOrder() {
        val challenges = info.getJSONArray("challenges")
        val engine = (0 until challenges.length()).map { index ->
            val entry = challenges.getJSONObject(index)
            Triple(entry.getString("name"), entry.getInt("mask"), entry.getBoolean("changesLevelGeneration"))
        }
        // The stable document names, in declaration (mask) order; ResultsExport
        // keeps the same mapping privately for the results codec.
        val names = listOf(
            "on_diet", "faith_is_my_armor", "pharmacophobia", "barren_land", "swarm_intelligence",
            "into_darkness", "forbidden_runes", "hostile_champions", "badder_bosses",
        )
        val local = Challenge.entries.zip(names) { challenge, name -> Triple(name, challenge.bit, challenge.changesLevelGeneration) }
        assertEquals(engine, local)
        local.forEachIndexed { index, (_, mask, _) -> assertEquals(1 shl index, mask) }
        assertEquals(engine.fold(0) { mask, (_, bit, _) -> mask or bit }, Challenge.ALL_MASK)
    }
}
