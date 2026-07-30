import { Store } from '@tanstack/store'
import type { ParsedSeed, QueryDocument, ScoutRequest, ScoutResult } from '../wasm/types'
import { applyProgress, initialCoordinatorState, markWorkerDone, type CoordinatorState } from './coordinator-state'
import type { SearchWorkerRequest, SearchWorkerResponse } from './protocol'
import { distributeSegments, remainingSegments, segmentsLength } from './refine'
import { advanceTraversalStart, partitionRotated, randomTraversalStart, type SeedRange } from './traversal'

export const searchStore = new Store<CoordinatorState>(initialCoordinatorState())

export class SearchCoordinator {
  private workers: Worker[] = []
  private sessionId = 0
  private totalSeeds = 0
  private nextTraversalStart: number | undefined
  private startedWorkers = 0

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
    searchStore.setState(() => ({
      ...initialCoordinatorState(this.totalSeeds),
      sessionId,
      state: 'running',
      workerCount: workers.length,
      startedAt,
      segments,
      queryJson,
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
   * covered. `query` must extend the previous query (see `isRefinementOf`).
   */
  refine(query: QueryDocument, workerCount = Math.max(1, navigator.hardwareConcurrency ?? 4)): void {
    const previous = searchStore.state
    if (previous.state !== 'completed' && previous.state !== 'cancelled') return
    const sessionId = ++this.sessionId
    const startedAt = performance.now()
    const queryJson = JSON.stringify(query)
    const remainder = remainingSegments(previous.segments, previous.workerTested)
    const previousMatches = previous.matches
    this.startedWorkers = 0
    searchStore.setState(() => ({
      ...initialCoordinatorState(segmentsLength(remainder)),
      sessionId,
      state: 'running',
      startedAt,
      queryJson,
      refined: { kept: 0, of: previousMatches.length },
    }))
    void filterSeeds(queryJson, previousMatches.map((match) => match.value))
      .then((kept) => {
        if (searchStore.state.sessionId !== sessionId || searchStore.state.state !== 'running') return
        searchStore.setState((state) => ({
          ...state,
          matches: kept,
          refined: { kept: kept.length, of: previousMatches.length },
        }))
        this.resumeScan(queryJson, remainder, workerCount, sessionId)
      })
      .catch((error: unknown) => {
        if (searchStore.state.sessionId !== sessionId) return
        searchStore.setState((state) => ({
          ...state,
          state: 'cancelled',
          elapsed: performance.now() - state.startedAt,
          error: error instanceof Error ? error.message : String(error),
        }))
      })
  }

  private resumeScan(queryJson: string, remainder: SeedRange[], workerCount: number, sessionId: number): void {
    if (segmentsLength(remainder) === 0) {
      // The previous run already covered the whole traversal; the filtered
      // subset is the complete refined result.
      searchStore.setState((state) => ({ ...state, state: 'completed', elapsed: performance.now() - state.startedAt }))
      return
    }
    const workers = this.ensureWorkers(workerCount)
    const segments = distributeSegments(remainder, workers.length)
    searchStore.setState((state) => ({ ...state, workerCount: workers.length, segments }))
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
    if (current.state !== 'running') return
    if (this.startedWorkers === 0) {
      // Still filtering for a refine: no worker owns this session yet.
      searchStore.setState((state) => ({ ...state, state: 'cancelled', elapsed: performance.now() - state.startedAt }))
      return
    }
    this.workers.forEach((worker) => worker.postMessage({ type: 'search:stop', sessionId: current.sessionId } satisfies SearchWorkerRequest))
    // Workers acknowledge with search:stopped carrying their exact final
    // position; the state turns cancelled once every worker has reported.
    searchStore.setState((state) => ({ ...state, state: 'stopping', elapsed: performance.now() - state.startedAt }))
  }

  private onMessage(workerId: number, message: SearchWorkerResponse): void {
    if (!('sessionId' in message) || message.sessionId !== searchStore.state.sessionId) return
    if (message.type === 'search:progress') {
      searchStore.setState((state) => applyProgress(state, { ...message, workerId, now: performance.now() }))
      if (searchStore.state.capped) this.workers.forEach((worker) => worker.postMessage({ type: 'search:stop', sessionId: message.sessionId } satisfies SearchWorkerRequest))
    }
    if (message.type === 'search:done' || message.type === 'search:stopped') {
      const kind = message.type === 'search:done' ? 'done' : 'stopped'
      searchStore.setState((state) => markWorkerDone(state, { sessionId: message.sessionId, workerId, tested: message.tested, kind, now: performance.now() }))
    }
    if (message.type === 'search:error') searchStore.setState((state) => ({ ...state, state: 'cancelled', error: message.error }))
  }
}

let scoutWorker: Worker | undefined
let nextRequestId = 0
const scoutRequests = new Map<number, { resolve: (value: ScoutResult) => void; reject: (reason: Error) => void }>()
const filterRequests = new Map<number, { resolve: (value: ParsedSeed[]) => void; reject: (reason: Error) => void }>()

function getScoutWorker(): Worker {
  if (!scoutWorker) {
    scoutWorker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
    scoutWorker.addEventListener('message', (event: MessageEvent<SearchWorkerResponse>) => {
      const message = event.data
      if (message.type === 'filter:result' || message.type === 'filter:error') {
        const pending = filterRequests.get(message.requestId)
        if (!pending) return
        filterRequests.delete(message.requestId)
        if (message.type === 'filter:error') pending.reject(new Error(message.error))
        else pending.resolve(JSON.parse(message.resultJson) as ParsedSeed[])
        return
      }
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

/** Re-verifies specific seeds against a full query on the scout worker,
 * resolving with the matching seeds in input order. */
export function filterSeeds(queryJson: string, seeds: number[]): Promise<ParsedSeed[]> {
  const requestId = ++nextRequestId
  return new Promise((resolve, reject) => {
    filterRequests.set(requestId, { resolve, reject })
    getScoutWorker().postMessage({ type: 'filter', queryJson, seeds, requestId } satisfies SearchWorkerRequest)
  })
}
