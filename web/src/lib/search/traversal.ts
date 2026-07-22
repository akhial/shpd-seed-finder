export interface SeedRange { startSeed: number; endSeedExclusive: number }

// Mirrors the native session layer: each search claims a fresh traversal start
// on the seed circle so repeated searches for the same requirements surface
// different seeds. Starts advance by roughly one golden-ratio turn, which
// spaces consecutive searches as far apart as possible before repeating.
const GOLDEN_RATIO_CONJUGATE = 0.618_033_988_749_894_9

const gcd = (left: number, right: number): number => (right === 0 ? left : gcd(right, left % right))

export function goldenStride(totalSeeds: number): number {
  if (totalSeeds <= 1) return 1
  let stride = Math.max(1, Math.round(totalSeeds * GOLDEN_RATIO_CONJUGATE))
  while (gcd(stride, totalSeeds) !== 1) stride += 1
  return stride % totalSeeds
}

export function randomTraversalStart(totalSeeds: number): number {
  return Math.floor(Math.random() * totalSeeds) % totalSeeds
}

export function advanceTraversalStart(current: number, totalSeeds: number): number {
  return (current + goldenStride(totalSeeds)) % totalSeeds
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
