import Foundation
import XCTest
@testable import SeedSeekerKit

final class TrinketTests: XCTestCase {
    private func packet(_ order: [String]) -> Data {
        var bytes = Array("SSC4".utf8) + [11] + Array("AAA-AAA-AAA".utf8)
        bytes += RingGems.catalogDefault.ordinals.map { UInt8($0) }
        bytes += [0, 0, 0, UInt8(order.count)] // no quests or items, then the deck
        for id in order {
            let text = Array(id.utf8)
            bytes += [UInt8(text.count >> 8), UInt8(text.count & 255)] + text
        }
        return Data(bytes)
    }

    func testDeckPreservesWireOrderAndRejectsMalformedDecks() throws {
        let order = Array(ItemCatalog.trinkets.reversed()).map(\.id)
        let world = try ScoutCodec.decode(packet(order))
        XCTAssertEqual(world.trinketOrder.map(\.id), order)
        XCTAssertThrowsError(try ScoutCodec.decode(packet(Array(order.dropLast()))))
        var repeated = order
        repeated[16] = repeated[0]
        XCTAssertThrowsError(try ScoutCodec.decode(packet(repeated)))
        var unknown = order
        unknown[0] = "dagger"
        XCTAssertThrowsError(try ScoutCodec.decode(packet(unknown)))
    }

    func testNativeScoutTrinketsAndMatchIndicesAgree() async throws {
        let world = try await ProductionSeedFinderEngine().scoutSeed("AAA-AAA-AAA", challenges: 0)
        XCTAssertEqual(world.trinketOrder.count, 17)
        XCTAssertEqual(world.trinketOrder.prefix(4).map(\.id),
                       ["dimensional_sundial", "mimic_tooth", "parchment_scrap", "thirteen_leaf_clover"])
        let choices = world.items.filter { $0.item.kind == .trinket }
        XCTAssertEqual(choices.count, 4)
        XCTAssertEqual(Set(choices.map { $0.item.id }), Set(world.trinketOrder.prefix(4).map(\.id)))
        XCTAssertTrue(choices.allSatisfy { $0.depth == 3 && $0.source == .lockedChest })
        let requirement = try ItemRequirement(key: 1, item: XCTUnwrap(ItemCatalog.findById("mimic_tooth")),
                                               upgrade: 0, kind: .trinket, upgradeMatch: .any)
        let query = try SearchRequest(requirements: [requirement])
        let marks = try ScoutMatches.mark(seed: world.seed, challenges: 0, query: query)
        XCTAssertEqual(marks.matchedRequirements, 1)
        XCTAssertEqual(world.items[try XCTUnwrap(marks.matched.first)].item.id, "mimic_tooth")
    }

    func testNamedTrinketsJoinAnAlternativeAndRoundTrip() throws {
        let first = try ItemRequirement(key: 1, item: XCTUnwrap(ItemCatalog.findById("mimic_tooth")),
                                        upgrade: 0, kind: .trinket, upgradeMatch: .any)
        let second = try ItemRequirement(key: 2, item: XCTUnwrap(ItemCatalog.findById("rat_skull")),
                                         upgrade: 0, kind: .trinket, upgradeMatch: .any)
        let requirements = [first, second].joinAlternatives(source: 0, target: 1)
        XCTAssertEqual(requirements.slotCount, 1)
        XCTAssertFalse(requirements.canStack(requirements.boardItems()[0]))
        let query = SavedQuery(requirements: requirements)
        let document = ResultsExport.encodeQuery(query)
        let restored = try ResultsExport.decodeQuery(document)
        XCTAssertEqual(restored.requirements.map { $0.item?.id }, requirements.map { $0.item?.id })
        XCTAssertEqual(restored.requirements.slotCount, 1)
        XCTAssertEqual(first.title, "Mimic Tooth")
        XCTAssertThrowsError(try ItemRequirement(key: 3, item: nil, upgrade: 0,
                                               kind: .trinket, upgradeMatch: .any))
        XCTAssertThrowsError(try ItemRequirement(key: 3, item: first.item, upgrade: 1,
                                               kind: .trinket, upgradeMatch: .exactly))
    }
}
