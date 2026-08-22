import { Store } from '@tanstack/store'
import type { ParsedSeed, QueryDocument, ScoutRequest, ScoutResult } from '../wasm/types'
import { applyProgress, canClearResults, importedResultsState, initialCoordinatorState, markWorkerDone, resultCap, runSaturated, settleRun, type CoordinatorState, type RunKind, type SearchStatus } from './coordinator-state'
import type { SearchWorkerRequest, SearchWorkerResponse } from './protocol'
import { decideStart, distributeSegments, isContinuationOf, remainingSegments, segmentsLength } from './refine'
import { advanceTraversalStart, partitionRotated, randomTraversalStart, type SeedRange } from './traversal'

export const searchStore = new Store<CoordinatorState>(initialCoordinatorState())

/** How long a cancel waits for every worker's acknowledgment before the
 * coordinator force-finalizes the stop. Workers acknowledge within one chunk
 * normally; the timeout only covers a worker that died or never started. */
const STOP_ACK_TIMEOUT_MS = 2_000

/** Smallest filter slice worth its own worker: re-verifying a seed means
 * generating its world in full, but below this many seeds the fan-out
 * overhead outweighs the generation time saved. */
const MIN_FILTER_CHUNK = 16

/**
 * Replaces the results list with seeds restored from an imported results
 * file, remembering the query that produced them for later export. Callers
 * must ensure no search is running; stale worker messages are ignored
 * because progress only applies to a running session.
 */
export function loadImportedResults(matches: ParsedSeed[], query: QueryDocument, dropped = 0): void {
  searchStore.setState((state) => importedResultsState(state, matches, query, dropped))
}

/**
 * Empties the results list along with the Target behind it — the Target
 * Query, the Target Set, and the scanned coverage a later start would
 * otherwise refine or resume — so the next search anchors a new session
 * from scratch. This is the only action that discards the Target. Ignored
 * while a search is running or stopping, which owns that state. The session
 * counter is preserved so late messages from the previous session stay
 * stale.
 */
export function clearResults(): void {
  if (!canClearResults(searchStore.state)) return
  searchStore.setState((state) => ({ ...initialCoordinatorState(state.total), sessionId: state.sessionId }))
}

export class SearchCoordinator {
  private workers: Worker[] = []
  private sessionId = 0
  private totalSeeds = 0
  private nextTraversalStart: number | undefined
  private startedWorkers = 0
  /** Present while a refine's filter phase runs: how to restore the previous
   * finished state if the filter is cancelled or fails. */
  private filterRestore: { sessionId: number; state: SearchStatus; runKind: RunKind; refined?: { kept: number; of: number } } | undefined

  constructor(totalSeeds: number) {
    this.totalSeeds = totalSeeds
    searchStore.setState(() => initialCoordinatorState(totalSeeds))
  }

  private ensureWorkers(count: number): Worker[] {
    const target = Math.max(1, Math.floor(count) || 1)
    while (this.workers.length < target) {
      const workerId = this.workers.length
      const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
      worker.addEventListener('message', (event: MessageEvent<SearchWorkerResponse>) => this.onMessage(workerId, event.data))
      this.workers.push(worker)
    }
    return this.workers.slice(0, target)
  }

  /**
   * Runs `query`, dispatching on its relationship to the session's Target
   * (docs/search-semantics.md): a continuation refines the Target Set and
   * resumes its coverage, a query sharing an item filters the full set, and
   * an unrelated query scans the whole range without touching the Target —
   * continuing the previous detached scan when that is sound. None of this
   * is a user decision; only the Clear button discards anything.
   */
  start(query: QueryDocument, workerCount = Math.max(1, navigator.hardwareConcurrency ?? 4)): void {
    const state = searchStore.state
    if (state.state === 'running' || state.state === 'stopping') return
    const mode = decideStart(state, query)
    if (mode === 'target-refine' || mode === 'target-filter') this.refineTarget(query, mode, workerCount)
    else if (mode === 'continue-detached') this.continueDetached(query, workerCount)
    else this.startFresh(query, workerCount, mode)
  }

