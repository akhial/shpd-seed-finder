import Foundation
import Observation

/// Everything a later refine needs from a finished search: the request that
/// ran, plus where a follow-up scan must pick up (`remaining` seeds starting
/// at `resumeFrom`) to complete its seed-space coverage.
public struct BaseRun: Sendable {
    public let request: SearchRequest
    public let resumeFrom: Int64
    public let remaining: Int64
    public init(request: SearchRequest, resumeFrom: Int64, remaining: Int64) {
        self.request = request; self.resumeFrom = resumeFrom; self.remaining = remaining
    }
}

/// How the current (or last) run relates to the session's Target — see
/// docs/search-semantics.md. A continued detached scan stays `.detached`.
public enum RunKind: Sendable {
    case anchor, targetRefine, targetFilter, detached
}

/// What pressing Start Search does with a request, per docs/search-semantics.md.
public enum StartMode: Sendable {
    /// Fresh full-range scan that establishes the Target on conclusion.
    case anchor
    /// Filter the full Target Set, then resume the target's uncovered remainder.
    case targetRefine
    /// Filter the full Target Set only; coverage and set stay untouched.
    case targetFilter
    /// Continue the previous detached scan (filter its results, resume its remainder).
    case continueDetached
    /// Fresh full-range scan that leaves the Target untouched.
    case detached

    /// The lowercase name `seedfinder_decide_start` answers with, matching the
    /// terminology of docs/search-semantics.md.
    init?(engineName: String) {
        switch engineName {
        case "anchor": self = .anchor
        case "target-refine": self = .targetRefine
        case "target-filter": self = .targetFilter
        case "continue-detached": self = .continueDetached
        case "detached": self = .detached
        default: return nil
        }
    }
}

/// The session's anchor: established by the first concluded search (or an
/// import) and reset only by Clear. `seeds` is uncapped and a superset of any
/// related run's display, which is what lets a loosened query bring seeds
/// back. `resumeFrom`/`remaining` are the coverage the target traversal has
/// not completed; imports carry none.
public struct TargetState: Sendable {
    public let request: SearchRequest
    /// Every unique seed the Target Query's traversal has delivered, in
    /// discovery order.
    public var seeds: [String]
    public var resumeFrom: Int64
    public var remaining: Int64
    public init(request: SearchRequest, seeds: [String], resumeFrom: Int64, remaining: Int64) {
        self.request = request; self.seeds = seeds
        self.resumeFrom = resumeFrom; self.remaining = remaining
    }
}

@MainActor @Observable
public final class SearchController {
    public private(set) var state: SearchState?
    /// The listed results, capped at `resultCap` rows: a refine of a grown
    /// Target Set can keep far more survivors than the display holds, and an
    /// uncapped SwiftUI table is what a 5,000-row hang is made of. The run's
    /// full result set lives in `collected`.
    public private(set) var results: [SeedResult] = []
    public private(set) var scannedSeeds: Int64 = 0
    public private(set) var totalSeeds: Int64 = 0
    public private(set) var matchProbability: Double?
    public private(set) var seedsPerSecond: Double = 0
    public private(set) var elapsed: TimeInterval = 0
    public private(set) var errorCode: Int64 = 0
    public private(set) var message: String?
    public private(set) var isRunning = false
    /// The last finished (completed or cancelled) run, ready to be refined.
    public private(set) var baseRun: BaseRun?
    /// The session's Target, if one has been established — see
    /// docs/search-semantics.md. Only `clearResults()` discards it.
    public private(set) var target: TargetState?
    /// How the current (or last) run relates to the Target.
    public private(set) var runKind: RunKind = .anchor
    /// How many previous results survived the last refine; nil after a fresh search.
    public private(set) var refinedKept: Int?
    /// The size of the set the last refine filtered (the "of Y" in "kept X of
    /// Y"): the full Target Set for a target refine or filter, the previous
    /// run's results for a continued detached scan. Nil after a fresh search.
    public private(set) var refinedOf: Int?
    /// Whether the current results were restored from an imported file
    /// rather than produced by a search.
    public private(set) var isImported = false
    /// Imported entries dropped as duplicates or beyond the result cap.
    public private(set) var importedDropped = 0
    /// The query that produced the current results, snapshotted at search
    /// start (or import) so an export never reflects later editor changes.
    public private(set) var exportQuery: SavedQuery?
    public var selectedSeed: String?

