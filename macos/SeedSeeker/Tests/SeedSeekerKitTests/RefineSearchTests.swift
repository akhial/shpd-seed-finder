import Foundation
import XCTest
@testable import SeedSeekerKit

/// A scripted session: each poll drains one batch, the status turns terminal
/// once every batch is delivered, and the resume hint is fixed up front.
private final class FakeSearchSession: SeedFinderSearchSession, @unchecked Sendable {
    private let lock = NSLock()
    private var batches: [[SeedResult]]
    private let finalState: SearchState
    private let hint: ResumeHint
    private(set) var closed = false

    init(batches: [[SeedResult]], finalState: SearchState = .completed, hint: ResumeHint) {
        self.batches = batches; self.finalState = finalState; self.hint = hint
    }
    func poll(_ maximum: Int) async throws -> [SeedResult] {
        lock.withLock { batches.isEmpty ? [] : batches.removeFirst() }
    }
    func status() async throws -> SearchStatus {
        lock.withLock {
            SearchStatus(state: batches.isEmpty ? finalState : .running,
                         scannedSeeds: 0, totalSeeds: 0, errorCode: 0, matchProbability: 0)
        }
    }
    func resumeHint() async throws -> ResumeHint { hint }
    func cancel() async {}
    func close() async { lock.withLock { closed = true } }
}

private final class FakeEngine: SeedFinderEngine, @unchecked Sendable {
    private let lock = NSLock()
    var startSessions: [FakeSearchSession] = []
    var resumedSessions: [FakeSearchSession] = []
    var filterResult: [String] = []
    private(set) var filteredSeeds: [[String]] = []
    private(set) var resumedCalls: [(resumeFrom: Int64, scanLen: Int64)] = []

    func startSearch(_ request: SearchRequest) async throws -> any SeedFinderSearchSession {
        lock.withLock { startSessions.removeFirst() }
    }
    func startResumedSearch(_ request: SearchRequest, resumeFrom: Int64, scanLen: Int64) async throws -> any SeedFinderSearchSession {
        lock.withLock {
            resumedCalls.append((resumeFrom, scanLen))
            return resumedSessions.removeFirst()
        }
    }
    func filterSeeds(_ request: SearchRequest, seeds: [String]) async throws -> [String] {
        lock.withLock {
            filteredSeeds.append(seeds)
            return filterResult
        }
    }
    func scoutSeed(_ seed: String, challenges: Int) async throws -> ScoutWorld {
        throw SeedFinderEngineError.invalidArgument
    }
}

@MainActor
final class RefineSearchTests: XCTestCase {
    private func wandRequest(count: Int) throws -> SearchRequest {
        try SearchRequest(requirements: (1...count).map { key in
            try ItemRequirement(key: Int64(key), item: nil,
                                upgrade: key == 1 ? 3 : 0, kind: .wand,
                                upgradeMatch: key == 1 ? .exactly : .any)
        })
    }
    private func result(_ seed: String, matched: Int = 1) -> SeedResult {
        SeedResult(seed: seed, matchedRequirements: matched)
    }
    private func waitUntilIdle(_ controller: SearchController) async throws {
        let deadline = ContinuousClock.now + .seconds(5)
        while controller.isRunning && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertFalse(controller.isRunning)
    }

    func testRefineFiltersPreviousResultsThenStreamsDedupedResumedResults() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)
        let base = try wandRequest(count: 1)
        let refined = try wandRequest(count: 2)

        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAA"), result("AAA-AAA-AAB")], [result("AAA-AAA-AAC")]],
            hint: ResumeHint(position: 500, remaining: 100))]
        controller.start(base)
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.state, .completed)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA", "AAA-AAA-AAB", "AAA-AAA-AAC"])
        XCTAssertNil(controller.refinedKept)
        XCTAssertEqual(controller.baseRun?.resumeFrom, 500)
        XCTAssertEqual(controller.baseRun?.remaining, 100)

        XCTAssertTrue(controller.canRefine(with: refined))
        XCTAssertFalse(controller.canRefine(with: base), "identical request must not refine")

        engine.filterResult = ["AAA-AAA-AAA", "AAA-AAA-AAC"]
        engine.resumedSessions = [FakeSearchSession(
            // The resumed scan re-reports AAA-AAA-AAC, which must be deduplicated.
            batches: [[result("AAA-AAA-AAC", matched: 2), result("AAA-AAA-AAD", matched: 2)]],
            hint: ResumeHint(position: 0, remaining: 0))]
        controller.refine(refined)
        try await waitUntilIdle(controller)

        XCTAssertEqual(engine.filteredSeeds, [["AAA-AAA-AAA", "AAA-AAA-AAB", "AAA-AAA-AAC"]])
        XCTAssertEqual(engine.resumedCalls.count, 1)
        XCTAssertEqual(engine.resumedCalls.first?.resumeFrom, 500)
        XCTAssertEqual(engine.resumedCalls.first?.scanLen, 100)
        XCTAssertEqual(controller.state, .completed)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA", "AAA-AAA-AAC", "AAA-AAA-AAD"])
        XCTAssertEqual(controller.refinedKept, 2)
        // The refined run is the new base and is chainable.
        XCTAssertEqual(controller.baseRun?.remaining, 0)
        XCTAssertEqual(controller.baseRun?.request.requirements.count, 2)
    }

    func testRefineWithNothingRemainingCompletesWithFilteredSubsetOnly() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)
        let base = try wandRequest(count: 1)
        let refined = try wandRequest(count: 2)

        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAA"), result("AAA-AAA-AAB")]],
            hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(base)
        try await waitUntilIdle(controller)

        engine.filterResult = ["AAA-AAA-AAB"]
        controller.refine(refined)
        try await waitUntilIdle(controller)

        XCTAssertEqual(controller.state, .completed)
        XCTAssertTrue(engine.resumedCalls.isEmpty)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAB"])
        XCTAssertEqual(controller.refinedKept, 1)
        XCTAssertEqual(controller.baseRun?.remaining, 0)
        XCTAssertTrue(controller.canRefine(with: try wandRequest(count: 3)),
                      "a finished refine must itself be refinable")
    }

    func testCancelledRunStillBecomesBaseAndFreshStartClearsRefinedKept() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)
        let base = try wandRequest(count: 1)

        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAA")]], finalState: .cancelled,
            hint: ResumeHint(position: 123, remaining: 456))]
        controller.start(base)
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.state, .cancelled)
        XCTAssertEqual(controller.baseRun?.resumeFrom, 123)
        XCTAssertEqual(controller.baseRun?.remaining, 456)
        XCTAssertTrue(controller.canRefine(with: try wandRequest(count: 2)))

        engine.filterResult = ["AAA-AAA-AAA"]
        engine.resumedSessions = [FakeSearchSession(
            batches: [], hint: ResumeHint(position: 0, remaining: 0))]
        controller.refine(try wandRequest(count: 2))
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.refinedKept, 1)

        engine.startSessions = [FakeSearchSession(
            batches: [], hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(base)
        try await waitUntilIdle(controller)
        XCTAssertNil(controller.refinedKept, "a fresh search must clear the refine caption")
        XCTAssertTrue(controller.results.isEmpty)
    }
}