  /** Scans the whole seed space from a fresh traversal start, replacing the
   * displayed results. An 'anchor' run establishes the Target when it
   * concludes; a 'detached' run leaves the existing Target untouched. */
  private startFresh(query: QueryDocument, workerCount: number, runKind: 'anchor' | 'detached'): void {
    const workers = this.ensureWorkers(workerCount)
    const sessionId = ++this.sessionId
    const startedAt = performance.now()
    const queryJson = JSON.stringify(query)
    const segments = partitionRotated(this.totalSeeds, workers.length, this.claimTraversalStart())
    this.filterRestore = undefined
    searchStore.setState((state) => ({
      ...initialCoordinatorState(this.totalSeeds),
      sessionId,
      state: 'running',
      workerCount: workers.length,
      startedAt,
      segments,
      queryJson,
      // Snapshot the query so an export always describes the query that
      // actually produced the listed results, even after later edits.
      query,
      // The Target survives a detached scan untouched; an anchor run
      // replaces it (with its own results) only when it concludes.
      target: state.target,
      runKind,
    }))
    this.startedWorkers = workers.length
    workers.forEach((worker, index) => {
      worker.postMessage({ type: 'search:start', queryJson, segments: segments[index], sessionId } satisfies SearchWorkerRequest)
    })
  }

  /**
   * Refines against the Target: the full Target Set is re-verified on a
   * worker, the survivors become the displayed results, and — in
   * 'target-refine' mode only — the scan then resumes over the target's
   * uncovered remainder. The base is always the full Target Set rather than
   * the last run's survivors, so loosening back toward the Target Query
   * brings previously dropped seeds back. Nothing in the store is touched
   * until the re-verification succeeds; a cancelled or failed filter phase
   * falls back to the previous finished state.
   */
  private refineTarget(query: QueryDocument, mode: 'target-refine' | 'target-filter', workerCount: number): void {
    const previous = searchStore.state
    const target = previous.target
    if (!target) return
    // Re-assert the equal-or-superset invariant here rather than trusting
    // the decision helper: the soundness of resuming depends on it.
    if (mode === 'target-refine' && !isContinuationOf(query, target.query)) return
    const sessionId = ++this.sessionId
    const queryJson = JSON.stringify(query)
    this.startedWorkers = 0
    this.filterRestore = { sessionId, state: previous.state, runKind: previous.runKind, refined: previous.refined }
    searchStore.setState((state) => ({
      ...state,
      sessionId,
      state: 'running',
      filtering: true,
      refined: { kept: 0, of: target.matches.length },
      runKind: mode,
      error: undefined,
    }))
    void this.filterSeeds(queryJson, target.matches.map((match) => match.value), workerCount)
      .then((kept) => {
        if (this.filterRestore?.sessionId !== sessionId) return
        this.filterRestore = undefined
        // A filter never scans; a refine resumes the target's remainder.
        const remainder = mode === 'target-refine' ? target.remainder : []
        this.beginResumedScan(query, remainder, kept, target.matches.length, workerCount, sessionId)
      })
      .catch((error: unknown) => {
        this.restoreAfterFilter(sessionId, error instanceof Error ? error.message : String(error))
      })
  }

  /**
   * Continues the previous detached scan (the pre-Target refine behaviour,
   * scoped to the detached thread): its displayed matches are re-verified
   * and the scan resumes over the ranges it never covered. The Target is
   * untouched throughout.
   */
  private continueDetached(query: QueryDocument, workerCount: number): void {
    const previous = searchStore.state
    if (previous.state !== 'completed' && previous.state !== 'cancelled') return
    try {
      if (!isContinuationOf(query, JSON.parse(previous.queryJson) as QueryDocument)) return
    } catch {
      return
    }
    const sessionId = ++this.sessionId
    const queryJson = JSON.stringify(query)
    const previousMatches = previous.matches
    this.startedWorkers = 0
    this.filterRestore = { sessionId, state: previous.state, runKind: previous.runKind, refined: previous.refined }
    searchStore.setState((state) => ({
      ...state,
      sessionId,
      state: 'running',
      filtering: true,
      refined: { kept: 0, of: previousMatches.length },
      runKind: 'detached',
      error: undefined,
    }))
    void this.filterSeeds(queryJson, previousMatches.map((match) => match.value), workerCount)
      .then((kept) => {
        if (this.filterRestore?.sessionId !== sessionId) return
        this.filterRestore = undefined
        const remainder = remainingSegments(searchStore.state.segments, searchStore.state.workerScanned)
        this.beginResumedScan(query, remainder, kept, previousMatches.length, workerCount, sessionId)
      })
      .catch((error: unknown) => {
        this.restoreAfterFilter(sessionId, error instanceof Error ? error.message : String(error))
      })
  }

