import { Store } from '@tanstack/store'
import { defaultQueryState, fromQueryJson, toQueryJson } from './query'
import type { QueryState } from './wasm/types'

const QUERY_KEY = 'seedseeker.query.v1'
const PRESETS_KEY = 'seedseeker.presets.v1'
const WORKERS_KEY = 'seedseeker.workers.v1'

/** Logical processors available for search workers, always at least 1. */
export const maxWorkers = (): number => Math.max(1, (typeof navigator !== 'undefined' && navigator.hardwareConcurrency) || 4)

function hydrateWorkerCount(): number {
  const ceiling = maxWorkers()
  if (typeof localStorage === 'undefined') return ceiling
  const saved = Number(localStorage.getItem(WORKERS_KEY))
  if (!Number.isFinite(saved) || saved < 1) return ceiling
  return Math.min(Math.floor(saved), ceiling)
}

export const workerCountStore = new Store<number>(hydrateWorkerCount())
export function setWorkerCount(count: number): void {
  const clamped = Math.min(Math.max(1, Math.floor(count)), maxWorkers())
  workerCountStore.setState(() => clamped)
  if (typeof localStorage !== 'undefined') localStorage.setItem(WORKERS_KEY, String(clamped))
}

function hydrateQuery(): QueryState {
  if (typeof localStorage === 'undefined') return defaultQueryState()
  try {
    const saved = localStorage.getItem(QUERY_KEY)
    return saved ? fromQueryJson(saved) : defaultQueryState()
  } catch {
    return defaultQueryState()
  }
}

export const queryStore = new Store<QueryState>(hydrateQuery())
if (typeof localStorage !== 'undefined') {
  queryStore.subscribe(() => localStorage.setItem(QUERY_KEY, toQueryJson(queryStore.state)))
}

export interface Preset { name: string; query: QueryState }
export const builtInPresets: Preset[] = [
  {
    // Four wands where three share identity group A: upgrade transfer stacks them into a +21 staff.
    name: '+21 Staff',
    query: { ...defaultQueryState(), requirements: [
      { kind: 'wand', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'exact', value: 3 }, uncursed: false, identityGroup: 1 },
      { kind: 'wand', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'any', value: 1 }, uncursed: false, identityGroup: 1 },
      { kind: 'wand', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'any', value: 1 }, uncursed: false, identityGroup: 1 },
      { kind: 'wand', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'at_least', value: 1 }, uncursed: false },
    ] },
  },
  {
    name: 'Wand Bonanza',
    query: { ...defaultQueryState(), requirements: [
      { kind: 'wand', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'exact', value: 3 }, uncursed: false },
      { kind: 'wand', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'exact', value: 2 }, uncursed: false, maxDepth: 4 },
      { kind: 'wand', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'exact', value: 2 }, uncursed: false, maxDepth: 4 },
      { kind: 'wand', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'exact', value: 2 }, uncursed: false },
    ] },
  },
  {
    name: '+21 Ring of Wealth',
    query: { ...defaultQueryState(), requirements: [
      { kind: 'ring', item: 'ring_wealth', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'exact', value: 4 }, uncursed: false, source: 'imp_reward' },
      { kind: 'ring', item: 'ring_wealth', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'exact', value: 2 }, uncursed: false, maxDepth: 4 },
    ] },
  },
]

export function loadPresets(): Preset[] {
  try {
    const value = localStorage.getItem(PRESETS_KEY)
    if (!value) return []
    const raw = JSON.parse(value) as { name: string; query: unknown }[]
    return raw.map((preset) => ({ name: preset.name, query: fromQueryJson(JSON.stringify(preset.query)) }))
  } catch {
    return []
  }
}

export function savePresets(presets: Preset[]): void {
  localStorage.setItem(PRESETS_KEY, JSON.stringify(presets.map((preset) => ({ name: preset.name, query: JSON.parse(toQueryJson(preset.query)) }))))
}
