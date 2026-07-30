/**
 * Navigation through the ordered list of search-result seeds while scouting.
 *
 * The scout pane can be reached either from a search result or by typing a
 * seed by hand; navigation is only meaningful in the first case, so every
 * helper returns `undefined` when the current seed is not a search result.
 */

export interface ResultPosition {
  /** 0-based index of the scouted seed in the results list. */
  index: number
  /** Number of seeds in the results list. */
  total: number
}

/** Position of `seed` within the ordered search results, or `undefined` when it is not one of them. */
export function resultPosition(seeds: readonly string[], seed: string | undefined): ResultPosition | undefined {
  if (!seed) return undefined
  const index = seeds.indexOf(seed)
  return index >= 0 ? { index, total: seeds.length } : undefined
}

/**
 * The seed `delta` steps away from `seed` in the results, clamped to the list
 * ends. `undefined` when `seed` is not a search result or the step would not
 * move (already at the first or last result).
 */
export function stepResult(seeds: readonly string[], seed: string | undefined, delta: number): string | undefined {
  const position = resultPosition(seeds, seed)
  if (!position) return undefined
  const target = Math.min(Math.max(position.index + delta, 0), seeds.length - 1)
  return target === position.index ? undefined : seeds[target]
}
