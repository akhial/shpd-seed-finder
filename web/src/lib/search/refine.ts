import type { QueryDocument, RequirementDocument } from '../wasm/types'
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
 * Whether `candidate` refines `base`: identical scope (depth, challenges, and
 * flags) and a strict superset of the base requirements. Only then are the
 * base run's matches guaranteed to contain every candidate match in the
 * already-scanned region, which is what makes filter-and-resume sound.
 */
export function isRefinementOf(candidate: QueryDocument, base: QueryDocument): boolean {
  if ((candidate.max_depth ?? 24) !== (base.max_depth ?? 24)) return false
  if ((candidate.require_blacksmith ?? false) !== (base.require_blacksmith ?? false)) return false
  if ((candidate.exclude_blacksmith_rewards ?? false) !== (base.exclude_blacksmith_rewards ?? false)) return false
  if ((candidate.fast_mode ?? false) !== (base.fast_mode ?? false)) return false
  const candidateChallenges = [...(candidate.challenges ?? [])].sort()
  const baseChallenges = [...(base.challenges ?? [])].sort()
  if (candidateChallenges.length !== baseChallenges.length) return false
  if (candidateChallenges.some((name, index) => name !== baseChallenges[index])) return false
  if (candidate.requirements.length <= base.requirements.length) return false
  const available = requirementCounts(candidate.requirements)
  for (const [signature, needed] of requirementCounts(base.requirements)) {
    if ((available.get(signature) ?? 0) < needed) return false
  }
  return true
}

/**
 * The seed ranges a stopped search has not covered. Each worker scans its
 * ordered segments strictly front to back, so its scanned region is exactly
 * the first `workerTested[index]` seeds of its concatenated segments. Reported
 * counts lag the true position slightly, which only makes the remainder
 * conservative — a resumed scan may re-test a few seeds, never skip one.
 */
export function remainingSegments(segments: SeedRange[][], workerTested: Record<number, number>): SeedRange[] {
  const remainder: SeedRange[] = []
  segments.forEach((workerSegments, index) => {
    let toSkip = workerTested[index] ?? 0
    for (const segment of workerSegments) {
      const length = segment.endSeedExclusive - segment.startSeed
      if (toSkip >= length) {
        toSkip -= length
        continue
      }
      remainder.push({ startSeed: segment.startSeed + toSkip, endSeedExclusive: segment.endSeedExclusive })
      toSkip = 0
    }
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