  /**
   * Re-verifies seeds against a query across the scan worker pool, resolving
   * with the matching seeds in input order. The pool sits idle during a
   * refine's filter phase, so the set fans out over the same workers the
   * resumed scan will use — funnelling it through one worker made verifying
   * a large Target Set take longer than the scan that follows it. Chunks are
   * contiguous slices, so concatenating the results preserves input order.
   */
  private filterSeeds(queryJson: string, seeds: number[], workerCount: number): Promise<ParsedSeed[]> {
    if (seeds.length === 0) return Promise.resolve([])
    const poolSize = Math.max(1, Math.floor(workerCount) || 1)
    const chunkCount = Math.min(poolSize, Math.max(1, Math.ceil(seeds.length / MIN_FILTER_CHUNK)))
    const workers = this.ensureWorkers(chunkCount)
    const chunkSize = Math.ceil(seeds.length / chunkCount)
    const requests: Promise<ParsedSeed[]>[] = []
    for (let index = 0; index < chunkCount; index += 1) {
      const chunk = seeds.slice(index * chunkSize, (index + 1) * chunkSize)
      requests.push(new Promise((resolve, reject) => {
        const requestId = ++nextRequestId
        filterRequests.set(requestId, { resolve, reject })
        workers[index].postMessage({ type: 'filter', queryJson, seeds: chunk, requestId } satisfies SearchWorkerRequest)
      }))
    }
    return Promise.all(requests).then((parts) => parts.flat())
  }

  /** Puts the store back into the previous finished state after a cancelled
   * or failed filter phase. Matches and coverage were never touched. */
  private restoreAfterFilter(sessionId: number, error?: string): void {
    const restore = this.filterRestore
    if (restore?.sessionId !== sessionId) return
    this.filterRestore = undefined
    searchStore.setState((state) => ({
      ...state,
      state: restore.state,
      filtering: false,
      refined: restore.refined,
      runKind: restore.runKind,
      error,
    }))
  }

  private beginResumedScan(
    query: QueryDocument,
    remainder: SeedRange[],
    kept: ParsedSeed[],
    previousCount: number,
    workerCount: number,
    sessionId: number,
  ): void {
    const queryJson = JSON.stringify(query)
    const startedAt = performance.now()
    const refined = { kept: kept.length, of: previousCount }
    // Nothing left to scan: a target filter arrives here with an empty
    // remainder by construction, and a fully covered refine has no range to
    // resume. A cap-filling survivor set is deliberately NOT a reason to
    // skip — the scan still grows the collection past the display cap.
    if (segmentsLength(remainder) === 0) {
      searchStore.setState((state) => settleRun({
        ...state,
        state: 'completed',
        filtering: false,
        matches: kept,
        capped: kept.length >= resultCap(),
        queryJson,
        // From here on the listed matches belong to the refined query, so
        // that is what an export must claim. A cancelled or failed filter
        // phase leaves the previous matches — and their snapshot — untouched.
        query,
        refined,
        startedAt,
        elapsed: 0,
        tested: 0,
        total: segmentsLength(remainder),
        rate: 0,
        rateSamples: [],
        completedWorkers: 0,
        workerCount: 0,
        segments: [remainder],
        workerScanned: {},
      }))
      return
    }
    const workers = this.ensureWorkers(workerCount)
    const segments = distributeSegments(remainder, workers.length)
    searchStore.setState((state) => ({
      ...state,
      state: 'running',
      filtering: false,
      matches: kept,
      capped: kept.length >= resultCap(),
      sessionBaseline: kept.length,
      queryJson,
      query,
      refined,
      startedAt,
      elapsed: 0,
      tested: 0,
      total: segmentsLength(remainder),
      rate: 0,
      rateSamples: [],
      completedWorkers: 0,
      workerCount: workers.length,
      segments,
      workerScanned: {},
    }))
    this.startedWorkers = workers.length
    workers.forEach((worker, index) => {
      worker.postMessage({ type: 'search:start', queryJson, segments: segments[index], sessionId } satisfies SearchWorkerRequest)
    })
  }

  // Like the native session layer, each search starts one golden-ratio turn
  // beyond the previous one so identical queries surface different seeds.
  private claimTraversalStart(): number {
    const current = this.nextTraversalStart ?? randomTraversalStart(this.totalSeeds)
    this.nextTraversalStart = advanceTraversalStart(current, this.totalSeeds)
    return current
  }

