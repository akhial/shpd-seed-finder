import { describe, expect, it } from 'vitest'
import { applyProgress, calculateRate, importedResultsState, initialCoordinatorState, mergeMatches } from './coordinator-state'

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
  it('replaces state with imported results in file order and keeps the query snapshot', () => {
    const previous = { ...initialCoordinatorState(100), state: 'completed' as const, sessionId: 5, tested: 42, matches: [match(9)] }
    const imported = importedResultsState(previous, [match(3), match(1)], { requirements: [{ kind: 'wand' }] })
    expect(imported.state).toBe('imported')
    expect(imported.sessionId).toBe(5)
    expect(imported.tested).toBe(0)
    expect(imported.matches.map((item) => item.value)).toEqual([3, 1])
    expect(imported.capped).toBe(false)
    expect(imported.importedDropped).toBe(0)
    expect(imported.query).toEqual({ requirements: [{ kind: 'wand' }] })
  })
  it('deduplicates then caps imported results at 1024, reporting drops', () => {
    const deduped = importedResultsState(initialCoordinatorState(100), [match(3), match(1), match(3)], { requirements: [] })
    expect(deduped.matches.map((item) => item.value)).toEqual([3, 1])
    expect(deduped.importedDropped).toBe(1)
    expect(deduped.capped).toBe(false)

    const imported = importedResultsState(initialCoordinatorState(100), Array.from({ length: 1_030 }, (_, value) => match(value)), { requirements: [] })
    expect(imported.matches).toHaveLength(1_024)
    expect(imported.capped).toBe(true)
    expect(imported.importedDropped).toBe(6)
  })
})
