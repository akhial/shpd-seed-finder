import XCTest
@testable import SeedSeekerKit

/// The results-file codec is the Rust core's, reached over the FFI, so these
/// tests exercise the real engine: the round trips must survive it, the frozen
/// fixtures must still import through it, and everything it refuses must
/// surface as a plain import failure.
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

    private static func fixture(_ name: String) -> String {
        let repoRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // ResultsExportTests.swift -> SeedSeekerKitTests
            .deletingLastPathComponent() // -> Tests
            .deletingLastPathComponent() // -> SeedSeeker
            .deletingLastPathComponent() // -> macos
            .deletingLastPathComponent() // -> repository root
        let fixture = repoRoot.appendingPathComponent(
            "crates/seedfinder-core/tests/fixtures/\(name)")
        return (try? String(contentsOf: fixture, encoding: .utf8)) ?? ""
    }

    /// The canonical frozen fixture, read straight from the Rust core's test
    /// data. It still carries the `"format_version": 1` older releases wrote:
    /// files exported by an older release must always stay readable.
    private static let version1Fixture = fixture("results-export-v1.json")
    private static let wandmakerQuestFixture = fixture("results-export-wandmaker-quest.json")

    func testEncodeThenDecodeRoundTripsQueryAndSeeds() throws {
        let query = try loadedQuery()
        let text = ResultsExport.encode(query, seeds: ["AAA-AAA-BUH", "ABC-DEF-GHI"],
                                        appVersion: "0.6.1")
        let imported = try ResultsExport.decode(text)
        XCTAssertEqual(imported.seeds, ["AAA-AAA-BUH", "ABC-DEF-GHI"])
        XCTAssertEqual(imported.dropped, 0)
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
        XCTAssertEqual(document["shpd_version"] as? String, EngineInfo.shared.shpdVersion)
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

    /// v4.0.0's additions travel through the canonical document like every
    /// other predicate: the vault's item source, the appended weapon
    /// enchantments, and the weapon ceiling one above the other families'.
    func testVaultSourceAndNewEnchantmentsSurviveTheDocument() throws {
        let query = SavedQuery(requirements: [
            try ItemRequirement(key: 1, item: ItemCatalog.findById("battle_axe"), upgrade: 5,
                                effect: .oneOf(["Vorpal", "Venomous"]), kind: .weapon,
                                upgradeMatch: .exactly, source: .vaultTreasure),
        ])
        let text = ResultsExport.encode(query, seeds: ["AAA-AAA-BUH"], appVersion: "0.8.0")
        let document = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        let requirements = try XCTUnwrap(
            (document["query"] as? [String: Any])?["requirements"] as? [[String: Any]])
        XCTAssertEqual(requirements[0]["source"] as? String, "vault_treasure")
        XCTAssertEqual(requirements[0]["upgrade"] as? Int, 5)
        // Effect lists travel in the catalog asset's order, not the caller's.
        XCTAssertEqual(requirements[0]["effect"] as? [String], ["Venomous", "Vorpal"])

        var expected = query.requirements
        var actual = try ResultsExport.decode(text).query.requirements
        for index in expected.indices { expected[index].key = 0 }
        for index in actual.indices { actual[index].key = 0 }
        XCTAssertEqual(expected, actual)
    }

    /// An unencodable query never produces a half-written file.
    func testEncodeRefusesAQueryTheEngineRejects() {
        XCTAssertEqual(ResultsExport.encode(SavedQuery(), seeds: [], appVersion: "0.6.1"), "")
        XCTAssertEqual(
            ResultsExport.encode(try! loadedQuery(), seeds: ["aaa-aaa-aab"], appVersion: "0.6.1"),
            "")
    }

    func testVersionOneFixtureAlwaysDecodes() throws {
        XCTAssertFalse(Self.version1Fixture.isEmpty, "canonical fixture file not found")
        let imported = try ResultsExport.decode(Self.version1Fixture)
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
        let contents = Self.fixture("results-export-v1-weapon-categories.json")
        XCTAssertFalse(contents.isEmpty, "canonical fixture file not found")
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

    /// Everything the engine's validator refuses — foreign envelopes, wrong
    /// types, out-of-bounds values, unknown names, non-canonical seed codes and
    /// oversized files — reaches the user as one plain import failure, because
    /// the FFI answers with INVALID rather than a message.
    func testAMalformedFileSurfacesAsAnImportFailure() {
        let queries = [
            #"{"requirements":[{"item":"sword"}],"max_depth":"12"}"#,
            #"{"requirements":[{"item":"sword"}],"max_depth":99}"#,
            #"{"requirements":[{"item":42}]}"#,
            #"{"requirements":[{"item":"sword"}],"challenges":"barren_land"}"#,
            #"{"requirements":[{"item":"sword","upgrade":true}]}"#,
            #"{"requirements":[{"item":"sword","uncursed":"yes"}]}"#,
            #"{"requirements":[{"kind":"RING"}]}"#,
            #"{"requirements":[{"item":"item_from_the_future"}]}"#,
            #"{"requirements":[{"item":"sword"}],"wished_luck":7}"#,
            #"{"requirements":[{"item":"sword"}],"wandmaker_quest":"seed_of_rotberry"}"#,
        ]
        var files = queries.map {
            #"{"format":"seed-seeker-results","query":\#($0),"results":[]}"#
        }
        for seed in ["aaa-aaa-aab", "AAAAAAAAB", "AAA AAA AAB", " AAA-AAA-AAB", "AAA-AAA-AA0"] {
            files.append("""
                {"format":"seed-seeker-results",
                 "query":{"requirements":[{"item":"sword"}]},
                 "results":[{"seed":"\(seed)"}]}
                """)
        }
        files += ["not json", "[]", "{}", #"{"format":"other"}"#]
        // Above the engine's own 2 MiB import cap.
        files.append(String(repeating: " ", count: 2 * 1_024 * 1_024 + 1) + Self.version1Fixture)

        for file in files {
            XCTAssertThrowsError(try ResultsExport.decode(file), String(file.prefix(80))) { error in
                XCTAssertEqual((error as? ResultsExportError)?.message,
                               "This is not a Seed Seeker results file this version can import.")
            }
        }
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

    /// Dedupe-and-cap is the engine's import rule, applied while decoding, and
    /// `dropped` reports what it removed.
    @MainActor
    func testDecodeDeduplicatesAndCapsTheImportedSeeds() throws {
        let query = try loadedQuery()
        let duplicated = try ResultsExport.decode(ResultsExport.encode(
            query, seeds: ["AAA-AAA-AAB", "AAA-AAA-AAC", "AAA-AAA-AAB"], appVersion: "0.6.1"))
        XCTAssertEqual(duplicated.seeds, ["AAA-AAA-AAB", "AAA-AAA-AAC"])
        XCTAssertEqual(duplicated.dropped, 1)

        let many = (0..<1_500).map { value -> String in
            let letters = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZ")
            return "AAA-AAB-\(letters[value / 676])\(letters[(value / 26) % 26])\(letters[value % 26])"
        }
        let capped = try ResultsExport.decode(
            ResultsExport.encode(query, seeds: many, appVersion: "0.6.1"))
        XCTAssertEqual(capped.seeds.count, SearchController.resultCap)
        XCTAssertEqual(capped.dropped, 1_500 - SearchController.resultCap)
    }

    @MainActor
    func testControllerLoadImportedTakesTheEnginesSeedsAndSnapshotsTheQuery() throws {
        let controller = SearchController()
        let query = try loadedQuery()
        controller.loadImported(seeds: ["AAA-AAA-AAB", "AAA-AAA-AAC"], dropped: 1, query: query)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAB", "AAA-AAA-AAC"])
        XCTAssertEqual(controller.results[0].matchedRequirements, 2)
        XCTAssertEqual(controller.importedDropped, 1)
        XCTAssertEqual(controller.exportQuery?.maximumDepth, query.maximumDepth)
        XCTAssertEqual(controller.target?.seeds, ["AAA-AAA-AAB", "AAA-AAA-AAC"])
        XCTAssertTrue(controller.isImported)
        XCTAssertNil(controller.state)
        XCTAssertFalse(controller.isImpossibleQuery)
    }
}
