import Foundation
import XCTest
@testable import SeedSeekerKit

final class FloorFeelingTests: XCTestCase {
    private func packet(version: String = "SSC5", tail: [UInt8] = [0]) -> Data {
        var bytes = Array(version.utf8) + [11] + Array("AAA-AAA-AAA".utf8)
        bytes += RingGems.catalogDefault.ordinals.map { UInt8($0) }
        bytes += [0, 0, 0] // no quests or items
        if version != "SSC3" {
            bytes.append(UInt8(ItemCatalog.trinkets.count))
            for item in ItemCatalog.trinkets {
                let text = Array(item.id.utf8)
                bytes += [UInt8(text.count >> 8), UInt8(text.count & 255)] + text
            }
        }
        if version == "SSC5" { bytes += tail }
        return Data(bytes)
    }

    func testAllFeelingIDsAndLegacyPackets() throws {
        let depths: [UInt8] = [1, 2, 3, 4, 6, 7, 8, 9]
        var tail: [UInt8] = [8]
        for (id, depth) in depths.enumerated() { tail += [depth, UInt8(id)] }
        let world = try ScoutCodec.decode(packet(tail: tail))
        XCTAssertEqual(world.feelings.count, 8)
        for (id, depth) in depths.enumerated() {
            XCTAssertEqual(world.feelings[Int(depth)]?.rawValue, id)
        }
        XCTAssertEqual(world.trinketOrder.map(\.id), ItemCatalog.trinkets.map(\.id))
        for version in ["SSC3", "SSC4", "SSC5"] {
            XCTAssertTrue(try ScoutCodec.decode(packet(version: version)).feelings.isEmpty)
        }
    }

    func testFullRegularFloorTable() throws {
        let depths = (1...24).filter { $0 % 5 != 0 }
        var tail: [UInt8] = [20]
        for depth in depths { tail += [UInt8(depth), UInt8(depth % 8)] }
        XCTAssertEqual(try ScoutCodec.decode(packet(tail: tail)).feelings.count, 20)
    }

    func testRejectsMalformedFeelingTables() {
        let malformed: [[UInt8]] = [
            [], [1], [1, 1], // missing count, depth, or feeling
            [21], // too many floors
            [1, 0, 1], [1, 25, 1], [1, 255, 1], // out-of-range depths
            [1, 5, 1], [1, 10, 1], [1, 15, 1], [1, 20, 1], // boss floors
            [2, 1, 1, 1, 2], [2, 2, 1, 1, 2], // duplicate or descending
            [1, 1, 8], [1, 1, 255], // unknown feeling IDs
            [0, 0], // trailing bytes
        ]
        for tail in malformed {
            XCTAssertThrowsError(try ScoutCodec.decode(packet(tail: tail)), "Accepted \(tail)")
        }
    }

    func testNativeScoutProvidesRegularFloorFeelings() async throws {
        let world = try await ProductionSeedFinderEngine().scoutSeed("AAA-AAA-AAA", challenges: 0)
        XCTAssertEqual(Set(world.feelings.keys), Set((1...24).filter { $0 % 5 != 0 }))
    }
}
