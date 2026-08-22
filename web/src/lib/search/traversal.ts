import { engineInfo } from '../wasm'

export interface SeedRange { startSeed: number; endSeedExclusive: number }

export function randomTraversalStart(totalSeeds: number): number {
  return Math.floor(Math.random() * totalSeeds) % totalSeeds
}

/**
 * Claims the next traversal start on the seed circle, so repeated searches
 * for the same requirements surface different seeds. The stride is the
 * engine's own — roughly one golden-ratio turn of the full seed space, and
 * coprime with it, so every start is visited before any repeats — shared with
 * the native session layer rather than recomputed here.
 */
export function advanceTraversalStart(current: number, totalSeeds: number): number {
  return (current + engineInfo().search_start_stride) % totalSeeds
}

/** Splits the seed circle, rotated to begin at `traversalStart`, into one
 * contiguous logical range per worker. A worker whose range crosses the end of
 * the numeric seed space receives two physical segments. */
export function partitionRotated(totalSeeds: number, workerCount: number, traversalStart: number): SeedRange[][] {
  return Array.from({ length: workerCount }, (_, index) => {
    const logicalStart = Math.floor((totalSeeds * index) / workerCount)
    const length = Math.floor((totalSeeds * (index + 1)) / workerCount) - logicalStart
    if (length === 0) return []
    const startSeed = (logicalStart + traversalStart) % totalSeeds
    if (startSeed + length <= totalSeeds) return [{ startSeed, endSeedExclusive: startSeed + length }]
    return [
      { startSeed, endSeedExclusive: totalSeeds },
      { startSeed: 0, endSeedExclusive: startSeed + length - totalSeeds },
    ]
  })
}
