import Foundation
import XCTest
@testable import SeedSeekerKit

/// The requirement board's pure edits, ported case for case from the web
/// design's `web/src/designs/one/relations.test.ts` so both front-ends are
/// held to the same document encoding. Every case that ends in a list the
/// engine would see also asserts that the query builds, since the whole point
/// of the canonical encoding is that a board edit can never write a query the
/// search refuses.
final class RelationsTests: XCTestCase {
    // MARK: - Fixtures

    /// A requirement in the shape the web test's `req()` builds: a wildcard
    /// weapon unless told otherwise, with the item's own family when named.
    private func req(_ key: Int64, item id: String? = nil, kind: ItemKind? = nil,
                     upgradeMatch: UpgradeMatch = .any, upgrade: Int = 0,
                     maximumDepth: Int? = nil, identityGroup: Int? = nil,
                     alternativeGroup: Int? = nil, levelSum: LevelSum? = nil) throws -> ItemRequirement {
        let item = try id.map { try XCTUnwrap(ItemCatalog.findById($0), "unknown catalog item \($0)") }
        return try ItemRequirement(key: key, item: item, upgrade: upgrade,
                                   kind: kind ?? item?.kind ?? .weapon,
                                   upgradeMatch: upgradeMatch, identityGroup: identityGroup,
                                   maximumDepth: maximumDepth, alternativeGroup: alternativeGroup,
                                   levelSum: levelSum)
    }

    /// The board item holding requirement `index`, or a failure.
    private func item(_ requirements: [ItemRequirement], _ index: Int,
                      file: StaticString = #filePath, line: UInt = #line) throws -> BoardItem {
        try XCTUnwrap(requirements.boardItem(holding: index),
                      "no board item holds \(index)", file: file, line: line)
    }

    /// Each requirement's item id, or its category for a wildcard — the web
    /// test's `names()`.
    private func names(_ requirements: [ItemRequirement]) -> [String] {
        requirements.map { $0.item?.id ?? "\($0.kind)" }
    }

    private func assertSearchable(_ requirements: [ItemRequirement],
                                  file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertNoThrow(try SearchRequest(requirements: requirements), file: file, line: line)
    }

    /// The list after a round trip through the canonical query document.
    private func reloaded(_ requirements: [ItemRequirement]) throws -> [ItemRequirement] {
        let document = ResultsExport.encodeQuery(SavedQuery(requirements: requirements))
        return try ResultsExport.decodeQuery(document).requirements
    }

    // MARK: - Either/or clusters

    func testDroppingAChipOnAnotherMakesOneSlotPlacedAfterTheTarget() throws {
        let base = [try req(1, item: "spear"), try req(2, kind: .armor), try req(3, item: "shuriken")]
        let next = base.joinAlternatives(source: 2, target: 0)
        XCTAssertEqual(names(next), ["spear", "shuriken", "armor"])
        XCTAssertNotNil(next[0].alternativeGroup)
        XCTAssertEqual(next[0].alternativeGroup, next[1].alternativeGroup)
        XCTAssertEqual(next.boardItems().map(\.members), [[0, 1], [2]])
        let entries = ResultsExport.encodeQuery(SavedQuery(requirements: next))["requirements"] as? [Any]
        XCTAssertNotNil((entries?[0] as? [String: Any])?["any_of"])
    }

    func testJoiningAClusterDropsACombinedLevelAndLeavingAPairDissolvesIt() throws {
        let base = [
            try req(1, item: "ring_might", levelSum: LevelSum(group: 1, atLeast: 3)),
            try req(2, item: "ring_might", levelSum: LevelSum(group: 1, atLeast: 3)),
            try req(3, item: "shuriken"),
        ]
        let next = base.joinAlternatives(source: 0, target: 2)
        XCTAssertTrue(next.allSatisfy { $0.levelSum == nil })
        let shuriken = try XCTUnwrap(next.firstIndex { $0.item?.id == "shuriken" })
        let out = next.detach(shuriken)
        XCTAssertTrue(out.allSatisfy { $0.alternativeGroup == nil })
    }

    // MARK: - Stacks

