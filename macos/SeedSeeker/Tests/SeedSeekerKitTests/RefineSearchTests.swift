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
    var filterError: Error?
    var filterDelay: Duration?
    private(set) var filteredSeeds: [[String]] = []
    private(set) var resumedCalls: [(resumeFrom: Int64, scanLen: Int64)] = []
    private(set) var freshCalls = 0

    /// An unscripted call must not trap — tests assert on the call counters to
    /// tell a fresh scan from a refine, so an unexpected one has to survive
    /// long enough to be reported.
    private func nextSession(_ queue: inout [FakeSearchSession]) -> FakeSearchSession {
        queue.isEmpty ? FakeSearchSession(batches: [], hint: ResumeHint(position: 0, remaining: 0))
                      : queue.removeFirst()
    }
    func startSearch(_ request: SearchRequest) async throws -> any SeedFinderSearchSession {
        lock.withLock {
            freshCalls += 1
            return nextSession(&startSessions)
        }
    }
    func startResumedSearch(_ request: SearchRequest, resumeFrom: Int64, scanLen: Int64) async throws -> any SeedFinderSearchSession {
        lock.withLock {
            resumedCalls.append((resumeFrom, scanLen))
            return nextSession(&resumedSessions)
        }
    }
    func filterSeeds(_ request: SearchRequest, seeds: [String]) async throws -> [String] {
        if let filterDelay { try await Task.sleep(for: filterDelay) }
        if let filterError { throw filterError }
        return lock.withLock {
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
    private func ringRequest(count: Int = 1) throws -> SearchRequest {
        try SearchRequest(requirements: (1...count).map { key in
            try ItemRequirement(key: Int64(100 + key), item: nil, upgrade: 0, kind: .ring,
                                upgradeMatch: .any)
        })
    }
    private func waitUntilIdle(_ controller: SearchController) async throws {
        let deadline = ContinuousClock.now + .seconds(5)
        while controller.isRunning && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertFalse(controller.isRunning)
    }

    /// The headline of the implicit model: the same Start Search action that
    /// ran the base run narrows it, with no separate refine gesture.
    func testStartingANarrowedQueryRefinesFilteringThenStreamingDedupedResumedResults() async throws {
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
        XCTAssertTrue(controller.canRefine(with: base),
                      "an unchanged request continues the run rather than rescanning")

        engine.filterResult = ["AAA-AAA-AAA", "AAA-AAA-AAC"]
        engine.resumedSessions = [FakeSearchSession(
            // The resumed scan re-reports AAA-AAA-AAC, which must be deduplicated.
            batches: [[result("AAA-AAA-AAC", matched: 2), result("AAA-AAA-AAD", matched: 2)]],
            hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(refined)
        try await waitUntilIdle(controller)

        XCTAssertEqual(engine.filteredSeeds, [["AAA-AAA-AAA", "AAA-AAA-AAB", "AAA-AAA-AAC"]])
        XCTAssertEqual(engine.freshCalls, 1, "the narrowed start must not rescan from zero")
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
        controller.start(refined)
        try await waitUntilIdle(controller)

        XCTAssertEqual(controller.state, .completed)
        XCTAssertTrue(engine.resumedCalls.isEmpty)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAB"])
        XCTAssertEqual(controller.refinedKept, 1)
        XCTAssertEqual(controller.baseRun?.remaining, 0)
        XCTAssertTrue(controller.canRefine(with: try wandRequest(count: 3)),
                      "a finished refine must itself be refinable")
    }

    /// Runs a base search then one successful refine, leaving the controller
    /// idle with results ["AAA-AAA-AAA"], refinedKept == 1, and a chainable
    /// base run at (600, 50) for the two-requirement request.
    private func makeRefinedController(engine: FakeEngine) async throws -> SearchController {
        let controller = SearchController(engine: engine)
        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAA"), result("AAA-AAA-AAB")]],
            hint: ResumeHint(position: 500, remaining: 100))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)

        engine.filterResult = ["AAA-AAA-AAA"]
        engine.resumedSessions = [FakeSearchSession(
            batches: [], hint: ResumeHint(position: 600, remaining: 50))]
        controller.start(try wandRequest(count: 2))
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA"])
        XCTAssertEqual(controller.refinedKept, 1)
        XCTAssertEqual(controller.baseRun?.resumeFrom, 600)
        return controller
    }

    func testCancelDuringFilterPhaseKeepsResultsAndBaseRun() async throws {
        let engine = FakeEngine()
        let controller = try await makeRefinedController(engine: engine)

        engine.filterDelay = .seconds(60)
        controller.start(try wandRequest(count: 3))
        XCTAssertTrue(controller.isRunning)
        controller.cancel()
        try await waitUntilIdle(controller)

        XCTAssertEqual(controller.state, .cancelled)
        XCTAssertNil(controller.message)
        XCTAssertNil(controller.refinedKept, "a cancelled filter must clear the stale kept caption")
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA"],
                       "cancelling the filter phase must not touch the base results")
        XCTAssertEqual(controller.baseRun?.resumeFrom, 600)
        XCTAssertEqual(controller.baseRun?.remaining, 50)
        XCTAssertTrue(controller.canRefine(with: try wandRequest(count: 3)),
                      "the untouched base run must stay refinable")
    }

    func testFilterFailureKeepsBaseRunForRetry() async throws {
        let engine = FakeEngine()
        let controller = try await makeRefinedController(engine: engine)

        engine.filterError = SeedFinderEngineError.invalidArgument
        controller.start(try wandRequest(count: 3))
        try await waitUntilIdle(controller)

        XCTAssertEqual(controller.state, .failed)
        XCTAssertNotNil(controller.message)
        XCTAssertNil(controller.refinedKept, "a failed filter must clear the stale kept caption")
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA"])
        XCTAssertEqual(controller.baseRun?.resumeFrom, 600)
        XCTAssertEqual(controller.baseRun?.remaining, 50)
        XCTAssertTrue(controller.canRefine(with: try wandRequest(count: 3)),
                      "the intact base run must allow a retry")

        engine.filterError = nil
        engine.filterResult = ["AAA-AAA-AAA"]
        engine.resumedSessions = [FakeSearchSession(
            batches: [], hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(try wandRequest(count: 3))
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.state, .completed)
        XCTAssertEqual(controller.refinedKept, 1)
    }

    func testCancelledRunStillBecomesTargetAndADetachedStartClearsRefinedKept() async throws {
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
        controller.start(try wandRequest(count: 2))
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.refinedKept, 1)

        // An unrelated query is the one start that runs fresh here: it clears
        // the refine caption while the Target keeps the earlier results.
        engine.startSessions = [FakeSearchSession(
            batches: [], hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(try ringRequest())
        try await waitUntilIdle(controller)
        XCTAssertNil(controller.refinedKept, "a fresh detached scan must clear the refine caption")
        XCTAssertTrue(controller.results.isEmpty)
        XCTAssertEqual(controller.target?.seeds, ["AAA-AAA-AAA"])
    }

    /// QA repro: the session survives every Start/Cancel cycle until an
    /// explicit Clear. Re-running an untouched query continues the cancelled
    /// run — the filter trivially keeps every seed and the scan resumes —
    /// rather than falling back to a fresh scan that wipes the results.
    func testRestartingAnUnchangedQueryAfterCancelContinuesInsteadOfWiping() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)
        let refined = try wandRequest(count: 2)

        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAA"), result("AAA-AAA-AAB")]],
            hint: ResumeHint(position: 500, remaining: 100))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)

        // Adding a requirement refines; the user cancels the resumed scan, so
        // the refined request — not the original — becomes the base run.
        engine.filterResult = ["AAA-AAA-AAA"]
        engine.resumedSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAC", matched: 2)]], finalState: .cancelled,
            hint: ResumeHint(position: 600, remaining: 50))]
        controller.start(refined)
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.state, .cancelled)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA", "AAA-AAA-AAC"])
        XCTAssertTrue(controller.canRefine(with: refined),
                      "the unchanged query must stay eligible after a cancel")

        for cycle in 1...3 {
            engine.filterResult = controller.results.map(\.seed)
            engine.resumedSessions = [FakeSearchSession(
                batches: [], finalState: .cancelled,
                hint: ResumeHint(position: 600, remaining: 50))]
            controller.start(refined)
            try await waitUntilIdle(controller)

            XCTAssertEqual(engine.freshCalls, 1, "cycle \(cycle) must not rescan from zero")
            XCTAssertEqual(engine.resumedCalls.count, cycle + 1)
            XCTAssertEqual(engine.resumedCalls.last?.resumeFrom, 600)
            XCTAssertEqual(engine.resumedCalls.last?.scanLen, 50)
            XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA", "AAA-AAA-AAC"],
                           "cycle \(cycle) must keep the session's results")
            XCTAssertEqual(controller.refinedKept, 2)
        }

        // Only the explicit Clear ends the session.
        controller.clearResults()
        XCTAssertTrue(controller.results.isEmpty)
        XCTAssertFalse(controller.canRefine(with: refined))
    }

    /// A query that is no continuation but still names a target item — here
    /// through a scope change — filters the full Target Set instead of
    /// rescanning, leaving the Target and its coverage untouched.
    func testScopeChangedQuerySharingAnItemFiltersTheTargetSetWithoutScanning() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)

        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAA"), result("AAA-AAA-AAB")]],
            hint: ResumeHint(position: 500, remaining: 100))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA", "AAA-AAA-AAB"])

        // More requirements, but at a different floor limit: not a
        // continuation, yet the wand requirements still name a target item.
        let rescoped = try SearchRequest(requirements: wandRequest(count: 2).requirements,
                                         maximumDepth: 12)
        XCTAssertFalse(controller.canRefine(with: rescoped))
        XCTAssertEqual(controller.decideStart(rescoped), .targetFilter)
        engine.filterResult = ["AAA-AAA-AAB"]
        controller.start(rescoped)
        try await waitUntilIdle(controller)

        XCTAssertEqual(engine.filteredSeeds, [["AAA-AAA-AAA", "AAA-AAA-AAB"]],
                       "the filter must re-verify the full Target Set")
        XCTAssertEqual(engine.freshCalls, 1, "a target filter must not rescan")
        XCTAssertTrue(engine.resumedCalls.isEmpty, "a target filter must not resume any scan")
        XCTAssertEqual(controller.state, .completed)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAB"])
        XCTAssertEqual(controller.refinedKept, 1)
        XCTAssertEqual(controller.refinedOf, 2)

        // The Target survived untouched: a continuation of the Target Query
        // refines the full set again and resumes the original coverage.
        engine.filterResult = ["AAA-AAA-AAA", "AAA-AAA-AAB"]
        engine.resumedSessions = [FakeSearchSession(
            batches: [], hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)
        XCTAssertEqual(engine.filteredSeeds.last, ["AAA-AAA-AAA", "AAA-AAA-AAB"])
        XCTAssertEqual(engine.resumedCalls.count, 1)
        XCTAssertEqual(engine.resumedCalls.first?.resumeFrom, 500)
        XCTAssertEqual(engine.resumedCalls.first?.scanLen, 100)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA", "AAA-AAA-AAB"],
                       "loosening back to the Target Query must bring seeds back")
    }

    /// An unrelated query runs detached: a fresh full scan replaces the
    /// display while the Target keeps the earlier results for later, and a
    /// continuation of the detached query continues that thread (the classic
    /// pre-Target refine) without touching the Target either.
    func testUnrelatedQueryRunsDetachedAndARelatedSearchBringsResultsBack() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)

        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAA"), result("AAA-AAA-AAB")]],
            hint: ResumeHint(position: 500, remaining: 100))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)

        // Rings share nothing with the wand target: a detached fresh scan.
        XCTAssertEqual(controller.decideStart(try ringRequest()), .detached)
        engine.startSessions = [FakeSearchSession(
            batches: [[result("ZZZ-AAA-AAA")]], hint: ResumeHint(position: 7, remaining: 70))]
        controller.start(try ringRequest())
        try await waitUntilIdle(controller)
        XCTAssertEqual(engine.freshCalls, 2)
        XCTAssertTrue(engine.filteredSeeds.isEmpty)
        XCTAssertEqual(controller.results.map(\.seed), ["ZZZ-AAA-AAA"])
        XCTAssertNil(controller.refinedKept, "a fresh detached scan is not a refine")
        XCTAssertEqual(controller.runKind, .detached)
        XCTAssertEqual(controller.target?.seeds, ["AAA-AAA-AAA", "AAA-AAA-AAB"],
                       "the Target must survive a detached scan untouched")

        // Narrowing the detached query continues the detached run: filter its
        // displayed results and resume its own remainder.
        XCTAssertEqual(controller.decideStart(try ringRequest(count: 2)), .continueDetached)
        engine.filterResult = ["ZZZ-AAA-AAA"]
        engine.resumedSessions = [FakeSearchSession(
            batches: [[result("ZZZ-AAA-AAB", matched: 2)]], hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(try ringRequest(count: 2))
        try await waitUntilIdle(controller)
        XCTAssertEqual(engine.filteredSeeds, [["ZZZ-AAA-AAA"]])
        XCTAssertEqual(engine.resumedCalls.count, 1)
        XCTAssertEqual(engine.resumedCalls.first?.resumeFrom, 7)
        XCTAssertEqual(engine.resumedCalls.first?.scanLen, 70)
        XCTAssertEqual(controller.results.map(\.seed), ["ZZZ-AAA-AAA", "ZZZ-AAA-AAB"])
        XCTAssertEqual(controller.refinedKept, 1)
        XCTAssertEqual(controller.refinedOf, 1)
        XCTAssertEqual(controller.runKind, .detached, "a continued detached scan stays detached")
        XCTAssertEqual(controller.target?.seeds, ["AAA-AAA-AAA", "AAA-AAA-AAB"])

        // Returning to the Target Query refines the full Target Set and
        // resumes the target's own coverage, not the detached thread's.
        engine.filterResult = ["AAA-AAA-AAA", "AAA-AAA-AAB"]
        engine.resumedSessions = [FakeSearchSession(
            batches: [], hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)
        XCTAssertEqual(engine.filteredSeeds.last, ["AAA-AAA-AAA", "AAA-AAA-AAB"])
        XCTAssertEqual(engine.resumedCalls.last?.resumeFrom, 500)
        XCTAssertEqual(engine.resumedCalls.last?.scanLen, 100)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA", "AAA-AAA-AAB"])
    }

    func testClearResultsDropsTheBaseRunSoTheNextStartIsFresh() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)
        let refined = try wandRequest(count: 2)

        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAA")]], hint: ResumeHint(position: 500, remaining: 100))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)
        XCTAssertTrue(controller.canRefine(with: refined))
        XCTAssertTrue(controller.canClearResults)

        controller.clearResults()
        XCTAssertTrue(controller.results.isEmpty)
        XCTAssertNil(controller.state)
        XCTAssertNil(controller.baseRun)
        XCTAssertNil(controller.refinedKept)
        XCTAssertNil(controller.exportQuery)
        XCTAssertNil(controller.selectedSeed)
        XCTAssertFalse(controller.isImported)
        XCTAssertFalse(controller.canClearResults, "nothing left to clear")
        XCTAssertFalse(controller.canRefine(with: refined))

        // The otherwise-eligible narrowed query now has nothing to narrow.
        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAB")]], hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(refined)
        try await waitUntilIdle(controller)
        XCTAssertTrue(engine.filteredSeeds.isEmpty)
        XCTAssertTrue(engine.resumedCalls.isEmpty)
        XCTAssertEqual(engine.freshCalls, 2)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAB"])
        XCTAssertNil(controller.refinedKept)
    }

    func testClearResultsIsIgnoredWhileASearchIsRunning() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)

        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAA")]], hint: ResumeHint(position: 500, remaining: 100))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)

        // A refine's filter phase counts as running just like a scan does.
        engine.filterDelay = .seconds(60)
        controller.start(try wandRequest(count: 2))
        XCTAssertTrue(controller.isRunning)
        XCTAssertFalse(controller.canClearResults)
        controller.clearResults()
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA"],
                       "clearing mid-run must not touch the results")
        XCTAssertEqual(controller.baseRun?.resumeFrom, 500)
        XCTAssertTrue(controller.isRunning)

        controller.cancel()
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA"])
        XCTAssertEqual(controller.baseRun?.resumeFrom, 500)
    }

    /// Refines always re-verify the full Target Set — never the last run's
    /// survivors — so loosening a requirement brings dropped seeds back, and
    /// new finds from the resumed scan join the set.
    func testRefineBasesOnFullTargetSetSoLooseningBringsSeedsBack() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)

        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAA"), result("AAA-AAA-AAB")]],
            hint: ResumeHint(position: 500, remaining: 100))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)

        // Narrowing drops AAA-AAA-AAB from the display and finds a new seed,
        // which joins the Target Set alongside the survivors.
        engine.filterResult = ["AAA-AAA-AAA"]
        engine.resumedSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAC", matched: 2)]],
            hint: ResumeHint(position: 600, remaining: 50))]
        controller.start(try wandRequest(count: 2))
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA", "AAA-AAA-AAC"])
        XCTAssertEqual(controller.target?.seeds, ["AAA-AAA-AAA", "AAA-AAA-AAB", "AAA-AAA-AAC"])

        // Loosening back to the Target Query filters the grown full set, so
        // the dropped seed returns, and the scan resumes the advanced coverage.
        engine.filterResult = ["AAA-AAA-AAA", "AAA-AAA-AAB", "AAA-AAA-AAC"]
        engine.resumedSessions = [FakeSearchSession(
            batches: [], hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)
        XCTAssertEqual(engine.filteredSeeds.last, ["AAA-AAA-AAA", "AAA-AAA-AAB", "AAA-AAA-AAC"])
        XCTAssertEqual(engine.resumedCalls.last?.resumeFrom, 600)
        XCTAssertEqual(engine.resumedCalls.last?.scanLen, 50)
        XCTAssertEqual(controller.results.map(\.seed),
                       ["AAA-AAA-AAA", "AAA-AAA-AAB", "AAA-AAA-AAC"])
        XCTAssertEqual(controller.refinedKept, 3)
        XCTAssertEqual(controller.refinedOf, 3)
    }

    /// An anchor that found nothing still resumes its coverage for a
    /// continuing query, but anything else re-anchors: an empty Target Set
    /// holds nothing worth preserving.
    func testEmptyTargetSetResumesAContinuationAndReanchorsAnythingElse() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)

        engine.startSessions = [FakeSearchSession(
            batches: [], hint: ResumeHint(position: 500, remaining: 100))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)
        XCTAssertTrue(controller.results.isEmpty)
        XCTAssertEqual(controller.target?.seeds, [])

        XCTAssertEqual(controller.decideStart(try wandRequest(count: 2)), .targetRefine)
        engine.filterResult = []
        engine.resumedSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAA", matched: 2)]],
            hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(try wandRequest(count: 2))
        try await waitUntilIdle(controller)
        XCTAssertEqual(engine.freshCalls, 1, "a continuing query must resume, not rescan")
        XCTAssertEqual(engine.resumedCalls.count, 1)
        XCTAssertEqual(engine.resumedCalls.first?.resumeFrom, 500)
        XCTAssertEqual(engine.resumedCalls.first?.scanLen, 100)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA"])
        XCTAssertEqual(controller.target?.seeds, ["AAA-AAA-AAA"],
                       "the resumed scan's finds join the Target Set")
        XCTAssertEqual(controller.target?.request.requirements.count, 1,
                       "the Target Query stays the original anchor query")
    }

    func testEmptyTargetSetReanchorsOnANonContinuingQuery() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)

        engine.startSessions = [FakeSearchSession(
            batches: [], hint: ResumeHint(position: 500, remaining: 100))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.target?.seeds, [])

        // Shares an item with the empty target, but with nothing to filter
        // the search re-anchors on this run instead.
        let rescoped = try SearchRequest(requirements: wandRequest(count: 1).requirements,
                                         maximumDepth: 12)
        XCTAssertEqual(controller.decideStart(rescoped), .anchor)
        engine.startSessions = [FakeSearchSession(
            batches: [[result("AAA-AAA-AAZ")]], hint: ResumeHint(position: 9, remaining: 0))]
        controller.start(rescoped)
        try await waitUntilIdle(controller)
        XCTAssertEqual(engine.freshCalls, 2)
        XCTAssertTrue(engine.filteredSeeds.isEmpty)
        XCTAssertTrue(engine.resumedCalls.isEmpty)
        XCTAssertEqual(controller.target?.seeds, ["AAA-AAA-AAZ"])
        XCTAssertEqual(controller.target?.request.maximumDepth, 12,
                       "the re-anchoring run's conclusion replaces the empty Target")

        // With the coverage exhausted, even a continuing query re-anchors.
        controller.clearResults()
        engine.startSessions = [FakeSearchSession(
            batches: [], hint: ResumeHint(position: 0, remaining: 0))]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.decideStart(try wandRequest(count: 2)), .anchor)
    }

    /// Import establishes the Target with no coverage: related queries filter
    /// the imported set, and nothing ever resumes a scan from it.
    func testImportedResultsBecomeAFilterOnlyTarget() async throws {
        let engine = FakeEngine()
        let controller = SearchController(engine: engine)
        controller.loadImported(seeds: ["AAA-AAA-AAA", "AAA-AAA-AAB"],
                                query: SavedQuery(requirements: try wandRequest(count: 1).requirements))

        XCTAssertEqual(controller.decideStart(try wandRequest(count: 2)), .targetRefine)
        engine.filterResult = ["AAA-AAA-AAB"]
        controller.start(try wandRequest(count: 2))
        try await waitUntilIdle(controller)
        XCTAssertEqual(engine.freshCalls, 0)
        XCTAssertTrue(engine.resumedCalls.isEmpty, "an import carries no coverage to resume")
        XCTAssertEqual(engine.filteredSeeds, [["AAA-AAA-AAA", "AAA-AAA-AAB"]])
        XCTAssertEqual(controller.state, .completed)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAB"])
        XCTAssertEqual(controller.refinedKept, 1)
        XCTAssertEqual(controller.refinedOf, 2)

        // Loosening back re-filters the full imported set.
        engine.filterResult = ["AAA-AAA-AAA", "AAA-AAA-AAB"]
        controller.start(try wandRequest(count: 1))
        try await waitUntilIdle(controller)
        XCTAssertEqual(controller.results.map(\.seed), ["AAA-AAA-AAA", "AAA-AAA-AAB"])
        XCTAssertTrue(engine.resumedCalls.isEmpty)
        XCTAssertEqual(engine.freshCalls, 0)
    }

    func testClearResultsAlsoDiscardsImportedResults() throws {
        let controller = SearchController(engine: FakeEngine())
        let requirement = try ItemRequirement(key: 1, item: nil, upgrade: 3, kind: .wand)
        controller.loadImported(seeds: ["AAA-AAA-AAA"],
                                query: SavedQuery(requirements: [requirement]))
        XCTAssertTrue(controller.canClearResults)
        controller.clearResults()
        XCTAssertTrue(controller.results.isEmpty)
        XCTAssertFalse(controller.isImported)
        XCTAssertEqual(controller.importedDropped, 0)
        XCTAssertNil(controller.exportQuery)
        XCTAssertFalse(controller.canClearResults)
    }
}
