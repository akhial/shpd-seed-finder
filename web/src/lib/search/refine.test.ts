import { describe, expect, it } from 'vitest'
import type { QueryDocument } from '../wasm/types'
import { distributeSegments, isContinuationOf, remainingSegments, segmentsLength, shouldRefine } from './refine'
import type { SearchStatus } from './coordinator-state'
import type { SeedRange } from './traversal'

const base: QueryDocument = {
  requirements: [{ kind: 'ring', upgrade: { at_least: 2 } }],
}
const added: QueryDocument = {
  requirements: [{ kind: 'ring', upgrade: { at_least: 2 } }, { kind: 'weapon', upgrade: 3 }],
}

describe('isContinuationOf', () => {
  it('accepts adding a requirement with identical scope', () => {
    expect(isContinuationOf(added, base)).toBe(true)
  })
  it('accepts reordered requirements and reordered keys', () => {
    const reordered: QueryDocument = {
      requirements: [{ upgrade: 3, kind: 'weapon' }, { upgrade: { at_least: 2 }, kind: 'ring' }],
    }
    expect(isContinuationOf(reordered, base)).toBe(true)
  })
  it('accepts an unchanged query, which continues the run rather than restarting it', () => {
    expect(isContinuationOf(base, base)).toBe(true)
    // Equality is judged on content, not on key or requirement order.
    expect(isContinuationOf({ requirements: [{ upgrade: { at_least: 2 }, kind: 'ring' }] }, base)).toBe(true)
    expect(isContinuationOf(added, added)).toBe(true)
  })
  it('rejects removed or edited requirements', () => {
    expect(isContinuationOf({ requirements: [] }, base)).toBe(false)
    expect(isContinuationOf({ requirements: [{ kind: 'ring', upgrade: { at_least: 3 } }, { kind: 'weapon' }] }, base)).toBe(false)
    expect(isContinuationOf(base, added)).toBe(false)
  })
  it('respects requirement multiplicity', () => {
    const twoRings: QueryDocument = { requirements: [base.requirements[0], base.requirements[0]] }
    expect(isContinuationOf(twoRings, base)).toBe(true)
    expect(isContinuationOf(twoRings, twoRings)).toBe(true)
    expect(isContinuationOf(base, twoRings)).toBe(false)
    expect(isContinuationOf({ requirements: [base.requirements[0], { kind: 'wand' }] }, twoRings)).toBe(false)
  })
  it('rejects any scope change', () => {
    expect(isContinuationOf({ ...added, max_depth: 9 }, base)).toBe(false)
    expect(isContinuationOf({ ...added, fast_mode: true }, base)).toBe(false)
    expect(isContinuationOf({ ...added, require_blacksmith: true }, base)).toBe(false)
    expect(isContinuationOf({ ...added, exclude_blacksmith_rewards: true }, base)).toBe(false)
    expect(isContinuationOf({ ...added, challenges: ['on_diet'] }, base)).toBe(false)
    expect(isContinuationOf({ ...added, challenges: ['on_diet'] }, { ...base, challenges: ['on_diet'] })).toBe(true)
    // Even an otherwise unchanged query restarts when the scope moves.
    expect(isContinuationOf({ ...base, max_depth: 9 }, base)).toBe(false)
  })
})

describe('shouldRefine', () => {
  const finished = (state: SearchStatus) => ({ state, queryJson: JSON.stringify(base) })

  it('continues a completed or cancelled run whose query gained requirements', () => {
    expect(shouldRefine(finished('completed'), added)).toBe(true)
    expect(shouldRefine(finished('cancelled'), added)).toBe(true)
  })

  it('continues a completed or cancelled run whose query is unchanged', () => {
    // Pressing Start again after a cancel resumes the same session; results
    // survive until the user clears them.
    expect(shouldRefine(finished('completed'), base)).toBe(true)
    expect(shouldRefine(finished('cancelled'), base)).toBe(true)
  })

  it('rescans when the run never established its coverage', () => {
    // Imported results carry no scanned region, a failed run's is unknown,
    // and a running one is still moving.
    for (const state of ['idle', 'running', 'stopping', 'failed', 'imported'] as SearchStatus[]) {
      expect(shouldRefine(finished(state), added)).toBe(false)
    }
  })

  it('rescans when the query no longer covers the finished one', () => {
    expect(shouldRefine({ state: 'completed', queryJson: JSON.stringify(added) }, base)).toBe(false)
    expect(shouldRefine(finished('completed'), { requirements: [{ kind: 'wand' }] })).toBe(false)
    expect(shouldRefine(finished('completed'), { ...added, max_depth: 9 })).toBe(false)
    expect(shouldRefine(finished('completed'), { ...base, max_depth: 9 })).toBe(false)
  })

  it('rescans when there is no readable base query', () => {
    expect(shouldRefine({ state: 'completed', queryJson: '' }, added)).toBe(false)
    expect(shouldRefine({ state: 'completed', queryJson: '{not json' }, added)).toBe(false)
  })
})

describe('remainingSegments', () => {
  it('drops each segment\'s own scanned prefix', () => {
    const segments: SeedRange[][] = [
      [{ startSeed: 90, endSeedExclusive: 100 }, { startSeed: 0, endSeedExclusive: 15 }],
      [{ startSeed: 15, endSeedExclusive: 40 }],
    ]
    expect(remainingSegments(segments, { 0: [10, 2], 1: [25] })).toEqual([{ startSeed: 2, endSeedExclusive: 15 }])
    expect(remainingSegments(segments, { 0: [3] })).toEqual([
      { startSeed: 93, endSeedExclusive: 100 },
      { startSeed: 0, endSeedExclusive: 15 },
      { startSeed: 15, endSeedExclusive: 40 },
    ])
    expect(remainingSegments(segments, { 0: [10, 15], 1: [25] })).toEqual([])
  })

  it('keeps the tail of a segment abandoned at the session result cap', () => {
    // The first segment stopped early (its cooperative session hit the
    // per-session accept cap) while the second segment still completed.
    // A cumulative count would wrongly skip the first segment's tail.
    const segments: SeedRange[][] = [
      [{ startSeed: 900, endSeedExclusive: 1_000 }, { startSeed: 0, endSeedExclusive: 60 }],
    ]
    expect(remainingSegments(segments, { 0: [40, 60] })).toEqual([
      { startSeed: 940, endSeedExclusive: 1_000 },
    ])
  })
})

describe('distributeSegments', () => {
  it('splits ranges into near-equal contiguous shares covering every seed once', () => {
    const ranges: SeedRange[] = [
      { startSeed: 10, endSeedExclusive: 25 },
      { startSeed: 40, endSeedExclusive: 47 },
    ]
    const shares = distributeSegments(ranges, 3)
    expect(shares).toHaveLength(3)
    const flattened = shares.flat().flatMap((range) => Array.from({ length: range.endSeedExclusive - range.startSeed }, (_, offset) => range.startSeed + offset))
    const expected = [...Array.from({ length: 15 }, (_, offset) => 10 + offset), ...Array.from({ length: 7 }, (_, offset) => 40 + offset)]
    expect(flattened).toEqual(expected)
    for (const share of shares) {
      expect(Math.abs(segmentsLength(share) - 22 / 3)).toBeLessThanOrEqual(1)
    }
  })
  it('handles more workers than seeds and empty input', () => {
    const shares = distributeSegments([{ startSeed: 5, endSeedExclusive: 7 }], 4)
    expect(shares).toHaveLength(4)
    expect(segmentsLength(shares.flat())).toBe(2)
    expect(distributeSegments([], 3).every((share) => share.length === 0)).toBe(true)
  })
})