    func testAConcreteStackEncodesAsPlainRepeats() throws {
        let base = [try req(1, item: "ring_might", upgradeMatch: .exactly, upgrade: 2),
                    try req(2, kind: .wand)]
        let next = base.setStackCount(try item(base, 0), 3)
        XCTAssertEqual(next.count, 4)
        XCTAssertEqual(next.filter { $0.item?.id == "ring_might" }.count, 3)
        XCTAssertTrue(next.allSatisfy { $0.identityGroup == nil })
        // The board folds the repeats back into one ×3 chip.
        let board = next.boardItems()
        XCTAssertEqual(board.count, 2)
        XCTAssertEqual(board[0].stackCount, 3)
        XCTAssertNil(board[0].total)
        assertSearchable(next)
        // The round trip through the document keeps the stack.
        XCTAssertEqual(try reloaded(next).boardItems()[0].stackCount, 3)
    }

    func testAWildcardStackEncodesAsBareCopiesSharingAnIdentityGroup() throws {
        let base = [try req(1, kind: .wand, upgradeMatch: .atLeast, upgrade: 1)]
        let next = base.setStackCount(try item(base, 0), 3)
        XCTAssertEqual(next.count, 3)
        XCTAssertEqual(Set(next.map(\.identityGroup)).count, 1)
        XCTAssertEqual(next[0].identityGroup, 1)
        XCTAssertTrue(next.dropFirst().allSatisfy {
            $0.kind == .wand && $0.item == nil && $0.upgradeMatch == .any
        })
        assertSearchable(next)
        XCTAssertEqual(next.boardItems()[0].stackCount, 3)
        // Shrinking to one dissolves the group entirely.
        let shrunk = next.setStackCount(try item(next, 0), 1)
        XCTAssertEqual(shrunk.count, 1)
        XCTAssertNil(shrunk[0].identityGroup)
    }

    func testAnEitherOrClusterAnchorsAStack() throws {
        let base = [try req(1, item: "runic_blade"), try req(2, item: "war_hammer")]
            .joinAlternatives(source: 1, target: 0)
        let next = base.setStackCount(try item(base, 0), 3)
        XCTAssertEqual(next.count, 4)
        XCTAssertEqual(next.filter { $0.identityGroup == 1 }.count, 4)
        XCTAssertEqual(next.filter { $0.alternativeGroup != nil }.count, 2)
        assertSearchable(next)
        let board = next.boardItems()
        XCTAssertEqual(board.count, 1)
        XCTAssertNotNil(board[0].cluster)
        XCTAssertEqual(board[0].stackCount, 3)
        // Removing one cluster member keeps the stack on the survivor.
        let dissolved = next.removeMember(1)
        XCTAssertEqual(dissolved.boardItems().count, 1)
        XCTAssertEqual(dissolved.boardItems()[0].stackCount, 3)
        assertSearchable(dissolved)
    }

    func testAPlainRepeatStackTradesItsCopiesForLabelsWhenItJoinsACluster() throws {
        let start = [try req(1, item: "spear"), try req(2, item: "mace")]
        let base = start.setStackCount(try item(start, 0), 2)
        let mace = try XCTUnwrap(base.firstIndex { $0.item?.id == "mace" })
        let next = base.joinAlternatives(source: mace, target: 0)
        // The copy is now a bare weapon tied to the whole cluster.
        let bare = next.filter { $0.item == nil }
        XCTAssertEqual(bare.count, 1)
        XCTAssertNotNil(bare[0].identityGroup)
        XCTAssertTrue(next.filter { $0.alternativeGroup != nil }
            .allSatisfy { $0.identityGroup == bare[0].identityGroup })
        assertSearchable(next)
    }

