import { describe, expect, it } from 'vite-plus/test'
import { slotCount } from '../../lib/query'
import type { RequirementState } from '../../lib/wasm/types'
import { alternativesTitle, effectLabel, requirementDetails } from './summary'

const requirement = (patch: Partial<RequirementState> = {}): RequirementState => ({
  kind: 'weapon', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'any', value: 1 }, uncursed: false, ...patch,
})

describe('requirement summary text', () => {
  it('describes effect filters', () => {
    expect(effectLabel(requirement())).toBeUndefined()
    expect(effectLabel(requirement({ effect: 'Blazing' }))).toBe('Blazing')
    expect(effectLabel(requirement({ effect: ['Blocking', 'Projecting'] }))).toBe('effect: Blocking/Projecting')
    expect(effectLabel(requirement({ effect: 'any_enchantment' }))).toBe('any enchantment')
    expect(effectLabel(requirement({ kind: 'armor', effect: 'any_enchantment' }))).toBe('any glyph')
  })

  it('lists the combined level and the stack', () => {
    expect(requirementDetails(requirement({ kind: 'ring', item: 'ring_might', levelSum: { group: 1, atLeast: 4 } })))
      .toEqual(['levels ≥ 4 together'])
    expect(requirementDetails(requirement({ upgrade: { mode: 'exact', value: 2 }, effect: ['Blocking', 'Vampiric'], identityGroup: 2 })))
      .toEqual(['exactly +2', 'effect: Blocking/Vampiric', 'same-kind stack'])
  })

  it('titles "any of these" cards and counts slots', () => {
    expect(alternativesTitle(3)).toBe('Any of 3')
    expect(slotCount([
      requirement({ item: 'spear', alternativeGroup: 1 }),
      requirement({ item: 'shuriken', alternativeGroup: 1 }),
      requirement({ item: 'sword', alternativeGroup: 1 }),
      requirement({ kind: 'wand' }),
    ])).toBe(2)
  })
})
