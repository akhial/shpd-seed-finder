import type { QueryDocument, RequirementDocument } from '../wasm/types'
import type { CoordinatorState, SearchStatus } from './coordinator-state'
import type { SeedRange } from './traversal'

/** Canonical fingerprint for one requirement, independent of key order. */
function requirementSignature(requirement: RequirementDocument): string {
  return JSON.stringify(
    Object.fromEntries(Object.entries(requirement).sort(([left], [right]) => left.localeCompare(right))),
  )
}

function requirementCounts(requirements: RequirementDocument[]): Map<string, number> {
  const counts = new Map<string, number>()
  for (const requirement of requirements) {
    const signature = requirementSignature(requirement)
    counts.set(signature, (counts.get(signature) ?? 0) + 1)
  }
  return counts
}

/**
 * Whether a run of `candidate` can continue one of `base`: identical scope
 * (depth, challenges, and flags) and a requirement multiset equal to or a
 * superset of the base one. Only then are the base run's matches guaranteed
 * to contain every candidate match in the already-scanned region, which is
 * what makes filter-and-resume sound.
 *
 * Equality is deliberately included, not an edge case: an unchanged query
 * describes the identical world set, so the coverage argument holds and the
 * filter phase trivially keeps every previous match. That is what lets a
 * cancelled search be resumed by pressing Start again — results only ever
 * disappear when the query genuinely changes, or on an explicit clear.
 */
export function isContinuationOf(candidate: QueryDocument, base: QueryDocument): boolean {
  if ((candidate.max_depth ?? 24) !== (base.max_depth ?? 24)) return false
  if ((candidate.require_blacksmith ?? false) !== (base.require_blacksmith ?? false)) return false
  if ((candidate.exclude_blacksmith_rewards ?? false) !== (base.exclude_blacksmith_rewards ?? false)) return false
  if ((candidate.fast_mode ?? false) !== (base.fast_mode ?? false)) return false
  const candidateChallenges = [...(candidate.challenges ?? [])].sort()
  const baseChallenges = [...(base.challenges ?? [])].sort()
  if (candidateChallenges.length !== baseChallenges.length) return false
  if (candidateChallenges.some((name, index) => name !== baseChallenges[index])) return false
  const available = requirementCounts(candidate.requirements)
  for (const [signature, needed] of requirementCounts(base.requirements)) {
    if ((available.get(signature) ?? 0) < needed) return false
  }
  return true
}

/** The finished-run facts the refine decision reads out of the search store. */
export interface RefineBase {
  state: SearchStatus
  queryJson: string
}

/**
 * Whether starting `query` should continue the run described by `base`
 * instead of scanning from scratch. Only a completed or cancelled run knows
 * exactly how much of the seed space it covered; an imported, failed, or
 * still-running one does not, and a fresh state has no query at all.
 *
 * This is the single gate for the implicit refine: there is no separate
 * refine or resume action in the UI, so every start consults it. An unchanged
 * query continues too, which is what keeps a session alive across repeated
 * Cancel/Start cycles; only the Clear button ends it.
 */
export function shouldRefine(base: RefineBase, query: QueryDocument): boolean {
  if (base.state !== 'completed' && base.state !== 'cancelled') return false
  if (!base.queryJson) return false
  try {
    return isContinuationOf(query, JSON.parse(base.queryJson) as QueryDocument)
  } catch {
    return false
  }
}

/**
 * Whether two queries name a common item: some requirement of each has the
 * same kind, and either both name the same item or at least one names none
 * (a kind-level requirement subsumes every item of its kind). Scope and
 * challenge differences are irrelevant — a filter re-verifies seeds from
 * scratch — so this deliberately checks nothing else: it only estimates
 * whether the Target Set is enriched for the candidate query's matches.
 */
export function sharesRequirement(candidate: QueryDocument, base: QueryDocument): boolean {
  return candidate.requirements.some((left) =>
    base.requirements.some(
      (right) =>
        (left.kind == null || right.kind == null || left.kind === right.kind) &&
        (left.item == null || right.item == null || left.item === right.item),
    ),
  )
}

