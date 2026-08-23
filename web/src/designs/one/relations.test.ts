import { describe, expect, it } from 'vitest'
import { fromQueryJson, toQueryDocument, validateQuery } from '../../lib/query'
import type { RequirementState } from '../../lib/wasm/types'
import { boardItems, detach, joinAlternatives, joinBundle, linkIdentity, removeAt, setBundleTotal, unlinkIdentity } from './relations'

const req = (patch: Partial<RequirementState> = {}): RequirementState => ({
  kind: 'weapon', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'any', value: 1 }, uncursed: false, ...patch,
})

const names = (requirements: RequirementState[]) => requirements.map((r) => r.item ?? r.kind)

describe('either/or clusters', () => {
  it('dropping a chip on another makes one any_of slot, placed after the target', () => {
    const base = [req({ item: 'spear' }), req({ kind: 'armor' }), req({ item: 'shuriken' })]
    const next = joinAlternatives(base, 2, 0)
    expect(names(next)).toEqual(['spear', 'shuriken', 'armor'])
    expect(next[0].alternativeGroup).toBe(next[1].alternativeGroup)
    expect(boardItems(next)).toEqual([{ type: 'alternatives', group: 1, members: [0, 1] }, { type: 'chip', index: 2 }])
    expect(toQueryDocument({ ...fromQueryJson('{"requirements":[]}'), requirements: next }).requirements[0]).toHaveProperty('any_of')
  })

  it('joining a cluster drops a bundle membership, and leaving a pair dissolves it', () => {
    const base = [req({ item: 'spear', upgradeSum: { group: 1, atLeast: 2 } }), req({ item: 'mace', upgradeSum: { group: 1, atLeast: 2 } }), req({ item: 'shuriken' })]
    const next = joinAlternatives(base, 0, 2)
    expect(next.every((r) => r.upgradeSum === undefined)).toBe(true)
    expect(next.filter((r) => r.alternativeGroup !== undefined).map((r) => r.item)).toEqual(['shuriken', 'spear'])
    const out = detach(next, 1)
    expect(out.every((r) => r.alternativeGroup === undefined)).toBe(true)
  })
})

describe('same-item tethers', () => {
  it('links two chips with the first free group and keeps them in place', () => {
    const base = [req({ item: 'spear' }), req({ kind: 'armor' }), req({ kind: 'melee_weapon' })]
    const next = linkIdentity(base, 2, 0)!
    expect(names(next)).toEqual(['spear', 'armor', 'melee_weapon'])
    expect(next[0].identityGroup).toBe(1)
    expect(next[2].identityGroup).toBe(1)
    expect(validateQuery({ ...fromQueryJson('{"requirements":[]}'), requirements: next }).valid).toBe(true)
    expect(unlinkIdentity(next, 0).every((r) => r.identityGroup === undefined)).toBe(true)
  })

  it('adopts the target group and refuses when all four groups are taken', () => {
    const base = [1, 1, 2, 2, 3, 3, 4, 4].map((group) => req({ identityGroup: group }))
    expect(linkIdentity([...base, req()], 8, 0)![8].identityGroup).toBe(1)
    expect(linkIdentity([...base, req(), req()], 8, 9)).toBeUndefined()
  })
})

describe('Σ bundles', () => {
  it('forms a bundle with a reachable shared total and adopts an existing total', () => {
    const base = [req({ kind: 'ring' }), req({ kind: 'ring' }), req({ item: 'spear', upgrade: { mode: 'exact', value: 2 } })]
    const bundled = joinBundle(base, 1, 0)!
    expect(bundled[0].upgradeSum).toEqual({ group: 1, atLeast: 4 })
    expect(bundled[1].upgradeSum).toEqual({ group: 1, atLeast: 4 })
    const raised = setBundleTotal(bundled, 1, 6)
    const three = joinBundle(raised, 2, 0)!
    expect(three.map((r) => r.upgradeSum?.atLeast)).toEqual([6, 6, 6])
    expect(boardItems(three)).toEqual([{ type: 'bundle', group: 1, atLeast: 6, members: [0, 1, 2] }])
    expect(validateQuery({ ...fromQueryJson('{"requirements":[]}'), requirements: three }).valid).toBe(true)
  })

  it('removing a member collapses a bundle of one', () => {
    const base = [req({ kind: 'ring', upgradeSum: { group: 2, atLeast: 3 } }), req({ kind: 'ring', upgradeSum: { group: 2, atLeast: 3 } })]
    expect(removeAt(base, 0)).toEqual([req({ kind: 'ring' })])
  })
})