    func testDeletingTheAnchorDeletesItsCopiesAndLeavesNoStaleGroups() throws {
        let wildcardBase = [try req(1, kind: .wand), try req(2, kind: .armor)]
        let wildcard = wildcardBase.setStackCount(try item(wildcardBase, 0), 3)
        let afterWildcard = wildcard.removeItem(try item(wildcard, 0))
        XCTAssertEqual(afterWildcard.count, 1)
        XCTAssertEqual(afterWildcard[0].kind, .armor)
        XCTAssertTrue(afterWildcard.allSatisfy { $0.identityGroup == nil })

        let ringBase = [try req(1, item: "ring_might")]
        let stacked = ringBase.setStackCount(try item(ringBase, 0), 2)
        let total = stacked.setStackTotal(try item(stacked, 0), 3)
        XCTAssertTrue(total.removeItem(try item(total, 0)).isEmpty)
    }

    func testEjectingAMemberFromAStackedClusterStripsItsLabel() throws {
        var base = [try req(1, item: "spear"), try req(2, item: "mace")]
            .joinAlternatives(source: 1, target: 0)
        base = base.setStackCount(try item(base, 0), 2)
        let ejected = base.detach(0)
        let spear = try XCTUnwrap(ejected.first { $0.item?.id == "spear" })
        XCTAssertNil(spear.alternativeGroup)
        XCTAssertNil(spear.identityGroup)
        assertSearchable(ejected)
    }

    // MARK: - Combined levels

    func testATotalTurnsTheStackIntoIdenticalOptionalMembers() throws {
        var base = [try req(1, item: "ring_might", upgradeMatch: .exactly, upgrade: 2)]
        base = base.setStackCount(try item(base, 0), 2)
        let next = base.setStackTotal(try item(base, 0), 3)
        XCTAssertEqual(next.count, 2)
        XCTAssertTrue(next.allSatisfy { $0.levelSum == LevelSum(group: 1, atLeast: 3) })
        // The total speaks for the stack: per-member upgrades reset to any.
        XCTAssertTrue(next.allSatisfy { $0.upgradeMatch == .any })
        let board = next.boardItems()
        XCTAssertEqual(board.count, 1)
        XCTAssertEqual(board[0].total, 3)
        XCTAssertEqual(board[0].stackCount, 2)
        let entries = ResultsExport.encodeQuery(SavedQuery(requirements: next))["requirements"] as? [[String: Any]]
        XCTAssertEqual(entries?[0]["level_sum"] as? [String: Int], ["group": 1, "at_least": 3])
        assertSearchable(next)
        // Clearing the total returns to plain repeats.
        let cleared = next.setStackTotal(next.boardItems()[0], nil)
        XCTAssertTrue(cleared.allSatisfy { $0.levelSum == nil })
        XCTAssertEqual(cleared.boardItems()[0].stackCount, 2)
    }

    func testALoadedLevelSumDocumentCollapsesBackIntoOneChip() throws {
        let requirements = [
            try req(1, item: "ring_might", levelSum: LevelSum(group: 2, atLeast: 4)),
            try req(2, item: "ring_might", levelSum: LevelSum(group: 2, atLeast: 4)),
            try req(3, kind: .wand),
        ]
        let board = try reloaded(requirements).boardItems()
        XCTAssertEqual(board.count, 2)
        XCTAssertEqual(board[0].total, 4)
        XCTAssertEqual(board[0].stackCount, 2)
    }

    // MARK: - The editor round trip

    func testAppliesCountAndTotalFromTheEditorAndRebuildsTheStack() throws {
        var requirements = [ItemRequirement]()
            .applyEdit(index: nil, requirement: try req(1, item: "ring_might"), count: 2, total: 3)
        XCTAssertEqual(requirements.count, 2)
        XCTAssertTrue(requirements.allSatisfy { $0.levelSum?.atLeast == 3 })
        // Raising the count keeps the total; clearing it returns plain repeats.
        requirements = requirements.applyEdit(index: 0, requirement: requirements[0], count: 3, total: 5)
        XCTAssertEqual(requirements.count, 3)
        XCTAssertTrue(requirements.allSatisfy { $0.levelSum?.atLeast == 5 })
        requirements = requirements.applyEdit(index: 0, requirement: requirements[0], count: 2, total: nil)
        XCTAssertEqual(requirements.count, 2)
        XCTAssertTrue(requirements.allSatisfy { $0.levelSum == nil })
        XCTAssertEqual(requirements.filter { $0.item?.id == "ring_might" }.count, 2)
        assertSearchable(requirements)
    }

