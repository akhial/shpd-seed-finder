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

@MainActor @Observable
public final class SearchController {
    public private(set) var state: SearchState?
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
    /// How many previous results survived the last refine; nil after a fresh search.
    public private(set) var refinedKept: Int?
    /// Whether the current results were restored from an imported file
    /// rather than produced by a search.
    public private(set) var isImported = false
    /// Imported entries dropped as duplicates or beyond the result cap.
    public private(set) var importedDropped = 0
    /// The query that produced the current results, snapshotted at search
    /// start (or import) so an export never reflects later editor changes.
    public private(set) var exportQuery: SavedQuery?
    public var selectedSeed: String?

    /// Shared import rule on every platform: results are deduplicated
    /// (keeping first occurrences) and capped at the result limit.
    public static let importCap = 1_024

    private let engine: any SeedFinderEngine
    private var session: (any SeedFinderSearchSession)?
    private var task: Task<Void, Never>?

    public init(engine: any SeedFinderEngine = ProductionSeedFinderEngine()) { self.engine = engine }
    public var timeToSeed: TimeInterval? {
        guard let matchProbability, seedsPerSecond > 0 else { return nil }
        return 1 / matchProbability / seedsPerSecond
    }
    public var reachedResultCap: Bool { results.count >= 1_024 }
    /// The engine completes an unsatisfiable plan before scanning any seed,
    /// which would otherwise be indistinguishable from a malfunction.
    public var isImpossibleQuery: Bool {
        state == .completed && scannedSeeds == 0 && results.isEmpty
    }

    /// Replaces the results with seeds restored from an imported results
    /// file, deduplicating and capping per the shared import rule and
    /// remembering the query that produced them for later export. Callers
    /// must ensure no search is running.
    public func loadImported(seeds: [String], query: SavedQuery) {
        var unique: [String] = []
        var seen = Set<String>()
        for seed in seeds where unique.count < Self.importCap && seen.insert(seed).inserted {
            unique.append(seed)
        }
        results = unique.map { SeedResult(seed: $0, matchedRequirements: query.requirements.count) }
        importedDropped = seeds.count - unique.count
        exportQuery = query
        scannedSeeds = 0; totalSeeds = 0; matchProbability = nil; seedsPerSecond = 0; elapsed = 0
        errorCode = 0; message = nil; state = nil; isImported = true; selectedSeed = nil
        // Imported results carry no traversal state, so the previous
        // search's base run no longer describes the listed seeds.
        baseRun = nil; refinedKept = nil
    }

    public func start(_ request: SearchRequest) {
        task?.cancel(); results = []; refinedKept = nil; resetProgress()
        isImported = false; importedDropped = 0
        exportQuery = SavedQuery(
            requirements: request.requirements, maximumDepth: request.maximumDepth,
            requireBlacksmith: request.requireBlacksmith,
            excludeBlacksmithRewards: request.excludeBlacksmithRewards,
            fastMode: request.fastMode, challenges: request.challenges)
        task = Task { [weak self] in
            guard let self else { return }
            await self.run(request, alreadyShown: []) { engine in
                try await engine.startSearch(request)
            }
        }
    }

    /// Whether `request` can refine the last finished run: nothing running, a
    /// base run on record, and strictly more requirements at identical scope.
    public func canRefine(with request: SearchRequest) -> Bool {
        guard !isRunning, let baseRun else { return false }
        return request.isRefinement(of: baseRun.request)
    }

    /// Narrows the finished base run: re-verifies the seeds already found
    /// against the stricter request, then completes the base run's remaining
    /// seed-space coverage with a resumed scan, deduplicating by seed.
    public func refine(_ request: SearchRequest) {
        guard canRefine(with: request), let base = baseRun else { return }
        task?.cancel(); resetProgress()
        let previousSeeds = results.map(\.seed)
        task = Task { [weak self] in
            guard let self else { return }
            let kept: [String]
            do {
                kept = try await engine.filterSeeds(request, seeds: previousSeeds)
            } catch is CancellationError {
                // The user backed out before the filter finished; the base run
                // was never consumed, so it stays refinable as-is.
                self.state = .cancelled; self.refinedKept = nil; self.isRunning = false
                return
            } catch {
                // The base run is still intact — keep it so the user can retry.
                self.state = .failed; self.message = error.localizedDescription
                self.refinedKept = nil; self.isRunning = false
                return
            }
            self.results = kept.map { SeedResult(seed: $0, matchedRequirements: request.requirements.count) }
            self.refinedKept = kept.count
            // From here on the listed results match the refined request, so
            // that is what an export must claim. A cancel or failure above
            // leaves the base results — and their snapshot — untouched.
            self.exportQuery = SavedQuery(
                requirements: request.requirements, maximumDepth: request.maximumDepth,
                requireBlacksmith: request.requireBlacksmith,
                excludeBlacksmithRewards: request.excludeBlacksmithRewards,
                fastMode: request.fastMode, challenges: request.challenges)
            if base.remaining > 0 {
                await self.run(request, alreadyShown: Set(kept)) { engine in
                    try await engine.startResumedSearch(request, resumeFrom: base.resumeFrom, scanLen: base.remaining)
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
                self.baseRun = (try? await session.resumeHint()).map {
                    BaseRun(request: request, resumeFrom: $0.position, remaining: $0.remaining)
                }
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

    private func append(_ batch: [SeedResult], excluding shown: inout Set<String>) {
        guard !batch.isEmpty else { return }
        results.append(contentsOf: batch.filter { shown.insert($0.seed).inserted })
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
