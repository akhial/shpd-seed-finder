import Foundation
import XCTest
@testable import SeedSeekerKit

/// The canonical JSON query document is the one encoding the app hands the
/// engine — search, filter, continuation, start decision and scout marks —
/// and the one share links and results files carry. Its writer rules are the
/// core's (`crates/seedfinder-core/src/json_query.rs`), so besides the golden
/// shapes below every document is pushed through the engine's own encoder
/// (`seedfinder_results_encode`) and must come back byte-for-byte the same.
final class QueryDocumentTests: XCTestCase {
    private func object(_ request: SearchRequest) throws -> [String: Any] {
        try XCTUnwrap(
            try JSONSerialization.jsonObject(with: QueryDocument.encode(request)) as? [String: Any])
    }

    /// The query object as the engine re-encodes it, to compare with ours.
    private func engineNormalized(_ query: SavedQuery) throws -> [String: Any] {
        let text = ResultsExport.encode(query, seeds: ["AAA-AAA-AAA"], appVersion: "test")
        XCTAssertFalse(text.isEmpty, "the engine refused the document")
        let document = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        return try XCTUnwrap(document["query"] as? [String: Any])
    }

    private func assertEngineAgrees(_ query: SavedQuery, file: StaticString = #filePath, line: UInt = #line) throws {
        let ours = ResultsExport.encodeQuery(query) as NSDictionary
        let engine = try engineNormalized(query) as NSDictionary
        XCTAssertEqual(ours, engine, file: file, line: line)
    }