    /// How many rows the displayed list holds at most.
    public static let resultCap = 1_024

    private let engine: any SeedFinderEngine
    private var session: (any SeedFinderSearchSession)?
    private var task: Task<Void, Never>?
    /// Every unique seed of the current run — filter survivors plus scanned
    /// finds — in discovery order and uncapped, unlike the displayed
    /// `results`. This is what settles into the Target and what a detached
    /// continuation filters.
    private var collected: [String] = []

    public init(engine: any SeedFinderEngine = ProductionSeedFinderEngine()) { self.engine = engine }
    public var timeToSeed: TimeInterval? {
        guard let matchProbability, seedsPerSecond > 0 else { return nil }
        return 1 / matchProbability / seedsPerSecond
    }
    public var reachedResultCap: Bool { results.count >= Self.resultCap }
    /// The engine completes an unsatisfiable plan before scanning any seed,
    /// which would otherwise be indistinguishable from a malfunction.
    public var isImpossibleQuery: Bool {
        state == .completed && scannedSeeds == 0 && results.isEmpty
    }

    /// Replaces the results with seeds restored from an imported results
    /// file and remembers the query that produced them for later export. The
    /// shared import rule — deduplicate, then cap at the result limit — is the
    /// engine's, applied while decoding the file, so `seeds` is taken as given
    /// and `dropped` is what that step removed. Callers must ensure no search
    /// is running.
    public func loadImported(seeds: [String], dropped: Int = 0, query: SavedQuery) {
        results = seeds.map { SeedResult(seed: $0, matchedRequirements: query.requirements.slotCount) }
        collected = seeds
        importedDropped = dropped
        exportQuery = query
        scannedSeeds = 0; totalSeeds = 0; matchProbability = nil; seedsPerSecond = 0; elapsed = 0
        errorCode = 0; message = nil; state = nil; isImported = true; selectedSeed = nil
        // Imported results carry no traversal state, so the previous
        // search's base run no longer describes the listed seeds.
        baseRun = nil; refinedKept = nil; refinedOf = nil; runKind = .anchor
        // The imported query and seeds replace the session's Target, with no
        // coverage: refines of an import are filter-only.
        let request = try? SearchRequest(
            requirements: query.requirements, maximumDepth: query.maximumDepth,
            requireBlacksmith: query.requireBlacksmith,
            excludeBlacksmithRewards: query.excludeBlacksmithRewards,
            wandmakerQuest: query.wandmakerQuest,
            challenges: query.challenges)
        target = request.map { TargetState(request: $0, seeds: seeds, resumeFrom: 0, remaining: 0) }
    }

    /// Starts `request`, dispatching on its relationship to the session's
    /// Target (docs/search-semantics.md): a continuation of the Target Query
    /// refines the Target Set and resumes its coverage, a request sharing an
    /// item filters the full set, and an unrelated request scans the whole
    /// range without touching the Target — continuing the previous detached
    /// scan when that is sound. There is no user-facing choice: eligibility
    /// alone decides, and only `clearResults()` discards anything.
    ///
    /// `workers` is the device-local thread count (see `WorkerPersistence`),
    /// carried alongside the request rather than in it: it reaches every
    /// native start this run makes — the fresh scan and the resumed remainder
    /// of a refine alike — without ever touching what the query means.
    public func start(_ request: SearchRequest, workers: Int = WorkerPersistence.unset) {
        switch decideStart(request) {
        case .targetRefine: refineTarget(request, workers: workers, resumesScan: true)
        case .targetFilter: refineTarget(request, workers: workers, resumesScan: false)
        case .continueDetached: continueDetached(request, workers: workers)
        case .anchor: freshSearch(request, workers: workers, as: .anchor)
        case .detached: freshSearch(request, workers: workers, as: .detached)
        }
    }

