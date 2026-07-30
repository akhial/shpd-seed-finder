import Foundation
import SeedSeekerKit
import XCTest

final class SeedSeekerKitTests: XCTestCase {
    func testBundledStaffPreset() throws {
        let preset = BuiltInPresets.staff21
        XCTAssertEqual(preset.name, "+21 Staff")
        XCTAssertEqual(preset.query.requirements.count, 4)
        XCTAssertEqual(preset.query.requirements.map(\.kind), [.wand, .wand, .wand, .wand])
        XCTAssertEqual(preset.query.requirements.map(\.upgradeMatch), [.exactly, .any, .any, .atLeast])
        XCTAssertEqual(preset.query.requirements.map(\.upgrade), [3, 0, 0, 1])
        XCTAssertEqual(preset.query.requirements.map(\.identityGroup), [1, 1, 1, nil])
        XCTAssertNotNil(preset.query.validated())
    }

    func testBundledWandBonanzaPreset() throws {
        let preset = BuiltInPresets.wandBonanza
        XCTAssertEqual(preset.name, "Wand Bonanza")
        XCTAssertEqual(preset.query.requirements.map(\.kind), [.wand, .wand, .wand, .wand])
        XCTAssertEqual(preset.query.requirements.map(\.item), [nil, nil, nil, nil])
        XCTAssertEqual(preset.query.requirements.map(\.upgradeMatch), [.exactly, .exactly, .exactly, .exactly])
        XCTAssertEqual(preset.query.requirements.map(\.upgrade), [3, 2, 2, 2])
        XCTAssertEqual(preset.query.requirements.map(\.maximumDepth), [nil, 4, 4, nil])
        XCTAssertEqual(preset.query.requirements.map(\.identityGroup), [nil, nil, nil, nil])
        XCTAssertNotNil(preset.query.validated())
    }

    func testBundledRingOfWealthPreset() throws {
        let preset = BuiltInPresets.ringOfWealth21
        XCTAssertEqual(preset.name, "+21 Ring of Wealth")
        XCTAssertEqual(preset.query.requirements.map(\.item?.id),
                       ["ring_wealth", "ring_wealth", "ring_wealth"])
        XCTAssertEqual(preset.query.requirements.map(\.upgradeMatch), [.exactly, .exactly, .any])
        XCTAssertEqual(preset.query.requirements.map(\.upgrade), [4, 2, 0])
        XCTAssertEqual(preset.query.requirements.map(\.maximumDepth), [nil, nil, nil])
        XCTAssertEqual(preset.query.requirements.first?.source, .impReward)
        XCTAssertNotNil(preset.query.validated())
    }

    func testPresetPersistenceDropsInvalidEntries() throws {
        let requirement = try ItemRequirement(key: 99, item: nil, upgrade: 1, kind: .wand,
                                              upgradeMatch: .atLeast, requireUncursed: true)
        let valid = QueryPreset(name: "My preset",
                                query: SavedQuery(requirements: [requirement]))
        let invalid = QueryPreset(name: "   ", query: BuiltInPresets.ringOfWealth21.query)
        let encoded = try XCTUnwrap(PresetPersistence.encode([valid, invalid]))
        let decoded = PresetPersistence.decode(encoded)
        XCTAssertEqual(decoded, [valid])
        XCTAssertEqual(decoded.first?.query.requirements.first?.requireUncursed, true)
        XCTAssertEqual(PresetPersistence.decode("not json"), [])
    }

    func testPresetPersistenceDropsOnlyUnreadableElements() throws {
        // A preset written by a future build (say, an unknown kind raw value)
        // must drop alone instead of taking the whole collection with it.
        let requirement = try ItemRequirement(key: 7, item: nil, upgrade: 0, kind: .thrownWeapon,
                                              upgradeMatch: .any)
        let valid = QueryPreset(name: "Thrown", query: SavedQuery(requirements: [requirement]))
        let encoded = try XCTUnwrap(PresetPersistence.encode([valid]))
        let future = """
        {"id":"6F9619FF-8B86-D011-B42D-00C04FC964FF","name":"Future","query":{"requirements":\
        [{"key":1,"upgrade":0,"kind":99,"tier":0,"tierMatch":0,"upgradeMatch":0,\
        "requireUncursed":false}],"maximumDepth":24,"requireBlacksmith":false,\
        "excludeBlacksmithRewards":false,"fastMode":false,"challenges":0}}
        """
        // Splice the valid preset into an array after two unreadable elements.
        let futuristic = "[" + future + ",\"garbage\"," + String(encoded.dropFirst())
        let decoded = PresetPersistence.decode(futuristic)
        XCTAssertEqual(decoded, [valid])
    }

    func testScoutMatchesSelectOnlyOneMutuallyExclusiveReward() throws {
        let warding = try XCTUnwrap(ItemCatalog.findById("wand_warding"))
        let light = try XCTUnwrap(ItemCatalog.findById("wand_prismatic_light"))
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 3, kind: .wand,
                                              upgradeMatch: .exactly, source: .wandmakerReward)
        let items = [
            ScoutItem(item: warding, depth: 8, upgrade: 3, source: .wandmakerReward,
                      accessibility: .choice(group: 2, option: 0)),
            ScoutItem(item: light, depth: 8, upgrade: 3, source: .wandmakerReward,
                      accessibility: .choice(group: 2, option: 1)),
        ]

