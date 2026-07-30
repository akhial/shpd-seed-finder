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

    func testFloorLimitOptionsSkipEmptyBossFloors() {
        XCTAssertEqual(FloorLimits.options.count, 21)
        XCTAssertFalse(FloorLimits.options.contains(5))
        XCTAssertFalse(FloorLimits.options.contains(10))
        XCTAssertFalse(FloorLimits.options.contains(15))
        XCTAssertTrue(FloorLimits.options.contains(20))
        XCTAssertEqual(FloorLimits.options.first, 1)
        XCTAssertEqual(FloorLimits.options.last, 24)
        XCTAssertEqual([4, 5, 9, 10, 14, 15, 20, 24].map(FloorLimits.normalize),
                       [4, 4, 9, 9, 14, 14, 20, 24])
    }

    func testFloorLimitIndexSnapsOffListValuesToTheNearestOptionBelow() {
        // Every selectable floor maps to its own slot.
        for (index, floor) in FloorLimits.options.enumerated() {
            XCTAssertEqual(FloorLimits.index(of: floor), index)
        }
        // Empty boss floors map to the slot of the equivalent floor below.
        XCTAssertEqual(FloorLimits.index(of: 5), FloorLimits.options.firstIndex(of: 4))
        XCTAssertEqual(FloorLimits.index(of: 10), FloorLimits.options.firstIndex(of: 9))
        XCTAssertEqual(FloorLimits.index(of: 15), FloorLimits.options.firstIndex(of: 14))
        // Out-of-range values snap to the nearest option below, never slot 0.
        XCTAssertEqual(FloorLimits.index(of: 30), FloorLimits.options.count - 1)
        XCTAssertEqual(FloorLimits.index(of: 0), 0)
    }

    func testSavedQueryDecodeSnapsEmptyBossFloorLimits() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .wand,
                                              upgradeMatch: .exactly, maximumDepth: 10)
        let query = SavedQuery(requirements: [requirement], maximumDepth: 15)
        let encoded = try XCTUnwrap(QueryPersistence.encode(query))
        let decoded = QueryPersistence.decode(encoded)
        XCTAssertEqual(decoded.maximumDepth, 14)
        XCTAssertEqual(decoded.requirements.first?.maximumDepth, 9)
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

    func testQueryCodecTierPredicateUsesSSF7WithZeroChallenges() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .armor,
            tier: 4, tierMatch: .atLeast, upgradeMatch: .any)
        let request = try SearchRequest(requirements: [requirement])
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 55, 24, 0, 0, 0, 0, 1,
            1, 0, 0, 2, 4, 0, 0, 0, 0, 0, 0, 0, 0,
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
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 0, modifier: "Projecting",
            kind: .thrownWeapon, tier: 5, tierMatch: .exactly, upgradeMatch: .any))
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
        let first = try ItemRequirement(key: 1, item: dagger, upgrade: 2, modifier: "Lucky",
            kind: .weapon, upgradeMatch: .exactly, source: .chest, identityGroup: 1,
            maximumDepth: 5)
        let second = try ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .ring,
            upgradeMatch: .atLeast)
        let request = try SearchRequest(requirements: [first, second], maximumDepth: 12,
                                        requireBlacksmith: true, challenges: 104)
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 55, 12, 1, 104, 0, 0, 2,
            0, 0, 6, 100, 97, 103, 103, 101, 114, 0, 0, 1, 2,
            0, 5, 76, 117, 99, 107, 121, 2, 1, 5, 0,
            3, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0,
        ])
    }

    func testQueryCodecFastModeSetsFlagBitOne() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 3, kind: .armor,
                                              upgradeMatch: .exactly)
        let request = try SearchRequest(requirements: [requirement], fastMode: true)
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 55, 24, 2, 0, 0, 0, 1,
            1, 0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0,
        ])
    }

    func testQueryCodecExcludeBlacksmithRewardsSetsFlagBitTwo() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 2, kind: .weapon)
        let request = try SearchRequest(requirements: [requirement],
                                        excludeBlacksmithRewards: true)
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 55, 24, 4, 0, 0, 0, 1,
            0, 0, 0, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0,
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
        XCTAssertFalse(world.items[0].secret)
        XCTAssertEqual(world.items[0].accessibility, .choice(group: 3, option: 1))

        let secretOnly = try ScoutCodec.decode(scoutPacket(depth: 3, flags: 2, effect: "", option: 1)).items[0]
        XCTAssertTrue(secretOnly.secret); XCTAssertFalse(secretOnly.cursed)
        let secretCursed = try ScoutCodec.decode(scoutPacket(depth: 3, flags: 3, effect: "", option: 1)).items[0]
        XCTAssertTrue(secretCursed.secret); XCTAssertTrue(secretCursed.cursed)

        XCTAssertThrowsError(try ScoutCodec.decode(scoutPacket(depth: 0, flags: 0, effect: "", option: 1)))
        XCTAssertThrowsError(try ScoutCodec.decode(scoutPacket(depth: 1, flags: 4, effect: "", option: 1)))
        XCTAssertThrowsError(try ScoutCodec.decode(scoutPacket(depth: 1, flags: 0, effect: "Bogus", option: 1)))
        XCTAssertThrowsError(try ScoutCodec.decode(scoutPacket(depth: 1, flags: 0, effect: "", option: 64)))
        XCTAssertEqual(try ScoutCodec.decode(scenarioPacket(mask: 4)).items[0].accessibility,
                       .scenarios(group: 2, mask: 4))
        XCTAssertThrowsError(try ScoutCodec.decode(scenarioPacket(mask: 0)))
        XCTAssertThrowsError(try ScoutCodec.decode(packet + Data([0])))
    }

    func testScoutCodecGoldenQuestBlock() throws {
        let world = try ScoutCodec.decode(questPacket(
            [1, 3, 4], [2, 3, 8], [3, 1, 13], [4, 2, 18]))
        XCTAssertEqual(world.seed, "AAA-AAA-AAA")
        XCTAssertTrue(world.items.isEmpty)
        XCTAssertEqual(world.quests.map(\.kind), [.ghost, .wandmaker, .blacksmith, .imp])
        XCTAssertEqual(world.quests.map(\.variant), [.greatCrab, .rotberry, .crystal, .golem])
        XCTAssertEqual(world.quests.map(\.depth), [4, 8, 13, 18])
        XCTAssertEqual(world.quests.map(\.kind.giverLabel),
                       ["Sad ghost", "Wandmaker", "Blacksmith", "Imp"])
        XCTAssertEqual(world.quests.map(\.variant.label),
                       ["Great crab", "Rotberry", "Crystal spire", "Golems"])
        XCTAssertTrue(try ScoutCodec.decode(questPacket()).quests.isEmpty)
    }

    func testScoutCodecRejectsMalformedQuestBlocks() throws {
        // Unknown quest id, and unknown variants (zero, too large, wrong quest).
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([5, 1, 3])))
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([0, 1, 3])))
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([1, 0, 3])))
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([1, 4, 3])))
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([3, 3, 13])))
        // Depth outside the quest's floor range.
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([1, 1, 5])))
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([2, 1, 6])))
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([4, 2, 16])))
        // Duplicate and descending quest ids, and an over-limit count.
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([1, 1, 2], [1, 2, 3])))
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([2, 1, 8], [1, 1, 3])))
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket(
            count: 5, [1, 1, 2], [2, 1, 7], [3, 1, 12], [4, 1, 17], [4, 2, 18])))
        // Truncated quest block.
        XCTAssertThrowsError(try ScoutCodec.decode(Data(
            Array("SSC2".utf8) + [11] + Array("AAA-AAA-AAA".utf8) + [1, 1, 3])))
    }

    func testSeedCodeFormatting() {
        XCTAssertEqual(SeedCode.formatInput("abc"), "ABC")
        XCTAssertEqual(SeedCode.formatInput("abcd efgh ijk!"), "ABC-DEF-GHI")
        XCTAssertEqual(SeedCode.formatInput("a-b_C 12d"), "ABC-D")
        XCTAssertTrue(SeedCode.isCanonical("ABC-DEF-GHI"))
        XCTAssertFalse(SeedCode.isCanonical("abc-def-ghi"))
    }

    func testSeedCodeNumericValue() {
        XCTAssertEqual(SeedCode.value(of: "AAA-AAA-AAA"), 0)
        XCTAssertEqual(SeedCode.value(of: "AAA-AAA-AAB"), 1)
        XCTAssertEqual(SeedCode.value(of: "AAA-AAA-ABA"), 26)
        XCTAssertEqual(SeedCode.value(of: "ZZZ-ZZZ-ZZZ"), 5_429_503_678_975)
        XCTAssertNil(SeedCode.value(of: "aaa-aaa-aaa"))
        XCTAssertNil(SeedCode.value(of: "AAAAAAAAA"))
        XCTAssertNil(SeedCode.value(of: ""))
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
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, modifier: "Lucky", kind: .wand))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1,
            modifier: "Displacing", kind: .weapon, requireUncursed: true))
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

    // The continuation predicate is the engine's, exercised end to end over
    // the wire in RefineSearchTests; the "shares an item" rule below stays a
    // local estimate and is tested here.

    func testSearchRequestSharedItemRules() throws {
        func request(_ kind: ItemKind, item: CatalogItem? = nil, maximumDepth: Int = 24,
                     challenges: Int = 0) throws -> SearchRequest {
            try SearchRequest(requirements: [
                ItemRequirement(key: 1, item: item, upgrade: 0, kind: kind, upgradeMatch: .any)],
                maximumDepth: maximumDepth, challenges: challenges)
        }
        let anyWand = try request(.wand)
        let missile = try request(.wand, item: ItemCatalog.wands[0])
        let fireblast = try request(.wand, item: ItemCatalog.wands[1])
        // Same kind: a kind-level requirement subsumes every item of its kind.
        XCTAssertTrue(anyWand.sharesRequirement(with: missile))
        XCTAssertTrue(missile.sharesRequirement(with: anyWand))
        XCTAssertTrue(missile.sharesRequirement(with: missile))
        // Same kind but two different named items share nothing.
        XCTAssertFalse(missile.sharesRequirement(with: fireblast))
        // Different kinds never share, and the narrowed weapon kinds count as
        // kinds of their own (matching the other platforms).
        XCTAssertFalse(anyWand.sharesRequirement(with: try request(.ring)))
        XCTAssertFalse(try request(.weapon).sharesRequirement(with: try request(.meleeWeapon)))
        // Scope and challenge differences are irrelevant to sharing.
        XCTAssertTrue(anyWand.sharesRequirement(with: try request(.wand, maximumDepth: 12)))
        XCTAssertTrue(anyWand.sharesRequirement(with: try request(.wand, challenges: 32)))
    }

    func testRealFFIScout() async throws {
        let world = try await ProductionSeedFinderEngine().scoutSeed("AAA-AAA-AAA", challenges: 0)
        XCTAssertFalse(world.items.isEmpty)
        XCTAssertTrue(world.items.allSatisfy { (1...24).contains($0.depth) })
        XCTAssertEqual(world.quests.map(\.kind), [.ghost, .wandmaker, .blacksmith, .imp])
        XCTAssertEqual(world.quests.map(\.variant),
                       [.greatCrab, .elementalEmbers, .crystal, .golem])
        XCTAssertEqual(world.quests.map(\.depth), [4, 9, 13, 19])
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

    func testRealFFIResumeHintResumedSearchAndFilter() async throws {
        let requirement = try ItemRequirement(key: 1, item: ItemCatalog.findById("wand_frost"),
            upgrade: 2, kind: .wand)
        let request = try SearchRequest(requirements: [requirement])
        let engine = ProductionSeedFinderEngine()

        let session = try await engine.startSearch(request)
        await session.cancel()
        try await waitForTerminal(session)
        let hint = try await session.resumeHint()
        XCTAssertGreaterThanOrEqual(hint.position, 0)
        XCTAssertGreaterThanOrEqual(hint.remaining, 0)
        await session.close()

        // A one-seed resumed scan finishes almost immediately.
        let resumed = try await engine.startResumedSearch(request, resumeFrom: hint.position, scanLen: 1)
        try await waitForTerminal(resumed)
        let status = try await resumed.status()
        XCTAssertEqual(status.state, .completed)
        await resumed.close()

        // The authoritative filter returns a subset of its input, in order.
        let filtered = try await engine.filterSeeds(request, seeds: ["AAA-AAA-AAA", "AAA-AAA-AAB"])
        XCTAssertTrue(Set(filtered).isSubset(of: ["AAA-AAA-AAA", "AAA-AAA-AAB"]))
        let empty = try await engine.filterSeeds(request, seeds: [])
        XCTAssertEqual(empty, [])
    }

    private func waitForTerminal(_ session: any SeedFinderSearchSession) async throws {
        let deadline = ContinuousClock.now + .seconds(5)
        var terminal = false
        repeat {
            _ = try await session.poll(4)
            terminal = try await session.status().state != .running
            if !terminal { try await Task.sleep(for: .milliseconds(10)) }
        } while !terminal && ContinuousClock.now < deadline
        XCTAssertTrue(terminal, "native session should reach a terminal state promptly")
    }

    private func scoutPacket(depth: UInt8, flags: UInt8, effect: String, option: UInt8) -> Data {
        var bytes = Array("SSC2".utf8) + [11] + Array("AAA-AAA-AAA".utf8) + [0] + [0, 1]
        let id = Array("dagger".utf8); bytes += [0, UInt8(id.count)] + id
        bytes += [depth, 2, flags, 0, UInt8(effect.utf8.count)] + Array(effect.utf8)
        bytes += [UInt8(ScoutItemSource.chest.rawValue), 1, 0, 3, option]
        return Data(bytes)
    }

    private func scenarioPacket(mask: UInt64) -> Data {
        var bytes = Array("SSC2".utf8) + [11] + Array("AAA-AAA-AAA".utf8) + [0] + [0, 1]
        let id = Array("ring_haste".utf8); bytes += [0, UInt8(id.count)] + id
        bytes += [4, 1, 0, 0, 0, UInt8(ScoutItemSource.heap.rawValue), 2, 0, 2]
        bytes += (0..<8).reversed().map { UInt8((mask >> UInt64($0 * 8)) & 0xff) }
        return Data(bytes)
    }

    /// An SSC2 packet with the given raw quest triples and zero items.
    private func questPacket(count: UInt8? = nil, _ quests: [UInt8]...) -> Data {
        var bytes = Array("SSC2".utf8) + [11] + Array("AAA-AAA-AAA".utf8)
        bytes += [count ?? UInt8(quests.count)] + quests.flatMap { $0 } + [0, 0]
        return Data(bytes)
    }
}
