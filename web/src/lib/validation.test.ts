import { describe, expect, it } from 'vitest'
import { defaultQueryState, validateQuery } from './query'
import type { QueryState, RequirementState } from './wasm/types'

const requirement = (patch: Partial<RequirementState> = {}): RequirementState => ({
  kind: 'weapon', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'any', value: 1 }, uncursed: false, ...patch,
})
const state = (...requirements: RequirementState[]): QueryState => ({ ...defaultQueryState(), requirements })

describe('query validation', () => {
  it('rejects a tier on an item-specific requirement', () => {
    expect(validateQuery(state(requirement({ item: 'sword', tier: { mode: 'exact', value: 3 } }))).errors.join(' ')).toMatch(/wildcard/)
  })
  it('rejects ring upgrade +5', () => {
    expect(validateQuery(state(requirement({ kind: 'ring', item: 'ring_haste', upgrade: { mode: 'exact', value: 5 } }))).valid).toBe(false)
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
