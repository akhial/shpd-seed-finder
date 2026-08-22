import { readFile } from 'node:fs/promises'
import { beforeAll, describe, expect, it } from 'vitest'
import { defaultQueryState, validateQuery } from './query'
import type { QueryState, RequirementState } from './wasm/types'
import { engineInfo } from './wasm'
import init from './wasm/pkg/seedfinder.js'

const requirement = (patch: Partial<RequirementState> = {}): RequirementState => ({
  kind: 'weapon', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'any', value: 1 }, uncursed: false, ...patch,
})
const state = (...requirements: RequirementState[]): QueryState => ({ ...defaultQueryState(), requirements })

/**
 * The validation bounds and the floor list are engine constants read through
 * `engineInfo()`, so these tests run against the real wasm module. Node has no
 * `fetch` for `file:` URLs, so it is instantiated from bytes.
 */
beforeAll(async () => {
  await init({ module_or_path: await readFile(new URL('./wasm/pkg/seedfinder_bg.wasm', import.meta.url)) })
})

describe('query validation', () => {
  it('rejects a tier on an item-specific requirement', () => {
    expect(validateQuery(state(requirement({ item: 'sword', tier: { mode: 'exact', value: 3 } }))).errors.join(' ')).toMatch(/wildcard/)
  })
  it('rejects ring upgrade +5', () => {
    expect(validateQuery(state(requirement({ kind: 'ring', item: 'ring_haste', upgrade: { mode: 'exact', value: 5 } }))).valid).toBe(false)
  })
  it('accepts a minimum upgrade at the engine maximum', () => {
    // The editor used to stop one below it, so "+3 or higher" — a query the
    // engine validates and searches — could not be expressed.
    const limits = engineInfo().limits
    expect(validateQuery(state(requirement({ upgrade: { mode: 'at_least', value: limits.max_upgrade_default } })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'ring', upgrade: { mode: 'at_least', value: limits.max_upgrade_ring } })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'ring', upgrade: { mode: 'at_least', value: limits.max_upgrade_ring + 1 } }))).valid).toBe(false)
  })
  it('bounds tiers by the engine limits', () => {
    const limits = engineInfo().limits
    expect(validateQuery(state(requirement({ tier: { mode: 'exact', value: limits.exact_tier_max } })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ tier: { mode: 'exact', value: limits.exact_tier_max + 1 } }))).valid).toBe(false)
    expect(validateQuery(state(requirement({ tier: { mode: 'at_most', value: limits.bounded_tier_max + 1 } }))).valid).toBe(false)
    expect(validateQuery({ ...state(requirement()), maxDepth: limits.max_depth + 1 }).errors.join(' ')).toMatch(/Maximum floor/)
  })
  it('rejects curse with uncursed', () => {
    expect(validateQuery(state(requirement({ effect: 'Annoying', uncursed: true }))).errors.join(' ')).toMatch(/curse/)
  })
  it('rejects mismatched identity groups', () => {
    expect(validateQuery(state(requirement({ identityGroup: 1 }), requirement({ kind: 'armor', identityGroup: 1 }))).errors.join(' ')).toMatch(/Identity group/)
  })
  it('validates melee and thrown weapon kinds', () => {
    expect(validateQuery(state(requirement({ kind: 'melee_weapon' })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'thrown_weapon', tier: { mode: 'exact', value: 5 } })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'thrown_weapon', item: 'shuriken' })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'thrown_weapon', effect: 'Projecting' })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'melee_weapon', item: 'shuriken' }))).errors.join(' ')).toMatch(/melee weapon/)
    expect(validateQuery(state(requirement({ kind: 'thrown_weapon', item: 'sword' }))).errors.join(' ')).toMatch(/thrown weapon/)
    expect(validateQuery(state(requirement({ kind: 'melee_weapon', item: 'ring_haste' }))).errors.join(' ')).toMatch(/category/)
  })

  it('accepts a valid full query', () => {
    const query = state(
      requirement({ tier: { mode: 'at_least', value: 3 }, upgrade: { mode: 'at_least', value: 2 }, effect: 'Blazing', source: 'locked_chest', maxDepth: 12, identityGroup: 1 }),
      requirement({ item: 'sword', upgrade: { mode: 'exact', value: 1 }, identityGroup: 1 }),
    )
    query.maxDepth = 20; query.requireBlacksmith = true; query.challenges = ['on_diet']
    expect(validateQuery(query)).toEqual({ valid: true, errors: [] })
  })
})