    /// The single gate for what Start Search does. The decision itself is the
    /// engine's (`seedfinder_decide_start`, per docs/search-semantics.md): the
    /// controller only supplies the session state it reads — the Target Query,
    /// whether the Target Set is empty and whether its coverage has seeds
    /// left, and the last concluded run's query when that run was detached.
    public func decideStart(_ request: SearchRequest) -> StartMode {
        // A start while a search is running restarts from scratch (the UI
        // offers Cancel instead); only an idle controller can filter the
        // Target Set or resume a scan soundly.
        guard !isRunning else { return target == nil ? .anchor : .detached }
        return StartDecision.decide(
            candidate: request, target: target?.request,
            targetSetEmpty: target?.seeds.isEmpty ?? true,
            targetHasUncoveredSeeds: (target?.remaining ?? 0) > 0,
            detachedBase: runKind == .detached ? baseRun?.request : nil)
    }

    /// Scans the whole seed space from scratch, replacing the displayed
    /// results. An `.anchor` run establishes the Target when it concludes; a
    /// `.detached` run leaves the existing Target untouched.
    private func freshSearch(_ request: SearchRequest, workers: Int, as kind: RunKind) {
        task?.cancel(); results = []; collected = []; refinedKept = nil; refinedOf = nil; baseRun = nil; resetProgress()
        isImported = false; importedDropped = 0
        runKind = kind
        exportQuery = SavedQuery(
            requirements: request.requirements, maximumDepth: request.maximumDepth,
            requireBlacksmith: request.requireBlacksmith,
            excludeBlacksmithRewards: request.excludeBlacksmithRewards,
            wandmakerQuest: request.wandmakerQuest,
            challenges: request.challenges)
        task = Task { [weak self] in
            guard let self else { return }
            await self.run(request, alreadyShown: []) { engine in
                try await engine.startSearch(request, workers: workers)
            }
        }
    }

    /// Whether starting `request` could continue the last finished run rather
    /// than rescan: nothing running, a base run on record, and the same
    /// requirements or more under a scope it never widens. `decideStart(_:)` consults
    /// this for the detached thread only; a continuation of the Target Query
    /// always refines the Target instead.
    public func canRefine(with request: SearchRequest) -> Bool {
        guard !isRunning, let baseRun else { return false }
        return request.isRefinement(of: baseRun.request)
    }

    /// Whether there is anything for `clearResults()` to discard.
    public var canClearResults: Bool {
        !isRunning && (!results.isEmpty || state != nil || baseRun != nil
            || exportQuery != nil || target != nil)
    }

    /// Empties the results area along with the Target behind it — the Target
    /// Query, the Target Set, and the coverage a later start would otherwise
    /// refine or resume — so the next search anchors a new session from
    /// scratch. This is the only action that discards the Target. Ignored
    /// while a search or filter phase is running.
    public func clearResults() {
        guard !isRunning else { return }
        results = []; collected = []; selectedSeed = nil; exportQuery = nil
        isImported = false; importedDropped = 0
        baseRun = nil; refinedKept = nil; refinedOf = nil
        target = nil; runKind = .anchor
        scannedSeeds = 0; totalSeeds = 0; matchProbability = nil; seedsPerSecond = 0; elapsed = 0
        errorCode = 0; message = nil; state = nil
    }

