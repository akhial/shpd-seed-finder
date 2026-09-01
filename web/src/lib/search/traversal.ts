export interface SeedRange {
  startSeed: number;
  endSeedExclusive: number;
}

/** The number of dungeon seeds: 26^9, one per `XXX-XXX-XXX` code. */
export const TOTAL_SEEDS = 5_429_503_678_976;

/**
 * Distance between the starting points of consecutive searches, mirroring the
 * native session layer's `PRODUCTION_SEARCH_START_STRIDE`: each search claims
 * a fresh traversal start on the seed circle so repeated searches for the same
 * requirements surface different seeds.
 *
 * Approximately one golden-ratio turn of the seed circle. `TOTAL_SEEDS` only
 * has 2 and 13 as prime factors; this odd, non-multiple-of-13 stride is
 * therefore coprime and visits every possible start before repeating. It is
 * the engine's literal rather than re-derived here: deriving it in doubles
 * once landed ~406M seeds away from where the native frontends start.
 */
export const SEARCH_START_STRIDE = 3_355_211_884_971;

export function randomTraversalStart(totalSeeds: number): number {
  return Math.floor(Math.random() * totalSeeds) % totalSeeds;
}

export function advanceTraversalStart(current: number, totalSeeds: number): number {
  return (current + SEARCH_START_STRIDE) % totalSeeds;
}

/** Splits the seed circle, rotated to begin at `traversalStart`, into one
 * contiguous logical range per worker. A worker whose range crosses the end of
 * the numeric seed space receives two physical segments. */
export function partitionRotated(
  totalSeeds: number,
  workerCount: number,
  traversalStart: number,
): SeedRange[][] {
  return Array.from({ length: workerCount }, (_, index) => {
    const logicalStart = Math.floor((totalSeeds * index) / workerCount);
    const length = Math.floor((totalSeeds * (index + 1)) / workerCount) - logicalStart;
    if (length === 0) return [];
    const startSeed = (logicalStart + traversalStart) % totalSeeds;
    if (startSeed + length <= totalSeeds)
      return [{ startSeed, endSeedExclusive: startSeed + length }];
    return [
      { startSeed, endSeedExclusive: totalSeeds },
      { startSeed: 0, endSeedExclusive: startSeed + length - totalSeeds },
    ];
  });
}
