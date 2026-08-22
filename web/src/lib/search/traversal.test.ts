import { describe, expect, it } from 'vitest'
import { SEARCH_START_STRIDE, TOTAL_SEEDS, advanceTraversalStart, partitionRotated, randomTraversalStart } from './traversal'

const coveredSeeds = (segments: ReturnType<typeof partitionRotated>) =>
  segments.flat().reduce((sum, range) => sum + (range.endSeedExclusive - range.startSeed), 0)

describe('rotated traversal partitioning', () => {
  it('covers the full seed space exactly once for any rotation', () => {
    for (const start of [0, 1, TOTAL_SEEDS / 2, TOTAL_SEEDS - 1]) {
      const segments = partitionRotated(TOTAL_SEEDS, 8, start)
      expect(segments).toHaveLength(8)
      expect(coveredSeeds(segments)).toBe(TOTAL_SEEDS)
    }
  })

  it('matches the unrotated partition when the start is zero', () => {
    const segments = partitionRotated(TOTAL_SEEDS, 3, 0)
    expect(segments).toEqual([
      [{ startSeed: 0, endSeedExclusive: Math.floor(TOTAL_SEEDS / 3) }],
      [{ startSeed: Math.floor(TOTAL_SEEDS / 3), endSeedExclusive: Math.floor((TOTAL_SEEDS * 2) / 3) }],
      [{ startSeed: Math.floor((TOTAL_SEEDS * 2) / 3), endSeedExclusive: TOTAL_SEEDS }],
    ])
  })

  it('splits exactly one worker across the wrap point', () => {
    const segments = partitionRotated(100, 4, 90)
    expect(segments).toEqual([
      [{ startSeed: 90, endSeedExclusive: 100 }, { startSeed: 0, endSeedExclusive: 15 }],
      [{ startSeed: 15, endSeedExclusive: 40 }],
      [{ startSeed: 40, endSeedExclusive: 65 }],
      [{ startSeed: 65, endSeedExclusive: 90 }],
    ])
  })

  it('keeps every segment inside the numeric seed space', () => {
    for (const workers of [1, 5, 16]) {
      for (const range of partitionRotated(TOTAL_SEEDS, workers, TOTAL_SEEDS - 7).flat()) {
        expect(range.startSeed).toBeGreaterThanOrEqual(0)
        expect(range.startSeed).toBeLessThan(range.endSeedExclusive)
        expect(range.endSeedExclusive).toBeLessThanOrEqual(TOTAL_SEEDS)
        expect(Number.isSafeInteger(range.startSeed) && Number.isSafeInteger(range.endSeedExclusive)).toBe(true)
      }
    }
  })
})

describe('traversal start rotation', () => {
  it('advances by the engine stride, which is coprime with the seed count', () => {
    // The browser used to re-derive the stride from the golden ratio and
    // landed ~406M seeds away from the engine's; it now carries the literal.
    expect(advanceTraversalStart(0, TOTAL_SEEDS)).toBe(SEARCH_START_STRIDE)
    expect(SEARCH_START_STRIDE % 2).toBe(1)
    expect(SEARCH_START_STRIDE % 13).not.toBe(0)
    const starts = new Set<number>()
    let current = 0
    for (let step = 0; step < 64; step += 1) {
      starts.add(current)
      current = advanceTraversalStart(current, TOTAL_SEEDS)
      expect(Number.isSafeInteger(current)).toBe(true)
    }
    expect(starts.size).toBe(64)
  })

  it('spaces consecutive full-range starts by roughly a golden-ratio turn', () => {
    const first = advanceTraversalStart(0, TOTAL_SEEDS)
    expect(first / TOTAL_SEEDS).toBeGreaterThan(0.6)
    expect(first / TOTAL_SEEDS).toBeLessThan(0.63)
  })

  it('produces random starts inside the seed space', () => {
    for (let sample = 0; sample < 100; sample += 1) {
      const start = randomTraversalStart(TOTAL_SEEDS)
      expect(start).toBeGreaterThanOrEqual(0)
      expect(start).toBeLessThan(TOTAL_SEEDS)
      expect(Number.isSafeInteger(start)).toBe(true)
    }
  })
})
