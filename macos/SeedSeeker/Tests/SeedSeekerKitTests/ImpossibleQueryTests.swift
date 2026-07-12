import XCTest
@testable import SeedSeekerKit

final class ImpossibleQueryTests: XCTestCase {
    /// A +4 ring only comes from the Imp's quest (through floor 19), so a
    /// 14-floor search is unsatisfiable: the engine completes it before
    /// scanning a single seed, and the app must present that as an impossible
    /// query rather than a silent no-op.
    func testUnsatisfiableQueryCompletesInstantlyWithZeroScanned() async throws {
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 4, kind: .ring,
            upgradeMatch: .exactly)
        let session = try await ProductionSeedFinderEngine().startSearch(
            try SearchRequest(requirements: [requirement], maximumDepth: 14))
        let deadline = ContinuousClock.now + .seconds(5)
        var status = try await session.status()
        while status.state == .running && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
            status = try await session.status()
        }
        XCTAssertEqual(status.state, .completed)
        XCTAssertEqual(status.scannedSeeds, 0)
        await session.close()

        let satisfiable = try await ProductionSeedFinderEngine().startSearch(
            try SearchRequest(requirements: [requirement], maximumDepth: 24))
        let runningStatus = try await satisfiable.status()
        XCTAssertEqual(runningStatus.state, .running)
        await satisfiable.cancel()
        await satisfiable.close()
    }
}