    func testPlainQueryWritesOnlyNonDefaults() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .armor,
                                              tier: 4, tierMatch: .atLeast, upgradeMatch: .any)
        let document = try object(try SearchRequest(requirements: [requirement]))
        XCTAssertEqual(document.keys.sorted(), ["requirements"])
        let entries = try XCTUnwrap(document["requirements"] as? [[String: Any]])
        XCTAssertEqual(entries.count, 1)
        XCTAssertEqual(entries[0]["kind"] as? String, "armor")
        XCTAssertEqual(entries[0]["tier"] as? [String: Int], ["at_least": 4])
        XCTAssertNil(entries[0]["upgrade"])
        XCTAssertNil(entries[0]["effect"])
        XCTAssertEqual(requirement.title, "Any Tier 4+ armor")
    }

    func testLoadedQueryGoldenDocument() throws {
        let dagger = try XCTUnwrap(ItemCatalog.findById("dagger"))
        let first = try ItemRequirement(key: 1, item: dagger, upgrade: 2, modifier: "Lucky",
            kind: .weapon, upgradeMatch: .exactly, source: .chest, identityGroup: 1,
            maximumDepth: 4)
        let second = try ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .thrownWeapon,
            upgradeMatch: .atLeast, requireUncursed: true)
        let request = try SearchRequest(requirements: [first, second], maximumDepth: 12,
                                        requireBlacksmith: true, excludeBlacksmithRewards: true,
                                        wandmakerQuest: .rotberry, challenges: 104)
        let bytes = try QueryDocument.encode(request)
        XCTAssertEqual(bytes.first, UInt8(ascii: "{"), "the engine tells JSON from a packet by its first byte")
        XCTAssertEqual(String(data: bytes, encoding: .utf8), """
            {"challenges":["barren_land","into_darkness","forbidden_runes"],"exclude_blacksmith_rewards":true,\
            "max_depth":12,"require_blacksmith":true,"requirements":[\
            {"effect":"Lucky","identity_group":1,"item":"dagger","kind":"weapon","max_depth":4,\
            "source":"chest","upgrade":2},\
            {"kind":"thrown_weapon","uncursed":true,"upgrade":{"at_least":0}}],\
            "wandmaker_quest":"rotberry"}
            """)
        try assertEngineAgrees(SavedQuery(requirements: [first, second], maximumDepth: 12,
                                          requireBlacksmith: true, excludeBlacksmithRewards: true,
                                          wandmakerQuest: .rotberry, challenges: 104))
    }

    /// Effect lists are written in the shared asset's order: non-curse
    /// effects alphabetically, then curses alphabetically.
    func testEffectListsTakeTheAssetOrder() throws {
        let mixed = try ItemRequirement(key: 1, item: nil, upgrade: 0,
                                        effect: .oneOf(["Wayward", "Chilling", "Annoying", "Blazing"]),
                                        kind: .weapon, upgradeMatch: .any)
        XCTAssertEqual(mixed.effect, .oneOf(["Blazing", "Chilling", "Annoying", "Wayward"]))
        try assertEngineAgrees(SavedQuery(requirements: [mixed]))
        let glyphs = try ItemRequirement(key: 2, item: nil, upgrade: 0,
                                         effect: .oneOf(["Viscosity", "Bulk", "Affection", "Anti-Entropy"]),
                                         kind: .armor, upgradeMatch: .any)
        XCTAssertEqual(glyphs.effect, .oneOf(["Viscosity", "Affection", "Anti-Entropy", "Bulk"]))
        try assertEngineAgrees(SavedQuery(requirements: [glyphs]))
    }

    func testEffectFiltersFollowTheWriterRules() throws {
        let one = try ItemRequirement(key: 1, item: nil, upgrade: 0, effect: .oneOf(["Blazing"]),
                                      kind: .weapon, upgradeMatch: .any)
        let set = try ItemRequirement(key: 2, item: ItemCatalog.findById("greatshield"), upgrade: 2,
                                      effect: .oneOf(["Vampiric", "Blocking", "Projecting"]), kind: .weapon)
        let anyEnchantment = try ItemRequirement(key: 3, item: nil, upgrade: 0, effect: .anyEnchantment,
                                                 kind: .armor, upgradeMatch: .any, requireUncursed: true)
        // The whole non-curse family set is the shorthand, however it was spelled.
        let wholeFamily = try ItemRequirement(key: 4, item: nil, upgrade: 0,
                                              effect: .oneOf(ItemCatalog.glyphs.reversed()),
                                              kind: .armor, upgradeMatch: .any)
        XCTAssertEqual(wholeFamily.effect, .anyEnchantment)
        XCTAssertEqual(one.modifier, "Blazing")
        XCTAssertNil(set.modifier)
        XCTAssertEqual(set.effect, .oneOf(["Blocking", "Projecting", "Vampiric"]), "asset order")
        XCTAssertEqual(set.effect.glowName, "Blocking")

        let entries = try XCTUnwrap(
            try object(SearchRequest(requirements: [one, set, anyEnchantment, wholeFamily]))["requirements"]
                as? [[String: Any]])
        XCTAssertEqual(entries[0]["effect"] as? String, "Blazing")
        XCTAssertEqual(entries[1]["effect"] as? [String], ["Blocking", "Projecting", "Vampiric"])
        XCTAssertEqual(entries[2]["effect"] as? String, "any_enchantment")
        XCTAssertEqual(entries[2]["uncursed"] as? Bool, true)
        XCTAssertEqual(entries[3]["effect"] as? String, "any_enchantment")
        try assertEngineAgrees(SavedQuery(requirements: [one, set, anyEnchantment, wholeFamily]))
    }

    func testAlternativeGroupsWriteOneAnyOfEntryAtTheFirstMembersPosition() throws {
        let spear = try ItemRequirement(key: 1, item: ItemCatalog.findById("spear"), upgrade: 3,
                                        kind: .weapon, alternativeGroup: 7)
        let ring = try ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .ring, upgradeMatch: .any)
        let shuriken = try ItemRequirement(key: 3, item: ItemCatalog.findById("shuriken"), upgrade: 2,
                                           kind: .weapon, alternativeGroup: 7)
        let sword = try ItemRequirement(key: 4, item: ItemCatalog.findById("sword"), upgrade: 1,
                                        kind: .weapon, alternativeGroup: 7)
        let requirements = [spear, ring, shuriken, sword]
        XCTAssertEqual(requirements.slotCount, 2)
        XCTAssertEqual(requirements.slots.map { $0.map(\.key) }, [[1, 3, 4], [2]])

        let entries = try XCTUnwrap(
            try object(SearchRequest(requirements: requirements))["requirements"] as? [[String: Any]])
        XCTAssertEqual(entries.count, 2)
        let members = try XCTUnwrap(entries[0]["any_of"] as? [[String: Any]])
        XCTAssertEqual(members.map { $0["item"] as? String }, ["spear", "shuriken", "sword"])
        XCTAssertEqual(members.map { $0["upgrade"] as? Int }, [3, 2, 1])
        XCTAssertEqual(entries[1]["kind"] as? String, "ring")
        try assertEngineAgrees(SavedQuery(requirements: requirements))

        // A lone member is a plain requirement, so the group id never leaks.
        let single = try object(SearchRequest(requirements: [spear]))
        let plain = try XCTUnwrap((single["requirements"] as? [[String: Any]])?.first)
        XCTAssertNil(plain["any_of"])
        XCTAssertEqual(plain["item"] as? String, "spear")
    }

    func testCombinedLevelGroupsWriteLevelSum() throws {
        let might = try XCTUnwrap(ItemCatalog.findById("ring_might"))
        let first = try ItemRequirement(key: 1, item: might, upgrade: 0, kind: .ring, upgradeMatch: .any,
                                        maximumDepth: 4, levelSum: LevelSum(group: 1, atLeast: 4))
        let second = try ItemRequirement(key: 2, item: might, upgrade: 0, kind: .ring, upgradeMatch: .any,
                                         maximumDepth: 4, levelSum: LevelSum(group: 1, atLeast: 4))
        let entries = try XCTUnwrap(
            try object(SearchRequest(requirements: [first, second]))["requirements"] as? [[String: Any]])
        XCTAssertEqual(entries[0]["level_sum"] as? [String: Int], ["group": 1, "at_least": 4])
        XCTAssertEqual(entries[1]["level_sum"] as? [String: Int], ["group": 1, "at_least": 4])
        XCTAssertNil(entries[0]["upgrade_sum"])
        XCTAssertEqual(first.description, "Any upgrade • levels ≥ 4 together • by floor 4")
        try assertEngineAgrees(SavedQuery(requirements: [first, second]))

        // A same-item group is a stack: the anchor may be constrained, the copies are plain.
        let anchor = try ItemRequirement(key: 3, item: might, upgrade: 2, kind: .ring, identityGroup: 2)
        let copy = try ItemRequirement(key: 4, item: nil, upgrade: 0, kind: .ring, upgradeMatch: .any, identityGroup: 2)
        try assertEngineAgrees(SavedQuery(requirements: [anchor, copy]))
    }

    func testTheUnreleasedUpgradeSumKeyIsRefused() throws {
        let document: [String: Any] = ["requirements": [["kind": "ring", "upgrade_sum": ["group": 1, "at_least": 2]]]]
        XCTAssertThrowsError(try ResultsExport.decodeQuery(document)) { error in
            XCTAssertTrue("\(error)".contains("upgrade_sum"), "\(error)")
        }
        let renamed: [String: Any] = ["requirements": [["kind": "ring", "level_sum": ["group": 1, "at_least": 2]]]]
        XCTAssertEqual(try ResultsExport.decodeQuery(renamed).requirements.first?.levelSum, LevelSum(group: 1, atLeast: 2))
    }

    /// The document decodes back to the models it came from, alternative
    /// groups renumbered from 1 in document order.
    func testDocumentRoundTripsThroughTheEngineShareCodec() throws {
        let requirements = [
            try ItemRequirement(key: 1, item: ItemCatalog.findById("spear"), upgrade: 3,
                                kind: .weapon, alternativeGroup: 9),
            try ItemRequirement(key: 2, item: nil, upgrade: 0, effect: .oneOf(["Blocking", "Projecting"]),
                                kind: .thrownWeapon, upgradeMatch: .any, alternativeGroup: 9),
            try ItemRequirement(key: 3, item: nil, upgrade: 0, effect: .anyEnchantment,
                                kind: .armor, upgradeMatch: .any, requireUncursed: true),
            try ItemRequirement(key: 4, item: ItemCatalog.findById("ring_might"), upgrade: 0, kind: .ring,
                                upgradeMatch: .any, levelSum: LevelSum(group: 2, atLeast: 4)),
            try ItemRequirement(key: 5, item: ItemCatalog.findById("ring_might"), upgrade: 0, kind: .ring,
                                upgradeMatch: .any, levelSum: LevelSum(group: 2, atLeast: 4)),
        ]
        let query = SavedQuery(requirements: requirements, maximumDepth: 20)
        let link = try DeepLink.encodeLink(for: query)
        let decoded = try DeepLink.decode(link)
        XCTAssertEqual(decoded.requirements.count, 5)
        XCTAssertEqual(decoded.requirements.map(\.alternativeGroup), [1, 1, nil, nil, nil])
        XCTAssertEqual(decoded.requirements.map(\.effect),
                       [.any, .oneOf(["Blocking", "Projecting"]), .anyEnchantment, .any, .any])
        XCTAssertEqual(decoded.requirements.map(\.levelSum),
                       [nil, nil, nil, LevelSum(group: 2, atLeast: 4), LevelSum(group: 2, atLeast: 4)])
        XCTAssertEqual(decoded.requirements.slotCount, 4)
        XCTAssertNotNil(decoded.validated())
        XCTAssertNoThrow(try SearchRequest(requirements: decoded.requirements))

        // The same document through the results-file codec.
        let imported = try ResultsExport.decode(
            ResultsExport.encode(query, seeds: ["AAA-AAA-BUH"], appVersion: "test"))
        XCTAssertEqual(imported.query.requirements.map(\.alternativeGroup), [1, 1, nil, nil, nil])
        XCTAssertEqual(imported.query.requirements[1].effect, .oneOf(["Blocking", "Projecting"]))
    }

    func testDecoderReadsEveryEffectSpellingAndNestedGroups() throws {
        let imported = try ResultsExport.decode("""
            {"format":"seed-seeker-results",
             "query":{"requirements":[
               {"any_of":[{"item":"spear","upgrade":3},{"item":"shuriken","upgrade":2}]},
               {"kind":"weapon","effect":["blocking","PROJECTING"]},
               {"kind":"armor","effect":"ANY_ENCHANTMENT"},
               {"any_of":[{"kind":"ring"},{"kind":"wand"}]}
             ]},
             "results":[]}
            """)
        let requirements = imported.query.requirements
        XCTAssertEqual(requirements.map(\.alternativeGroup), [1, 1, nil, nil, 2, 2])
        XCTAssertEqual(requirements.map(\.key), [1, 2, 3, 4, 5, 6])
        XCTAssertEqual(requirements[2].effect, .oneOf(["Blocking", "Projecting"]))
        XCTAssertEqual(requirements[3].effect, .anyEnchantment)
        XCTAssertEqual(requirements.slotCount, 4)
    }

    // MARK: Engine transport

    func testEngineAcceptsTheDocumentForEveryQueryTakingEntryPoint() async throws {
        let requirement = try ItemRequirement(key: 1, item: ItemCatalog.findById("wand_frost"),
                                              upgrade: 2, kind: .wand)
        let request = try SearchRequest(requirements: [requirement])
        let engine = ProductionSeedFinderEngine()
        let session = try await engine.startSearch(request)
        await session.cancel(); await session.close()
        let resumed = try await engine.startResumedSearch(request, resumeFrom: 0, scanLen: 1)
        await resumed.cancel(); await resumed.close()
        _ = try await engine.filterSeeds(request, seeds: ["AAA-AAA-AAA"])
        XCTAssertTrue(request.isRefinement(of: request))
        XCTAssertEqual(StartDecision.decide(candidate: request, target: request, targetSetEmpty: false,
                                            targetHasUncoveredSeeds: false, detachedBase: nil), .targetRefine)
        XCTAssertEqual(try ScoutMatches.mark(seed: "AAA-AAA-AAA", challenges: 0, query: request).totalRequirements, 1)
    }

    /// The scouted world of a pinned seed; the indices below name its items.
    private static let pinnedSeed = "AAA-AAA-BUH"
    private func marks(_ requirements: [ItemRequirement]) throws -> ScoutMatches {
        try ScoutMatches.mark(seed: Self.pinnedSeed, challenges: 0,
                              query: SearchRequest(requirements: requirements))
    }

    func testScoutCountsAnAlternativeGroupAsOneSlot() async throws {
        let world = try await ProductionSeedFinderEngine().scoutSeed(Self.pinnedSeed, challenges: 0)
        let sharpshooting = try XCTUnwrap(ItemCatalog.findById("ring_sharpshooting"))
        let corrosion = try XCTUnwrap(ItemCatalog.findById("wand_corrosion"))
        // The world's only Wand of Corrosion is cursed; the ring is there.
        let wand = try ItemRequirement(key: 1, item: corrosion, upgrade: 0, kind: .wand,
                                       upgradeMatch: .any, requireUncursed: true, alternativeGroup: 1)
        let ring = try ItemRequirement(key: 2, item: sharpshooting, upgrade: 1, kind: .ring, alternativeGroup: 1)
        let either = try marks([wand, ring])
        XCTAssertEqual(either.totalRequirements, 1, "two alternatives are one slot")
        XCTAssertEqual(either.matchedRequirements, 1)
        XCTAssertEqual(either.matched.count, 1)
        XCTAssertEqual(world.items[try XCTUnwrap(either.matched.first)].item.id, "ring_sharpshooting")

        let might = try ItemRequirement(key: 2, item: ItemCatalog.findById("ring_might"), upgrade: 4,
                                        kind: .ring, requireUncursed: true, alternativeGroup: 1)
        let neither = try marks([wand, might])
        XCTAssertEqual(neither.totalRequirements, 1)
        XCTAssertEqual(neither.matchedRequirements, 0)
        XCTAssertTrue(neither.matched.isEmpty)
    }

    func testScoutMarksACombinedLevelGroupAsOneCondition() async throws {
        let world = try await ProductionSeedFinderEngine().scoutSeed(Self.pinnedSeed, challenges: 0)
        let tenacity = try XCTUnwrap(ItemCatalog.findById("ring_tenacity"))
        func pair(total: Int) throws -> [ItemRequirement] {
            try [1, 2].map { key in
                try ItemRequirement(key: Int64(key), item: tenacity, upgrade: 0, kind: .ring, upgradeMatch: .any,
                                    requireUncursed: true, levelSum: LevelSum(group: 1, atLeast: total))
            }
        }
        // The world's Rings of Tenacity reach four levels between them: one of
        // the vault prize's +2 options and a mimic's plain +0. The group is
        // one condition, and every contributing item is marked.
        let reached = try marks(try pair(total: 4))
        XCTAssertEqual(reached.totalRequirements, 1, "a combined-level group is one condition")
        XCTAssertEqual(reached.matchedRequirements, 1)
        XCTAssertEqual(Set(reached.matched.map { world.items[$0].upgrade }), [0, 2])

        // Members are optional: the +2 ring's three levels satisfy a total of 3 alone.
        let single = try marks(try pair(total: 3))
        XCTAssertEqual(single.matchedRequirements, 1)
        XCTAssertFalse(single.matched.isEmpty)

        let short = try marks(try pair(total: 5))
        XCTAssertEqual(short.totalRequirements, 1)
        XCTAssertEqual(short.matchedRequirements, 0)
        XCTAssertTrue(short.matched.isEmpty, "a short level group marks nothing")
    }

    func testScoutMatchesEffectSetsAndAnyEnchantment() async throws {
        let world = try await ProductionSeedFinderEngine().scoutSeed(Self.pinnedSeed, challenges: 0)
        let leather = try XCTUnwrap(ItemCatalog.findById("leather_armor"))
        // Among the world's leather armors: one Obfuscation, one cursed and Multiplicity.
        let obfuscation = try marks([ItemRequirement(key: 1, item: leather, upgrade: 0,
                                                     effect: .oneOf(["Swiftness", "Obfuscation"]),
                                                     kind: .armor, upgradeMatch: .any)])
        XCTAssertEqual(obfuscation.matchedRequirements, 1)
        XCTAssertEqual(world.items[try XCTUnwrap(obfuscation.matched.first)].effect, "Obfuscation")
        let multiplicity = try marks([ItemRequirement(key: 1, item: leather, upgrade: 0,
                                                      effect: .oneOf(["Multiplicity"]), kind: .armor, upgradeMatch: .any)])
        XCTAssertEqual(multiplicity.matchedRequirements, 1)
        XCTAssertEqual(world.items[try XCTUnwrap(multiplicity.matched.first)].effect, "Multiplicity")
        XCTAssertEqual(try marks([ItemRequirement(key: 1, item: leather, upgrade: 0, effect: .oneOf(["Multiplicity"]),
                                                  kind: .armor, upgradeMatch: .any, requireUncursed: false)])
                           .matchedRequirements, 1)
        let enchanted = try marks([ItemRequirement(key: 1, item: leather, upgrade: 0, effect: .anyEnchantment,
                                                   kind: .armor, upgradeMatch: .any)])
        XCTAssertEqual(enchanted.matchedRequirements, 1)
        XCTAssertEqual(world.items[try XCTUnwrap(enchanted.matched.first)].effect, "Obfuscation",
                       "a curse is not a glyph")
        // The enchantments v4.0.0 added are searchable like any other: the
        // world's Dirk from the statue carries Venomous.
        let dirk = try XCTUnwrap(ItemCatalog.findById("dirk"))
        let venomous = try marks([ItemRequirement(key: 1, item: dirk, upgrade: 0,
                                                  effect: .oneOf(["Venomous", "Vorpal"]),
                                                  kind: .weapon, upgradeMatch: .any)])
        XCTAssertEqual(venomous.matchedRequirements, 1)
        XCTAssertEqual(world.items[try XCTUnwrap(venomous.matched.first)].effect, "Venomous")
        let plain = try XCTUnwrap(ItemCatalog.findById("shortsword"))
        XCTAssertEqual(try marks([ItemRequirement(key: 1, item: plain, upgrade: 0, effect: .anyEnchantment,
                                                  kind: .weapon, upgradeMatch: .any)]).matchedRequirements, 0,
                       "the world's Shortsword carries no enchantment")
    }

    // MARK: Local validation and summaries

    func testCombinedLevelGroupsAreValidatedAcrossTheQuery() throws {
        let might = try XCTUnwrap(ItemCatalog.findById("ring_might"))
        func ring(_ key: Int64, upgrade: Int = 0, match: UpgradeMatch = .any, total: Int, group: Int = 1) throws -> ItemRequirement {
            try ItemRequirement(key: key, item: might, upgrade: upgrade, kind: .ring, upgradeMatch: match,
                                levelSum: LevelSum(group: group, atLeast: total))
        }
        // A ring reaches +4 (five levels), but only one per world — the Imp
        // vault's prize; every other ring stops at +2 (three levels). Two
        // rings therefore reach eight levels together, not ten.
        XCTAssertNoThrow(try SearchRequest(requirements: [ring(1, total: 8), ring(2, total: 8)]))
        XCTAssertThrowsError(try SearchRequest(requirements: [ring(1, total: 9), ring(2, total: 9)])) { error in
            XCTAssertEqual(error as? ModelValidationError,
                           .levelSumUnattainable(group: 1, needed: 9, maximum: 8))
        }
        XCTAssertNoThrow(try SearchRequest(requirements: [
            ring(1, total: 11), ring(2, total: 11), ring(3, total: 11),
        ]))
        XCTAssertThrowsError(try SearchRequest(requirements: [
            ring(1, total: 12), ring(2, total: 12), ring(3, total: 12),
        ])) { error in
            XCTAssertEqual(error as? ModelValidationError,
                           .levelSumUnattainable(group: 1, needed: 12, maximum: 11))
        }
        XCTAssertThrowsError(try SearchRequest(requirements: [ring(1, total: 3), ring(2, total: 4)])) { error in
            XCTAssertEqual(error as? ModelValidationError, .levelSumMismatch(group: 1))
            XCTAssertEqual(error.localizedDescription,
                           "Combined level group A must share one total across its items")
        }
        // An exact upgrade counts as itself, anything else as the family cap —
        // plus one level each for the item itself.
        XCTAssertThrowsError(try SearchRequest(requirements: [
            ring(1, upgrade: 1, match: .exactly, total: 8, group: 2), ring(2, total: 8, group: 2),
        ])) { error in
            XCTAssertEqual(error as? ModelValidationError,
                           .levelSumUnattainable(group: 2, needed: 8, maximum: 7))
            XCTAssertEqual(error.localizedDescription,
                           "Combined level group B needs 8 levels but its items can reach at most 7")
        }
        XCTAssertNoThrow(try SearchRequest(requirements: [
            ring(1, upgrade: 1, match: .exactly, total: 7, group: 2), ring(2, total: 7, group: 2),
        ]))
        XCTAssertThrowsError(try SearchRequest(requirements: [ring(1, total: 6, group: 3)])) { error in
            XCTAssertEqual(error as? ModelValidationError,
                           .levelSumUnattainable(group: 3, needed: 6, maximum: 5))
        }
        // Levels only combine meaningfully across rings; no other family's
        // effects add up that way.
        XCTAssertThrowsError(try ItemRequirement(key: 3, item: nil, upgrade: 0, kind: .wand,
                                                 upgradeMatch: .any,
                                                 levelSum: LevelSum(group: 3, atLeast: 3))) { error in
            XCTAssertEqual(error as? ModelValidationError, .levelSumOutsideRings)
            XCTAssertEqual(error.localizedDescription, "Only rings can count levels together")
        }
        XCTAssertNoThrow(try SearchRequest(requirements: [ring(1, total: 4), ring(2, total: 3, group: 2)]))
    }

    func testSameItemGroupsAreStacksWithOneAnchor() throws {
        let might = try XCTUnwrap(ItemCatalog.findById("ring_might"))
        func named(_ key: Int64, group: Int = 1, alternative: Int? = nil) throws -> ItemRequirement {
            try ItemRequirement(key: key, item: might, upgrade: 2, kind: .ring,
                                identityGroup: group, alternativeGroup: alternative)
        }
        func plain(_ key: Int64, kind: ItemKind = .ring, group: Int = 1, maximumDepth: Int? = nil) throws -> ItemRequirement {
            try ItemRequirement(key: key, item: nil, upgrade: 0, kind: kind, upgradeMatch: .any,
                                identityGroup: group, maximumDepth: maximumDepth)
        }
        // One anchor with plain copies is the intended shape; a floor limit on a copy is fine.
        XCTAssertNoThrow(try SearchRequest(requirements: [named(1), plain(2), plain(3, maximumDepth: 6)]))
        XCTAssertNoThrow(try SearchRequest(requirements: [plain(1), plain(2)]))
        // Two constrained members would force two described items to be one.
        XCTAssertThrowsError(try SearchRequest(requirements: [named(1), named(2)])) { error in
            XCTAssertEqual(error as? ModelValidationError, .identityGroupOverconstrained(group: 1))
            XCTAssertEqual(error.localizedDescription,
                           "Same-item group A can describe one item (or one set of alternatives); its other members must be plain")
        }
        let uncursed = try ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .ring, upgradeMatch: .any,
                                           identityGroup: 1, requireUncursed: true)
        XCTAssertThrowsError(try SearchRequest(requirements: [named(1), uncursed]))
        // The members of one alternative group form a single anchor unit.
        XCTAssertNoThrow(try SearchRequest(requirements: [named(1, alternative: 1), named(2, alternative: 1), plain(3)]))
        XCTAssertThrowsError(try SearchRequest(requirements: [named(1, alternative: 1), named(2, alternative: 1), named(3)]))
        // Members of different categories never describe one item; a narrowed kind is a constraint.
        XCTAssertThrowsError(try SearchRequest(requirements: [named(1), plain(2, kind: .wand)])) { error in
            XCTAssertEqual(error as? ModelValidationError, .identityGroupMixedKinds(group: 1))
        }
        let thrown = try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .thrownWeapon, upgradeMatch: .any, identityGroup: 2)
        let weapon = try ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .weapon, upgradeMatch: .any, identityGroup: 2)
        XCTAssertFalse(thrown.isBare); XCTAssertTrue(weapon.isBare)
        XCTAssertNoThrow(try SearchRequest(requirements: [thrown, weapon]))
        // Separate groups are separate stacks.
        XCTAssertNoThrow(try SearchRequest(requirements: [named(1), named(2, group: 2)]))
    }

    func testSummaryTextDescribesTheNewState() throws {
        let set = try ItemRequirement(key: 1, item: ItemCatalog.findById("greatshield"), upgrade: 2,
                                      effect: .oneOf(["Projecting", "Blocking"]), kind: .weapon)
        XCTAssertEqual(set.description, "+2 exactly • Blocking/Projecting")
        let enchanted = try ItemRequirement(key: 2, item: nil, upgrade: 0, effect: .anyEnchantment,
                                            kind: .weapon, upgradeMatch: .any, requireUncursed: true)
        XCTAssertEqual(enchanted.description, "Any upgrade • any enchantment • uncursed")
        let glyphed = try ItemRequirement(key: 3, item: nil, upgrade: 1, effect: .anyEnchantment,
                                          kind: .armor, upgradeMatch: .atLeast)
        XCTAssertEqual(glyphed.description, "+1 or higher • any glyph")
        let summed = try ItemRequirement(key: 4, item: nil, upgrade: 0, kind: .ring, upgradeMatch: .any,
                                         levelSum: LevelSum(group: 2, atLeast: 4))
        XCTAssertEqual(summed.description, "Any upgrade • levels ≥ 4 together")
        XCTAssertEqual(summed.maximumContributedUpgrade, 4)
        XCTAssertEqual(summed.maximumLevel, 5)
        let exact = try ItemRequirement(key: 5, item: nil, upgrade: 2, kind: .wand)
        XCTAssertEqual(exact.maximumContributedUpgrade, 2)
        XCTAssertEqual(exact.maximumLevel, 3)
        XCTAssertEqual(groupLetter(1), "A"); XCTAssertEqual(groupLetter(4), "D")
    }

    // MARK: Persistence

    func testSavedQueriesFromOlderBuildsStillLoad() throws {
        // `fastMode` is a retired key kept in this fixture deliberately: a
        // query saved while the toggle existed must still load, the flag read
        // past and ignored, as an ordinary full search.
        let legacy = """
        {"requirements":[{"key":1,"upgrade":2,"modifier":"Lucky","kind":0,"tier":0,"tierMatch":0,\
        "upgradeMatch":1,"requireUncursed":false},\
        {"key":2,"upgrade":0,"kind":3,"tier":0,"tierMatch":0,"upgradeMatch":0,"identityGroup":1,\
        "requireUncursed":true}],\
        "maximumDepth":12,"requireBlacksmith":false,"excludeBlacksmithRewards":false,"fastMode":true,"challenges":0}
        """
        let decoded = QueryPersistence.decode(legacy)
        XCTAssertEqual(decoded.requirements.count, 2)
        XCTAssertEqual(decoded.requirements[0].effect, .oneOf(["Lucky"]))
        XCTAssertEqual(decoded.requirements[0].modifier, "Lucky")
        XCTAssertEqual(decoded.requirements.map(\.alternativeGroup), [nil, nil])
        XCTAssertEqual(decoded.requirements.map(\.levelSum), [nil, nil])
        XCTAssertEqual(decoded.maximumDepth, 12)
    }

    func testSavedQueriesCarryTheNewFieldsAdditively() throws {
        let query = SavedQuery(requirements: [
            try ItemRequirement(key: 1, item: nil, upgrade: 0, effect: .oneOf(["Blazing"]),
                                kind: .weapon, upgradeMatch: .any, alternativeGroup: 3),
            try ItemRequirement(key: 2, item: nil, upgrade: 0, effect: .oneOf(["Blazing", "Chilling"]),
                                kind: .weapon, upgradeMatch: .any, alternativeGroup: 3),
            try ItemRequirement(key: 3, item: nil, upgrade: 0, effect: .anyEnchantment,
                                kind: .armor, upgradeMatch: .any),
            try ItemRequirement(key: 4, item: nil, upgrade: 0, kind: .ring, upgradeMatch: .any,
                                levelSum: LevelSum(group: 1, atLeast: 5)),
        ])
        let text = try XCTUnwrap(QueryPersistence.encode(query))
        let object = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        let saved = try XCTUnwrap(object["requirements"] as? [[String: Any]])
        // A single effect keeps the classic key, so older builds still read it.
        XCTAssertEqual(saved[0]["modifier"] as? String, "Blazing")
        XCTAssertNil(saved[0]["effect"])
        XCTAssertEqual(saved[1]["effect"] as? [String], ["Blazing", "Chilling"])
        XCTAssertEqual(saved[2]["effect"] as? String, "any_enchantment")
        XCTAssertEqual(saved[3]["levelSum"] as? [String: Int], ["group": 1, "atLeast": 5])
        XCTAssertEqual(QueryPersistence.decode(text), query)

        let presets = try XCTUnwrap(PresetPersistence.encode([QueryPreset(name: "Complex", query: query)]))
        XCTAssertEqual(PresetPersistence.decode(presets).map(\.query), [query])
        // A saved effect this build does not know drops the query, as before.
        XCTAssertEqual(QueryPersistence.decode(text.replacingOccurrences(of: "Chilling", with: "Frosty"))
                           .requirements, [])
    }

    func testBuiltInPresetsAreUnchangedAndStillEncode() throws {
        for preset in BuiltInPresets.all {
            XCTAssertEqual(preset.query.requirements.slotCount, preset.query.requirements.count, preset.name)
            XCTAssertTrue(preset.query.requirements.allSatisfy { $0.effect == .any && $0.levelSum == nil }, preset.name)
            try assertEngineAgrees(preset.query)
        }
    }
}
