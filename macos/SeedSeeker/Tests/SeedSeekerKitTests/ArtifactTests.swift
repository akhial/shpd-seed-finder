import Foundation
import XCTest
@testable import SeedSeekerKit

final class ArtifactTests: XCTestCase {
    func testNamedArtifactsUseTheirOwnUpgradeCeilingAndCannotStack() throws {
        XCTAssertEqual(ItemKind.artifact.rawValue, 7)
        XCTAssertEqual(ItemCatalog.artifacts.count, 11)
        XCTAssertNil(ItemCatalog.findById("cloak_of_shadows"))
        XCTAssertNil(ItemCatalog.findById("holy_tome"))
        for item in ItemCatalog.artifacts {
            let requirement = try ItemRequirement(key: 1, item: item, upgrade: 5,
                kind: .artifact, upgradeMatch: .exactly, source: .impReward,
                maximumDepth: 19, requireUncursed: true)
            XCTAssertEqual(requirement.maximumUpgrade, 5)
            XCTAssertFalse([requirement].canStack([requirement].boardItems()[0]))
            XCTAssertThrowsError(try ItemRequirement(key: 2, item: item, upgrade: 6,
                kind: .artifact, upgradeMatch: .exactly))
            XCTAssertThrowsError(try ItemRequirement(key: 2, item: item, upgrade: 0,
                effect: .anyEnchantment, kind: .artifact, upgradeMatch: .any))
        }
        XCTAssertThrowsError(try ItemRequirement(key: 1, item: nil, upgrade: 0,
            kind: .artifact, upgradeMatch: .any))
    }

    func testRepeatedArtifactsRemainSeparateAndJoinWithoutUnnamedCopies() throws {
        let item = try XCTUnwrap(ItemCatalog.findById("ethereal_chains"))
        let first = try ItemRequirement(key: 1, item: item, upgrade: 0,
            kind: .artifact, upgradeMatch: .any)
        let second = try ItemRequirement(key: 2, item: item, upgrade: 0,
            kind: .artifact, upgradeMatch: .any, maximumDepth: 14)
        let requirements = [first, second]
        let board = requirements.boardItems()
        XCTAssertEqual(board.count, 2)
        XCTAssertTrue(board.allSatisfy { $0.extras.isEmpty && !requirements.canStack($0) })
        let joined = requirements.joinAlternatives(source: 0, target: 1)
        XCTAssertEqual(joined.count, 2)
        XCTAssertEqual(joined.boardCount, 1)
        XCTAssertEqual(joined.slotCount, 1)
        XCTAssertTrue(joined.allSatisfy { $0.item == item && $0.identityGroup == nil })
        XCTAssertNotNil(SavedQuery(requirements: joined).validated())
        XCTAssertNoThrow(try SearchRequest(requirements: joined))
    }

    func testArtifactAlternativesRoundTripThroughPersistenceDocumentsAndLinks() throws {
        let first = try ItemRequirement(key: 1,
            item: XCTUnwrap(ItemCatalog.findById("sandals_of_nature")), upgrade: 5,
            kind: .artifact, upgradeMatch: .exactly, source: .impReward,
            maximumDepth: 19, requireUncursed: true)
        let second = try ItemRequirement(key: 2,
            item: XCTUnwrap(ItemCatalog.findById("ethereal_chains")), upgrade: 0,
            kind: .artifact, upgradeMatch: .any, maximumDepth: 14)
        let requirements = [first, second].joinAlternatives(source: 0, target: 1)
        XCTAssertEqual(requirements.slotCount, 1)
        XCTAssertFalse(requirements.canStack(requirements.boardItems()[0]))
        let query = SavedQuery(requirements: requirements)
        let persisted = try JSONDecoder().decode(SavedQuery.self, from: JSONEncoder().encode(query))
        XCTAssertEqual(persisted, query)
        XCTAssertNotNil(persisted.validated())
        let restored = try ResultsExport.decodeQuery(ResultsExport.encodeQuery(query))
        let linked = try DeepLink.decode(DeepLink.encodeLink(for: query))
        for decoded in [restored, linked] {
            var expected = requirements
            var actual = decoded.requirements
            for index in expected.indices { expected[index].key = 0 }
            for index in actual.indices { actual[index].key = 0 }
            XCTAssertEqual(actual, expected)
            XCTAssertEqual(actual.slotCount, 1)
        }
    }

    func testNativeScoutVaultUpgradeAndMatchIndicesAgree() async throws {
        let world = try await ProductionSeedFinderEngine().scoutSeed("AAA-AAA-AAA", challenges: 0)
        let artifacts = world.items.filter { $0.item.kind == .artifact }
        XCTAssertEqual(artifacts.count, 4)
        let vault = try XCTUnwrap(artifacts.first { $0.source == .impReward })
        XCTAssertEqual(vault.item.id, "sandals_of_nature")
        XCTAssertEqual(vault.depth, 19)
        XCTAssertEqual(vault.upgrade, 5)
        XCTAssertFalse(vault.cursed)
        XCTAssertTrue(artifacts.filter { $0.source != .impReward }.allSatisfy { $0.upgrade == 0 })
        var requirement = try ItemRequirement(key: 1, item: vault.item, upgrade: 5,
            kind: .artifact, upgradeMatch: .exactly, source: .impReward,
            maximumDepth: 19, requireUncursed: true)
        let marks = try ScoutMatches.mark(seed: world.seed, challenges: 0,
            query: SearchRequest(requirements: [requirement]))
        XCTAssertEqual(marks.matchedRequirements, 1)
        let matched = world.items[try XCTUnwrap(marks.matched.first)]
        XCTAssertEqual(matched.item.id, vault.item.id)
        XCTAssertEqual(matched.upgrade, 5)
        requirement.maximumDepth = 18
        let tooEarly = try ScoutMatches.mark(seed: world.seed, challenges: 0,
            query: SearchRequest(requirements: [requirement]))
        XCTAssertEqual(tooEarly.matchedRequirements, 0)
    }

    @MainActor
    func testArtifactProbabilityLabelIsUnavailable() throws {
        let requirement = try ItemRequirement(key: 1,
            item: XCTUnwrap(ItemCatalog.findById("ethereal_chains")), upgrade: 0,
            kind: .artifact, upgradeMatch: .any)
        let controller = SearchController()
        controller.loadImported(seeds: ["AAA-AAA-AAA"], query: SavedQuery(requirements: [requirement]))
        XCTAssertEqual(controller.probabilityLabel, "unavailable")
        XCTAssertNil(controller.timeToSeed)
        XCTAssertFalse(controller.isImpossibleQuery)
    }

    func testUnavailableArtifactProbabilityDoesNotFailNativeStatus() async throws {
        let requirement = try ItemRequirement(key: 1,
            item: XCTUnwrap(ItemCatalog.findById("ethereal_chains")), upgrade: 0,
            kind: .artifact, upgradeMatch: .any)
        let session = try await ProductionSeedFinderEngine().startSearch(
            SearchRequest(requirements: [requirement]), workers: 1)
        do {
            let status = try await session.status()
            XCTAssertTrue(status.matchProbability.isNaN)
            XCTAssertEqual(status.state, .running)
        } catch {
            await session.cancel()
            await session.close()
            throw error
        }
        await session.cancel()
        await session.close()
    }
}
