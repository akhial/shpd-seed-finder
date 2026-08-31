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

    func testBundledStaff22Preset() throws {
        let preset = BuiltInPresets.staff22
        XCTAssertEqual(preset.name, "+22 Staff")
        XCTAssertEqual(preset.query.maximumDepth, 19)
        XCTAssertEqual(preset.query.requirements.map(\.kind), [.wand, .wand, .wand, .wand])
        XCTAssertEqual(preset.query.requirements.map(\.upgradeMatch), [.exactly, .any, .any, .atLeast])
        XCTAssertEqual(preset.query.requirements.map(\.upgrade), [4, 0, 0, 1])
        XCTAssertEqual(preset.query.requirements.map(\.identityGroup), [1, 1, 1, nil])
        XCTAssertNotNil(preset.query.validated())
    }

    func testBundledTier4WeaponPreset() throws {
        let preset = BuiltInPresets.tier4Weapon26
        XCTAssertEqual(preset.name, "+26 Tier 4 Weapon")
        XCTAssertEqual(preset.query.maximumDepth, 19)
        XCTAssertEqual(preset.query.requirements.map(\.kind), [.weapon, .weapon, .weapon])
        XCTAssertEqual(preset.query.requirements.map(\.tierMatch), [.exactly, .any, .any])
        XCTAssertEqual(preset.query.requirements.map(\.tier), [4, 0, 0])
        XCTAssertEqual(preset.query.requirements.map(\.upgradeMatch), [.exactly, .any, .any])
        XCTAssertEqual(preset.query.requirements.map(\.upgrade), [5, 0, 0])
        XCTAssertEqual(preset.query.requirements.map(\.identityGroup), [1, 1, 1])
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

    /// The catalog is parsed from the shared upstream asset, so it must agree
    /// with that file entry for entry rather than with a table kept here.
    func testCatalogIsLoadedFromTheSharedAsset() throws {
        let asset = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // SeedSeekerKitTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // SeedSeeker
            .deletingLastPathComponent() // macos
            .deletingLastPathComponent() // repository root
            .appendingPathComponent(
                "android/app/src/main/assets/third_party/shattered-pixel-dungeon/catalog-v4.0.0.json")
        let document = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(contentsOf: asset)) as? [String: Any])
        let entries = try XCTUnwrap(document["entries"] as? [[String: Any]])

        XCTAssertEqual(entries.count, 88)
        XCTAssertEqual(ItemCatalog.all.count, entries.count)
        XCTAssertEqual(ItemCatalog.meleeWeapons.count, 31)
        XCTAssertEqual(ItemCatalog.thrownWeapons.count, 27)
        XCTAssertEqual(ItemCatalog.armor.count, 5)
        XCTAssertEqual(ItemCatalog.wands.count, 13)
        XCTAssertEqual(ItemCatalog.rings.count, 12)

        let kinds: [String: ItemKind] = ["weapon": .weapon, "armor": .armor,
                                         "wand": .wand, "ring": .ring]
        for entry in entries {
            let id = try XCTUnwrap(entry["id"] as? String)
            let item = try XCTUnwrap(ItemCatalog.findById(id), id)
            XCTAssertEqual(item.name, entry["name"] as? String, id)
            XCTAssertEqual(item.spriteIndex, entry["sprite"] as? Int, id)
            XCTAssertEqual(item.tier, entry["tier"] as? Int, id)
            XCTAssertEqual(item.kind, kinds[try XCTUnwrap(entry["type"] as? String)], id)
            switch entry["class"] as? String {
            case "melee": XCTAssertEqual(ItemCatalog.weaponClass(of: id), .melee, id)
            case "thrown": XCTAssertEqual(ItemCatalog.weaponClass(of: id), .thrown, id)
            default: XCTAssertNil(ItemCatalog.weaponClass(of: id), id)
            }
        }

        let modifiers = try XCTUnwrap(document["modifiers"] as? [String: [String]])
        XCTAssertEqual(ItemCatalog.enchantments, modifiers["weaponEnchantments"])
        XCTAssertEqual(ItemCatalog.weaponCurses, modifiers["weaponCurses"])
        XCTAssertEqual(ItemCatalog.glyphs, modifiers["armorGlyphs"])
        XCTAssertEqual(ItemCatalog.armorCurses, modifiers["armorCurses"])
        XCTAssertEqual(ItemCatalog.modifiersFor(.armor),
                       ItemCatalog.glyphs + ItemCatalog.armorCurses)
        XCTAssertEqual(ItemCatalog.enchantmentsFor(.weapon), ItemCatalog.enchantments)
        XCTAssertEqual(ItemCatalog.enchantmentsFor(.armor), ItemCatalog.glyphs)
        XCTAssertEqual(ItemCatalog.cursesFor(.thrownWeapon), ItemCatalog.weaponCurses)
        XCTAssertTrue(ItemCatalog.modifiersFor(.wand).isEmpty)
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

    /// The scouted world of a pinned seed, generated once: the marks index
    /// this very item list, so the assertions below name its entries.
    private static let pinnedSeed = "AAA-AAA-BUH"
    private func pinnedWorld() async throws -> ScoutWorld {
        try await ProductionSeedFinderEngine().scoutSeed(Self.pinnedSeed, challenges: 0)
    }
    private func marks(_ requirements: [ItemRequirement], maximumDepth: Int = 24,
                       excludeBlacksmithRewards: Bool = false) throws -> ScoutMatches {
        try ScoutMatches.mark(
            seed: Self.pinnedSeed, challenges: 0,
            query: SearchRequest(requirements: requirements, maximumDepth: maximumDepth,
                                 excludeBlacksmithRewards: excludeBlacksmithRewards))
    }

    /// The Wandmaker hands out one of two wands, so a query asking for both
    /// can only ever be explained by one of them — and the engine says so,
    /// marking a partial match rather than an impossible pair.
    func testEngineMarksOnlyOneMutuallyExclusiveRewardAndReportsPartialMatches() async throws {
        let world = try await pinnedWorld()
        let blastWave = try ItemRequirement(key: 1, item: nil, upgrade: 2, kind: .wand,
                                            upgradeMatch: .exactly, source: .wandmakerReward)
        let warding = try ItemRequirement(key: 2, item: nil, upgrade: 1, kind: .wand,
                                          upgradeMatch: .exactly, source: .wandmakerReward)

        let single = try marks([blastWave])
        XCTAssertEqual(single.matchedRequirements, 1)
        XCTAssertEqual(single.totalRequirements, 1)
        let index = try XCTUnwrap(single.matched.first)
        XCTAssertEqual(single.matched.count, 1)
        XCTAssertEqual(world.items[index].item.id, "wand_blast_wave")
        XCTAssertEqual(world.items[index].source, .wandmakerReward)

        let both = try marks([blastWave, warding])
        XCTAssertEqual(both.totalRequirements, 2)
        XCTAssertEqual(both.matchedRequirements, 1, "the two rewards exclude each other")
        XCTAssertEqual(both.matched.count, 1)
    }

    /// Item, upgrade, curse, floor-limit and blacksmith-exclusion predicates
    /// all reach the engine's selection through the same query packet.
    func testEngineMarksRespectTheQueryPredicates() async throws {
        let world = try await pinnedWorld()
        let sharpshooting = try XCTUnwrap(ItemCatalog.findById("ring_sharpshooting"))
        let named = try ItemRequirement(key: 1, item: sharpshooting, upgrade: 1, kind: .ring,
                                        upgradeMatch: .exactly)
        let found = try marks([named])
        let index = try XCTUnwrap(found.matched.first)
        XCTAssertEqual(found.matched.count, 1)
        XCTAssertEqual(world.items[index].item.id, "ring_sharpshooting")
        XCTAssertEqual(world.items[index].depth, 11)

        // The world's only one sits on floor 11, out of reach of a shallower run.
        XCTAssertTrue(try marks([named], maximumDepth: 8).matched.isEmpty)
        XCTAssertEqual(try marks([named], maximumDepth: 8).matchedRequirements, 0)
        // Two requirements cannot both claim that single ring.
        let second = try ItemRequirement(key: 2, item: sharpshooting, upgrade: 1, kind: .ring,
                                         upgradeMatch: .exactly)
        XCTAssertEqual(try marks([named, second]).matchedRequirements, 1)

        // The world's only Wand of Corrosion is cursed.
        let corrosion = try XCTUnwrap(ItemCatalog.findById("wand_corrosion"))
        XCTAssertEqual(try marks([ItemRequirement(key: 1, item: corrosion, upgrade: 0,
                                                  kind: .wand, upgradeMatch: .any)])
                           .matchedRequirements, 1)
        XCTAssertTrue(try marks([ItemRequirement(key: 1, item: corrosion, upgrade: 0, kind: .wand,
                                                 upgradeMatch: .any, requireUncursed: true)])
                          .matched.isEmpty)

        // A crossbow is both a Smith reward and shop stock; excluding the
        // reward leaves the shop's.
        let crossbow = try XCTUnwrap(ItemCatalog.findById("crossbow"))
        let anyCrossbow = try ItemRequirement(key: 1, item: crossbow, upgrade: 0, kind: .weapon,
                                              upgradeMatch: .any, source: .blacksmithReward)
        XCTAssertEqual(try marks([anyCrossbow]).matchedRequirements, 1)
        XCTAssertTrue(try marks([anyCrossbow], excludeBlacksmithRewards: true).matched.isEmpty)
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
            [1, 3, 4], [2, 3, 8], [3, 1, 13], [4, 1, 18]))
        XCTAssertEqual(world.seed, "AAA-AAA-AAA")
        XCTAssertTrue(world.items.isEmpty)
        XCTAssertEqual(world.quests.map(\.kind), [.ghost, .wandmaker, .blacksmith, .imp])
        XCTAssertEqual(world.quests.map(\.variant), [.greatCrab, .rotberry, .crystal, .vault])
        XCTAssertEqual(world.quests.map(\.depth), [4, 8, 13, 18])
        XCTAssertEqual(world.quests.map(\.kind.giverLabel),
                       ["Sad ghost", "Wandmaker", "Blacksmith", "Imp"])
        XCTAssertEqual(world.quests.map(\.variant.label),
                       ["Great crab", "Rotberry", "Crystal spire", "Vault"])
        XCTAssertTrue(try ScoutCodec.decode(questPacket()).quests.isEmpty)
    }

    /// v4.0.0's own wire values: the vault's item source, and the weapon
    /// upgrade one above the ceiling every other family stops at.
    func testScoutCodecReadsTheVaultSourceAndTheWeaponCeiling() throws {
        func packet(_ id: String, upgrade: UInt8, source: ScoutItemSource) -> Data {
            var bytes = Array("SSC2".utf8) + [11] + Array("AAA-AAA-AAA".utf8) + [0] + [0, 1]
            let name = Array(id.utf8); bytes += [0, UInt8(name.count)] + name
            bytes += [17, upgrade, 0, 0, 0, UInt8(source.rawValue), 0]
            return Data(bytes)
        }
        let vault = try ScoutCodec.decode(packet("greatsword", upgrade: 5, source: .vaultTreasure)).items[0]
        XCTAssertEqual(vault.upgrade, 5)
        XCTAssertEqual(vault.source, .vaultTreasure)
        XCTAssertEqual(vault.source.label, "Vault treasure")
        XCTAssertThrowsError(try ScoutCodec.decode(packet("greatsword", upgrade: 6, source: .vaultTreasure)))
        XCTAssertNoThrow(try ScoutCodec.decode(packet("plate_armor", upgrade: 4, source: .impReward)))
        XCTAssertThrowsError(try ScoutCodec.decode(packet("plate_armor", upgrade: 5, source: .impReward)))
    }

    func testScoutCodecRejectsMalformedQuestBlocks() throws {
        // Unknown quest id, and unknown variants (zero, too large, wrong quest).
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([5, 1, 3])))
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([0, 1, 3])))
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([1, 0, 3])))
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([1, 4, 3])))
        // v4.0.0 left the Imp one variant, the vault; the old golem code is gone.
        XCTAssertThrowsError(try ScoutCodec.decode(questPacket([4, 2, 18])))
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

    /// The as-you-type masker is `seedfinder_seed_format`: non-letters are
    /// dropped, the first nine ASCII letters kept, and only those uppercased.
    func testSeedCodeFormattingComesFromTheEngine() {
        XCTAssertEqual(SeedCode.formatInput("abc"), "ABC")
        XCTAssertEqual(SeedCode.formatInput("abcd efgh ijk!"), "ABC-DEF-GHI")
        XCTAssertEqual(SeedCode.formatInput("a-b_C 12d"), "ABC-D")
        XCTAssertEqual(SeedCode.formatInput(""), "")
        // Non-ASCII letters contribute nothing, whatever their own alphabet
        // would uppercase them to — the Turkish dotless i is the classic trap.
        XCTAssertEqual(SeedCode.formatInput("ıİabc"), "ABC")
        XCTAssertEqual(SeedCode.formatInput("日本語"), "")
        XCTAssertTrue(SeedCode.isCanonical("ABC-DEF-GHI"))
        XCTAssertFalse(SeedCode.isCanonical("abc-def-ghi"))
        XCTAssertFalse(SeedCode.isCanonical("ABCDEFGHI"))
    }

    /// The parser is `seedfinder_seed_parse`: it hands back the canonical code
    /// to display and the numeric value the search takes.
    func testSeedCodeParsingComesFromTheEngine() {
        XCTAssertEqual(SeedCode.parse("AAA-AAA-AAA")?.value, 0)
        XCTAssertEqual(SeedCode.parse("AAA-AAA-AAB")?.value, 1)
        XCTAssertEqual(SeedCode.parse("AAA-AAA-ABA")?.value, 26)
        XCTAssertEqual(SeedCode.parse("ZZZ-ZZZ-ZZZ")?.value, 5_429_503_678_975)
        XCTAssertEqual(SeedCode.parse("ABC-DEF-GHI")?.code, "ABC-DEF-GHI")
        // The game accepts a properly dashed code in any case and canonicalizes it.
        XCTAssertEqual(SeedCode.parse("abc-def-ghi")?.code, "ABC-DEF-GHI")
        XCTAssertNil(SeedCode.parse(""))
        XCTAssertNil(SeedCode.parse("ABC-DEF-GH"))
        XCTAssertNil(SeedCode.parse("ıİabcdefghi"))
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
        // v4.0.0's ceilings: the vault's tier-4 prizes take weapons to +5,
        // every other family to +4.
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 5, kind: .weapon, upgradeMatch: .exactly))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 5, kind: .thrownWeapon, upgradeMatch: .exactly))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 6, kind: .weapon, upgradeMatch: .exactly))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 4, kind: .armor, upgradeMatch: .exactly))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 5, kind: .armor, upgradeMatch: .exactly))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 5, kind: .wand, upgradeMatch: .exactly))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, modifier: "Lucky", kind: .wand))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1,
            modifier: "Displacing", kind: .weapon, requireUncursed: true))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, modifier: "Bogus", kind: .weapon))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1,
            effect: .anyEnchantment, kind: .ring))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1,
            effect: .oneOf([]), kind: .weapon))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1,
            effect: .oneOf(["Blazing", "Obfuscation"]), kind: .weapon), "mixed families")
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1,
            effect: .oneOf(["Annoying", "Wayward"]), kind: .weapon, requireUncursed: true), "curses only")
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 1,
            effect: .oneOf(["Annoying", "Blazing"]), kind: .weapon, requireUncursed: true), "a mixed set is fine")
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .ring,
            levelSum: LevelSum(group: 5, atLeast: 2)))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .ring,
            levelSum: LevelSum(group: 1, atLeast: 0)))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .ring,
            alternativeGroup: 1, levelSum: LevelSum(group: 1, atLeast: 2)))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .ring,
            levelSum: LevelSum(group: 4, atLeast: 1)))
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

    // The continuation predicate and the "shares an item" rule are both the
    // engine's, exercised end to end over the wire: the continuation in
    // RefineSearchTests, the sharing rule below through the one outcome it
    // decides on its own.

    /// Sharing an item reaches the app only as the target-filter outcome of
    /// the start decision. Every candidate here changes the floor limit, so it
    /// can never continue the target and sharing alone decides.
    func testSharedItemsDecideTheTargetFilterOutcome() throws {
        func request(_ kind: ItemKind, item: CatalogItem? = nil,
                     maximumDepth: Int = 24) throws -> SearchRequest {
            try SearchRequest(requirements: [
                ItemRequirement(key: 1, item: item, upgrade: 0, kind: kind, upgradeMatch: .any)],
                maximumDepth: maximumDepth)
        }
        func decide(_ candidate: SearchRequest, _ target: SearchRequest) -> StartMode {
            StartDecision.decide(candidate: candidate, target: target, targetSetEmpty: false,
                                 targetHasUncoveredSeeds: true, detachedBase: nil)
        }
        let anyWand = try request(.wand, maximumDepth: 12)
        let missile = try request(.wand, item: ItemCatalog.wands[0], maximumDepth: 12)
        // Same kind: a kind-level requirement subsumes every item of its kind.
        XCTAssertEqual(decide(anyWand, try request(.wand, item: ItemCatalog.wands[0])), .targetFilter)
        XCTAssertEqual(decide(missile, try request(.wand)), .targetFilter)
        XCTAssertEqual(decide(missile, try request(.wand, item: ItemCatalog.wands[0])), .targetFilter)
        // Same kind but two different named items share nothing.
        XCTAssertEqual(decide(missile, try request(.wand, item: ItemCatalog.wands[1])), .detached)
        // Different kinds never share.
        XCTAssertEqual(decide(anyWand, try request(.ring)), .detached)
        // The engine's rule reads the item family, so a wielded-weapon
        // requirement still shares with a plain weapon one — the local copy
        // this replaces treated the narrowed kinds as kinds of their own.
        XCTAssertEqual(decide(try request(.weapon, maximumDepth: 12), try request(.meleeWeapon)),
                       .targetFilter)
    }

    func testRealFFIScout() async throws {
        let world = try await ProductionSeedFinderEngine().scoutSeed("AAA-AAA-AAA", challenges: 0)
        XCTAssertFalse(world.items.isEmpty)
        XCTAssertTrue(world.items.allSatisfy { (1...24).contains($0.depth) })
        XCTAssertEqual(world.quests.map(\.kind), [.ghost, .wandmaker, .blacksmith, .imp])
        XCTAssertEqual(world.quests.map(\.variant),
                       [.gnollTrickster, .elementalEmbers, .crystal, .vault])
        XCTAssertEqual(world.quests.map(\.depth), [3, 9, 13, 19])
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