    func testRebuildsTheCopiesWhenTheEditChangesTheAnchorsCategory() throws {
        var requirements = [ItemRequirement]()
            .applyEdit(index: nil, requirement: try req(1, kind: .wand), count: 3, total: nil)
        XCTAssertTrue(requirements.allSatisfy { $0.kind == .wand })
        // The old copies named wands; the edited chip asks for rings, so the
        // stack comes down and is rebuilt rather than keeping stale wands.
        requirements = requirements.applyEdit(index: 0, requirement: try req(1, kind: .ring),
                                              count: 3, total: nil)
        XCTAssertEqual(requirements.count, 3)
        XCTAssertTrue(requirements.allSatisfy { $0.kind == .ring })
        assertSearchable(requirements)
    }

    func testShrinkingALevelSumStackFromTheEditorDropsItsOrphanedMembers() throws {
        var requirements = [ItemRequirement]()
            .applyEdit(index: nil, requirement: try req(1, item: "ring_might"), count: 3, total: 4)
        XCTAssertEqual(requirements.count, 3)
        requirements = requirements.applyEdit(index: 0, requirement: try req(1, item: "ring_might"),
                                              count: 1, total: nil)
        XCTAssertEqual(requirements.count, 1)
        XCTAssertNil(requirements[0].levelSum)
    }

    /// What the editor actually hands back is a plain row: the relationships
    /// are `applyEdit`'s to write. So re-saving a combined-level stack from a
    /// row carrying no `levelSum` of its own must still land on one.
    func testTheEditorsPlainRowRebuildsACombinedLevelStack() throws {
        var requirements = [ItemRequirement]()
            .applyEdit(index: nil, requirement: try req(1, item: "ring_might"), count: 2, total: 4)
        XCTAssertEqual(requirements.map { $0.levelSum?.atLeast }, [4, 4])
        let plain = try req(requirements[0].key, item: "ring_might")
        requirements = requirements.applyEdit(index: 0, requirement: plain, count: 3, total: 6)
        XCTAssertEqual(requirements.count, 3)
        XCTAssertTrue(requirements.allSatisfy { $0.levelSum == LevelSum(group: 1, atLeast: 6) })
        assertSearchable(requirements)
        // Saving again with the box unticked returns plain repeats, and the
        // copies take the floor limit the editor gave them.
        requirements = requirements.applyEdit(index: 0, requirement: plain, count: 3,
                                              total: nil, copyDepth: 4)
        XCTAssertTrue(requirements.allSatisfy { $0.levelSum == nil })
        XCTAssertEqual(requirements.map(\.maximumDepth), [nil, 4, 4])
        XCTAssertEqual(requirements.boardItems().map(\.stackCount), [3])
        assertSearchable(requirements)
    }

    // MARK: - Categories

    func testAStackDoesNotFollowItsChipIntoAClusterOfAnotherCategory() throws {
        // A copy has to name the kind it copies, and "ring or wand" names none,
        // so the second ring stays the standalone chip it already encodes as.
        var requirements = [ItemRequirement]()
            .applyEdit(index: nil, requirement: try req(1, item: "ring_might"), count: 2, total: nil)
        requirements += [try req(99, kind: .wand)]
        let joined = requirements.joinAlternatives(source: 0, target: 2)
        XCTAssertFalse(joined.contains { $0.identityGroup != nil })
        assertSearchable(joined)
        XCTAssertEqual(joined.boardItems().count, 2)
    }

    // MARK: - Copy floor limits

    func testTheAnchorAndItsCopiesCarryIndependentFloorLimits() throws {
        let anchor = try req(1, item: "plate_armor", upgradeMatch: .exactly, upgrade: 3, maximumDepth: 4)
        let requirements = [ItemRequirement]()
            .applyEdit(index: nil, requirement: anchor, count: 2, total: nil, copyDepth: 9)
        XCTAssertEqual(requirements.count, 2)
        XCTAssertEqual(requirements[0].maximumDepth, 4)
        XCTAssertEqual(requirements[1].maximumDepth, 9)
        // Still one chip: a repeat with only a floor limit folds into its stack.
        let board = requirements.boardItems()
        XCTAssertEqual(board.count, 1)
        XCTAssertEqual(board[0].stackCount, 2)
        XCTAssertEqual(requirements.copyDepth(of: board[0]), 9)
        assertSearchable(requirements)
        // The round trip through the document keeps both limits.
        let round = try reloaded(requirements)
        XCTAssertEqual(round.map(\.maximumDepth), [4, 9])
        XCTAssertEqual(round.boardItems().count, 1)
    }

