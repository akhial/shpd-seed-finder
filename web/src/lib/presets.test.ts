import { describe, expect, it } from 'vite-plus/test'
import { VAULT_FLOOR_LIMIT, builtInPresets } from './store'
import { validateQuery } from './query'

describe('built-in presets', () => {
  it('ships a query the editor accepts', () => {
    for (const preset of builtInPresets) {
      expect(validateQuery(preset.query).errors, preset.name).toEqual([])
    }
  })

  it('anchors the +22 staff on a +4 wand, within the Imp floors', () => {
    const preset = builtInPresets.find((entry) => entry.name === '+22 Staff')!
    expect(preset.query.maxDepth).toBe(VAULT_FLOOR_LIMIT)
    expect(preset.query.requirements.map((requirement) => requirement.upgrade))
      .toEqual([{ mode: 'exact', value: 4 }, { mode: 'any', value: 1 }, { mode: 'any', value: 1 }, { mode: 'at_least', value: 1 }])
    expect(preset.query.requirements.map((requirement) => requirement.identityGroup)).toEqual([1, 1, 1, undefined])
  })

  it('stacks two more copies on the +5 tier-4 weapon', () => {
    const preset = builtInPresets.find((entry) => entry.name === '+26 Tier 4 Weapon')!
    expect(preset.query.maxDepth).toBe(VAULT_FLOOR_LIMIT)
    expect(preset.query.requirements).toHaveLength(3)
    expect(preset.query.requirements.every((requirement) => requirement.kind === 'weapon' && requirement.identityGroup === 1)).toBe(true)
    expect(preset.query.requirements[0].tier).toEqual({ mode: 'exact', value: 4 })
    expect(preset.query.requirements[0].upgrade).toEqual({ mode: 'exact', value: 5 })
    expect(preset.query.requirements.slice(1).map((requirement) => requirement.upgrade.mode)).toEqual(['any', 'any'])
  })
})
