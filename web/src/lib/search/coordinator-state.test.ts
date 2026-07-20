import { describe, expect, it } from 'vitest'
import { applyProgress, calculateRate, initialCoordinatorState, mergeMatches } from './coordinator-state'

const match = (value: number) => ({ value, code: value.toString().padStart(9, 'A') })

describe('coordinator aggregation', () => {
  it('merges and sorts batches while retaining duplicates', () => {
    expect(mergeMatches([match(4), match(2)], [match(3), match(2)]).matches.map((item) => item.value)).toEqual([2, 2, 3, 4])
  })
  it('caps at 1024 and reports the cap', () => {
    const merged = mergeMatches([], Array.from({ length: 1025 }, (_, value) => match(value)))
    expect(merged.matches).toHaveLength(1024)
    expect(merged.capped).toBe(true)
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
