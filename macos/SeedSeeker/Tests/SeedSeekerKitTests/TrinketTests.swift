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
    func testSelectedTrinketPersistsAndLegacyQueriesDefaultOff() throws {
        let requirement = try ItemRequirement(key: 1, item: XCTUnwrap(ItemCatalog.findById("mimic_tooth")),
            upgrade: 0, kind: .trinket, upgradeMatch: .any, selectTrinket: true)
        let saved = SavedQuery(requirements: [requirement])
        XCTAssertTrue(QueryPersistence.decode(try XCTUnwrap(QueryPersistence.encode(saved))).requirements[0].selectTrinket)
        let document = ResultsExport.encodeQuery(saved)
        XCTAssertTrue(try ResultsExport.decodeQuery(document).requirements[0].selectTrinket)
        let encoded = try JSONEncoder().encode(requirement)
        var legacy = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
        legacy.removeValue(forKey: "selectTrinket")
        XCTAssertFalse(try JSONDecoder().decode(ItemRequirement.self,
            from: JSONSerialization.data(withJSONObject: legacy)).selectTrinket)
        XCTAssertThrowsError(try ItemRequirement(key: 2, item: nil, upgrade: 0,
            kind: .ring, upgradeMatch: .any, selectTrinket: true))
    }

    func testSelectedScoutResponseValidatesInitialOffers() throws {
        let order = ItemCatalog.trinkets.map(\.id)
        func selectedPacket(_ id: String) -> Data {
            var result = packet(order)
            result.replaceSubrange(0..<4, with: "SSC5".utf8)
            result.append(contentsOf: [UInt8(id.utf8.count >> 8), UInt8(id.utf8.count & 255)])
            result.append(contentsOf: id.utf8)
            return result
        }
        XCTAssertEqual(try ScoutCodec.decode(selectedPacket(order[0])).selectedTrinket, order[0])
        XCTAssertNil(try ScoutCodec.decode(selectedPacket("")).selectedTrinket)
        XCTAssertThrowsError(try ScoutCodec.decode(selectedPacket(order[4])))
        XCTAssertThrowsError(try ScoutCodec.decode(selectedPacket("unknown")))
    }

    func testSelectedScoutAndMarksUseTheSameRequest() async throws {
        let requirement = try ItemRequirement(key: 1, item: XCTUnwrap(ItemCatalog.findById("mimic_tooth")),
            upgrade: 0, kind: .trinket, upgradeMatch: .any, selectTrinket: true)
        let query = try SearchRequest(requirements: [requirement])
        let engine = ProductionSeedFinderEngine()
        let selected = try await engine.scoutSeed("AAA-AAA-AAA", challenges: 0, query: query, trinket: nil)
        XCTAssertEqual(selected.selectedTrinket, "mimic_tooth")
        for override in ["mimic_tooth", "none", "parchment_scrap"] {
            let world = try await engine.scoutSeed("AAA-AAA-AAA", challenges: 0, query: query, trinket: override)
            XCTAssertEqual(world.selectedTrinket, override == "none" ? nil : override)
            let request = try ScoutCodec.encodeRequest(seed: world.seed, challenges: 0, query: query, trinket: override)
            XCTAssertEqual(String(data: request.prefix(4), encoding: .utf8), "SSQ3")
            let marks = try ScoutMatches.mark(request, query: QueryDocument.encode(query))
            XCTAssertEqual(marks.matchedRequirements, 1)
            XCTAssertEqual(world.items[try XCTUnwrap(marks.matched.first)].item.id, "mimic_tooth")
        }
    }

}
