import { describe, expect, it } from 'vitest'
import { applyProgress, calculateRate, initialCoordinatorState, markWorkerDone, mergeMatches, type CoordinatorState } from './coordinator-state'

const match = (value: number) => ({ value, code: value.toString().padStart(9, 'A') })

describe('coordinator aggregation', () => {
  it('merges and sorts batches while dropping duplicate seeds', () => {
    expect(mergeMatches([match(4), match(2)], [match(3), match(2)]).matches.map((item) => item.value)).toEqual([2, 3, 4])
  })
  it('reports the cap without evicting any delivered match', () => {
    // Every delivered match belongs to a region recorded as scanned; evicting
    // one at the cap would silently lose it for a later refine.
    const merged = mergeMatches([], Array.from({ length: 1_030 }, (_, value) => match(value)))
    expect(merged.matches).toHaveLength(1_030)
    expect(merged.capped).toBe(true)
  })
  it('does not report the cap when duplicates collapse below it', () => {
    const seeds = Array.from({ length: 1_000 }, (_, value) => match(value))
    const merged = mergeMatches(seeds, seeds)
    expect(merged.matches).toHaveLength(1_000)
    expect(merged.capped).toBe(false)
  })
  it('calculates rate over synthetic progress samples', () => {
    expect(calculateRate([{ at: 1_000, tested: 2_000 }, { at: 3_000, tested: 8_000 }])).toBe(3_000)
  })
  it('tracks per-segment scan positions and sums them into tested', () => {
    const state = { ...initialCoordinatorState(100), state: 'running' as const, sessionId: 3, workerCount: 2, startedAt: 1_000 }
    const updated = applyProgress(state, { sessionId: 3, workerId: 0, scanned: [40, 5], matches: [match(1)], now: 2_000 })
    expect(updated.workerScanned[0]).toEqual([40, 5])
    expect(updated.tested).toBe(45)
  })
  it('ignores stale session progress', () => {
    const state = { ...initialCoordinatorState(100), state: 'running' as const, sessionId: 3, workerCount: 1, startedAt: 1_000 }
    const updated = applyProgress(state, { sessionId: 2, workerId: 0, scanned: [10], matches: [match(1)], now: 2_000 })
    expect(updated).toBe(state)
  })
})

describe('stop bookkeeping', () => {
  const running = () => ({
    ...initialCoordinatorState(100),
    state: 'running' as const,
    sessionId: 3,
    workerCount: 2,
    startedAt: 1_000,
  })

  it('accepts the final progress flush while stopping', () => {
    const stopping = { ...running(), state: 'stopping' as const }
    const updated = applyProgress(stopping, { sessionId: 3, workerId: 0, scanned: [40], matches: [match(7)], now: 2_000 })
    expect(updated.workerScanned[0]).toEqual([40])
    expect(updated.matches.map((item) => item.value)).toEqual([7])
    expect(updated.state).toBe('stopping')
  })

  it('records exact final worker positions and cancels once all workers stop', () => {
    let state: CoordinatorState = { ...running(), state: 'stopping' }
    state = markWorkerDone(state, { sessionId: 3, workerId: 0, scanned: [55], kind: 'stopped', now: 2_000 })
    expect(state.state).toBe('stopping')
    state = markWorkerDone(state, { sessionId: 3, workerId: 1, scanned: [30, 15], kind: 'done', now: 2_100 })
    expect(state.state).toBe('cancelled')
    expect(state.workerScanned).toEqual({ 0: [55], 1: [30, 15] })
    expect(state.tested).toBe(100)
  })

  it('completes a running search when every worker reports done', () => {
    let state: CoordinatorState = running()
    state = markWorkerDone(state, { sessionId: 3, workerId: 0, scanned: [50], kind: 'done', now: 2_000 })
    state = markWorkerDone(state, { sessionId: 3, workerId: 1, scanned: [50], kind: 'done', now: 2_100 })
    expect(state.state).toBe('completed')
  })

  it('ignores terminal reports from stale sessions', () => {
    const state = running()
    expect(markWorkerDone(state, { sessionId: 2, workerId: 0, scanned: [10], kind: 'done', now: 2_000 })).toBe(state)
  })
})
