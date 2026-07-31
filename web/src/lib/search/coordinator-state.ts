import type { ParsedSeed, QueryDocument } from '../wasm/types'
import type { SeedRange } from './traversal'

export type SearchStatus = 'idle' | 'running' | 'stopping' | 'completed' | 'cancelled' | 'failed' | 'imported'
export interface RateSample { at: number; tested: number }
export interface RefineSummary { kept: number; of: number }
export interface CoordinatorState {
  sessionId: number
  state: SearchStatus
  tested: number
  total: number
  rate: number
  elapsed: number
  /** Every unique match delivered so far, sorted by seed value. Never
   * truncated: matches beyond the display cap fall inside the scanned region
   * and must survive into a refine's filter set. */
  matches: ParsedSeed[]
  capped: boolean
  /** Per-worker, per-segment scanned prefix lengths, aligned with
   * `segments`. */
  workerScanned: Record<number, number[]>
  completedWorkers: number
  workerCount: number
  startedAt: number
  rateSamples: RateSample[]
  /** Per-worker segment assignment of the active traversal, recorded so a
   * later refine can compute exactly which ranges remain unscanned. */
  segments: SeedRange[][]
  /** JSON of the query document these matches belong to. */
  queryJson: string
  /** Set while the current results came from refining a previous run. */
  refined?: RefineSummary
  /** True while a refine is re-verifying the previous results and no worker
   * has started scanning yet. */
  filtering: boolean
  error?: string
  /** The query that produced `matches` (captured at search start or import). */
  query?: QueryDocument
  /** Imported entries dropped as duplicates or beyond the result cap. */
  importedDropped?: number
}

export const RESULT_CAP = 1_024
export const initialCoordinatorState = (total = 0): CoordinatorState => ({
  sessionId: 0,
  state: 'idle',
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
  queryJson: '',
  filtering: false,
})

/**
 * Whether "Clear results" has anything to discard. A running or stopping
 * search owns the state — including the coverage bookkeeping a later refine
 * needs — so it is never cleared from underneath, and a state that is already
 * idle and empty has nothing to clear.
 */
export function canClearResults(state: CoordinatorState): boolean {
  if (state.state === 'running' || state.state === 'stopping') return false
  return state.state !== 'idle' || state.matches.length > 0
}

export function mergeMatches(existing: ParsedSeed[], incoming: ParsedSeed[], cap = RESULT_CAP): { matches: ParsedSeed[]; capped: boolean } {
  // Deduplicate by seed value: a refined search may re-test a small overlap
  // around the previous stop position and rediscover a filtered survivor.
  // Nothing is evicted at the cap — the workers of a capped session are told
  // to stop, and every match they delivered belongs to the scanned region a
  // refine relies on. Only the display is limited to `RESULT_CAP`.
  const byValue = new Map<number, ParsedSeed>()
  for (const match of [...existing, ...incoming]) {
    if (!byValue.has(match.value)) byValue.set(match.value, match)
  }
  const unique = [...byValue.values()].sort((left, right) => left.value - right.value)
  return { matches: unique, capped: unique.length >= cap }
}

export function calculateRate(samples: RateSample[]): number {
  if (samples.length < 2) return 0
  const first = samples[0]
  const last = samples[samples.length - 1]
  const seconds = (last.at - first.at) / 1_000
  return seconds > 0 ? (last.tested - first.tested) / seconds : 0
}

/**
 * Replaces the whole search state with results restored from a file.
 * Matching every other platform, imported seeds are deduplicated (keeping the
 * first occurrence) and capped at the result limit, with the dropped count
 * reported for the UI. The fresh base state carries no segments or scanned
 * regions, so an imported result set is never offered for refining.
 */
export function importedResultsState(state: CoordinatorState, matches: ParsedSeed[], query: QueryDocument): CoordinatorState {
  const seen = new Set<string>()
  const kept: ParsedSeed[] = []
  for (const match of matches) {
    if (kept.length === RESULT_CAP) break
    if (!seen.has(match.code)) {
      seen.add(match.code)
      kept.push(match)
    }
  }
  return {
    ...initialCoordinatorState(state.total),
    sessionId: state.sessionId,
    state: 'imported',
    matches: kept,
    capped: kept.length === RESULT_CAP && matches.length > RESULT_CAP,
    query,
    importedDropped: matches.length - kept.length,
  }
}

const sumScanned = (workerScanned: Record<number, number[]>): number =>
  Object.values(workerScanned).reduce((sum, scanned) => sum + scanned.reduce((s, value) => s + value, 0), 0)

export interface ProgressUpdate { sessionId: number; workerId: number; scanned: number[]; matches: ParsedSeed[]; now: number }

export function applyProgress(state: CoordinatorState, update: ProgressUpdate): CoordinatorState {
  // Progress is also accepted while stopping: the final flush carries matches
  // and counts from the region recorded as scanned, which a refine relies on.
  if (update.sessionId !== state.sessionId || (state.state !== 'running' && state.state !== 'stopping')) return state
  const workerScanned = { ...state.workerScanned, [update.workerId]: update.scanned }
  const tested = sumScanned(workerScanned)
  const merged = mergeMatches(state.matches, update.matches)
  const rateSamples = [...state.rateSamples, { at: update.now, tested }].filter((sample) => update.now - sample.at <= 5_000)
  return {
    ...state,
    workerScanned,
    tested,
    matches: merged.matches,
    capped: merged.capped,
    state: merged.capped && state.state === 'running' ? 'completed' : state.state,
    elapsed: update.now - state.startedAt,
    rateSamples,
    rate: calculateRate(rateSamples),
  }
}

export interface WorkerTerminal { sessionId: number; workerId: number; scanned: number[]; kind: 'done' | 'stopped'; now: number }

export function markWorkerDone(state: CoordinatorState, update: WorkerTerminal): CoordinatorState {
  if (update.sessionId !== state.sessionId || (state.state !== 'running' && state.state !== 'stopping')) return state
  const workerScanned = { ...state.workerScanned, [update.workerId]: update.scanned }
  const completedWorkers = state.completedWorkers + 1
  const finished = completedWorkers >= state.workerCount
  return {
    ...state,
    workerScanned,
    tested: sumScanned(workerScanned),
    completedWorkers,
    state: finished ? (state.state === 'stopping' ? 'cancelled' : 'completed') : state.state,
    elapsed: update.now - state.startedAt,
  }
}
