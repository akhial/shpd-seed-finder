import { describe, expect, it } from 'vitest'
import { applyProgress, calculateRate, initialCoordinatorState, markWorkerDone, mergeMatches, type CoordinatorState } from './coordinator-state'

const match = (value: number) => ({ value, code: value.toString().padStart(9, 'A') })

describe('coordinator aggregation', () => {
  it('merges and sorts batches while dropping duplicate seeds', () => {
    expect(mergeMatches([match(4), match(2)], [match(3), match(2)]).matches.map((item) => item.value)).toEqual([2, 3, 4])
  })
  it('caps at 1024 and reports the cap', () => {
    const merged = mergeMatches([], Array.from({ length: 1025 }, (_, value) => match(value)))
    expect(merged.matches).toHaveLength(1024)
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
  it('ignores stale session progress', () => {
    const state = { ...initialCoordinatorState(100), state: 'running' as const, sessionId: 3, workerCount: 1, startedAt: 1_000 }
    const updated = applyProgress(state, { sessionId: 2, workerId: 0, tested: 10, matches: [match(1)], now: 2_000 })
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
    const updated = applyProgress(stopping, { sessionId: 3, workerId: 0, tested: 40, matches: [match(7)], now: 2_000 })
    expect(updated.workerTested[0]).toBe(40)
    expect(updated.matches.map((item) => item.value)).toEqual([7])
    expect(updated.state).toBe('stopping')
  })

  it('records exact final worker positions and cancels once all workers stop', () => {
    let state: CoordinatorState = { ...running(), state: 'stopping' }
    state = markWorkerDone(state, { sessionId: 3, workerId: 0, tested: 55, kind: 'stopped', now: 2_000 })
    expect(state.state).toBe('stopping')
    state = markWorkerDone(state, { sessionId: 3, workerId: 1, tested: 45, kind: 'done', now: 2_100 })
    expect(state.state).toBe('cancelled')
    expect(state.workerTested).toEqual({ 0: 55, 1: 45 })
    expect(state.tested).toBe(100)
  })

  it('completes a running search when every worker reports done', () => {
    let state: CoordinatorState = running()
    state = markWorkerDone(state, { sessionId: 3, workerId: 0, tested: 50, kind: 'done', now: 2_000 })
    state = markWorkerDone(state, { sessionId: 3, workerId: 1, tested: 50, kind: 'done', now: 2_100 })
    expect(state.state).toBe('completed')
  })

  it('ignores terminal reports from stale sessions', () => {
    const state = running()
    expect(markWorkerDone(state, { sessionId: 2, workerId: 0, tested: 10, kind: 'done', now: 2_000 })).toBe(state)
  })
})
