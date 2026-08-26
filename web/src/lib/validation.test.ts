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
    expect(validateQuery(state(requirement({ identityGroup: 1 }), requirement({ kind: 'armor', identityGroup: 1 }))).errors.join(' ')).toMatch(/share its category/)
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
      requirement({ identityGroup: 1 }),
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

  it('checks combined levels for agreement and attainability', () => {
    const might = (patch: Partial<RequirementState>) => requirement({ kind: 'ring', item: 'ring_might', ...patch })
    expect(validateQuery(state(might({ levelSum: { group: 1, atLeast: 4 } }), might({ levelSum: { group: 1, atLeast: 4 } })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(might({ levelSum: { group: 1, atLeast: 2 } }), might({ levelSum: { group: 1, atLeast: 3 } }))).errors)
      .toEqual(['A stack must share one combined level.'])
    // An item counts its upgrade plus one: an exact +3 ring reaches 4 levels,
    // an open one 5, so the pair can reach 9 together.
    expect(validateQuery(state(
      might({ levelSum: { group: 2, atLeast: 10 } }),
      might({ upgrade: { mode: 'exact', value: 3 }, levelSum: { group: 2, atLeast: 10 } }),
    )).errors).toEqual(['A combined level of 10 needs more items: these 2 can reach 9.'])
    expect(validateQuery(state(might({ levelSum: { group: 1, atLeast: 0 } }))).errors.join(' ')).toMatch(/at least 1/)
    expect(validateQuery(state(might({ levelSum: { group: 5, atLeast: 1 } }))).errors.join(' ')).toMatch(/1 through 4/)
    expect(validateQuery(state(
      might({ alternativeGroup: 1, levelSum: { group: 1, atLeast: 2 } }),
      might({ alternativeGroup: 1 }),
    )).errors.join(' ')).toMatch(/alternative cannot/)
  })

  it('checks stacks: one category, one constrained anchor unit', () => {
    // An either/or cluster may anchor a stack; its bare copy follows
    // whichever member matched.
    expect(validateQuery(state(
      requirement({ item: 'spear', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ item: 'sword', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ identityGroup: 1 }),
    ))).toEqual({ valid: true, errors: [] })
    // Copies of another category can never be the same item.
    expect(validateQuery(state(
      requirement({ item: 'spear', identityGroup: 1 }),
      requirement({ kind: 'armor', identityGroup: 1 }),
    )).errors).toEqual(['The copies of a stack must share its category.'])
    // A second constrained unit would describe two different items forced
    // to be the same one.
    expect(validateQuery(state(
      requirement({ item: 'spear', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ item: 'sword', identityGroup: 1, alternativeGroup: 1 }),
      requirement({ item: 'mace', identityGroup: 1 }),
    )).errors).toEqual(['Only one item of a stack can carry constraints; the extra copies are plain.'])
  })
})
