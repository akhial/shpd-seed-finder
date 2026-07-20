import type { ParsedSeed } from '../wasm/types'

export type SearchStatus = 'idle' | 'running' | 'completed' | 'cancelled'
export interface RateSample { at: number; tested: number }
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
})

export function mergeMatches(existing: ParsedSeed[], incoming: ParsedSeed[], cap = RESULT_CAP): { matches: ParsedSeed[]; capped: boolean } {
  const matches = [...existing, ...incoming].sort((left, right) => left.value - right.value).slice(0, cap)
  return { matches, capped: existing.length + incoming.length >= cap }
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
  if (update.sessionId !== state.sessionId || state.state !== 'running') return state
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
    state: merged.capped ? 'completed' : state.state,
    elapsed: update.now - state.startedAt,
    rateSamples,
    rate: calculateRate(rateSamples),
  }
}

export function markWorkerDone(state: CoordinatorState, sessionId: number, now: number): CoordinatorState {
  if (sessionId !== state.sessionId || state.state !== 'running') return state
  const completedWorkers = state.completedWorkers + 1
  return {
    ...state,
    completedWorkers,
    state: completedWorkers >= state.workerCount ? 'completed' : 'running',
    elapsed: now - state.startedAt,
  }
}
