import Foundation
import SeedSeekerKit
import XCTest

final class SeedSeekerKitTests: XCTestCase {
    func testQueryCodecTierPredicateUsesSSF4() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .armor,
            tier: 4, tierMatch: .atLeast, upgradeMatch: .any)
        let request = try SearchRequest(requirements: [requirement])
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 52, 24, 0, 0, 1,
            1, 0, 0, 2, 4, 0, 0, 0, 0, 0, 0, 0,
        ])
        XCTAssertEqual(requirement.title, "Any Tier 4+ armor")
    }

    func testQueryCodecGoldenTwoRequirements() throws {
        let dagger = try XCTUnwrap(ItemCatalog.findById("dagger"))
        let first = try ItemRequirement(key: 1, item: dagger, upgrade: 2, modifier: "Lucky",
            kind: .weapon, upgradeMatch: .exactly, source: .chest, identityGroup: 1,
            maximumDepth: 5)
        let second = try ItemRequirement(key: 2, item: nil, upgrade: 0, kind: .ring,
            upgradeMatch: .atLeast)
        let request = try SearchRequest(requirements: [first, second], maximumDepth: 12,
                                        requireBlacksmith: true)
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 51, 12, 1, 0, 2,
            0, 0, 6, 100, 97, 103, 103, 101, 114, 1, 2,
            0, 5, 76, 117, 99, 107, 121, 2, 1, 5,
            3, 0, 0, 2, 0, 0, 0, 0, 0, 0,
        ])
    }

    func testQueryCodecFastModeSetsFlagBitOne() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 3, kind: .armor,
                                              upgradeMatch: .exactly)
        let request = try SearchRequest(requirements: [requirement], fastMode: true)
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 51, 24, 2, 0, 1,
            1, 0, 0, 1, 3, 0, 0, 0, 0, 0,
        ])
    }

    func testQueryCodecExcludeBlacksmithRewardsSetsFlagBitTwo() throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 2, kind: .weapon)
        let request = try SearchRequest(requirements: [requirement],
                                        excludeBlacksmithRewards: true)
        XCTAssertEqual(Array(try QueryCodec.encode(request)), [
            83, 83, 70, 51, 24, 4, 0, 1,
            0, 0, 0, 1, 2, 0, 0, 0, 0, 0,
        ])
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
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, modifier: "Lucky", kind: .wand))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon, identityGroup: 5))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 1, kind: .weapon, maximumDepth: 25))
        XCTAssertNoThrow(try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .weapon,
            tier: 5, tierMatch: .exactly, upgradeMatch: .any))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 0, kind: .weapon,
            tier: 1, tierMatch: .exactly, upgradeMatch: .any))
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: ItemCatalog.weapons[0], upgrade: 1,
            kind: .weapon, tier: 1, tierMatch: .exactly))
    }

    func testRealFFIScout() async throws {
        let world = try await ProductionSeedFinderEngine().scoutSeed("AAA-AAA-AAA")
        XCTAssertFalse(world.items.isEmpty)
        XCTAssertTrue(world.items.allSatisfy { (1...24).contains($0.depth) })
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
