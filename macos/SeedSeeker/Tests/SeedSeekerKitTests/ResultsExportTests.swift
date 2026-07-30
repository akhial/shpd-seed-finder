import XCTest
@testable import SeedSeekerKit

final class ResultsExportTests: XCTestCase {
    private func loadedQuery() throws -> SavedQuery {
        SavedQuery(
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
            challenges: Challenge.noHerbalism.rawValue)
    }

    /// The canonical version-1 fixture, schema-identical to
    /// crates/seedfinder-core/tests/fixtures/results-export-v1.json. Files
    /// exported today must always stay readable; never edit this fixture.
    private let version1Fixture = """
        {
          "format": "seed-seeker-results",
          "format_version": 1,
          "app_version": "0.6.1",
          "shpd_version": "3.3.8",
          "query": {
            "requirements": [
              {
                "kind": "ring",
                "item": "ring_tenacity",
                "upgrade": 4,
                "source": "imp_reward"
              },
              {
                "kind": "wand",
                "upgrade": { "at_least": 2 },
                "uncursed": true,
                "identity_group": 1,
                "max_depth": 9
              }
            ],
            "max_depth": 12,
            "require_blacksmith": true,
            "challenges": ["barren_land"]
          },
          "results": [
            { "seed": "AAA-AAA-BUH" },
            { "seed": "ABC-DEF-GHI" }
          ]
        }
        """

    func testEncodeThenDecodeRoundTripsQueryAndSeeds() throws {
        let query = try loadedQuery()
        let text = ResultsExport.encode(query, seeds: ["AAA-AAA-BUH", "ABC-DEF-GHI"],
                                        appVersion: "0.6.1")
        let imported = try ResultsExport.decode(text)
        XCTAssertEqual(imported.seeds, ["AAA-AAA-BUH", "ABC-DEF-GHI"])
        XCTAssertEqual(imported.query.maximumDepth, 12)
        XCTAssertTrue(imported.query.requireBlacksmith)
        XCTAssertEqual(imported.query.challenges, Challenge.noHerbalism.rawValue)
        // Requirements compare equal except for the session-local row keys.
        var expected = query.requirements
        var actual = imported.query.requirements
        for index in expected.indices { expected[index].key = 0 }
        for index in actual.indices { actual[index].key = 0 }
        XCTAssertEqual(expected, actual)
    }

    func testEncodeEmitsTheDocumentedEnvelopeAndMinimalQuery() throws {
        let text = ResultsExport.encode(try loadedQuery(), seeds: ["AAA-AAA-BUH"],
                                        appVersion: "0.6.1")
        let document = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        XCTAssertEqual(document["format"] as? String, "seed-seeker-results")
        XCTAssertEqual(document["format_version"] as? Int, 1)
        XCTAssertEqual(document["app_version"] as? String, "0.6.1")
        XCTAssertEqual(document["shpd_version"] as? String, "3.3.8")
        let results = try XCTUnwrap(document["results"] as? [[String: Any]])
        XCTAssertEqual(results.count, 1)
        XCTAssertEqual(results[0]["seed"] as? String, "AAA-AAA-BUH")
        let query = try XCTUnwrap(document["query"] as? [String: Any])
        XCTAssertEqual(query["max_depth"] as? Int, 12)
        XCTAssertEqual(query["require_blacksmith"] as? Bool, true)
        XCTAssertEqual(query["challenges"] as? [String], ["barren_land"])
        let requirements = try XCTUnwrap(query["requirements"] as? [[String: Any]])
        XCTAssertEqual(requirements[0]["kind"] as? String, "ring")
        XCTAssertEqual(requirements[0]["item"] as? String, "ring_tenacity")
        XCTAssertEqual(requirements[0]["upgrade"] as? Int, 4)
        XCTAssertEqual(requirements[0]["source"] as? String, "imp_reward")
        XCTAssertNil(requirements[0]["tier"])
        XCTAssertEqual((requirements[1]["upgrade"] as? [String: Any])?["at_least"] as? Int, 2)
        XCTAssertEqual(requirements[1]["uncursed"] as? Bool, true)
        XCTAssertEqual(requirements[1]["identity_group"] as? Int, 1)
        XCTAssertEqual(requirements[1]["max_depth"] as? Int, 9)
    }

    func testVersionOneFixtureAlwaysDecodes() throws {
        let imported = try ResultsExport.decode(version1Fixture)
        XCTAssertEqual(imported.seeds, ["AAA-AAA-BUH", "ABC-DEF-GHI"])
        XCTAssertEqual(imported.query.maximumDepth, 12)
        XCTAssertEqual(imported.query.challenges, Challenge.noHerbalism.rawValue)
        XCTAssertEqual(imported.query.requirements[0].item?.id, "ring_tenacity")
        XCTAssertEqual(imported.query.requirements[1].kind, .wand)
        XCTAssertEqual(imported.query.requirements[1].upgradeMatch, .atLeast)
        XCTAssertNotNil(imported.query.validated())
    }

