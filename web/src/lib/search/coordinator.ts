import { Store } from '@tanstack/store'
import type { ParsedSeed, QueryDocument, ScoutRequest, ScoutResult } from '../wasm/types'
import { applyProgress, importedResultsState, initialCoordinatorState, markWorkerDone, RESULT_CAP, type CoordinatorState } from './coordinator-state'
import type { SearchWorkerRequest, SearchWorkerResponse } from './protocol'
import { distributeSegments, isRefinementOf, remainingSegments, segmentsLength } from './refine'
import { advanceTraversalStart, partitionRotated, randomTraversalStart, type SeedRange } from './traversal'

export const searchStore = new Store<CoordinatorState>(initialCoordinatorState())

/** How long a cancel waits for every worker's acknowledgment before the
 * coordinator force-finalizes the stop. Workers acknowledge within one chunk
 * normally; the timeout only covers a worker that died or never started. */
const STOP_ACK_TIMEOUT_MS = 2_000

/**
 * Replaces the results list with seeds restored from an imported results
 * file, remembering the query that produced them for later export. Callers
 * must ensure no search is running; stale worker messages are ignored
 * because progress only applies to a running session.
 */
export function loadImportedResults(matches: ParsedSeed[], query: QueryDocument): void {
  searchStore.setState((state) => importedResultsState(state, matches, query))
}

export class SearchCoordinator {
  private workers: Worker[] = []
  private sessionId = 0
  private totalSeeds = 0
  private nextTraversalStart: number | undefined
  private startedWorkers = 0
  /** Present while a refine's filter phase runs: how to restore the previous
   * finished state if the filter is cancelled or fails. */
  private filterRestore: { sessionId: number; state: 'completed' | 'cancelled'; refined?: { kept: number; of: number } } | undefined

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

  start(query: QueryDocument, workerCount = Math.max(1, navigator.hardwareConcurrency ?? 4)): void {
    const workers = this.ensureWorkers(workerCount)
    const sessionId = ++this.sessionId
    const startedAt = performance.now()
    const queryJson = JSON.stringify(query)
    const segments = partitionRotated(this.totalSeeds, workers.length, this.claimTraversalStart())
    this.filterRestore = undefined
    searchStore.setState(() => ({
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
    }))
    this.startedWorkers = workers.length
    workers.forEach((worker, index) => {
      worker.postMessage({ type: 'search:start', queryJson, segments: segments[index], sessionId } satisfies SearchWorkerRequest)
    })
  }

  /**
   * Narrows a finished (completed or cancelled) search without discarding it:
   * the existing matches are re-verified against the combined query, and the
   * scan continues over exactly the seed ranges the previous run never
   * covered. The previous run's matches, coverage, and query stay untouched
   * in the store until the re-verification has succeeded, so a cancelled or
   * failed filter phase falls back to the still-finished previous search.
   */
  refine(query: QueryDocument, workerCount = Math.max(1, navigator.hardwareConcurrency ?? 4)): void {
    const previous = searchStore.state
    if (previous.state !== 'completed' && previous.state !== 'cancelled') return
    // Re-assert the superset invariant here rather than trusting the UI: the
    // soundness of filter-and-resume depends on it.
    try {
      if (!isRefinementOf(query, JSON.parse(previous.queryJson) as QueryDocument)) return
    } catch {
      return
    }
    const sessionId = ++this.sessionId
    const queryJson = JSON.stringify(query)
    const previousMatches = previous.matches
    this.startedWorkers = 0
    this.filterRestore = { sessionId, state: previous.state, refined: previous.refined }
    searchStore.setState((state) => ({
      ...state,
      sessionId,
      state: 'running',
      filtering: true,
      refined: { kept: 0, of: previousMatches.length },
      error: undefined,
    }))
    void filterSeeds(queryJson, previousMatches.map((match) => match.value))
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
    // A filtered subset that already fills the display cap cannot surface
    // anything new; skip the scan but keep the coverage bookkeeping intact so
    // a further refine can continue from the same remainder.
    if (segmentsLength(remainder) === 0 || kept.length >= RESULT_CAP) {
      searchStore.setState((state) => ({
        ...state,
        state: 'completed',
        filtering: false,
        matches: kept,
        capped: kept.length >= RESULT_CAP,
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
      capped: false,
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
      searchStore.setState((state) => ({ ...state, state: 'cancelled', elapsed: performance.now() - state.startedAt }))
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
        searchStore.setState((stuck) => ({ ...stuck, state: 'cancelled' }))
      }
    }, STOP_ACK_TIMEOUT_MS)
  }

  private onMessage(workerId: number, message: SearchWorkerResponse): void {
    if (!('sessionId' in message) || message.sessionId !== searchStore.state.sessionId) return
    if (message.type === 'search:progress') {
      searchStore.setState((state) => applyProgress(state, { ...message, workerId, now: performance.now() }))
      if (searchStore.state.capped) this.workers.forEach((worker) => worker.postMessage({ type: 'search:stop', sessionId: message.sessionId } satisfies SearchWorkerRequest))
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
let filterWorker: Worker | undefined
let nextRequestId = 0
const scoutRequests = new Map<number, { resolve: (value: ScoutResult) => void; reject: (reason: Error) => void }>()
const filterRequests = new Map<number, { resolve: (value: ParsedSeed[]) => void; reject: (reason: Error) => void }>()

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

// Filtering re-generates up to a thousand worlds and can take a while; it
// gets its own worker so interactive scouting never queues behind it.
function getFilterWorker(): Worker {
  if (!filterWorker) {
    filterWorker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
    filterWorker.addEventListener('message', (event: MessageEvent<SearchWorkerResponse>) => {
      const message = event.data
      if (message.type !== 'filter:result' && message.type !== 'filter:error') return
      const pending = filterRequests.get(message.requestId)
      if (!pending) return
      filterRequests.delete(message.requestId)
      if (message.type === 'filter:error') pending.reject(new Error(message.error))
      else pending.resolve(JSON.parse(message.resultJson) as ParsedSeed[])
    })
  }
  return filterWorker
}

export function scoutSeed(request: ScoutRequest): Promise<ScoutResult> {
  const requestId = ++nextRequestId
  return new Promise((resolve, reject) => {
    scoutRequests.set(requestId, { resolve, reject })
    const requestJson = JSON.stringify(request satisfies ScoutRequest & { query?: QueryDocument })
    getScoutWorker().postMessage({ type: 'scout', requestJson, requestId } satisfies SearchWorkerRequest)
  })
}

/** Re-verifies specific seeds against a full query on a dedicated worker,
 * resolving with the matching seeds in input order. */
export function filterSeeds(queryJson: string, seeds: number[]): Promise<ParsedSeed[]> {
  const requestId = ++nextRequestId
  return new Promise((resolve, reject) => {
    filterRequests.set(requestId, { resolve, reject })
    getFilterWorker().postMessage({ type: 'filter', queryJson, seeds, requestId } satisfies SearchWorkerRequest)
  })
}
