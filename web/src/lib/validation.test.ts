import { describe, expect, it } from 'vite-plus/test'
import { clampUpgrade, defaultQueryState, validateQuery, validateRequirement } from './query'
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
  it('accepts the v4.0.0 ceilings and rejects one step above each', () => {
    // The Imp's vault reaches +5 on weapons and +4 on everything else.
    const upgrade = (kind: RequirementState['kind'], value: number) =>
      validateQuery(state(requirement({ kind, upgrade: { mode: 'exact', value } }))).valid
    expect([upgrade('weapon', 5), upgrade('melee_weapon', 5), upgrade('thrown_weapon', 5)]).toEqual([true, true, true])
    expect([upgrade('armor', 4), upgrade('wand', 4), upgrade('ring', 4)]).toEqual([true, true, true])
    expect([upgrade('weapon', 6), upgrade('armor', 5), upgrade('wand', 5)]).toEqual([false, false, false])
    // An "at least" bound answers to the same ceiling.
    expect(validateQuery(state(requirement({ upgrade: { mode: 'at_least', value: 5 } }))).valid).toBe(true)
    expect(validateQuery(state(requirement({ kind: 'armor', upgrade: { mode: 'at_least', value: 5 } }))).errors.join(' ')).toMatch(/0 through \+4/)
  })
  it('reserves the top weapon upgrade for the tier that reaches it', () => {
    // The vault levels its tier-4 weapon past every other prize, so a +5 is
    // only meaningful while tier 4 is still in reach.
    const plus5 = (patch: Partial<RequirementState>) =>
      validateQuery(state(requirement({ upgrade: { mode: 'exact', value: 5 }, ...patch }))).valid
    expect(plus5({})).toBe(true)
    expect(plus5({ tier: { mode: 'exact', value: 4 } })).toBe(true)
    expect(plus5({ tier: { mode: 'at_least', value: 4 } })).toBe(true)
    expect(plus5({ item: 'battle_axe' })).toBe(true)
    expect(plus5({ item: 'javelin' })).toBe(true)
    expect(plus5({ tier: { mode: 'exact', value: 5 } })).toBe(false)
    expect(plus5({ tier: { mode: 'at_most', value: 3 } })).toBe(false)
    expect(plus5({ item: 'sword' })).toBe(false)
    expect(validateQuery(state(requirement({ item: 'sword', upgrade: { mode: 'exact', value: 5 } }))).errors.join(' '))
      .toMatch(/only a tier-4 weapon/)
  })

  it('pulls an out-of-reach upgrade back when the tier narrows', () => {
    const plus5 = requirement({ upgrade: { mode: 'exact', value: 5 } })
    expect(clampUpgrade(plus5)).toBe(plus5)
    expect(clampUpgrade({ ...plus5, tier: { mode: 'exact', value: 5 } }).upgrade).toEqual({ mode: 'exact', value: 4 })
    expect(clampUpgrade({ ...plus5, item: 'sword' }).upgrade).toEqual({ mode: 'exact', value: 4 })
    expect(clampUpgrade({ ...plus5, item: 'battle_axe' }).upgrade).toEqual({ mode: 'exact', value: 5 })
    // An "at least" bound stops one below the ceiling: the top level is what
    // "exactly" already says.
    const atLeast4 = requirement({ upgrade: { mode: 'at_least', value: 4 } })
    expect(clampUpgrade({ ...atLeast4, tier: { mode: 'exact', value: 5 } }).upgrade).toEqual({ mode: 'at_least', value: 3 })
  })

  it('accepts the enchantments v4.0.0 added on a weapon and refuses them on armor', () => {
    expect(validateQuery(state(requirement({ effect: ['Venomous', 'Eldritch', 'Vorpal', 'Crystal'] })))).toEqual({ valid: true, errors: [] })
    expect(validateQuery(state(requirement({ effect: ['Pressurized', 'Wondrous'], uncursed: true }))).errors.join(' ')).toMatch(/only curse/)
    expect(validateQuery(state(requirement({ kind: 'armor', effect: 'Vorpal' }))).errors.join(' ')).toMatch(/Vorpal does not belong/)
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
