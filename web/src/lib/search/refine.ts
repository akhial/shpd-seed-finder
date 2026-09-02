import type { QueryDocument } from "../wasm/types";
import { decideStart as decideStartInEngine, queryContinues } from "../wasm";
import type { CoordinatorState } from "./coordinator-state";
import type { SeedRange } from "./traversal";

/**
 * Whether a run of `candidate` can continue one of `base`: an identical depth
 * and challenge set, world conditions (the blacksmith flags and the
 * Wandmaker filter) at least as strict as the base's, and every base
 * requirement covered by a
 * distinct candidate requirement at least as strict — equal, added-to, or
 * strengthened (a named item where the base wanted any of its kind, a
 * tightened bound). Only then are the base run's matches guaranteed
 * to contain every candidate match in the already-scanned region, which is
 * what makes filter-and-resume sound.
 *
 * Equality is deliberately included, not an edge case: an unchanged query
 * describes the identical world set, so the coverage argument holds and the
 * filter phase trivially keeps every previous match. That is what lets a
 * cancelled search be resumed by pressing Start again — results only ever
 * disappear when the query genuinely changes, or on an explicit clear.
 *
 * The rule itself lives in the engine and is shared with every other
 * frontend; this only feeds it the two documents. A query the engine cannot
 * decode continues nothing: an invalid query has no world set to compare, so
 * the only sound answer is a fresh scan. The UI never starts one anyway.
 */
export function isContinuationOf(candidate: QueryDocument, base: QueryDocument): boolean {
  try {
    return queryContinues(JSON.stringify(candidate), JSON.stringify(base));
  } catch {
    return false;
  }
}

/** What pressing Start Search does with a query, per docs/search-semantics.md. */
export type StartMode =
  /** Fresh full-range scan that establishes the Target on conclusion. */
  | "anchor"
  /** Filter the Target Set, then resume the target's uncovered remainder. */
  | "target-refine"
  /** Filter the Target Set only; coverage and set stay untouched. */
  | "target-filter"
  /** Continue the previous detached scan (filter its results, resume its remainder). */
  | "continue-detached"
  /** Fresh full-range scan that leaves the Target untouched. */
  | "detached";

/**
 * The single gate for what Start Search does. The rule itself — including
 * both the continuation and the shares-an-item predicate — lives in the
 * engine and is shared with every other frontend; this only reads the store's
 * side of it: the Target Query, whether the Target Set is empty, whether the
 * target traversal left range uncovered, and the last run's query when that
 * run was a concluded detached scan (only a completed or cancelled run knows
 * how much it covered, so an imported, failed or still-running one is never
 * a continuation base).
 */
export function decideStart(state: CoordinatorState, query: QueryDocument): StartMode {
  const target = state.target;
  const concluded = state.state === "completed" || state.state === "cancelled";
  const detachedBase =
    state.runKind === "detached" && concluded && state.queryJson ? state.queryJson : undefined;
  try {
    return decideStartInEngine(
      JSON.stringify(query),
      target?.queryJson,
      !target || target.matches.length === 0,
      target !== undefined && segmentsLength(target.remainder) > 0,
      detachedBase,
    ) as StartMode;
  } catch {
    // An unreadable query cannot be judged against the Target, so the only
    // sound answer is a scan that leaves it alone. The UI never starts one.
    return target ? "detached" : "anchor";
  }
}

/**
 * The seed ranges a stopped search has not covered. Workers report a scanned
 * prefix length for each individual segment (never one cumulative count): a
 * segment can be abandoned mid-way when its session hits the per-session
 * result cap, and its untested tail must stay in the remainder. Reported
 * counts lag the true position slightly, which only makes the remainder
 * conservative — a resumed scan may re-test a few seeds, never skip one.
 */
export function remainingSegments(
  segments: SeedRange[][],
  workerScanned: Record<number, number[]>,
): SeedRange[] {
  const remainder: SeedRange[] = [];
  segments.forEach((workerSegments, workerIndex) => {
    workerSegments.forEach((segment, segmentIndex) => {
      const scanned = workerScanned[workerIndex]?.[segmentIndex] ?? 0;
      if (segment.startSeed + scanned < segment.endSeedExclusive) {
        remainder.push({
          startSeed: segment.startSeed + scanned,
          endSeedExclusive: segment.endSeedExclusive,
        });
      }
    });
  });
  return remainder;
}

export function segmentsLength(segments: SeedRange[]): number {
  return segments.reduce((sum, segment) => sum + (segment.endSeedExclusive - segment.startSeed), 0);
}

/**
 * Splits a flat list of ranges into `workerCount` contiguous slices of nearly
 * equal seed count, preserving traversal order within each slice.
 */
export function distributeSegments(segments: SeedRange[], workerCount: number): SeedRange[][] {
  const total = segmentsLength(segments);
  const workers = Math.max(1, Math.floor(workerCount) || 1);
  const output: SeedRange[][] = Array.from({ length: workers }, () => []);
  if (total === 0) return output;
  let workerIndex = 0;
  let consumed = 0;
  let boundary = Math.floor((total * (workerIndex + 1)) / workers);
  for (let segment of segments) {
    let length = segment.endSeedExclusive - segment.startSeed;
    while (length > 0) {
      // Advance past workers whose share is already full (possible when a
      // share rounds down to zero seeds).
      while (consumed >= boundary && workerIndex < workers - 1) {
        workerIndex += 1;
        boundary = Math.floor((total * (workerIndex + 1)) / workers);
      }
      const take = Math.min(length, boundary - consumed) || length;
      output[workerIndex].push({
        startSeed: segment.startSeed,
        endSeedExclusive: segment.startSeed + take,
      });
      segment = { startSeed: segment.startSeed + take, endSeedExclusive: segment.endSeedExclusive };
      consumed += take;
      length -= take;
    }
  }
  return output;
}
