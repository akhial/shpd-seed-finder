// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SearchRequestValidationTest {
    private fun wand(
        key: Long,
        total: Int,
        match: UpgradeMatch = UpgradeMatch.ANY,
        upgrade: Int = 0,
    ) = ItemRequirement(
        key = key,
        item = null,
        upgrade = upgrade,
        kind = ItemKind.WAND,
        upgradeMatch = match,
        upgradeSumGroup = 1,
        upgradeSumTotal = total,
    )

    @Test
    fun unattainableCombinedUpgradeTotalsAreReportedBeforeReachingTheEngine() {
        val impossible = SearchRequest(listOf(wand(1, 7), wand(2, 7)))
        assertEquals(
            "Combined upgrade group A asks for +7 but its items can reach at most +6 together.",
            impossible.unattainableUpgradeSumMessage(),
        )
        assertNull(SearchRequest(listOf(wand(1, 6), wand(2, 6))).unattainableUpgradeSumMessage())
    }

    @Test
    fun exactUpgradesCapAMembersContributionToTheTotal() {
        val exact = wand(1, 5, UpgradeMatch.EXACT, upgrade = 1)
        assertEquals(
            "Combined upgrade group A asks for +5 but its items can reach at most +4 together.",
            SearchRequest(listOf(exact, wand(2, 5))).unattainableUpgradeSumMessage(),
        )
    }
}