    /// Refines against the Target: the full Target Set is re-verified against
    /// `request`, the survivors become the displayed results, and — when
    /// `resumesScan` — the scan then continues over the target's uncovered
    /// remainder, deduplicating by seed. The base is always the full Target
    /// Set rather than the last run's survivors, so loosening back toward the
    /// Target Query brings previously dropped seeds back. A cancelled or
    /// failed filter phase leaves the previous results and the Target intact.
    private func refineTarget(_ request: SearchRequest, workers: Int, resumesScan: Bool) {
        guard let target else { return }
        // Re-assert the equal-or-superset invariant here rather than trusting
        // the decision: the soundness of resuming depends on it.
        if resumesScan { guard request.isRefinement(of: target.request) else { return } }
        task?.cancel(); resetProgress()
        let restoreKind = runKind
        runKind = resumesScan ? .targetRefine : .targetFilter
        let baseSeeds = target.seeds
        task = Task { [weak self] in
            guard let self else { return }
            let kept: [String]
            do {
                kept = try await engine.filterSeeds(request, seeds: baseSeeds)
            } catch is CancellationError {
                // The user backed out before the filter finished; the Target
                // was never consumed, so it stays refinable as-is.
                self.state = .cancelled; self.refinedKept = nil; self.refinedOf = nil
                self.runKind = restoreKind; self.isRunning = false
                return
            } catch {
                // The Target is still intact — keep it so the user can retry.
                self.state = .failed; self.message = error.localizedDescription
                self.refinedKept = nil; self.refinedOf = nil
                self.runKind = restoreKind; self.isRunning = false
                return
            }
            self.collected = kept
            self.results = kept.prefix(Self.resultCap).map { SeedResult(seed: $0, matchedRequirements: request.requirements.slotCount) }
            self.refinedKept = kept.count; self.refinedOf = baseSeeds.count
            // From here on the listed results match the refined request, so
            // that is what an export must claim. A cancel or failure above
            // leaves the previous results — and their snapshot — untouched.
            self.exportQuery = SavedQuery(
                requirements: request.requirements, maximumDepth: request.maximumDepth,
                requireBlacksmith: request.requireBlacksmith,
                excludeBlacksmithRewards: request.excludeBlacksmithRewards,
                wandmakerQuest: request.wandmakerQuest,
                challenges: request.challenges)
            // A filter never scans; a refine resumes the target's remainder.
            if resumesScan && target.remaining > 0 {
                await self.run(request, alreadyShown: Set(kept)) { engine in
                    try await engine.startResumedSearch(request, resumeFrom: target.resumeFrom,
                                                        scanLen: target.remaining, workers: workers)
                }
            } else {
                self.state = .completed
                self.baseRun = BaseRun(request: request, resumeFrom: target.resumeFrom, remaining: 0)
                self.isRunning = false
            }
        }
    }

    /// Continues the previous detached scan (the classic pre-Target refine
    /// behaviour, scoped to the detached thread): its displayed results are
    /// re-verified against `request` and the scan resumes over the range it
    /// never covered. The Target is untouched throughout, and `runKind` stays
    /// `.detached`.
    private func continueDetached(_ request: SearchRequest, workers: Int) {
        guard canRefine(with: request), let base = baseRun else { return }
        task?.cancel(); resetProgress()
        // The last run's full result set, not the capped display: finds
        // beyond the display cap belong to the continuation's filter base.
        let previousSeeds = collected
        task = Task { [weak self] in
            guard let self else { return }
            let kept: [String]
            do {
                kept = try await engine.filterSeeds(request, seeds: previousSeeds)
            } catch is CancellationError {
                // The user backed out before the filter finished; the base run
                // was never consumed, so it stays refinable as-is.
                self.state = .cancelled; self.refinedKept = nil; self.refinedOf = nil
                self.isRunning = false
                return
            } catch {
                // The base run is still intact — keep it so the user can retry.
                self.state = .failed; self.message = error.localizedDescription
                self.refinedKept = nil; self.refinedOf = nil; self.isRunning = false
                return
            }
            self.collected = kept
            self.results = kept.prefix(Self.resultCap).map { SeedResult(seed: $0, matchedRequirements: request.requirements.slotCount) }
            self.refinedKept = kept.count; self.refinedOf = previousSeeds.count
            self.exportQuery = SavedQuery(
                requirements: request.requirements, maximumDepth: request.maximumDepth,
                requireBlacksmith: request.requireBlacksmith,
                excludeBlacksmithRewards: request.excludeBlacksmithRewards,
                wandmakerQuest: request.wandmakerQuest,
                challenges: request.challenges)
            if base.remaining > 0 {
                await self.run(request, alreadyShown: Set(kept)) { engine in
                    try await engine.startResumedSearch(request, resumeFrom: base.resumeFrom,
                                                        scanLen: base.remaining, workers: workers)
                }
            } else {
                self.state = .completed
                self.baseRun = BaseRun(request: request, resumeFrom: base.resumeFrom, remaining: 0)
                self.isRunning = false
            }
        }
    }

