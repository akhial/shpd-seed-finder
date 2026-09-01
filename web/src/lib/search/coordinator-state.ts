import type { ParsedSeed, QueryDocument } from "../wasm/types";
// Type-only in the other direction, so this runtime import is not a cycle.
import { remainingSegments } from "./refine";
import type { SeedRange } from "./traversal";

export type SearchStatus =
  | "idle"
  | "running"
  | "stopping"
  | "completed"
  | "cancelled"
  | "failed"
  | "imported";
export interface RateSample {
  at: number;
  tested: number;
}
export interface RefineSummary {
  kept: number;
  of: number;
}

/** How the current (or last) run relates to the Target — see
 * docs/search-semantics.md. A continued detached scan stays 'detached'. */
export type RunKind = "anchor" | "target-refine" | "target-filter" | "detached";

/** The session's anchor: established by the first concluded search (or an
 * import) and reset only by Clear. `matches` is uncapped and a superset of
 * any related run's display, which is what lets a loosened query bring
 * seeds back. */
export interface TargetState {
  queryJson: string;
  query: QueryDocument;
  /** Every unique match delivered for the Target Query, sorted by value. */
  matches: ParsedSeed[];
  /** Seed ranges the target traversal has not covered; empty for imports. */
  remainder: SeedRange[];
}
export interface CoordinatorState {
  sessionId: number;
  state: SearchStatus;
  tested: number;
  total: number;
  rate: number;
  elapsed: number;
  /** Every unique match delivered so far, sorted by seed value. Never
   * truncated: matches beyond the display cap fall inside the scanned region
   * and must survive into a refine's filter set. */
  matches: ParsedSeed[];
  capped: boolean;
  /** Per-worker, per-segment scanned prefix lengths, aligned with
   * `segments`. */
  workerScanned: Record<number, number[]>;
  completedWorkers: number;
  workerCount: number;
  startedAt: number;
  rateSamples: RateSample[];
  /** Per-worker segment assignment of the active traversal, recorded so a
   * later refine can compute exactly which ranges remain unscanned. */
  segments: SeedRange[][];
  /** JSON of the query document these matches belong to. */
  queryJson: string;
  /** Set while the current results came from refining a previous run. */
  refined?: RefineSummary;
  /** How many unique matches existed when the current scan started (the
   * refine survivors; zero for a fresh scan). The scan stops once it has
   * added `RESULT_CAP` matches beyond this — the per-session accept cap the
   * native engines enforce — so repeating a query keeps growing the
   * collection while each run stays bounded. */
  sessionBaseline: number;
  /** True while a refine is re-verifying the previous results and no worker
   * has started scanning yet. */
  filtering: boolean;
  error?: string;
  /** The query that produced `matches` (captured at search start or import). */
  query?: QueryDocument;
  /** Imported entries dropped as duplicates or beyond the result cap. */
  importedDropped?: number;
  /** The session's Target, if one has been established. */
  target?: TargetState;
  runKind: RunKind;
}

export const RESULT_CAP = 1_024;
export const initialCoordinatorState = (total = 0): CoordinatorState => ({
  sessionId: 0,
  state: "idle",
  tested: 0,
  total,
  rate: 0,
  elapsed: 0,
  matches: [],
  capped: false,
  workerScanned: {},
  completedWorkers: 0,
  workerCount: 0,
  startedAt: 0,
  rateSamples: [],
  segments: [],
  queryJson: "",
  filtering: false,
  runKind: "anchor",
  sessionBaseline: 0,
});

/** Whether the current run has delivered its per-session quota of new
 * matches; the coordinator stops the workers once it has. */
export function runSaturated(state: CoordinatorState): boolean {
  return state.matches.length - state.sessionBaseline >= RESULT_CAP;
}

/**
 * Whether "Clear results" has anything to discard. A running or stopping
 * search owns the state — including the coverage bookkeeping a later refine
 * needs — so it is never cleared from underneath, and a state that is already
 * idle and empty has nothing to clear.
 */
export function canClearResults(state: CoordinatorState): boolean {
  if (state.state === "running" || state.state === "stopping") return false;
  return state.state !== "idle" || state.matches.length > 0 || state.target !== undefined;
}

export function mergeMatches(
  existing: ParsedSeed[],
  incoming: ParsedSeed[],
  cap = RESULT_CAP,
): { matches: ParsedSeed[]; capped: boolean } {
  // Deduplicate by seed value: a refined search may re-test a small overlap
  // around the previous stop position and rediscover a filtered survivor.
  // Nothing is evicted at the cap — the workers of a capped session are told
  // to stop, and every match they delivered belongs to the scanned region a
  // refine relies on. Only the display is limited to `RESULT_CAP`.
  const byValue = new Map<number, ParsedSeed>();
  for (const match of [...existing, ...incoming]) {
    if (!byValue.has(match.value)) byValue.set(match.value, match);
  }
  const unique = [...byValue.values()].sort((left, right) => left.value - right.value);
  return { matches: unique, capped: unique.length >= cap };
}

