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

  it('serializes and round-trips melee and thrown weapon kinds', () => {
    const state: QueryState = { ...defaultQueryState(), requirements: [
      { kind: 'melee_weapon', tier: { mode: 'exact', value: 5 }, upgrade: { mode: 'any', value: 1 }, uncursed: false },
      { kind: 'thrown_weapon', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'any', value: 1 }, uncursed: false },
      { kind: 'thrown_weapon', item: 'shuriken', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'any', value: 1 }, uncursed: false },
    ] }
    expect(JSON.parse(toQueryJson(state))).toEqual({
      requirements: [
        { kind: 'melee_weapon', tier: { exact: 5 } },
        { kind: 'thrown_weapon' },
        { kind: 'thrown_weapon', item: 'shuriken' },
      ],
    })
    expect(fromQueryJson(toQueryJson(state))).toEqual(state)
    // Pre-existing documents with a plain weapon kind keep decoding unchanged.
    expect(fromQueryJson('{"requirements":[{"kind":"weapon"}]}').requirements[0].kind).toBe('weapon')
  })

  it('round-trips a fully loaded state', () => {
    const state: QueryState = {
      requirements: [{
        kind: 'weapon', item: undefined, tier: { mode: 'at_most', value: 4 }, upgrade: { mode: 'exact', value: 3 },
        effect: 'Blazing', uncursed: false, source: 'locked_chest', identityGroup: 2, maxDepth: 8,
      }],
      maxDepth: 19, requireBlacksmith: true, excludeBlacksmithRewards: true, fastMode: true,
      challenges: ['faith_is_my_armor', 'hostile_champions'],
    }
    expect(fromQueryJson(toQueryJson(state))).toEqual(state)
  })
})