    public func cancel() {
        guard isRunning else { return }
        if let session {
            Task { await session.cancel() }
        } else {
            // No native session yet (refine's filter phase): cancel the
            // controller task so the awaited filter throws CancellationError.
            task?.cancel()
        }
    }

    private func resetProgress() {
        scannedSeeds = 0; totalSeeds = 0; matchProbability = nil; seedsPerSecond = 0; elapsed = 0
        errorCode = 0; message = nil; state = .running; isRunning = true
    }

    /// Runs one native session's poll loop, appending results not already in
    /// `alreadyShown`. A run that stops cleanly (completed or cancelled)
    /// records its resume hint as the new base run; a failure clears it.
    private func run(_ request: SearchRequest, alreadyShown: Set<String>,
                     startSession: (any SeedFinderEngine) async throws -> any SeedFinderSearchSession) async {
        let searchStart = ContinuousClock.now
        var shown = alreadyShown
        do {
            let session = try await startSession(engine)
            self.session = session
            var previousCount: Int64 = 0
            var previousTime = ContinuousClock.now
            var finalState: SearchState?
            while !Task.isCancelled {
                let batch = try await session.poll(1_024)
                self.append(batch, excluding: &shown)
                let status = try await session.status()
                let now = ContinuousClock.now
                let totalDuration = searchStart.duration(to: now).components
                self.elapsed = Double(totalDuration.seconds) + Double(totalDuration.attoseconds) / 1e18
                let seconds = Double(previousTime.duration(to: now).components.attoseconds) / 1e18
                    + Double(previousTime.duration(to: now).components.seconds)
                if seconds > 0 {
                    let instantRate = Double(max(0, status.scannedSeeds - previousCount)) / seconds
                    self.seedsPerSecond = self.seedsPerSecond == 0 ? instantRate : self.seedsPerSecond * 0.7 + instantRate * 0.3
                }
                previousCount = status.scannedSeeds; previousTime = now
                self.scannedSeeds = status.scannedSeeds; self.totalSeeds = status.totalSeeds
                self.matchProbability = status.matchProbability > 0 ? status.matchProbability : nil
                self.errorCode = status.errorCode; self.state = status.state
                if status.state != .running {
                    let finalBatch = try await session.poll(1_024)
                    self.append(finalBatch, excluding: &shown)
                    finalState = status.state
                    break
                }
                try await Task.sleep(for: .milliseconds(150))
            }
            if finalState == .completed || finalState == .cancelled {
                let hint = try? await session.resumeHint()
                self.baseRun = hint.map {
                    BaseRun(request: request, resumeFrom: $0.position, remaining: $0.remaining)
                }
                self.settleConcludedRun(request: request, hint: hint)
            }
            await session.close()
        } catch is CancellationError {
            await self.session?.cancel(); await self.session?.close()
            self.state = .cancelled; self.baseRun = nil
        } catch {
            await self.session?.close(); self.state = .failed; self.message = error.localizedDescription
            self.baseRun = nil
        }
        self.session = nil; self.isRunning = false
    }

