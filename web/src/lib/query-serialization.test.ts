import { describe, expect, it } from 'vitest'
import { FLOOR_LIMIT_OPTIONS, defaultQueryState, fromQueryJson, normalizeFloorLimit, toQueryJson } from './query'
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

  it('snaps stored empty boss-floor limits to the equivalent floor below', () => {
    const state = fromQueryJson('{"requirements":[{"kind":"wand","max_depth":5},{"kind":"ring","max_depth":10}],"max_depth":15}')
    expect(state.maxDepth).toBe(14)
    expect(state.requirements.map((requirement) => requirement.maxDepth)).toEqual([4, 9])
  })

  it('offers every floor except the empty boss floors as a limit', () => {
    expect(FLOOR_LIMIT_OPTIONS).toHaveLength(21)
    expect(FLOOR_LIMIT_OPTIONS).not.toContain(5)
    expect(FLOOR_LIMIT_OPTIONS).not.toContain(10)
    expect(FLOOR_LIMIT_OPTIONS).not.toContain(15)
    expect(FLOOR_LIMIT_OPTIONS).toContain(20)
    expect(FLOOR_LIMIT_OPTIONS).toContain(24)
    expect([4, 5, 9, 10, 14, 15, 20, 24].map(normalizeFloorLimit)).toEqual([4, 4, 9, 9, 14, 14, 20, 24])
  })
})
