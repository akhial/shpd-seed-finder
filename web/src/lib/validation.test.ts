import { describe, expect, it } from 'vitest'
import { defaultQueryState, validateQuery, validateRequirement } from './query'
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

  it('checks effect sets against the family and the uncursed flag', () => {
    expect(validateQuery(state(requirement({ effect: ['Blocking', 'Projecting', 'Vampiric'] })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'armor', effect: 'any_enchantment', uncursed: true })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ kind: 'armor', effect: ['Blocking'] }))).errors.join(' ')).toMatch(/Blocking does not belong/)
    expect(validateQuery(state(requirement({ kind: 'ring', effect: 'any_enchantment' }))).errors.join(' ')).toMatch(/weapon or armor/)
    // Only-curses with uncursed is an error; a mixed set is fine.
    expect(validateQuery(state(requirement({ effect: ['Annoying', 'Sacrificial'], uncursed: true }))).errors.join(' ')).toMatch(/only curse/)
    expect(validateQuery(state(requirement({ effect: ['Annoying', 'Blazing'], uncursed: true })))).toEqual({ valid: true, errors: [] })
    expect(validateRequirement(requirement({ effect: [] }))).toEqual(['Choose at least one effect.'])
  })

  it('checks combined upgrade groups for agreement and attainability, naming the group', () => {
    const might = (patch: Partial<RequirementState>) => requirement({ kind: 'ring', item: 'ring_might', ...patch })
    expect(validateQuery(state(might({ upgradeSum: { group: 1, atLeast: 4 } }), might({ upgradeSum: { group: 1, atLeast: 4 } })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(might({ upgradeSum: { group: 1, atLeast: 2 } }), might({ upgradeSum: { group: 1, atLeast: 3 } }))).errors)
      .toEqual(['Combined upgrade group A must share one total.'])
    // An exact upgrade counts as itself, anything else as the family cap (4 for rings).
    expect(validateQuery(state(
      might({ upgradeSum: { group: 2, atLeast: 9 } }),
      might({ upgrade: { mode: 'exact', value: 3 }, upgradeSum: { group: 2, atLeast: 9 } }),
    )).errors).toEqual(['Combined upgrade group B needs +9 but its items can carry at most +7.'])
    expect(validateQuery(state(might({ upgradeSum: { group: 1, atLeast: 0 } }))).errors.join(' ')).toMatch(/at least \+1/)
    expect(validateQuery(state(might({ upgradeSum: { group: 5, atLeast: 1 } }))).errors.join(' ')).toMatch(/A through D/)
    expect(validateQuery(state(
      might({ alternativeGroup: 1, upgradeSum: { group: 1, atLeast: 2 } }),
      might({ alternativeGroup: 1 }),
    )).errors.join(' ')).toMatch(/alternative cannot/)
  })

  it('exempts alternatives of one slot from identity-group agreement', () => {
    // Alternatives of one slot may disagree (only one is ever assigned);
    // every cross-slot pair must still agree.
    expect(validateQuery(state(
      requirement({ item: 'spear', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ kind: 'armor', identityGroup: 1, alternativeGroup: 1 }),
    ))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(
      requirement({ item: 'spear', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ kind: 'weapon', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ item: 'spear', identityGroup: 1 }),
    ))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(
      requirement({ item: 'spear', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ kind: 'armor', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ item: 'spear', identityGroup: 1 }),
    )).valid).toBe(false)
    expect(validateQuery(state(
      requirement({ item: 'spear', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ item: 'sword', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ item: 'mace', identityGroup: 1 }),
    )).errors).toEqual(['Identity group 1 has incompatible category or item requirements.'])
  })
})