/** What pressing Start Search does with a query, per docs/search-semantics.md. */
export type StartMode =
  /** Fresh full-range scan that establishes the Target on conclusion. */
  | 'anchor'
  /** Filter the Target Set, then resume the target's uncovered remainder. */
  | 'target-refine'
  /** Filter the Target Set only; coverage and set stay untouched. */
  | 'target-filter'
  /** Continue the previous detached scan (filter its results, resume its remainder). */
  | 'continue-detached'
  /** Fresh full-range scan that leaves the Target untouched. */
  | 'detached'

/**
 * The single gate for what Start Search does. The Target Set is the anchor:
 * a continuation of the Target Query refines it, a query sharing an item
 * filters it (always from the full set, so loosening a requirement brings
 * seeds back), and anything else scans the full range without touching it.
 * An empty Target Set holds nothing worth preserving, so a non-continuing
 * query re-anchors on this search instead of filtering nothing.
 */
export function decideStart(state: CoordinatorState, query: QueryDocument): StartMode {
  const target = state.target
  if (!target) return 'anchor'
  const continuesTarget = isContinuationOf(query, target.query)
  if (target.matches.length === 0) {
    return continuesTarget && segmentsLength(target.remainder) > 0 ? 'target-refine' : 'anchor'
  }
  if (continuesTarget) return 'target-refine'
  if (sharesRequirement(query, target.query)) return 'target-filter'
  if (state.runKind === 'detached' && shouldRefine(state, query)) return 'continue-detached'
  return 'detached'
}

/**
 * The seed ranges a stopped search has not covered. Workers report a scanned
 * prefix length for each individual segment (never one cumulative count): a
 * segment can be abandoned mid-way when its session hits the per-session
 * result cap, and its untested tail must stay in the remainder. Reported
 * counts lag the true position slightly, which only makes the remainder
 * conservative — a resumed scan may re-test a few seeds, never skip one.
 */
export function remainingSegments(segments: SeedRange[][], workerScanned: Record<number, number[]>): SeedRange[] {
  const remainder: SeedRange[] = []
  segments.forEach((workerSegments, workerIndex) => {
    workerSegments.forEach((segment, segmentIndex) => {
      const scanned = workerScanned[workerIndex]?.[segmentIndex] ?? 0
      if (segment.startSeed + scanned < segment.endSeedExclusive) {
        remainder.push({ startSeed: segment.startSeed + scanned, endSeedExclusive: segment.endSeedExclusive })
      }
    })
  })
  return remainder
}

export function segmentsLength(segments: SeedRange[]): number {
  return segments.reduce((sum, segment) => sum + (segment.endSeedExclusive - segment.startSeed), 0)
}

/**
 * Splits a flat list of ranges into `workerCount` contiguous slices of nearly
 * equal seed count, preserving traversal order within each slice.
 */
export function distributeSegments(segments: SeedRange[], workerCount: number): SeedRange[][] {
  const total = segmentsLength(segments)
  const workers = Math.max(1, Math.floor(workerCount) || 1)
  const output: SeedRange[][] = Array.from({ length: workers }, () => [])
  if (total === 0) return output
  let workerIndex = 0
  let consumed = 0
  let boundary = Math.floor((total * (workerIndex + 1)) / workers)
  for (let segment of segments) {
    let length = segment.endSeedExclusive - segment.startSeed
    while (length > 0) {
      // Advance past workers whose share is already full (possible when a
      // share rounds down to zero seeds).
      while (consumed >= boundary && workerIndex < workers - 1) {
        workerIndex += 1
        boundary = Math.floor((total * (workerIndex + 1)) / workers)
      }
      const take = Math.min(length, boundary - consumed) || length
      output[workerIndex].push({ startSeed: segment.startSeed, endSeedExclusive: segment.startSeed + take })
      segment = { startSeed: segment.startSeed + take, endSeedExclusive: segment.endSeedExclusive }
      consumed += take
      length -= take
    }
  }
  return output
}