    func testUnlimitedCopiesStayUnlimitedWhileTheAnchorIsFloorBound() throws {
        let anchor = try req(1, kind: .armor, upgradeMatch: .exactly, upgrade: 3, maximumDepth: 4)
        let requirements = [ItemRequirement]()
            .applyEdit(index: nil, requirement: anchor, count: 2, total: nil, copyDepth: nil)
        XCTAssertEqual(requirements[0].maximumDepth, 4)
        XCTAssertNil(requirements[1].maximumDepth)
        XCTAssertEqual(requirements[1].identityGroup, requirements[0].identityGroup)
        assertSearchable(requirements)
    }

    func testAWildcardStackLimitsItsBareCopiesWithoutConstrainingThemOtherwise() throws {
        let anchor = try req(1, kind: .wand, upgradeMatch: .atLeast, upgrade: 2)
        var requirements = [ItemRequirement]()
            .applyEdit(index: nil, requirement: anchor, count: 2, total: nil, copyDepth: 9)
        XCTAssertTrue(requirements.dropFirst().allSatisfy {
            $0.maximumDepth == 9 && $0.upgradeMatch == .any
        })
        assertSearchable(requirements)
        // Growing the stack from the chip badge keeps the copies' floor.
        requirements = requirements.setStackCount(try item(requirements, 0), 3)
        XCTAssertEqual(requirements.count, 3)
        XCTAssertTrue(requirements.dropFirst().allSatisfy { $0.maximumDepth == 9 })
    }

    func testEditingAwayTheLimitClearsItFromEveryCopy() throws {
        var requirements = [ItemRequirement]()
            .applyEdit(index: nil, requirement: try req(1, item: "longsword"),
                       count: 3, total: nil, copyDepth: 6)
        XCTAssertTrue(requirements.dropFirst().allSatisfy { $0.maximumDepth == 6 })
        requirements = requirements.applyEdit(index: 0, requirement: try req(1, item: "longsword"),
                                              count: 3, total: nil, copyDepth: nil)
        XCTAssertTrue(requirements.allSatisfy { $0.maximumDepth == nil })
    }

    func testTheCopiesKeepTheirFloorWhenTheStackFollowsItsChipIntoACluster() throws {
        var requirements = [ItemRequirement]()
            .applyEdit(index: nil, requirement: try req(1, item: "ring_might"),
                       count: 2, total: nil, copyDepth: 7)
        requirements += [try req(99, item: "ring_haste")]
        let joined = requirements.joinAlternatives(source: 0, target: 2)
        XCTAssertEqual(joined.first { $0.item == nil }?.maximumDepth, 7)
        assertSearchable(joined)
    }

    // MARK: - Board bookkeeping

    /// Two chips of the same item only fold into one stack while they are the
    /// plain repeats a stack is made of; a second constrained chip stays its
    /// own board entry, and `boardCount` is what the pane's header counts.
    func testBoardCountCountsChipsAndClustersNotRows() throws {
        let requirements = [
            try req(1, item: "longsword", upgradeMatch: .exactly, upgrade: 2),
            try req(2, item: "longsword"),
            try req(3, item: "longsword", upgradeMatch: .exactly, upgrade: 1),
            try req(4, kind: .armor),
        ]
        XCTAssertEqual(requirements.boardCount, 3)
        XCTAssertEqual(requirements.boardItems().map(\.stackCount), [2, 1, 1])
        // The pane's count and the engine's slot count are different questions:
        // a stack is one chip but four requirements to search for.
        XCTAssertEqual(requirements.slotCount, 4)
    }
}
