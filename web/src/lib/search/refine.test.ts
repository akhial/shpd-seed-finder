import { describe, expect, it } from 'vitest'
import type { QueryDocument } from '../wasm/types'
import { distributeSegments, isRefinementOf, remainingSegments, segmentsLength } from './refine'
import type { SeedRange } from './traversal'

const base: QueryDocument = {
  requirements: [{ kind: 'ring', upgrade: { at_least: 2 } }],
}
const added: QueryDocument = {
  requirements: [{ kind: 'ring', upgrade: { at_least: 2 } }, { kind: 'weapon', upgrade: 3 }],
}

describe('isRefinementOf', () => {
  it('accepts adding a requirement with identical scope', () => {
    expect(isRefinementOf(added, base)).toBe(true)
  })
  it('accepts reordered requirements and reordered keys', () => {
    const reordered: QueryDocument = {
      requirements: [{ upgrade: 3, kind: 'weapon' }, { upgrade: { at_least: 2 }, kind: 'ring' }],
    }
    expect(isRefinementOf(reordered, base)).toBe(true)
  })
  it('rejects an identical query', () => {
    expect(isRefinementOf(base, base)).toBe(false)
  })
  it('rejects removed or edited requirements', () => {
    expect(isRefinementOf({ requirements: [] }, base)).toBe(false)
    expect(isRefinementOf({ requirements: [{ kind: 'ring', upgrade: { at_least: 3 } }, { kind: 'weapon' }] }, base)).toBe(false)
  })
  it('respects requirement multiplicity', () => {
    const twoRings: QueryDocument = { requirements: [base.requirements[0], base.requirements[0]] }
    expect(isRefinementOf(twoRings, base)).toBe(true)
    expect(isRefinementOf({ requirements: [base.requirements[0], { kind: 'wand' }] }, twoRings)).toBe(false)
  })
  it('rejects any scope change', () => {
    expect(isRefinementOf({ ...added, max_depth: 9 }, base)).toBe(false)
    expect(isRefinementOf({ ...added, fast_mode: true }, base)).toBe(false)
    expect(isRefinementOf({ ...added, require_blacksmith: true }, base)).toBe(false)
    expect(isRefinementOf({ ...added, exclude_blacksmith_rewards: true }, base)).toBe(false)
    expect(isRefinementOf({ ...added, challenges: ['on_diet'] }, base)).toBe(false)
    expect(isRefinementOf({ ...added, challenges: ['on_diet'] }, { ...base, challenges: ['on_diet'] })).toBe(true)
  })
})

describe('remainingSegments', () => {
  it('drops each worker\'s scanned prefix across its ordered segments', () => {
    const segments: SeedRange[][] = [
      [{ startSeed: 90, endSeedExclusive: 100 }, { startSeed: 0, endSeedExclusive: 15 }],
      [{ startSeed: 15, endSeedExclusive: 40 }],
    ]
    expect(remainingSegments(segments, { 0: 12, 1: 25 })).toEqual([{ startSeed: 2, endSeedExclusive: 15 }])
    expect(remainingSegments(segments, { 0: 3 })).toEqual([
      { startSeed: 93, endSeedExclusive: 100 },
      { startSeed: 0, endSeedExclusive: 15 },
      { startSeed: 15, endSeedExclusive: 40 },
    ])
    expect(remainingSegments(segments, { 0: 25, 1: 25 })).toEqual([])
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
