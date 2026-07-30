import type { ParsedSeed } from '../wasm/types'
import type { SeedRange } from './traversal'

export type SearchStatus = 'idle' | 'running' | 'stopping' | 'completed' | 'cancelled'
export interface RateSample { at: number; tested: number }
export interface RefineSummary { kept: number; of: number }
export interface CoordinatorState {
  sessionId: number
  state: SearchStatus
  tested: number
  total: number
  rate: number
  elapsed: number
  matches: ParsedSeed[]
  capped: boolean
  workerTested: Record<number, number>
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
  error?: string
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
  workerTested: {},
  completedWorkers: 0,
  workerCount: 0,
  startedAt: 0,
  rateSamples: [],
  segments: [],
  queryJson: '',
})

export function mergeMatches(existing: ParsedSeed[], incoming: ParsedSeed[], cap = RESULT_CAP): { matches: ParsedSeed[]; capped: boolean } {
  // Deduplicate by seed value: a refined search may re-test a small overlap
  // around the previous stop position and rediscover a filtered survivor.
  const byValue = new Map<number, ParsedSeed>()
  for (const match of [...existing, ...incoming]) {
    if (!byValue.has(match.value)) byValue.set(match.value, match)
  }
  const unique = [...byValue.values()].sort((left, right) => left.value - right.value)
  return { matches: unique.slice(0, cap), capped: unique.length >= cap }
}

export function calculateRate(samples: RateSample[]): number {
  if (samples.length < 2) return 0
  const first = samples[0]
  const last = samples[samples.length - 1]
  const seconds = (last.at - first.at) / 1_000
  return seconds > 0 ? (last.tested - first.tested) / seconds : 0
}

export interface ProgressUpdate { sessionId: number; workerId: number; tested: number; matches: ParsedSeed[]; now: number }

export function applyProgress(state: CoordinatorState, update: ProgressUpdate): CoordinatorState {
  // Progress is also accepted while stopping: the final flush carries matches
  // and counts from the region recorded as scanned, which a refine relies on.
  if (update.sessionId !== state.sessionId || (state.state !== 'running' && state.state !== 'stopping')) return state
  const workerTested = { ...state.workerTested, [update.workerId]: update.tested }
  const tested = Object.values(workerTested).reduce((sum, value) => sum + value, 0)
  const merged = mergeMatches(state.matches, update.matches)
  const rateSamples = [...state.rateSamples, { at: update.now, tested }].filter((sample) => update.now - sample.at <= 5_000)
  return {
    ...state,
    workerTested,
    tested,
    matches: merged.matches,
    capped: merged.capped,
    state: merged.capped && state.state === 'running' ? 'completed' : state.state,
    elapsed: update.now - state.startedAt,
    rateSamples,
    rate: calculateRate(rateSamples),
  }
}

export interface WorkerTerminal { sessionId: number; workerId: number; tested: number; kind: 'done' | 'stopped'; now: number }

export function markWorkerDone(state: CoordinatorState, update: WorkerTerminal): CoordinatorState {
  if (update.sessionId !== state.sessionId || (state.state !== 'running' && state.state !== 'stopping')) return state
  const workerTested = { ...state.workerTested, [update.workerId]: update.tested }
  const completedWorkers = state.completedWorkers + 1
  const finished = completedWorkers >= state.workerCount
  return {
    ...state,
    workerTested,
    tested: Object.values(workerTested).reduce((sum, value) => sum + value, 0),
    completedWorkers,
    state: finished ? (state.state === 'stopping' ? 'cancelled' : 'completed') : state.state,
    elapsed: update.now - state.startedAt,
  }
}
