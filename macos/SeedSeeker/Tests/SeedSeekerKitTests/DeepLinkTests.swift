import XCTest
@testable import SeedSeekerKit

final class DeepLinkTests: XCTestCase {
    /// Cross-platform pinned vector: this query and this link must stay
    /// interchangeable on every platform. Re-frozen when the retired link
    /// formats were dropped alongside the journal-order effect table.
    private static let pinnedLink = "https://shpd-seed-seeker.web.app/#q=MAGWhMAA"

    private func pinnedQuery() throws -> SavedQuery {
        SavedQuery(requirements: [
            try ItemRequirement(key: 1, item: ItemCatalog.findById("wand_fireblast"),
                                upgrade: 3, kind: .wand, upgradeMatch: .atLeast),
        ])
    }

    func testPinnedVectorEncodesToTheExactLink() throws {
        XCTAssertEqual(try DeepLink.encodeLink(for: pinnedQuery()), Self.pinnedLink)
    }

    func testPinnedVectorDecodesFromEveryLinkForm() throws {
        for text in [Self.pinnedLink, "MAGWhMAA", "seedseeker://q/MAGWhMAA"] {
            let query = try DeepLink.decode(text)
            XCTAssertEqual(query.requirements.count, 1, text)
            let requirement = try XCTUnwrap(query.requirements.first)
            XCTAssertEqual(requirement.item?.id, "wand_fireblast", text)
            XCTAssertEqual(requirement.kind, .wand, text)
            XCTAssertEqual(requirement.upgradeMatch, .atLeast, text)
            XCTAssertEqual(requirement.upgrade, 3, text)
            XCTAssertEqual(query.maximumDepth, 24, text)
            XCTAssertNotNil(query.validated(), text)
        }
    }

    func testSavedQueryRoundTripsThroughALink() throws {
        let query = SavedQuery(
            requirements: [
                try ItemRequirement(key: 1, item: ItemCatalog.findById("ring_tenacity"),
                                    upgrade: 4, kind: .ring, upgradeMatch: .exactly,
                                    source: .impReward),
                try ItemRequirement(key: 2, item: nil, upgrade: 2, kind: .wand,
                                    upgradeMatch: .atLeast, identityGroup: 1,
                                    maximumDepth: 9, requireUncursed: true),
            ],
            maximumDepth: 12,
            requireBlacksmith: true,
            wandmakerQuest: .rotberry,
            fastMode: true,
            challenges: Challenge.noHerbalism.rawValue)
        let link = try DeepLink.encodeLink(for: query)
        XCTAssertTrue(link.hasPrefix("https://shpd-seed-seeker.web.app/#q="), link)
        let decoded = try DeepLink.decode(link)
        XCTAssertEqual(decoded.maximumDepth, 12)
        XCTAssertTrue(decoded.requireBlacksmith)
        XCTAssertFalse(decoded.excludeBlacksmithRewards)
        XCTAssertEqual(decoded.wandmakerQuest, .rotberry)
        XCTAssertTrue(decoded.fastMode)
        XCTAssertEqual(decoded.challenges, Challenge.noHerbalism.rawValue)
        // Requirements compare equal except for the session-local row keys.
        var expected = query.requirements
        var actual = decoded.requirements
        for index in expected.indices { expected[index].key = 0 }
        for index in actual.indices { actual[index].key = 0 }
        XCTAssertEqual(expected, actual)
    }

    /// The enchantments and the item source v4.0.0 added are what the engine
    /// writes format-3 links for; the pinned version-1 link above must keep
    /// decoding all the same, so both eras stay shareable from this build.
    func testAQueryOfV4EffectsAndTheVaultSourceRoundTripsThroughALink() throws {
        let query = SavedQuery(requirements: [
            try ItemRequirement(key: 1, item: ItemCatalog.findById("greatsword"), upgrade: 5,
                                effect: .oneOf(["Vorpal", "Crystal"]), kind: .weapon,
                                upgradeMatch: .exactly, source: .vaultTreasure),
        ])
        let decoded = try DeepLink.decode(try DeepLink.encodeLink(for: query))
        var expected = query.requirements
        var actual = decoded.requirements
        for index in expected.indices { expected[index].key = 0 }
        for index in actual.indices { actual[index].key = 0 }
        XCTAssertEqual(expected, actual)
        XCTAssertEqual(actual.first?.effect, .oneOf(["Vorpal", "Crystal"]))
        XCTAssertEqual(actual.first?.source, .vaultTreasure)
    }

    func testGarbageAndEmptyTextAreRejected() {
        for text in ["", "   ", "not a link", "!!!", "https://example.com/",
                     "https://shpd-seed-seeker.web.app/#q=", "seedseeker://q/",
                     "https://shpd-seed-seeker.web.app/#q=EAGWhM"] {
            XCTAssertThrowsError(try DeepLink.decode(text), text) { error in
                XCTAssertEqual(error as? DeepLinkError,
                               DeepLinkError("This is not a Seed Seeker query link."),
                               text)
            }
        }
    }

    func testAQueryWithoutRequirementsCannotBeEncoded() {
        XCTAssertThrowsError(try DeepLink.encodeLink(for: SavedQuery())) { error in
            XCTAssertTrue(error is DeepLinkError, "\(error)")
        }
    }
}