    /// Folds a concluded (completed or cancelled) run into the Target, per
    /// docs/search-semantics.md: an anchor run establishes it from its own
    /// results and coverage, a target refine grows the set with its new finds
    /// and advances the coverage, and a target filter or detached run leaves
    /// it exactly as it was. A failed run never reaches here — its coverage
    /// is unknown.
    private func settleConcludedRun(request: SearchRequest, hint: ResumeHint?) {
        switch runKind {
        case .targetFilter, .detached:
            return
        case .anchor:
            target = TargetState(request: request, seeds: collected,
                                 resumeFrom: hint?.position ?? 0, remaining: hint?.remaining ?? 0)
        case .targetRefine:
            guard var updated = target else {
                target = TargetState(request: request, seeds: collected,
                                     resumeFrom: hint?.position ?? 0, remaining: hint?.remaining ?? 0)
                return
            }
            // The filter's survivors were already members; only new finds
            // from the resumed scan grow the set. The stored set is never
            // capped, and the Target Query stays the original one.
            var seen = Set(updated.seeds)
            updated.seeds += collected.filter { seen.insert($0).inserted }
            if let hint { updated.resumeFrom = hint.position; updated.remaining = hint.remaining }
            target = updated
        }
    }

    private func append(_ batch: [SeedResult], excluding shown: inout Set<String>) {
        guard !batch.isEmpty else { return }
        let fresh = batch.filter { shown.insert($0.seed).inserted }
        guard !fresh.isEmpty else { return }
        collected.append(contentsOf: fresh.map(\.seed))
        // Only the display is capped; everything delivered stays collected
        // for the Target and later refines.
        if results.count < Self.resultCap {
            results.append(contentsOf: fresh.prefix(Self.resultCap - results.count))
        }
    }
}

public enum NumberFormat {
    public static func si(_ value: Double) -> String {
        let units = [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K")]
        for (scale, suffix) in units where value >= scale {
            let scaled = value / scale
            return String(format: scaled >= 100 ? "%.0f%@" : scaled >= 10 ? "%.1f%@" : "%.2f%@", scaled, suffix)
        }
        return String(format: "%.0f", value)
    }
    public static func duration(_ seconds: TimeInterval?) -> String {
        guard let seconds, seconds.isFinite else { return "—" }
        let total = Int(seconds.rounded())
        if total < 60 { return "\(total)s" }
        if total < 3_600 { return "\(total / 60)m \(total % 60)s" }
        return "\(total / 3_600)h \((total % 3_600) / 60)m"
    }
    public static func probabilityPercent(_ probability: Double?) -> String {
        guard let probability, probability > 0 else { return "estimating…" }
        let percent = probability * 100
        var exponent = Int(floor(log10(percent)))
        var mantissa = percent / pow(10, Double(exponent))
        if mantissa >= 9.95 { mantissa = 1; exponent += 1 }
        return String(format: "%.1fx10^%d%%", mantissa, exponent)
    }
    public static func estimateDuration(_ seconds: TimeInterval?) -> String {
        guard let seconds, seconds.isFinite else { return "estimating…" }
        let value: Double
        let unit: String
        if seconds < 60 { value = seconds; unit = "second" }
        else if seconds < 3_600 { value = seconds / 60; unit = "minute" }
        else if seconds < 86_400 { value = seconds / 3_600; unit = "hour" }
        else { value = seconds / 86_400; unit = "day" }
        let suffix = value >= 0.95 && value < 1.05 ? "" : "s"
        return String(format: "%.1f %@%@", value, unit, suffix)
    }
    public static func seedRate(_ value: Double) -> String {
        guard value > 0 else { return "—" }
        if value >= 1e6 { return String(format: "%.1fM", value / 1e6) }
        if value >= 1e3 { return String(format: "%.1fk", value / 1e3) }
        return String(format: "%.0f", value)
    }
}