  cancel(): void {
    const current = searchStore.state
    if (current.state !== 'running' && current.state !== 'stopping') return
    if (current.filtering) {
      // Still re-verifying for a refine: no worker owns this session yet, so
      // fall straight back to the previous finished results.
      this.restoreAfterFilter(current.sessionId)
      return
    }
    if (this.startedWorkers === 0) {
      searchStore.setState((state) => settleRun({ ...state, state: 'cancelled', elapsed: performance.now() - state.startedAt }))
      return
    }
    const sessionId = current.sessionId
    this.workers.forEach((worker) => worker.postMessage({ type: 'search:stop', sessionId } satisfies SearchWorkerRequest))
    // Workers acknowledge with search:stopped carrying their exact final
    // positions; the state turns cancelled once every worker has reported.
    // Cancel stays available while stopping (it re-broadcasts), and a
    // watchdog finalizes the stop even if an acknowledgment never arrives.
    if (current.state === 'running') {
      searchStore.setState((state) => ({ ...state, state: 'stopping', elapsed: performance.now() - state.startedAt }))
    }
    window.setTimeout(() => {
      const state = searchStore.state
      if (state.sessionId === sessionId && state.state === 'stopping') {
        searchStore.setState((stuck) => settleRun({ ...stuck, state: 'cancelled' }))
      }
    }, STOP_ACK_TIMEOUT_MS)
  }

  private onMessage(workerId: number, message: SearchWorkerResponse): void {
    // Filter replies belong to a fan-out request, not a scan session.
    if (message.type === 'filter:result' || message.type === 'filter:error') {
      const pending = filterRequests.get(message.requestId)
      if (!pending) return
      filterRequests.delete(message.requestId)
      if (message.type === 'filter:error') pending.reject(new Error(message.error))
      else pending.resolve(JSON.parse(message.resultJson) as ParsedSeed[])
      return
    }
    if (!('sessionId' in message) || message.sessionId !== searchStore.state.sessionId) return
    if (message.type === 'search:progress') {
      searchStore.setState((state) => applyProgress(state, { ...message, workerId, now: performance.now() }))
      if (runSaturated(searchStore.state)) this.workers.forEach((worker) => worker.postMessage({ type: 'search:stop', sessionId: message.sessionId } satisfies SearchWorkerRequest))
    }
    if (message.type === 'search:done' || message.type === 'search:stopped') {
      const kind = message.type === 'search:done' ? 'done' : 'stopped'
      searchStore.setState((state) => markWorkerDone(state, { sessionId: message.sessionId, workerId, scanned: message.scanned, kind, now: performance.now() }))
    }
    if (message.type === 'search:error') {
      // A failed run cannot become a refine base: its coverage is unknown.
      this.workers.forEach((worker) => worker.postMessage({ type: 'search:stop', sessionId: message.sessionId } satisfies SearchWorkerRequest))
      searchStore.setState((state) => ({
        ...state,
        state: 'failed',
        error: message.error,
        elapsed: performance.now() - state.startedAt,
      }))
    }
  }
}

let scoutWorker: Worker | undefined
let nextRequestId = 0
const scoutRequests = new Map<number, { resolve: (value: ScoutResult) => void; reject: (reason: Error) => void }>()
const filterRequests = new Map<number, { resolve: (value: ParsedSeed[]) => void; reject: (reason: Error) => void }>()

// Scouting keeps a worker of its own so it stays interactive: filter fan-outs
// and scans own the coordinator's pool, sometimes for seconds at a time.
function getScoutWorker(): Worker {
  if (!scoutWorker) {
    scoutWorker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
    scoutWorker.addEventListener('message', (event: MessageEvent<SearchWorkerResponse>) => {
      const message = event.data
      if (message.type !== 'scout:result' && message.type !== 'scout:error') return
      const pending = scoutRequests.get(message.requestId)
      if (!pending) return
      scoutRequests.delete(message.requestId)
      if (message.type === 'scout:error') pending.reject(new Error(message.error))
      else pending.resolve(JSON.parse(message.resultJson) as ScoutResult)
    })
  }
  return scoutWorker
}

export function scoutSeed(request: ScoutRequest): Promise<ScoutResult> {
  const requestId = ++nextRequestId
  return new Promise((resolve, reject) => {
    scoutRequests.set(requestId, { resolve, reject })
    const requestJson = JSON.stringify(request satisfies ScoutRequest & { query?: QueryDocument })
    getScoutWorker().postMessage({ type: 'scout', requestJson, requestId } satisfies SearchWorkerRequest)
  })
}
