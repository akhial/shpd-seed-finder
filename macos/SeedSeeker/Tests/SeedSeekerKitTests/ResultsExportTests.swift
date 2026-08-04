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

    /// The canonical frozen fixture, read straight from the Rust core's test
    /// data so this codec can never silently drift from it. It still carries
    /// the `"format_version": 1` older releases wrote: files exported by an
    /// older release must always stay readable; never edit the fixture.
    private static let version1Fixture: String = {
        let repoRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // ResultsExportTests.swift -> SeedSeekerKitTests
            .deletingLastPathComponent() // -> Tests
            .deletingLastPathComponent() // -> SeedSeeker
            .deletingLastPathComponent() // -> macos
            .deletingLastPathComponent() // -> repository root
        let fixture = repoRoot.appendingPathComponent(
            "crates/seedfinder-core/tests/fixtures/results-export-v1.json")
        return (try? String(contentsOf: fixture, encoding: .utf8)) ?? ""
    }()
    private var version1Fixture: String { Self.version1Fixture }

    /// The canonical frozen quest fixture, read from the same place.
    private static let wandmakerQuestFixture: String = {
        let repoRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let fixture = repoRoot.appendingPathComponent(
            "crates/seedfinder-core/tests/fixtures/results-export-wandmaker-quest.json")
        return (try? String(contentsOf: fixture, encoding: .utf8)) ?? ""
    }()

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
        XCTAssertNil(document["format_version"])
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
        XCTAssertFalse(version1Fixture.isEmpty, "canonical fixture file not found")
        let imported = try ResultsExport.decode(version1Fixture)
        XCTAssertEqual(imported.seeds, ["AAA-AAA-BUH", "ABC-DEF-GHI"])
        XCTAssertEqual(imported.shpdVersion, "3.3.8")
        XCTAssertEqual(imported.query.maximumDepth, 12)
        XCTAssertEqual(imported.query.challenges, Challenge.noHerbalism.rawValue)
        XCTAssertEqual(imported.query.requirements[0].item?.id, "ring_tenacity")
        XCTAssertEqual(imported.query.requirements[1].kind, .wand)
        XCTAssertEqual(imported.query.requirements[1].upgradeMatch, .atLeast)
        XCTAssertNotNil(imported.query.validated())
    }

    /// Widening the narrowed weapon kinds back to "weapon" on either side
    /// would silently change the query's meaning.
    func testWeaponCategoryFixtureDecodesAndRoundTrips() throws {
        let repoRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let fixture = repoRoot.appendingPathComponent(
            "crates/seedfinder-core/tests/fixtures/results-export-v1-weapon-categories.json")
        let contents = try String(contentsOf: fixture, encoding: .utf8)
        let imported = try ResultsExport.decode(contents)
        XCTAssertEqual(imported.query.requirements.map(\.kind),
                       [.thrownWeapon, .meleeWeapon, .weapon])
        XCTAssertEqual(imported.query.requirements[1].item?.id, "sword")
        XCTAssertEqual(imported.seeds, ["AAA-AAA-ACO"])

        let reImported = try ResultsExport.decode(
            ResultsExport.encode(imported.query, seeds: imported.seeds, appVersion: "0.6.1"))
        var expected = imported.query.requirements
        var actual = reImported.query.requirements
        for index in expected.indices { expected[index].key = 0 }
        for index in actual.indices { actual[index].key = 0 }
        XCTAssertEqual(expected, actual)
    }

    /// The number carried no meaning for a reader newer than the file, so it
    /// is now just another unknown envelope field.
    func testAnyDeclaredFormatVersionIsIgnored() throws {
        for version in ["1", "2", "99", "0", "1.5", "true", "\"1\"", "-1"] {
            let imported = try ResultsExport.decode("""
                {"format":"seed-seeker-results","format_version":\(version),
                 "query":{"requirements":[{"item":"sword"}]},
                 "results":[{"seed":"AAA-AAA-AAB"}]}
                """)
            XCTAssertEqual(imported.seeds, ["AAA-AAA-AAB"], version)
        }
    }

    func testWrongTypedQueryFieldsAreRejectedNotCoerced() {
        let payloads = [
            #"{"requirements":[{"item":"sword"}],"max_depth":"12"}"#,
            #"{"requirements":[{"item":"sword"}],"max_depth":99}"#,
            #"{"requirements":[{"item":42}]}"#,
            #"{"requirements":[{"item":"sword"}],"challenges":"barren_land"}"#,
            #"{"requirements":[{"item":"sword","upgrade":true}]}"#,
            #"{"requirements":[{"item":"sword","uncursed":"yes"}]}"#,
            #"{"requirements":[{"kind":"RING"}]}"#,
        ]
        for query in payloads {
            XCTAssertThrowsError(try ResultsExport.decode("""
                {"format":"seed-seeker-results",
                 "query":\(query),"results":[]}
                """), query)
        }
    }

    func testOnlyCanonicalSeedCodesAreAccepted() {
        for seed in ["aaa-aaa-aab", "AAAAAAAAB", "AAA AAA AAB", " AAA-AAA-AAB"] {
            XCTAssertThrowsError(try ResultsExport.decode("""
                {"format":"seed-seeker-results",
                 "query":{"requirements":[{"item":"sword"}]},
                 "results":[{"seed":"\(seed)"}]}
                """)) { error in
                let message = (error as? ResultsExportError)?.message ?? ""
                XCTAssertTrue(message.contains("Result 1"), "\(seed): \(message)")
            }
        }
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

    func testAWandmakerQuestRoundTrips() throws {
        var quested = try loadedQuery()
        quested.wandmakerQuest = .corpseDust
        let text = ResultsExport.encode(quested, seeds: ["AAA-AAA-BUH"], appVersion: "0.6.1")
        let document = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        let query = try XCTUnwrap(document["query"] as? [String: Any])
        XCTAssertEqual(query["wandmaker_quest"] as? String, "corpse_dust")
        XCTAssertEqual(try ResultsExport.decode(text).query.wandmakerQuest, .corpseDust)
    }

    func testWandmakerQuestFixtureCarriesTheQuest() throws {
        XCTAssertFalse(Self.wandmakerQuestFixture.isEmpty, "canonical quest fixture not found")
        let imported = try ResultsExport.decode(Self.wandmakerQuestFixture)
        XCTAssertEqual(imported.query.wandmakerQuest, .rotberry)
        XCTAssertEqual(imported.query.maximumDepth, 9)
        XCTAssertEqual(imported.seeds, ["AAA-AAA-BUH", "ABC-DEF-GHI"])
    }

    func testUnknownWandmakerQuestIsRejected() {
        XCTAssertThrowsError(try ResultsExport.decode("""
            {"format":"seed-seeker-results",
             "query":{"requirements":[{"item":"sword"}],"wandmaker_quest":"seed_of_rotberry"},
             "results":[]}
            """)) { error in
            let message = (error as? ResultsExportError)?.message ?? ""
            XCTAssertTrue(message.contains("Wandmaker quest"), message)
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
            {"format":"seed-seeker-results",
             "query":{"requirements":[{"item":"item_from_the_future"}]},"results":[]}
            """)) { error in
            let message = (error as? ResultsExportError)?.message ?? ""
            XCTAssertTrue(message.contains("item_from_the_future"), message)
        }
        XCTAssertThrowsError(try ResultsExport.decode("""
            {"format":"seed-seeker-results",
             "query":{"requirements":[{"item":"sword"}],"wished_luck":7},"results":[]}
            """)) { error in
            let message = (error as? ResultsExportError)?.message ?? ""
            XCTAssertTrue(message.contains("wished_luck"), message)
        }
    }

    func testInvalidSeedCodesNameTheOffendingResult() {
        XCTAssertThrowsError(try ResultsExport.decode("""
            {"format":"seed-seeker-results",
             "query":{"requirements":[{"item":"sword"}]},
             "results":[{"seed":"AAA-AAA-AAB"},{"seed":"AAA-AAA-AA0"}]}
            """)) { error in
            let message = (error as? ResultsExportError)?.message ?? ""
            XCTAssertTrue(message.contains("Result 2"), message)
        }
    }

    func testDecodeAcceptsAllCoreTierAndUpgradeForms() throws {
        let imported = try ResultsExport.decode("""
            {"format":"seed-seeker-results",
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
    func testControllerLoadImportedDeduplicatesCapsAndSnapshotsTheQuery() throws {
        let controller = SearchController()
        let query = try loadedQuery()
        controller.loadImported(seeds: ["AAA-AAA-AAB", "AAA-AAA-AAC", "AAA-AAA-AAB"], query: query)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAB", "AAA-AAA-AAC"])
        XCTAssertEqual(controller.results[0].matchedRequirements, 2)
        XCTAssertEqual(controller.importedDropped, 1)
        XCTAssertEqual(controller.exportQuery?.maximumDepth, query.maximumDepth)
        XCTAssertTrue(controller.isImported)
        XCTAssertNil(controller.state)
        XCTAssertFalse(controller.isImpossibleQuery)

        let many = (0..<1_500).map { value -> String in
            // Distinct synthetic canonical codes.
            let letters = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZ")
            return "AAA-AAB-\(letters[value / 676])\(letters[(value / 26) % 26])\(letters[value % 26])"
        }
        controller.loadImported(seeds: many, query: query)
        XCTAssertEqual(controller.results.count, SearchController.importCap)
        XCTAssertEqual(controller.importedDropped, 1_500 - SearchController.importCap)
    }
}
