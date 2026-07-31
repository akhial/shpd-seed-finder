import SeedSeekerKit
import XCTest

final class ResultNavigationTests: XCTestCase {
    private let seeds = ["AAA-AAA-AAA", "BBB-BBB-BBB", "CCC-CCC-CCC"]

    func testPositionLocatesAScoutedSeedInsideTheResults() {
        XCTAssertEqual(ResultNavigation.position(of: "AAA-AAA-AAA", in: seeds), 0)
        XCTAssertEqual(ResultNavigation.position(of: "CCC-CCC-CCC", in: seeds), 2)
    }

    func testPositionIsNilOutsideTheResults() {
        XCTAssertNil(ResultNavigation.position(of: "ZZZ-ZZZ-ZZZ", in: seeds))
        XCTAssertNil(ResultNavigation.position(of: nil, in: seeds))
        XCTAssertNil(ResultNavigation.position(of: "", in: seeds))
        XCTAssertNil(ResultNavigation.position(of: "AAA-AAA-AAA", in: []))
    }

    func testSeedMovesForwardAndBackward() {
        XCTAssertEqual(ResultNavigation.seed(from: "AAA-AAA-AAA", in: seeds, offset: 1), "BBB-BBB-BBB")
        XCTAssertEqual(ResultNavigation.seed(from: "BBB-BBB-BBB", in: seeds, offset: 1), "CCC-CCC-CCC")
        XCTAssertEqual(ResultNavigation.seed(from: "CCC-CCC-CCC", in: seeds, offset: -1), "BBB-BBB-BBB")
    }

    func testSeedDoesNotWrapPastTheEnds() {
        XCTAssertNil(ResultNavigation.seed(from: "AAA-AAA-AAA", in: seeds, offset: -1))
        XCTAssertNil(ResultNavigation.seed(from: "CCC-CCC-CCC", in: seeds, offset: 1))
    }

    func testSeedClampsLargerJumpsToTheListEnds() {
        XCTAssertEqual(ResultNavigation.seed(from: "BBB-BBB-BBB", in: seeds, offset: 5), "CCC-CCC-CCC")
        XCTAssertEqual(ResultNavigation.seed(from: "BBB-BBB-BBB", in: seeds, offset: -5), "AAA-AAA-AAA")
    }

    func testSeedIsInertWithoutAnAnchorInTheResults() {
        XCTAssertNil(ResultNavigation.seed(from: "ZZZ-ZZZ-ZZZ", in: seeds, offset: 1))
        XCTAssertNil(ResultNavigation.seed(from: nil, in: seeds, offset: 1))
        XCTAssertNil(ResultNavigation.seed(from: "AAA-AAA-AAA", in: [], offset: 1))
        XCTAssertNil(ResultNavigation.seed(from: "AAA-AAA-AAA", in: ["AAA-AAA-AAA"], offset: 1))
    }
}
