import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import type { QueryDocument } from '../wasm/types'
import { SearchCoordinator, clearResults, searchStore } from './coordinator'
import { canClearResults, initialCoordinatorState, type CoordinatorState } from './coordinator-state'

/**
 * Enough of the Worker interface for the coordinator, which only constructs
 * workers and posts to them. Nothing ever answers, so every assertion here is
 * about the state the coordinator commits synchronously when a search starts —
 * which is exactly where the fresh/refine decision shows.
 */
class StubWorker {
  static created: StubWorker[] = []
  posted: { type: string }[] = []
  constructor() {
    StubWorker.created.push(this)
  }
  addEventListener(): void {}
  postMessage(message: { type: string }): void {
    this.posted.push(message)
  }
  terminate(): void {}
}

const postedTypes = () => StubWorker.created.flatMap((worker) => worker.posted.map((message) => message.type))

beforeAll(() => vi.stubGlobal('Worker', StubWorker))
afterAll(() => vi.unstubAllGlobals())
beforeEach(() => {
  StubWorker.created = []
})

const TOTAL = 1_000
const baseQuery: QueryDocument = { requirements: [{ kind: 'ring' }] }
const superset: QueryDocument = { requirements: [{ kind: 'ring' }, { kind: 'weapon' }] }
const unrelated: QueryDocument = { requirements: [{ kind: 'wand' }] }

const match = (value: number) => ({ value, code: value.toString().padStart(9, 'A') })

/** Leaves the store as a completed run of `query` that covered the first 400
 * of 1,000 seeds and found two matches — a valid refine base. */
function seedFinishedRun(query: QueryDocument): void {
  searchStore.setState((state) => ({
    ...state,
    sessionId: 1,
    state: 'completed',
    matches: [match(11), match(22)],
    queryJson: JSON.stringify(query),
    query,
    segments: [[{ startSeed: 0, endSeedExclusive: TOTAL }]],
    workerScanned: { 0: [400] },
    workerCount: 1,
    completedWorkers: 1,
  }))
}

describe('implicit refine on start', () => {
  it('continues the previous run when the query only gained requirements', () => {
    const coordinator = new SearchCoordinator(TOTAL)
    seedFinishedRun(baseQuery)
    coordinator.start(superset, 2)

    const state = searchStore.state
    // The filter phase is the refine's first half: previous matches are being
    // re-verified, and they survive in the store until it succeeds.
    expect(state.state).toBe('running')
    expect(state.filtering).toBe(true)
    expect(state.refined).toEqual({ kept: 0, of: 2 })
    expect(state.matches.map((item) => item.value)).toEqual([11, 22])
    expect(postedTypes()).toEqual(['filter'])
  })

  it('starts fresh when the query is not a refinement', () => {
    const coordinator = new SearchCoordinator(TOTAL)
    seedFinishedRun(baseQuery)
    coordinator.start(unrelated, 2)

    const state = searchStore.state
    expect(state.state).toBe('running')
    expect(state.filtering).toBe(false)
    expect(state.refined).toBeUndefined()
    expect(state.matches).toEqual([])
    expect(state.workerScanned).toEqual({})
    // A fresh scan covers the whole seed space, not just the untouched tail.
    expect(state.segments.flat().reduce((sum, range) => sum + (range.endSeedExclusive - range.startSeed), 0)).toBe(TOTAL)
    expect(postedTypes()).toEqual(['search:start', 'search:start'])
  })

  it('starts fresh once the results have been cleared', () => {
    const coordinator = new SearchCoordinator(TOTAL)
    seedFinishedRun(baseQuery)
    expect(canClearResults(searchStore.state)).toBe(true)

    clearResults()
    expect(searchStore.state.state).toBe('idle')
    expect(searchStore.state.matches).toEqual([])
    expect(searchStore.state.queryJson).toBe('')
    // The session counter survives so a late message from the cleared run is
    // still recognised as stale.
    expect(searchStore.state.sessionId).toBe(1)

    // Same query that refined a moment ago; with no base left it rescans.
    coordinator.start(superset, 1)
    expect(searchStore.state.filtering).toBe(false)
    expect(searchStore.state.refined).toBeUndefined()
    expect(searchStore.state.matches).toEqual([])
    expect(postedTypes()).toEqual(['search:start'])
  })
})

describe('clearing results', () => {
  const withState = (state: CoordinatorState['state'], matches = [match(1)]): CoordinatorState => ({
    ...initialCoordinatorState(TOTAL),
    state,
    matches,
  })

  it('is unavailable while a search owns the state, or with nothing to clear', () => {
    expect(canClearResults(withState('running'))).toBe(false)
    expect(canClearResults(withState('stopping'))).toBe(false)
    expect(canClearResults(initialCoordinatorState(TOTAL))).toBe(false)
  })

  it('is available for any finished or loaded result set', () => {
    for (const state of ['completed', 'cancelled', 'failed', 'imported'] as const) {
      expect(canClearResults(withState(state))).toBe(true)
    }
    // A completed run that matched nothing is still worth clearing: it is the
    // base a later start would refine from.
    expect(canClearResults(withState('completed', []))).toBe(true)
  })

  it('leaves a running search untouched', () => {
    new SearchCoordinator(TOTAL)
    searchStore.setState((state) => ({ ...state, state: 'running', matches: [match(7)], queryJson: '{}' }))
    clearResults()
    expect(searchStore.state.state).toBe('running')
    expect(searchStore.state.matches).toHaveLength(1)
  })
})