    func testUnknownEnvelopeAndResultFieldsAreIgnored() throws {
        let imported = try ResultsExport.decode("""
            {"format": "seed-seeker-results", "format_version": 1,
             "exported_at": "2031-01-01T00:00:00Z", "future_minor_field": {"nested": true},
             "query": {"requirements": [{"item": "sword"}]},
             "results": [{"seed": "AAA-AAA-AAB", "future_note": "still fine"}]}
            """)
        XCTAssertEqual(imported.seeds, ["AAA-AAA-AAB"])
        XCTAssertEqual(imported.query.maximumDepth, 24)
    }

    func testFutureFormatVersionsFailWithAnUpdateMessage() {
        XCTAssertThrowsError(try ResultsExport.decode("""
            {"format":"seed-seeker-results","format_version":2,
             "query":{"requirements":[{"item":"sword"}]},"results":[]}
            """)) { error in
            let message = (error as? ResultsExportError)?.message ?? ""
            XCTAssertTrue(message.contains("format version 2"), message)
            XCTAssertTrue(message.contains("Update Seed Seeker"), message)
        }
    }

    func testForeignAndMalformedFilesAreRejectedClearly() {
        for text in ["not json", "[]", "{}", #"{"format":"other"}"#] {
            XCTAssertThrowsError(try ResultsExport.decode(text)) { error in
                let message = (error as? ResultsExportError)?.message ?? ""
                XCTAssertTrue(message.contains("not a Seed Seeker results file"), message)
            }
        }
    }

    func testUnknownQueryContentFailsInsteadOfChangingMeaning() {
        XCTAssertThrowsError(try ResultsExport.decode("""
            {"format":"seed-seeker-results","format_version":1,
             "query":{"requirements":[{"item":"item_from_the_future"}]},"results":[]}
            """)) { error in
            let message = (error as? ResultsExportError)?.message ?? ""
            XCTAssertTrue(message.contains("item_from_the_future"), message)
        }
        XCTAssertThrowsError(try ResultsExport.decode("""
            {"format":"seed-seeker-results","format_version":1,
             "query":{"requirements":[{"item":"sword"}],"wished_luck":7},"results":[]}
            """)) { error in
            let message = (error as? ResultsExportError)?.message ?? ""
            XCTAssertTrue(message.contains("wished_luck"), message)
        }
    }

    func testInvalidSeedCodesNameTheOffendingResult() {
        XCTAssertThrowsError(try ResultsExport.decode("""
            {"format":"seed-seeker-results","format_version":1,
             "query":{"requirements":[{"item":"sword"}]},
             "results":[{"seed":"AAA-AAA-AAB"},{"seed":"AAA-AAA-AA0"}]}
            """)) { error in
            let message = (error as? ResultsExportError)?.message ?? ""
            XCTAssertTrue(message.contains("Result 2"), message)
        }
    }

    func testDecodeAcceptsAllCoreTierAndUpgradeForms() throws {
        let imported = try ResultsExport.decode("""
            {"format":"seed-seeker-results","format_version":1,
             "query":{"requirements":[
               {"kind":"weapon","tier":"any","upgrade":"any"},
               {"kind":"weapon","tier":{"exact":2},"upgrade":{"exact":3}},
               {"kind":"armor","tier":{"at_least":3},"upgrade":{"at_least":1}},
               {"kind":"armor","tier":{"at_most":4},"effect":"anti-magic"}
             ]},
             "results":[]}
            """)
        let requirements = imported.query.requirements
        XCTAssertEqual(requirements[0].tierMatch, .any)
        XCTAssertEqual(requirements[0].upgradeMatch, .any)
        XCTAssertEqual(requirements[1].tierMatch, .exactly)
        XCTAssertEqual(requirements[1].tier, 2)
        XCTAssertEqual(requirements[1].upgradeMatch, .exactly)
        XCTAssertEqual(requirements[1].upgrade, 3)
        XCTAssertEqual(requirements[2].tierMatch, .atLeast)
        XCTAssertEqual(requirements[2].upgradeMatch, .atLeast)
        XCTAssertEqual(requirements[3].tierMatch, .atMost)
        // Effect matching is case-insensitive and canonicalizes to the catalog name.
        XCTAssertEqual(requirements[3].modifier, "Anti-Magic")
    }

    @MainActor
    func testControllerLoadImportedReplacesAndDeduplicatesResults() {
        let controller = SearchController()
        controller.loadImported(seeds: ["AAA-AAA-AAB", "AAA-AAA-AAC", "AAA-AAA-AAB"],
                                matchedRequirements: 2)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAB", "AAA-AAA-AAC"])
        XCTAssertEqual(controller.results[0].matchedRequirements, 2)
        XCTAssertTrue(controller.isImported)
        XCTAssertNil(controller.state)
        XCTAssertFalse(controller.isImpossibleQuery)
    }
}