export function calculateRate(samples: RateSample[]): number {
  if (samples.length < 2) return 0;
  const first = samples[0];
  const last = samples[samples.length - 1];
  const seconds = (last.at - first.at) / 1_000;
  return seconds > 0 ? (last.tested - first.tested) / seconds : 0;
}

/**
 * Replaces the whole search state with results restored from a file. The
 * engine's decoder already deduplicated and capped the seeds — identically on
 * every platform — and counted the entries that removed, so `dropped` is
 * reported straight to the UI. The import becomes the session's Target with
 * empty coverage — related queries filter it, but nothing ever resumes a scan
 * from it.
 */
export function importedResultsState(
  state: CoordinatorState,
  matches: ParsedSeed[],
  query: QueryDocument,
  dropped = 0,
): CoordinatorState {
  return {
    ...initialCoordinatorState(state.total),
    sessionId: state.sessionId,
    state: "imported",
    matches,
    capped: matches.length >= RESULT_CAP,
    query,
    importedDropped: dropped,
    // The imported query and seeds become the session's Target, with no
    // coverage: refines of an import are filter-only.
    target: { queryJson: JSON.stringify(query), query, matches, remainder: [] },
  };
}

/**
 * Folds a run that just reached a terminal state into the Target, per
 * docs/search-semantics.md: an anchor run (or the first run of a session)
 * establishes the Target from its own results and coverage; a target refine
 * grows the set with its new finds and advances the coverage; a target
 * filter or detached run leaves the Target exactly as it was. Every
 * transition into 'completed' or 'cancelled' must pass through here.
 * A failed run establishes nothing — its coverage is unknown.
 */
export function settleRun(state: CoordinatorState): CoordinatorState {
  if (state.state !== "completed" && state.state !== "cancelled") return state;
  if (state.runKind === "target-filter" || state.runKind === "detached") return state;
  const remainder = remainingSegments(state.segments, state.workerScanned);
  if (state.runKind === "anchor" || !state.target) {
    if (!state.query) return state;
    return {
      ...state,
      target: { queryJson: state.queryJson, query: state.query, matches: state.matches, remainder },
    };
  }
  // The refined run's survivors were already members; only new finds from
  // the resumed scan grow the set. The stored set is never capped.
  const merged = mergeMatches(state.target.matches, state.matches, Number.POSITIVE_INFINITY);
  return { ...state, target: { ...state.target, matches: merged.matches, remainder } };
}

const sumScanned = (workerScanned: Record<number, number[]>): number =>
  Object.values(workerScanned).reduce(
    (sum, scanned) => sum + scanned.reduce((s, value) => s + value, 0),
    0,
  );

export interface ProgressUpdate {
  sessionId: number;
  workerId: number;
  scanned: number[];
  matches: ParsedSeed[];
  now: number;
}

export function applyProgress(state: CoordinatorState, update: ProgressUpdate): CoordinatorState {
  // Progress is also accepted while stopping: the final flush carries matches
  // and counts from the region recorded as scanned, which a refine relies on.
  if (
    update.sessionId !== state.sessionId ||
    (state.state !== "running" && state.state !== "stopping")
  )
    return state;
  const workerScanned = { ...state.workerScanned, [update.workerId]: update.scanned };
  const tested = sumScanned(workerScanned);
  const merged = mergeMatches(state.matches, update.matches);
  const rateSamples = [...state.rateSamples, { at: update.now, tested }].filter(
    (sample) => update.now - sample.at <= 5_000,
  );
  // `capped` reports display truncation; the run itself ends on its own
  // accept quota, so a refine whose survivors already fill the display still
  // scans for more.
  const saturated = merged.matches.length - state.sessionBaseline >= RESULT_CAP;
  return settleRun({
    ...state,
    workerScanned,
    tested,
    matches: merged.matches,
    capped: merged.capped,
    state: saturated && state.state === "running" ? "completed" : state.state,
    elapsed: update.now - state.startedAt,
    rateSamples,
    rate: calculateRate(rateSamples),
  });
}

export interface WorkerTerminal {
  sessionId: number;
  workerId: number;
  scanned: number[];
  kind: "done" | "stopped";
  now: number;
}

export function markWorkerDone(state: CoordinatorState, update: WorkerTerminal): CoordinatorState {
  if (
    update.sessionId !== state.sessionId ||
    (state.state !== "running" && state.state !== "stopping")
  )
    return state;
  const workerScanned = { ...state.workerScanned, [update.workerId]: update.scanned };
  const completedWorkers = state.completedWorkers + 1;
  const finished = completedWorkers >= state.workerCount;
  return settleRun({
    ...state,
    workerScanned,
    tested: sumScanned(workerScanned),
    completedWorkers,
    state: finished ? (state.state === "stopping" ? "cancelled" : "completed") : state.state,
    elapsed: update.now - state.startedAt,
  });
}
