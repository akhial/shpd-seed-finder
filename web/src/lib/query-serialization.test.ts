import { describe, expect, it } from 'vitest'
import { defaultQueryState, fromQueryJson, toQueryJson } from './query'
import type { QueryState } from './wasm/types'

describe('query serialization', () => {
  it('omits query and requirement defaults', () => {
    expect(toQueryJson({ ...defaultQueryState(), requirements: [{ kind: 'wand', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'any', value: 1 }, uncursed: false }] }))
      .toBe('{"requirements":[{"kind":"wand"}]}')
  })

  it('emits tier and upgrade wire forms exactly', () => {
    const state = { ...defaultQueryState(), requirements: [
      { kind: 'armor' as const, tier: { mode: 'at_least' as const, value: 4 }, upgrade: { mode: 'at_least' as const, value: 2 }, uncursed: false },
      { kind: 'ring' as const, item: 'ring_haste', tier: { mode: 'any' as const, value: 3 }, upgrade: { mode: 'exact' as const, value: 4 }, uncursed: false },
    ], challenges: ['on_diet' as const, 'into_darkness' as const] }
    expect(JSON.parse(toQueryJson(state))).toEqual({
      requirements: [
        { kind: 'armor', tier: { at_least: 4 }, upgrade: { at_least: 2 } },
        { kind: 'ring', item: 'ring_haste', upgrade: 4 },
      ],
      challenges: ['on_diet', 'into_darkness'],
    })
  })

  it('round-trips a fully loaded state', () => {
    const state: QueryState = {
      requirements: [{
        kind: 'weapon', item: undefined, tier: { mode: 'at_most', value: 4 }, upgrade: { mode: 'exact', value: 3 },
        effect: { mode: 'one_of', names: ['Blazing'] }, uncursed: false, source: 'locked_chest', identityGroup: 2, maxDepth: 8,
      }],
      maxDepth: 19, requireBlacksmith: true, excludeBlacksmithRewards: true, fastMode: true,
      challenges: ['faith_is_my_armor', 'hostile_champions'],
    }
    expect(fromQueryJson(toQueryJson(state))).toEqual(state)
  })

  it('emits effect predicates in every wire form', () => {
    const requirement = (effect: QueryState['requirements'][number]['effect']) => ({
      kind: 'weapon' as const, tier: { mode: 'any' as const, value: 3 }, upgrade: { mode: 'any' as const, value: 1 }, uncursed: false, effect,
    })
    const state = { ...defaultQueryState(), requirements: [
      requirement({ mode: 'one_of', names: ['Blazing'] }),
      requirement({ mode: 'one_of', names: ['Blocking', 'Projecting', 'Vampiric'] }),
      requirement({ mode: 'any_enchantment' }),
    ] }
    expect(JSON.parse(toQueryJson(state)).requirements).toEqual([
      { kind: 'weapon', effect: 'Blazing' },
      { kind: 'weapon', effect: ['Blocking', 'Projecting', 'Vampiric'] },
      { kind: 'weapon', effect: 'any_enchantment' },
    ])
    expect(fromQueryJson(toQueryJson(state)).requirements).toEqual(state.requirements)
  })

  it('serializes alternative groups as any_of entries and back', () => {
    const alternative = (item: string, upgrade: number) => ({
      kind: 'weapon' as const, item, tier: { mode: 'any' as const, value: 3 },
      upgrade: { mode: 'exact' as const, value: upgrade }, uncursed: false, alternativeGroup: 7,
    })
    const plain = { kind: 'wand' as const, tier: { mode: 'any' as const, value: 3 }, upgrade: { mode: 'any' as const, value: 1 }, uncursed: false }
    const state = { ...defaultQueryState(), requirements: [alternative('spear', 3), alternative('shuriken', 2), plain] }
    expect(JSON.parse(toQueryJson(state)).requirements).toEqual([
      { any_of: [{ kind: 'weapon', item: 'spear', upgrade: 3 }, { kind: 'weapon', item: 'shuriken', upgrade: 2 }] },
      { kind: 'wand' },
    ])
    // Group numbers are renumbered from 1 on load; structure survives.
    const loaded = fromQueryJson(toQueryJson(state))
    expect(loaded.requirements.map((r) => r.alternativeGroup)).toEqual([1, 1, undefined])
    expect(loaded.requirements[0].item).toBe('spear')
  })

  it('round-trips combined upgrade totals', () => {
    const ring = {
      kind: 'ring' as const, item: 'ring_might', tier: { mode: 'any' as const, value: 3 },
      upgrade: { mode: 'any' as const, value: 1 }, uncursed: false, identityGroup: 1,
      upgradeSum: { group: 1, atLeast: 2 },
    }
    const state = { ...defaultQueryState(), requirements: [ring, { ...ring }] }
    expect(JSON.parse(toQueryJson(state)).requirements[0].upgrade_sum).toEqual({ group: 1, at_least: 2 })
    expect(fromQueryJson(toQueryJson(state)).requirements).toEqual(state.requirements)
  })
})
