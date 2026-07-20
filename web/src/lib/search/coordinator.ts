import { Store } from '@tanstack/store'
import type { QueryDocument, ScoutRequest, ScoutResult } from '../wasm/types'
import { applyProgress, initialCoordinatorState, markWorkerDone, type CoordinatorState } from './coordinator-state'
import type { SearchWorkerRequest, SearchWorkerResponse } from './protocol'

export const searchStore = new Store<CoordinatorState>(initialCoordinatorState())

export class SearchCoordinator {
  private workers: Worker[] = []
  private sessionId = 0
  private totalSeeds = 0

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
    searchStore.setState(() => ({
      ...initialCoordinatorState(this.totalSeeds),
      sessionId,
      state: 'running',
      workerCount: workers.length,
      startedAt,
    }))
    const queryJson = JSON.stringify(query)
    workers.forEach((worker, index) => {
      const startSeed = Math.floor((this.totalSeeds * index) / workers.length)
      const endSeedExclusive = Math.floor((this.totalSeeds * (index + 1)) / workers.length)
      worker.postMessage({ type: 'search:start', queryJson, startSeed, endSeedExclusive, sessionId } satisfies SearchWorkerRequest)
    })
  }

  cancel(): void {
    const current = searchStore.state
    if (current.state !== 'running') return
    this.workers.forEach((worker) => worker.postMessage({ type: 'search:stop', sessionId: current.sessionId } satisfies SearchWorkerRequest))
    searchStore.setState((state) => ({ ...state, state: 'cancelled', elapsed: performance.now() - state.startedAt }))
  }

  private onMessage(workerId: number, message: SearchWorkerResponse): void {
    if (!('sessionId' in message) || message.sessionId !== searchStore.state.sessionId) return
    if (message.type === 'search:progress') {
      searchStore.setState((state) => applyProgress(state, { ...message, workerId, now: performance.now() }))
      if (searchStore.state.capped) this.workers.forEach((worker) => worker.postMessage({ type: 'search:stop', sessionId: message.sessionId } satisfies SearchWorkerRequest))
    }
    if (message.type === 'search:done') searchStore.setState((state) => markWorkerDone(state, message.sessionId, performance.now()))
    if (message.type === 'search:error') searchStore.setState((state) => ({ ...state, state: 'cancelled', error: message.error }))
  }
}

let scoutWorker: Worker | undefined
let nextRequestId = 0
const scoutRequests = new Map<number, { resolve: (value: ScoutResult) => void; reject: (reason: Error) => void }>()

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
