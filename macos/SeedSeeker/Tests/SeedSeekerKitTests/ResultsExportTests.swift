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

    /// The canonical frozen version-1 fixture, read straight from the Rust
    /// core's test data so this codec can never silently drift from it.
    /// Files exported today must always stay readable; never edit the
    /// fixture.
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

    /// The narrowed weapon kinds are additive within format version 1;
    /// widening them to "weapon" on either side would silently change the
    /// query's meaning.
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

    func testFormatVersionMustBeAPositiveInteger() {
        for version in ["0", "1.5", "true", "\"1\"", "-1"] {
            XCTAssertThrowsError(try ResultsExport.decode("""
                {"format":"seed-seeker-results","format_version":\(version),
                 "query":{"requirements":[{"item":"sword"}]},"results":[]}
                """)) { error in
                let message = (error as? ResultsExportError)?.message ?? ""
                XCTAssertTrue(message.contains("format version"), "\(version): \(message)")
            }
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
                {"format":"seed-seeker-results","format_version":1,
                 "query":\(query),"results":[]}
                """), query)
        }
    }

    func testOnlyCanonicalSeedCodesAreAccepted() {
        for seed in ["aaa-aaa-aab", "AAAAAAAAB", "AAA AAA AAB", " AAA-AAA-AAB"] {
            XCTAssertThrowsError(try ResultsExport.decode("""
                {"format":"seed-seeker-results","format_version":1,
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
        XCTAssertEqual(requirements[3].effect, .oneOf(["Anti-Magic"]))
    }

    /// Requirements compare equal except for the session-local row keys.
    private func assertSameRequirements(_ lhs: [ItemRequirement], _ rhs: [ItemRequirement],
                                        file: StaticString = #filePath, line: UInt = #line) {
        var expected = lhs
        var actual = rhs
        for index in expected.indices { expected[index].key = 0 }
        for index in actual.indices { actual[index].key = 0 }
        XCTAssertEqual(expected, actual, file: file, line: line)
    }

    func testAlternativeGroupsRoundTripAsAnyOfEntries() throws {
        let query = SavedQuery(requirements: [
            try ItemRequirement(key: 1, item: ItemCatalog.findById("spear"), upgrade: 3,
                                kind: .weapon, upgradeMatch: .exactly, alternativeGroup: 1),
            try ItemRequirement(key: 2, item: ItemCatalog.findById("shuriken"), upgrade: 2,
                                kind: .thrownWeapon, upgradeMatch: .exactly, alternativeGroup: 1),
            try ItemRequirement(key: 3, item: nil, upgrade: 0, kind: .wand, upgradeMatch: .any),
            try ItemRequirement(key: 4, item: ItemCatalog.findById("sword"), upgrade: 0,
                                kind: .meleeWeapon, upgradeMatch: .any, alternativeGroup: 2),
            try ItemRequirement(key: 5, item: ItemCatalog.findById("mace"), upgrade: 0,
                                kind: .weapon, upgradeMatch: .any, alternativeGroup: 2),
        ])
        let text = ResultsExport.encode(query, seeds: ["AAA-AAA-AAB"], appVersion: "0.6.1")
        let document = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        let entries = try XCTUnwrap((document["query"] as? [String: Any])?["requirements"] as? [Any])
        XCTAssertEqual(entries.count, 3)
        let first = try XCTUnwrap((entries[0] as? [String: Any])?["any_of"] as? [[String: Any]])
        XCTAssertEqual(first.count, 2)
        XCTAssertEqual(first[1]["kind"] as? String, "thrown_weapon")
        XCTAssertNil((entries[1] as? [String: Any])?["any_of"])
        let imported = try ResultsExport.decode(text)
        assertSameRequirements(query.requirements, imported.query.requirements)
    }

    func testImportRenumbersAlternativeGroupsSequentially() throws {
        let query = SavedQuery(requirements: [
            try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .wand,
                                upgradeMatch: .any, alternativeGroup: 9),
            try ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .ring,
                                upgradeMatch: .any, alternativeGroup: 9),
            try ItemRequirement(key: 3, item: nil, upgrade: 0, kind: .armor,
                                upgradeMatch: .any, alternativeGroup: 4),
            try ItemRequirement(key: 4, item: nil, upgrade: 0, kind: .weapon,
                                upgradeMatch: .any, alternativeGroup: 4),
        ])
        let imported = try ResultsExport.decode(
            ResultsExport.encode(query, seeds: [], appVersion: "0.6.1"))
        XCTAssertEqual(imported.query.requirements.map(\.alternativeGroup), [1, 1, 2, 2])
    }

    func testSingleMemberAnyOfCollapsesToAPlainRequirement() throws {
        let imported = try ResultsExport.decode("""
            {"format":"seed-seeker-results","format_version":1,
             "query":{"requirements":[{"any_of":[{"item":"sword"}]},{"kind":"wand"}]},
             "results":[]}
            """)
        XCTAssertEqual(imported.query.requirements.count, 2)
        XCTAssertEqual(imported.query.requirements.map(\.alternativeGroup), [nil, nil])
        XCTAssertEqual(imported.query.requirements[0].item?.id, "sword")
    }

    func testEffectListsAndAnyEnchantmentRoundTrip() throws {
        let query = SavedQuery(requirements: [
            try ItemRequirement(key: 1, item: nil, upgrade: 0, effect: .anyEnchantment,
                                kind: .meleeWeapon, upgradeMatch: .any),
            try ItemRequirement(key: 2, item: ItemCatalog.findById("greatshield"), upgrade: 2,
                                effect: .oneOf(["Blocking", "Projecting", "Vampiric"]),
                                kind: .weapon, upgradeMatch: .exactly),
            try ItemRequirement(key: 3, item: nil, upgrade: 0, effect: .oneOf(["Thorns"]),
                                kind: .armor, upgradeMatch: .any),
        ])
        let text = ResultsExport.encode(query, seeds: [], appVersion: "0.6.1")
        let document = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        let entries = try XCTUnwrap(
            (document["query"] as? [String: Any])?["requirements"] as? [[String: Any]])
        XCTAssertEqual(entries[0]["effect"] as? String, "any_enchantment")
        XCTAssertEqual(entries[1]["effect"] as? [String], ["Blocking", "Projecting", "Vampiric"])
        XCTAssertEqual(entries[2]["effect"] as? String, "Thorns")
        let imported = try ResultsExport.decode(text)
        assertSameRequirements(query.requirements, imported.query.requirements)
    }

    func testFullFamilyEffectSetsUseTheAnyEnchantmentShorthand() throws {
        let query = SavedQuery(requirements: [
            try ItemRequirement(key: 1, item: nil, upgrade: 0,
                                effect: .oneOf(ItemCatalog.glyphs),
                                kind: .armor, upgradeMatch: .any),
        ])
        let text = ResultsExport.encode(query, seeds: [], appVersion: "0.6.1")
        let document = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        let entries = try XCTUnwrap(
            (document["query"] as? [String: Any])?["requirements"] as? [[String: Any]])
        XCTAssertEqual(entries[0]["effect"] as? String, "any_enchantment")
        // The shorthand imports as the canonical any-enchantment predicate.
        let imported = try ResultsExport.decode(text)
        XCTAssertEqual(imported.query.requirements[0].effect, .anyEnchantment)
    }

    func testUpgradeSumGroupsRoundTrip() throws {
        let query = SavedQuery(requirements: [
            try ItemRequirement(key: 1, item: ItemCatalog.findById("ring_might"), upgrade: 0,
                                kind: .ring, upgradeMatch: .any, identityGroup: 1,
                                upgradeSumGroup: 1, upgradeSumTotal: 4),
            try ItemRequirement(key: 2, item: ItemCatalog.findById("ring_might"), upgrade: 0,
                                kind: .ring, upgradeMatch: .any, identityGroup: 1,
                                upgradeSumGroup: 1, upgradeSumTotal: 4),
        ])
        let text = ResultsExport.encode(query, seeds: ["AAA-AAA-AAB"], appVersion: "0.6.1")
        let document = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        let entries = try XCTUnwrap(
            (document["query"] as? [String: Any])?["requirements"] as? [[String: Any]])
        XCTAssertEqual(entries[0]["upgrade_sum"] as? [String: Int], ["group": 1, "at_least": 4])
        let imported = try ResultsExport.decode(text)
        assertSameRequirements(query.requirements, imported.query.requirements)
    }

    func testNarrowedWeaponKindsRoundTripWithEffectsAndGroups() throws {
        let query = SavedQuery(requirements: [
            try ItemRequirement(key: 1, item: nil, upgrade: 1, effect: .anyEnchantment,
                                kind: .thrownWeapon, upgradeMatch: .atLeast, alternativeGroup: 1),
            try ItemRequirement(key: 2, item: nil, upgrade: 0, effect: .oneOf(["Grim"]),
                                kind: .meleeWeapon, tier: 5, tierMatch: .exactly,
                                upgradeMatch: .any, alternativeGroup: 1),
        ])
        let imported = try ResultsExport.decode(
            ResultsExport.encode(query, seeds: [], appVersion: "0.6.1"))
        XCTAssertEqual(imported.query.requirements.map(\.kind), [.thrownWeapon, .meleeWeapon])
        assertSameRequirements(query.requirements, imported.query.requirements)
    }

    func testUpgradeSumInsideAnyOfIsRejected() {
        XCTAssertThrowsError(try ResultsExport.decode("""
            {"format":"seed-seeker-results","format_version":1,
             "query":{"requirements":[{"any_of":[
               {"item":"ring_might","upgrade_sum":{"group":1,"at_least":2}},
               {"item":"ring_haste"}
             ]}]},
             "results":[]}
            """)) { error in
            let message = (error as? ResultsExportError)?.message ?? ""
            XCTAssertTrue(message.contains("any_of"), message)
        }
        // Even a single-member group cannot carry a combined upgrade total.
        XCTAssertThrowsError(try ResultsExport.decode("""
            {"format":"seed-seeker-results","format_version":1,
             "query":{"requirements":[{"any_of":[
               {"item":"ring_might","upgrade_sum":{"group":1,"at_least":2}}
             ]}]},
             "results":[]}
            """))
    }

    func testMalformedGroupsSumsAndEffectsAreRejected() {
        let payloads = [
            // any_of must be the entry's only field, non-empty, with object members.
            #"{"requirements":[{"any_of":[]}]}"#,
            #"{"requirements":[{"any_of":[{"item":"sword"}],"extra":1}]}"#,
            #"{"requirements":[{"any_of":["sword"]}]}"#,
            // Nested groups are not representable.
            #"{"requirements":[{"any_of":[{"any_of":[{"item":"sword"}]}]}]}"#,
            // upgrade_sum needs exactly {"group", "at_least"} in range.
            #"{"requirements":[{"item":"sword","upgrade_sum":3}]}"#,
            #"{"requirements":[{"item":"sword","upgrade_sum":{"group":1}}]}"#,
            #"{"requirements":[{"item":"sword","upgrade_sum":{"group":1,"at_least":2,"x":3}}]}"#,
            #"{"requirements":[{"item":"sword","upgrade_sum":{"group":0,"at_least":2}}]}"#,
            #"{"requirements":[{"item":"sword","upgrade_sum":{"group":1,"at_least":9}}]}"#,
            // Disagreeing totals within one group.
            #"{"requirements":[{"item":"sword","upgrade_sum":{"group":1,"at_least":2}},"# +
                #"{"item":"mace","upgrade_sum":{"group":1,"at_least":3}}]}"#,
            // Effect lists must be non-empty, known, and family-consistent.
            #"{"requirements":[{"kind":"weapon","effect":[]}]}"#,
            #"{"requirements":[{"kind":"weapon","effect":["Thorns"]}]}"#,
            #"{"requirements":[{"kind":"weapon","effect":[3]}]}"#,
            #"{"requirements":[{"kind":"ring","effect":"any_enchantment"}]}"#,
            #"{"requirements":[{"kind":"weapon","effect":{"name":"Grim"}}]}"#,
        ]
        for query in payloads {
            XCTAssertThrowsError(try ResultsExport.decode("""
                {"format":"seed-seeker-results","format_version":1,
                 "query":\(query),"results":[]}
                """), query)
        }
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