        XCTAssertEqual(scoutMatchIndices(items: items, requirements: [requirement]), [0])
    }

    func testScoutMatchesRespectCompatibleScenarioMasksAndDistinctRequirements() throws {
        let warding = try XCTUnwrap(ItemCatalog.findById("wand_warding"))
        let light = try XCTUnwrap(ItemCatalog.findById("wand_prismatic_light"))
        let requirements = try [warding, light].enumerated().map { index, item in
            try ItemRequirement(key: Int64(index), item: item, upgrade: 3, kind: .wand,
                                upgradeMatch: .exactly)
        }
        let compatible = [
            ScoutItem(item: warding, depth: 8, upgrade: 3, source: .wandmakerReward,
                      accessibility: .scenarios(group: 4, mask: 0b11)),
            ScoutItem(item: light, depth: 8, upgrade: 3, source: .wandmakerReward,
                      accessibility: .scenarios(group: 4, mask: 0b10)),
        ]
        let incompatible = [compatible[0],
            ScoutItem(item: light, depth: 8, upgrade: 3, source: .wandmakerReward,
                      accessibility: .scenarios(group: 4, mask: 0b100))]

        XCTAssertEqual(scoutMatchIndices(items: compatible, requirements: requirements), [0, 1])
        XCTAssertEqual(scoutMatchIndices(items: incompatible, requirements: requirements).count, 1)
    }

    func testScoutMatchesRequireUncursedItems() throws {
        let warding = try XCTUnwrap(ItemCatalog.findById("wand_warding"))
        let requirement = try ItemRequirement(key: 1, item: warding, upgrade: 3,
                                               kind: .wand, requireUncursed: true)
        let clean = ScoutItem(item: warding, depth: 8, upgrade: 3,
                              source: .wandmakerReward)
        let cursed = ScoutItem(item: warding, depth: 8, upgrade: 3, cursed: true,
                               source: .wandmakerReward)

        XCTAssertEqual(scoutMatchIndices(items: [clean, cursed], requirements: [requirement]), [0])
        XCTAssertTrue(scoutMatchIndices(items: [cursed], requirements: [requirement]).isEmpty)
    }

    func testQueryCodecTierPredicateUsesSSF8WithZeroChallenges() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .armor,
            tier: 4, tierMatch: .atLeast, upgradeMatch: .any)
        let request = try SearchRequest(requirements: [requirement])
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 56, 24, 0, 0, 0, 0, 1,
            1, 0, 0, 2, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ])
        XCTAssertEqual(requirement.title, "Any Tier 4+ armor")
    }

    func testQueryCodecMeleeAndThrownKindsUseWireIdsFourAndFive() throws {
        let melee = try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .meleeWeapon,
            upgradeMatch: .any)
        XCTAssertEqual(try QueryCodec.encode(SearchRequest(requirements: [melee]))[10], 4)
        XCTAssertEqual(melee.title, "Any melee weapon")

        let shuriken = try XCTUnwrap(ItemCatalog.findById("shuriken"))
        let thrown = try ItemRequirement(key: 2, item: shuriken, upgrade: 0, kind: .thrownWeapon,
            upgradeMatch: .any)
        XCTAssertEqual(try QueryCodec.encode(SearchRequest(requirements: [thrown]))[10], 5)
    }

    func testWeaponClassificationAndNarrowedKindValidation() throws {
        XCTAssertEqual(ItemCatalog.meleeWeapons.count, 31)
        XCTAssertEqual(ItemCatalog.thrownWeapons.count, 27)
        XCTAssertEqual(ItemCatalog.weapons, ItemCatalog.meleeWeapons + ItemCatalog.thrownWeapons)
        XCTAssertEqual(ItemCatalog.weaponClass(of: "crossbow"), .melee)
        XCTAssertEqual(ItemCatalog.weaponClass(of: "shuriken"), .thrown)
        XCTAssertEqual(ItemCatalog.weaponClass(of: "poison_dart"), .thrown)
        XCTAssertNil(ItemCatalog.weaponClass(of: "plate_armor"))
        XCTAssertEqual(ItemCatalog.forKind(.meleeWeapon), ItemCatalog.meleeWeapons)
        XCTAssertEqual(ItemCatalog.forKind(.thrownWeapon), ItemCatalog.thrownWeapons)
        XCTAssertEqual(ItemCatalog.modifiersFor(.thrownWeapon), ItemCatalog.modifiersFor(.weapon))
        XCTAssertEqual(ItemCatalog.cursesFor(.meleeWeapon), ItemCatalog.cursesFor(.weapon))

        // A narrowed kind accepts only items of its class; the broad kind takes both.
        let sword = try XCTUnwrap(ItemCatalog.findById("sword"))
        let shuriken = try XCTUnwrap(ItemCatalog.findById("shuriken"))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: sword, upgrade: 1, kind: .meleeWeapon))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: shuriken, upgrade: 1, kind: .thrownWeapon))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: shuriken, upgrade: 1, kind: .weapon))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: shuriken, upgrade: 1, kind: .meleeWeapon))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: sword, upgrade: 1, kind: .thrownWeapon))
        // Wildcard narrowed kinds keep tier filters and enchantments.
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 0,
            effect: .oneOf(["Projecting"]),
            kind: .thrownWeapon, tier: 5, tierMatch: .exactly, upgradeMatch: .any))
    }

    func testScoutMatchesRespectWeaponClassNarrowingAndNarrowedKindsCompose() throws {
        let sword = try XCTUnwrap(ItemCatalog.findById("sword"))
        let shuriken = try XCTUnwrap(ItemCatalog.findById("shuriken"))
        let items = [ScoutItem(item: sword, depth: 3, upgrade: 1, source: .heap),
                     ScoutItem(item: shuriken, depth: 4, upgrade: 1, source: .heap)]
        let melee = try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .meleeWeapon,
                                        upgradeMatch: .any)
        let thrown = try ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .thrownWeapon,
                                         upgradeMatch: .any)
        let broad = try ItemRequirement(key: 3, item: nil, upgrade: 0, kind: .weapon,
                                        upgradeMatch: .any)
        XCTAssertEqual(scoutMatchIndices(items: items, requirements: [melee]), [0])
        XCTAssertEqual(scoutMatchIndices(items: items, requirements: [thrown]), [1])
        XCTAssertEqual(scoutMatchIndices(items: items, requirements: [broad]).count, 1)

        // A narrowed kind joins alternative and combined-upgrade groups like
        // a broad weapon, contributing up to the weapon cap (+3).
        let narrowedAlternative = try ItemRequirement(key: 4, item: nil, upgrade: 0,
            kind: .thrownWeapon, upgradeMatch: .any, alternativeGroup: 1)
        XCTAssertNoThrow(try ItemRequirement(key: 5, item: nil, upgrade: 0, kind: .meleeWeapon,
            upgradeMatch: .any, upgradeSumGroup: 1, upgradeSumTotal: 2))
        XCTAssertEqual(narrowedAlternative.alternativeGroup, 1)
        XCTAssertEqual(ItemKind.meleeWeapon.maximumSearchUpgrade, 3)
        XCTAssertEqual(ItemKind.thrownWeapon.maximumSearchUpgrade, 3)
        let sums = try [6, 7].map { key in
            try ItemRequirement(key: Int64(key), item: nil, upgrade: 0, kind: .meleeWeapon,
                                upgradeMatch: .any, upgradeSumGroup: 1, upgradeSumTotal: 7)
        }
        XCTAssertEqual(try SearchRequest(requirements: sums).unattainableUpgradeSumMessage,
                       "Combined upgrade group A asks for +7 but its items can reach at most +6 together.")
    }

    func testAnyEnchantmentOnNarrowedWeaponKindsUsesTheWeaponFamily() throws {
        let melee = try ItemRequirement(key: 1, item: nil, upgrade: 0,
            effect: .anyEnchantment, kind: .meleeWeapon, upgradeMatch: .any)
        XCTAssertEqual(melee.description, "Any upgrade • any enchantment")
        // The SSF8 encoder expands "any enchantment" to the weapon family's
        // non-curse set for narrowed kinds, not the glyph list.
        let packet = Array(try QueryCodec.encode(SearchRequest(requirements: [melee])))
        var expected: [UInt8] = [83, 83, 70, 56, 24, 0, 0, 0, 0, 1,
                                 4, 0, 0, 0, 0, 0, 0, 1, 13]
        for name in ItemCatalog.enchantments {
            expected += [0, UInt8(name.utf8.count)] + Array(name.utf8)
        }
        expected += [0, 0, 0, 0, 0, 0, 0]
        XCTAssertEqual(packet, expected)
    }

    func testQueryCodecEncodesAtMostTierPredicate() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .armor,
            tier: 4, tierMatch: .atMost, upgradeMatch: .any)
        let request = try SearchRequest(requirements: [requirement])
        let packet = Array(try QueryCodec.encode(request))
        XCTAssertEqual(Array(packet[13..<15]), [3, 4])
        XCTAssertEqual(requirement.title, "Any Tier 4 or lower armor")
    }

    func testQueryCodecGoldenTwoRequirements() throws {
        let dagger = try XCTUnwrap(ItemCatalog.findById("dagger"))
        let first = try ItemRequirement(key: 1, item: dagger, upgrade: 2,
            effect: .oneOf(["Lucky"]),
            kind: .weapon, upgradeMatch: .exactly, source: .chest, identityGroup: 1,
            maximumDepth: 5)
        let second = try ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .ring,
            upgradeMatch: .atLeast)
        let request = try SearchRequest(requirements: [first, second], maximumDepth: 12,
                                        requireBlacksmith: true, challenges: 104)
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 56, 12, 1, 104, 0, 0, 2,
            0, 0, 6, 100, 97, 103, 103, 101, 114, 0, 0, 1, 2,
            1, 1, 0, 5, 76, 117, 99, 107, 121, 2, 1, 5, 0, 0, 0, 0,
            3, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ])
    }

    func testQueryCodecEncodesOneOfEffectListPreservingOrder() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 0,
            effect: .oneOf(["Vampiric", "Blocking"]), kind: .weapon, upgradeMatch: .any)
        let request = try SearchRequest(requirements: [requirement])
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 56, 24, 0, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 0, 0,
            1, 2,
            0, 8] + Array("Vampiric".utf8) + [0, 8] + Array("Blocking".utf8) + [
            0, 0, 0, 0, 0, 0, 0,
        ])
    }

    func testQueryCodecExpandsAnyEnchantmentToFamilyNonCurseSet() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 0,
            effect: .anyEnchantment, kind: .armor, upgradeMatch: .any)
        let packet = Array(try QueryCodec.encode(SearchRequest(requirements: [requirement])))
        var expected: [UInt8] = [83, 83, 70, 56, 24, 0, 0, 0, 0, 1,
                                 1, 0, 0, 0, 0, 0, 0, 1, 13]
        for name in ItemCatalog.glyphs {
            expected += [0, UInt8(name.utf8.count)] + Array(name.utf8)
        }
        expected += [0, 0, 0, 0, 0, 0, 0]
        XCTAssertEqual(packet, expected)
        XCTAssertEqual(ItemCatalog.glyphs.count, 13)
        XCTAssertEqual(ItemCatalog.enchantments.count, 13)
    }

    func testQueryCodecEncodesAlternativeAndCombinedUpgradeGroups() throws {
        let spear = try XCTUnwrap(ItemCatalog.findById("spear"))
        let sword = try XCTUnwrap(ItemCatalog.findById("sword"))
        let might = try XCTUnwrap(ItemCatalog.findById("ring_might"))
        let first = try ItemRequirement(key: 1, item: spear, upgrade: 3, kind: .weapon,
                                        upgradeMatch: .exactly, alternativeGroup: 2)
        let second = try ItemRequirement(key: 2, item: sword, upgrade: 1, kind: .weapon,
                                         upgradeMatch: .exactly, alternativeGroup: 2)
        let ring = try ItemRequirement(key: 3, item: might, upgrade: 0, kind: .ring,
                                       upgradeMatch: .any, identityGroup: 1,
                                       upgradeSumGroup: 1, upgradeSumTotal: 2)
        let request = try SearchRequest(requirements: [first, second, ring, ring])
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 56, 24, 0, 0, 0, 0, 4,
            0, 0, 5, 115, 112, 101, 97, 114, 0, 0, 1, 3, 0, 0, 0, 0, 2, 0, 0, 0,
            0, 0, 5, 115, 119, 111, 114, 100, 0, 0, 1, 1, 0, 0, 0, 0, 2, 0, 0, 0,
            3, 0, 10, 114, 105, 110, 103, 95, 109, 105, 103, 104, 116,
            0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 2, 0,
            3, 0, 10, 114, 105, 110, 103, 95, 109, 105, 103, 104, 116,
            0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 2, 0,
        ])
    }

    func testUnattainableUpgradeSumMessageMirrorsTheEngineRule() throws {
        let wand = try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .wand,
                                       upgradeMatch: .any, upgradeSumGroup: 1, upgradeSumTotal: 7)
        let impossible = try SearchRequest(requirements: [wand, wand])
        XCTAssertEqual(impossible.unattainableUpgradeSumMessage,
                       "Combined upgrade group A asks for +7 but its items can reach at most +6 together.")
        var attainable = impossible
        attainable.requirements = attainable.requirements.map { requirement in
            var member = requirement
            member.upgradeSumTotal = 6
            return member
        }
        XCTAssertNil(attainable.unattainableUpgradeSumMessage)
        // Exact upgrades cap a member's contribution at the exact value.
        let exact = try ItemRequirement(key: 2, item: nil, upgrade: 1, kind: .wand,
                                        upgradeMatch: .exactly, upgradeSumGroup: 1, upgradeSumTotal: 5)
        let wildcard = try ItemRequirement(key: 3, item: nil, upgrade: 0, kind: .wand,
                                           upgradeMatch: .any, upgradeSumGroup: 1, upgradeSumTotal: 5)
        let pinned = try SearchRequest(requirements: [exact, wildcard])
        XCTAssertEqual(pinned.unattainableUpgradeSumMessage,
                       "Combined upgrade group A asks for +5 but its items can reach at most +4 together.")
    }

    func testQueryCodecFastModeSetsFlagBitOne() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 3, kind: .armor,
                                              upgradeMatch: .exactly)
        let request = try SearchRequest(requirements: [requirement], fastMode: true)
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 56, 24, 2, 0, 0, 0, 1,
            1, 0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0,
        ])
    }

    func testQueryCodecExcludeBlacksmithRewardsSetsFlagBitTwo() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 2, kind: .weapon)
        let request = try SearchRequest(requirements: [requirement],
                                        excludeBlacksmithRewards: true)
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 56, 24, 4, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0,
        ])
    }

    func testQueryCodecUncursedRequirementSetsFlagBitZero() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 0,
                                               kind: .ring, upgradeMatch: .any,
                                               requireUncursed: true)
        let request = try SearchRequest(requirements: [requirement])

        XCTAssertEqual(try QueryCodec.encode(request).last, 1)
    }

    func testScoutRequestGoldenZeroAndNonzeroChallenges() throws {
        XCTAssertEqual(Array(try ScoutCodec.encodeRequest(seed: "AAA-AAA-AAA", challenges: 0)),
                       Array("SSQ2".utf8) + [0, 0] + Array("AAA-AAA-AAA".utf8))
        XCTAssertEqual(Array(try ScoutCodec.encodeRequest(seed: "AAA-AAA-AAF", challenges: 320)),
                       Array("SSQ2".utf8) + [64, 1] + Array("AAA-AAA-AAF".utf8))
        XCTAssertThrowsError(try ScoutCodec.encodeRequest(seed: "bad", challenges: 0))
        XCTAssertThrowsError(try ScoutCodec.encodeRequest(seed: "AAA-AAA-AAA", challenges: 512))
    }

    func testResultCodecGoldenAndMalformedPackets() throws {
        let packet = Data([83, 83, 82, 49, 0, 1, 11] + Array("ABC-DEF-GHI".utf8))
        XCTAssertEqual(try ResultCodec.decode(packet, requirementCount: 2),
                       [SeedResult(seed: "ABC-DEF-GHI", matchedRequirements: 2)])
        XCTAssertThrowsError(try ResultCodec.decode(packet + Data([0]), requirementCount: 2))
        var malformed = packet; malformed[7] = Character("a").asciiValue!
        XCTAssertThrowsError(try ResultCodec.decode(malformed, requirementCount: 2))
        XCTAssertThrowsError(try ResultCodec.decode(Data("bad".utf8), requirementCount: 2))
    }

    func testScoutCodecGoldenAndMalformedPackets() throws {
        let packet = scoutPacket(depth: 3, flags: 1, effect: "Lucky", option: 1)
        let world = try ScoutCodec.decode(packet)
        XCTAssertEqual(world.seed, "AAA-AAA-AAA"); XCTAssertEqual(world.items.count, 1)
        XCTAssertEqual(world.items[0].item.id, "dagger"); XCTAssertEqual(world.items[0].depth, 3)
        XCTAssertEqual(world.items[0].effect, "Lucky"); XCTAssertTrue(world.items[0].cursed)
        XCTAssertEqual(world.items[0].accessibility, .choice(group: 3, option: 1))
        XCTAssertThrowsError(try ScoutCodec.decode(scoutPacket(depth: 0, flags: 0, effect: "", option: 1)))
        XCTAssertThrowsError(try ScoutCodec.decode(scoutPacket(depth: 1, flags: 2, effect: "", option: 1)))
        XCTAssertThrowsError(try ScoutCodec.decode(scoutPacket(depth: 1, flags: 0, effect: "Bogus", option: 1)))
        XCTAssertThrowsError(try ScoutCodec.decode(scoutPacket(depth: 1, flags: 0, effect: "", option: 64)))
        XCTAssertEqual(try ScoutCodec.decode(scenarioPacket(mask: 4)).items[0].accessibility,
                       .scenarios(group: 2, mask: 4))
        XCTAssertThrowsError(try ScoutCodec.decode(scenarioPacket(mask: 0)))
        XCTAssertThrowsError(try ScoutCodec.decode(packet + Data([0])))
    }

    func testSeedCodeFormatting() {
        XCTAssertEqual(SeedCode.formatInput("abc"), "ABC")
        XCTAssertEqual(SeedCode.formatInput("abcd efgh ijk!"), "ABC-DEF-GHI")
        XCTAssertEqual(SeedCode.formatInput("a-b_C 12d"), "ABC-D")
        XCTAssertTrue(SeedCode.isCanonical("ABC-DEF-GHI"))
        XCTAssertFalse(SeedCode.isCanonical("abc-def-ghi"))
    }

    func testSearchEstimateFormatting() {
        XCTAssertEqual(NumberFormat.probabilityPercent(13.0 / 10_000_000.0), "1.3x10^-4%")
        XCTAssertEqual(NumberFormat.seedRate(4_600), "4.6k")
        XCTAssertEqual(NumberFormat.estimateDuration(167.224), "2.8 minutes")
        XCTAssertEqual(NumberFormat.probabilityPercent(nil), "estimating…")
        XCTAssertEqual(NumberFormat.estimateDuration(nil), "estimating…")
    }

    func testRequirementValidationRules() throws {
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .weapon, upgradeMatch: .any))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon, upgradeMatch: .any))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .armor, upgradeMatch: .exactly))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 4, kind: .ring, upgradeMatch: .atLeast))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 5, kind: .ring, upgradeMatch: .atLeast))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, effect: .oneOf(["Lucky"]), kind: .wand))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, effect: .anyEnchantment, kind: .ring))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, effect: .oneOf([]), kind: .weapon))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1,
            effect: .oneOf(["Thorns"]), kind: .weapon))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1,
            effect: .oneOf(["Displacing"]), kind: .weapon, requireUncursed: true))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 1,
            effect: .oneOf(["Displacing", "Blazing"]), kind: .weapon, requireUncursed: true))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon,
            alternativeGroup: 0))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon,
            upgradeSumGroup: 1))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon,
            upgradeSumTotal: 2))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon,
            upgradeSumGroup: 1, upgradeSumTotal: 0))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon,
            upgradeSumGroup: 1, upgradeSumTotal: 9))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon,
            alternativeGroup: 1, upgradeSumGroup: 1, upgradeSumTotal: 2))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon,
            upgradeSumGroup: 1, upgradeSumTotal: 2))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon,
            alternativeGroup: 7))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon, identityGroup: 5))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon, maximumDepth: 25))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .weapon,
            tier: 5, tierMatch: .exactly, upgradeMatch: .any))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .armor,
            tier: 4, tierMatch: .atMost, upgradeMatch: .any))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .armor,
            tier: 5, tierMatch: .atMost, upgradeMatch: .any))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .armor,
            tier: 2, tierMatch: .atMost, upgradeMatch: .any))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .armor,
            tier: 2, tierMatch: .atLeast, upgradeMatch: .any))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .armor,
            tier: 5, tierMatch: .atLeast, upgradeMatch: .any))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .weapon,
            tier: 1, tierMatch: .exactly, upgradeMatch: .any))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: ItemCatalog.weapons[0], upgrade: 1,
            kind: .weapon, tier: 1, tierMatch: .exactly))
    }

    func testScoutMatchesEffectPredicates() throws {
        let sword = try XCTUnwrap(ItemCatalog.findById("sword"))
        let plain = ScoutItem(item: sword, depth: 3, upgrade: 0, source: .heap)
        let lucky = ScoutItem(item: sword, depth: 3, upgrade: 0, effect: "Lucky", source: .heap)
        let annoying = ScoutItem(item: sword, depth: 3, upgrade: 0, effect: "Annoying",
                                 source: .heap)
        let items = [plain, lucky, annoying]

        let anyEffect = try ItemRequirement(key: 1, item: sword, upgrade: 0, kind: .weapon,
                                            upgradeMatch: .any)
        XCTAssertEqual(scoutMatchIndices(items: items, requirements: [anyEffect]).count, 1)

        let enchanted = try ItemRequirement(key: 1, item: sword, upgrade: 0,
                                            effect: .anyEnchantment, kind: .weapon,
                                            upgradeMatch: .any)
        XCTAssertEqual(scoutMatchIndices(items: items, requirements: [enchanted]), [1])

        let cursed = try ItemRequirement(key: 1, item: sword, upgrade: 0,
                                         effect: .oneOf(["Annoying", "Blazing"]), kind: .weapon,
                                         upgradeMatch: .any)
        XCTAssertEqual(scoutMatchIndices(items: items, requirements: [cursed]), [2])
    }

    func testScoutAlternativeGroupIsSatisfiedByOneMemberAndOccupiesOneSlot() throws {
        let spear = try XCTUnwrap(ItemCatalog.findById("spear"))
        let sword = try XCTUnwrap(ItemCatalog.findById("sword"))
        let alternatives = [
            try ItemRequirement(key: 1, item: spear, upgrade: 3, kind: .weapon,
                                upgradeMatch: .exactly, alternativeGroup: 1),
            try ItemRequirement(key: 2, item: sword, upgrade: 1, kind: .weapon,
                                upgradeMatch: .exactly, alternativeGroup: 1),
        ]
        let items = [ScoutItem(item: sword, depth: 3, upgrade: 1, source: .heap)]
        XCTAssertEqual(scoutMatchIndices(items: items, requirements: alternatives), [0])

        // The group is one slot: a plain sword requirement needs its own item.
        let plain = try ItemRequirement(key: 3, item: sword, upgrade: 1, kind: .weapon,
                                        upgradeMatch: .exactly)
        XCTAssertEqual(
            scoutMatchIndices(items: items, requirements: alternatives + [plain]).count, 1)
        let two = items + [ScoutItem(item: sword, depth: 4, upgrade: 1, source: .chest)]
        XCTAssertEqual(
            scoutMatchIndices(items: two, requirements: alternatives + [plain]), [0, 1])
    }

    func testScoutCombinedUpgradeGroupsCountAllOrNothing() throws {
        let might = try XCTUnwrap(ItemCatalog.findById("ring_might"))
        func ring(_ key: Int64) throws -> ItemRequirement {
            try ItemRequirement(key: key, item: might, upgrade: 0, kind: .ring,
                                upgradeMatch: .any, identityGroup: 1,
                                upgradeSumGroup: 1, upgradeSumTotal: 2)
        }
        let pair = [try ring(1), try ring(2)]
        func make(_ upgrade: Int) -> ScoutItem {
            ScoutItem(item: might, depth: 3, upgrade: upgrade, source: .heap)
        }
        // +0/+2 and +1/+1 reach the total; +0/+0 falls short and a lone +2
        // ring is only one distinct copy.
        XCTAssertEqual(scoutMatchIndices(items: [make(0), make(2)], requirements: pair), [0, 1])
        XCTAssertEqual(scoutMatchIndices(items: [make(1), make(1)], requirements: pair), [0, 1])
        XCTAssertTrue(scoutMatchIndices(items: [make(0), make(0)], requirements: pair).isEmpty)
        XCTAssertTrue(scoutMatchIndices(items: [make(2)], requirements: pair).isEmpty)
    }

    func testScoutIdentityGroupConstrainsCombinedUpgradePairs() throws {
        let might = try XCTUnwrap(ItemCatalog.findById("ring_might"))
        let haste = try XCTUnwrap(ItemCatalog.findById("ring_haste"))
        func wildcardRing(_ key: Int64) throws -> ItemRequirement {
            try ItemRequirement(key: key, item: nil, upgrade: 0, kind: .ring,
                                upgradeMatch: .any, identityGroup: 1,
                                upgradeSumGroup: 1, upgradeSumTotal: 2)
        }
        let pair = [try wildcardRing(1), try wildcardRing(2)]
        let mixed = [ScoutItem(item: might, depth: 3, upgrade: 1, source: .heap),
                     ScoutItem(item: haste, depth: 3, upgrade: 1, source: .heap)]
        XCTAssertTrue(scoutMatchIndices(items: mixed, requirements: pair).isEmpty)
        let same = [ScoutItem(item: might, depth: 3, upgrade: 1, source: .heap),
                    ScoutItem(item: might, depth: 5, upgrade: 1, source: .chest)]
        XCTAssertEqual(scoutMatchIndices(items: same, requirements: pair), [0, 1])
    }

    func testScoutFailedSumPairIsNotHighlightedWhileIndependentMatchIs() throws {
        let might = try XCTUnwrap(ItemCatalog.findById("ring_might"))
        let sword = try XCTUnwrap(ItemCatalog.findById("sword"))
        func shortRing(_ key: Int64) throws -> ItemRequirement {
            try ItemRequirement(key: key, item: might, upgrade: 0, kind: .ring,
                                upgradeMatch: .any, identityGroup: 1,
                                upgradeSumGroup: 1, upgradeSumTotal: 4)
        }
        let requirements = [try shortRing(1), try shortRing(2),
                            try ItemRequirement(key: 3, item: sword, upgrade: 0, kind: .weapon,
                                                upgradeMatch: .any)]
        let items = [ScoutItem(item: might, depth: 2, upgrade: 0, source: .heap),
                     ScoutItem(item: sword, depth: 3, upgrade: 1, source: .heap),
                     ScoutItem(item: might, depth: 5, upgrade: 1, source: .heap)]
        XCTAssertEqual(scoutMatchIndices(items: items, requirements: requirements), [1])
    }

    func testRequirementDescriptionSummarizesEffectsAndCombinedUpgrades() throws {
        let single = try ItemRequirement(key: 1, item: nil, upgrade: 0,
            effect: .oneOf(["Lucky"]), kind: .weapon, upgradeMatch: .any)
        XCTAssertEqual(single.description, "Any upgrade • Lucky")
        let trio = try ItemRequirement(key: 1, item: nil, upgrade: 0,
            effect: .oneOf(["Blocking", "Projecting", "Vampiric"]), kind: .weapon,
            upgradeMatch: .any)
        XCTAssertEqual(trio.description, "Any upgrade • Blocking, Projecting or Vampiric")
        let many = try ItemRequirement(key: 1, item: nil, upgrade: 0,
            effect: .oneOf(Array(ItemCatalog.enchantments.prefix(5))), kind: .weapon,
            upgradeMatch: .any)
        XCTAssertEqual(many.description, "Any upgrade • any of 5 enchantments")
        let anyEnchantment = try ItemRequirement(key: 1, item: nil, upgrade: 0,
            effect: .anyEnchantment, kind: .weapon, upgradeMatch: .any)
        XCTAssertEqual(anyEnchantment.description, "Any upgrade • any enchantment")
        let fullGlyphSet = try ItemRequirement(key: 1, item: nil, upgrade: 0,
            effect: .oneOf(ItemCatalog.glyphs), kind: .armor, upgradeMatch: .any)
        XCTAssertEqual(fullGlyphSet.description, "Any upgrade • any glyph")
        let sum = try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .ring,
            upgradeMatch: .any, upgradeSumGroup: 1, upgradeSumTotal: 2)
        XCTAssertEqual(sum.description, "Any upgrade • combined +2 total (group A)")
    }

    func testLegacySingleModifierSaveLoadsAsOneElementSet() throws {
        let json = """
        {"requirements":[{"key":1,"upgrade":2,"modifier":"Lucky","kind":0,"tier":0,\
        "tierMatch":0,"upgradeMatch":1,"requireUncursed":false}],\
        "maximumDepth":24,"requireBlacksmith":false}
        """
        let saved = QueryPersistence.decode(json)
        XCTAssertEqual(saved.requirements.count, 1)
        XCTAssertEqual(saved.requirements.first?.effect, .oneOf(["Lucky"]))
        XCTAssertNil(saved.requirements.first?.alternativeGroup)
        XCTAssertNil(saved.requirements.first?.upgradeSumGroup)
    }

    func testSavedQueryRoundTripsNewEffectAndGroupFields() throws {
        let alternative = try ItemRequirement(key: 5, item: nil, upgrade: 0,
            effect: .oneOf(["Blocking", "Vampiric"]), kind: .weapon, upgradeMatch: .any,
            alternativeGroup: 3)
        let ring = try ItemRequirement(key: 6, item: ItemCatalog.findById("ring_might"),
            upgrade: 0, kind: .ring, upgradeMatch: .any, identityGroup: 1,
            upgradeSumGroup: 1, upgradeSumTotal: 2)
        let glyphed = try ItemRequirement(key: 7, item: nil, upgrade: 0,
            effect: .anyEnchantment, kind: .armor, upgradeMatch: .any)
        let query = SavedQuery(requirements: [alternative, ring, glyphed])
        let encoded = try XCTUnwrap(QueryPersistence.encode(query))
        XCTAssertEqual(QueryPersistence.decode(encoded), query)
        XCTAssertFalse(encoded.contains("\"modifier\""))
    }

    func testRealFFIScout() async throws {
        let world = try await ProductionSeedFinderEngine().scoutSeed("AAA-AAA-AAA", challenges: 0)
        XCTAssertFalse(world.items.isEmpty)
        XCTAssertTrue(world.items.allSatisfy { (1...24).contains($0.depth) })
    }

    func testRealFFIAcceptsAlternativeAndCombinedUpgradeRequirements() async throws {
        let might = try XCTUnwrap(ItemCatalog.findById("ring_might"))
        let requirements = [
            try ItemRequirement(key: 1, item: nil, upgrade: 0, effect: .anyEnchantment,
                                kind: .weapon, upgradeMatch: .any, alternativeGroup: 1),
            try ItemRequirement(key: 2, item: nil, upgrade: 0,
                                effect: .oneOf(["Thorns", "Stench"]), kind: .armor,
                                upgradeMatch: .any, alternativeGroup: 1),
            try ItemRequirement(key: 3, item: might, upgrade: 0, kind: .ring,
                                upgradeMatch: .any, identityGroup: 1,
                                upgradeSumGroup: 1, upgradeSumTotal: 2),
            try ItemRequirement(key: 4, item: might, upgrade: 0, kind: .ring,
                                upgradeMatch: .any, identityGroup: 1,
                                upgradeSumGroup: 1, upgradeSumTotal: 2),
        ]
        let session = try await ProductionSeedFinderEngine().startSearch(
            try SearchRequest(requirements: requirements))
        let status = try await session.status()
        // The native decoder accepted the SSF8 packet: an invalid or
        // undecodable query would report a failed state instead.
        XCTAssertNotEqual(status.state, .failed)
        await session.cancel()
        await session.close()
    }

    func testRealFFIStartCancelCloseLifecycle() async throws {
        let requirement = try ItemRequirement(key: 1, item: ItemCatalog.findById("wand_frost"),
            upgrade: 2, kind: .wand)
        let session = try await ProductionSeedFinderEngine().startSearch(
            try SearchRequest(requirements: [requirement]))
        await session.cancel()
        let deadline = ContinuousClock.now + .seconds(5)
        var terminal = false
        repeat {
            _ = try await session.poll(4)
            terminal = try await session.status().state != .running
            if !terminal { try await Task.sleep(for: .milliseconds(10)) }
        } while !terminal && ContinuousClock.now < deadline
        XCTAssertTrue(terminal, "cancelled native session should terminate promptly")
        await session.close(); await session.close()
    }

    private func scoutPacket(depth: UInt8, flags: UInt8, effect: String, option: UInt8) -> Data {
        var bytes = Array("SSC1".utf8) + [11] + Array("AAA-AAA-AAA".utf8) + [0, 1]
        let id = Array("dagger".utf8); bytes += [0, UInt8(id.count)] + id
        bytes += [depth, 2, flags, 0, UInt8(effect.utf8.count)] + Array(effect.utf8)
        bytes += [UInt8(ScoutItemSource.chest.rawValue), 1, 0, 3, option]
        return Data(bytes)
    }

    private func scenarioPacket(mask: UInt64) -> Data {
        var bytes = Array("SSC1".utf8) + [11] + Array("AAA-AAA-AAA".utf8) + [0, 1]
        let id = Array("ring_haste".utf8); bytes += [0, UInt8(id.count)] + id
        bytes += [4, 1, 0, 0, 0, UInt8(ScoutItemSource.heap.rawValue), 2, 0, 2]
        bytes += (0..<8).reversed().map { UInt8((mask >> UInt64($0 * 8)) & 0xff) }
        return Data(bytes)
    }
}
