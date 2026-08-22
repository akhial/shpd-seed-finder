import CSeedFinder
import Foundation
import XCTest
@testable import SeedSeekerKit

/// The app keeps local copies of the engine's scalar constants so the models
/// need nothing from the engine to validate. This is the one place they meet
/// the engine: every local is asserted against the `seedfinder_engine_info`
/// document the linked engine publishes, so a change on either side fails
/// here rather than as an editor offering a query the search refuses.
final class EngineConstantsTests: XCTestCase {
    /// Read per test case: the document is a constant of the linked engine
    /// and cheap to fetch, and an instance property keeps the test free of
    /// shared mutable state under strict concurrency.
    private let info: [String: Any] = {
        let packet = try! enginePacket { out, length in seedfinder_engine_info(out, length) }
        return try! JSONSerialization.jsonObject(with: packet) as! [String: Any]
    }()
    private var limits: [String: Any] { info["limits"] as! [String: Any] }
    private func limit(_ key: String) -> Int { limits[key] as! Int }

    func testQueryBoundsMatchTheEngine() {
        XCTAssertEqual(SearchLimits.maxDepth, limit("maxDepth"))
        XCTAssertEqual(SearchLimits.exactTiers, limit("exactTierMin")...limit("exactTierMax"))
        XCTAssertEqual(SearchLimits.boundedTiers, limit("boundedTierMin")...limit("boundedTierMax"))
        XCTAssertEqual(SearchLimits.identityGroupMax, limit("identityGroupMax"))
        XCTAssertEqual(SearchLimits.upgradeSumGroupMax, limit("upgradeSumGroupMax"))
        XCTAssertEqual(SearchLimits.maxUpgradeDefault, limit("maxUpgradeDefault"))
        XCTAssertEqual(SearchLimits.maxUpgradeRing, limit("maxUpgradeRing"))
        // The families route to the right maximum, narrowed weapon kinds included.
        XCTAssertEqual(ItemKind.ring.maximumSearchUpgrade, limit("maxUpgradeRing"))
        for kind in ItemKind.allCases where kind != .ring {
            XCTAssertEqual(kind.maximumSearchUpgrade, limit("maxUpgradeDefault"), "\(kind)")
        }
    }

    @MainActor
    func testSessionAndFileLimitsMatchTheEngine() {
        XCTAssertEqual(SearchController.resultCap, info["maxResults"] as? Int)
        // The import byte cap and the seed count have no local copies: the
        // codec applies the cap itself and the session reports the count.
        XCTAssertGreaterThan(limit("resultsFileMaxBytes"), 0)
        XCTAssertEqual(info["totalSeeds"] as? Int64, 5_429_503_678_976)
        XCTAssertEqual(EngineInfo.shared.shpdVersion, info["shpdVersion"] as? String)
    }

    func testEmptyBossFloorsMatchTheEngine() {
        XCTAssertEqual(FloorLimits.emptyBossFloors, Set(info["emptyBossFloors"] as! [Int]))
        XCTAssertEqual(FloorLimits.options,
                       (1...SearchLimits.maxDepth).filter { !FloorLimits.emptyBossFloors.contains($0) })
    }

    func testQuestWindowsMatchTheEngine() {
        let windows = info["questWindows"] as! [String: [Int]]
        let names: [ScoutQuestKind: String] = [.ghost: "ghost", .wandmaker: "wandmaker",
                                               .blacksmith: "blacksmith", .imp: "imp"]
        XCTAssertEqual(Set(windows.keys), Set(names.values))
        for kind in ScoutQuestKind.allCases {
            let window = windows[names[kind]!]!
            XCTAssertEqual(window.count, 2)
            XCTAssertEqual(kind.depthRange, window[0]...window[1], kind.giverLabel)
        }
    }

    func testChallengesMatchTheEngineInMaskOrder() {
        let engine = (info["challenges"] as! [[String: Any]]).map { entry in
            (entry["name"] as! String, entry["mask"] as! Int, entry["changesLevelGeneration"] as! Bool)
        }
        let localNames = ["on_diet", "faith_is_my_armor", "pharmacophobia", "barren_land", "swarm_intelligence",
                          "into_darkness", "forbidden_runes", "hostile_champions", "badder_bosses"]
        let local = zip(localNames, Challenge.allCases).map { ($0, $1.rawValue, $1.changesLevelGeneration) }
        XCTAssertEqual(engine.count, local.count)
        for (index, (engineEntry, localEntry)) in zip(engine, local).enumerated() {
            XCTAssertEqual(engineEntry.0, localEntry.0)
            XCTAssertEqual(engineEntry.1, localEntry.1)
            XCTAssertEqual(engineEntry.2, localEntry.2, engineEntry.0)
            XCTAssertEqual(localEntry.1, 1 << index)
        }
        XCTAssertEqual(SearchLimits.challengeMask, engine.reduce(0) { $0 | $1.1 })
    }
}
