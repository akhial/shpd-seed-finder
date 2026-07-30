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
  it('rejects an uncursed requirement limited to curses', () => {
    expect(validateQuery(state(requirement({ effect: { mode: 'one_of', names: ['Annoying'] }, uncursed: true }))).errors.join(' ')).toMatch(/curse/)
    // A mixed set stays valid: the curse members simply cannot match.
    expect(validateQuery(state(requirement({ effect: { mode: 'one_of', names: ['Annoying', 'Blazing'] }, uncursed: true }))).valid).toBe(true)
  })
  it('rejects empty and foreign effect selections', () => {
    expect(validateQuery(state(requirement({ effect: { mode: 'one_of', names: [] } }))).errors.join(' ')).toMatch(/at least one effect/)
    expect(validateQuery(state(requirement({ effect: { mode: 'one_of', names: ['Thorns'] } }))).errors.join(' ')).toMatch(/belong/)
    expect(validateQuery(state(requirement({ kind: 'ring', effect: { mode: 'any_enchantment' } }))).errors.join(' ')).toMatch(/weapon or armor/)
  })
  it('rejects mismatched identity groups', () => {
    expect(validateQuery(state(requirement({ identityGroup: 1 }), requirement({ kind: 'armor', identityGroup: 1 }))).errors.join(' ')).toMatch(/Identity group/)
  })
  it('allows identity groups to disagree across alternatives of one slot', () => {
    const alternatives = [
      requirement({ item: 'sword', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ item: 'mace', identityGroup: 1, alternativeGroup: 1 }),
    ]
    expect(validateQuery(state(...alternatives)).valid).toBe(true)
    // A requirement outside the slot must still agree with every member.
    expect(validateQuery(state(...alternatives, requirement({ item: 'spear', identityGroup: 1 }))).errors.join(' ')).toMatch(/Identity group/)
  })
  it('checks combined upgrade groups for agreement and attainability', () => {
    const ring = (patch: Partial<RequirementState> = {}) => requirement({ kind: 'ring', item: 'ring_might', ...patch })
    const sum = (atLeast: number) => ({ group: 1, atLeast })
    expect(validateQuery(state(ring({ upgradeSum: sum(2) }), ring({ upgradeSum: sum(2) }))).valid).toBe(true)
    expect(validateQuery(state(ring({ upgradeSum: sum(2) }), ring({ upgradeSum: sum(3) }))).errors.join(' ')).toMatch(/agree/)
    expect(validateQuery(state(ring({ upgradeSum: sum(9) }), ring({ upgradeSum: sum(9) }))).errors.join(' ')).toMatch(/more than/)
    expect(
      validateQuery(state(ring({ upgradeSum: sum(2), alternativeGroup: 1 }), ring({ upgradeSum: sum(2), alternativeGroup: 1 }))).errors.join(' '),
    ).toMatch(/alternative/)
  })
  it('validates melee and thrown weapon kinds', () => {
    expect(validateQuery(state(requirement({ kind: 'melee_weapon' })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'thrown_weapon', tier: { mode: 'exact', value: 5 } })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'thrown_weapon', item: 'shuriken' })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'thrown_weapon', effect: { mode: 'one_of', names: ['Projecting'] } })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'melee_weapon', item: 'shuriken' }))).errors.join(' ')).toMatch(/melee weapon/)
    expect(validateQuery(state(requirement({ kind: 'thrown_weapon', item: 'sword' }))).errors.join(' ')).toMatch(/thrown weapon/)
    expect(validateQuery(state(requirement({ kind: 'melee_weapon', item: 'ring_haste' }))).errors.join(' ')).toMatch(/category/)
  })
  it('accepts a valid full query', () => {
    const query = state(
      requirement({ tier: { mode: 'at_least', value: 3 }, upgrade: { mode: 'at_least', value: 2 }, effect: { mode: 'one_of', names: ['Blazing'] }, source: 'locked_chest', maxDepth: 12, identityGroup: 1 }),
      requirement({ item: 'sword', upgrade: { mode: 'exact', value: 1 }, identityGroup: 1 }),
      requirement({ item: 'spear', upgrade: { mode: 'exact', value: 3 }, alternativeGroup: 1 }),
      requirement({ item: 'shuriken', upgrade: { mode: 'exact', value: 2 }, alternativeGroup: 1 }),
      requirement({ effect: { mode: 'any_enchantment' } }),
    )
    query.maxDepth = 20; query.requireBlacksmith = true; query.challenges = ['on_diet']
    expect(validateQuery(query)).toEqual({ valid: true, errors: [] })
  })
})
