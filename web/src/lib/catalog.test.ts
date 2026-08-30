import { describe, expect, it } from 'vitest'
import {
  curseNamesForCategory,
  effectNamesForCategory,
  enchantmentNamesForCategory,
  isCurseForCategory,
  sourceLabel,
  sources,
} from './catalog'

/**
 * The effect tables and the item list come from the generated catalog, so
 * these check that the app offers what the engine's game version generates
 * rather than restating either.
 */
describe('effect tables', () => {
  it('offers the enchantments v4.0.0 added, for every weapon kind', () => {
    for (const kind of ['weapon', 'melee_weapon', 'thrown_weapon']) {
      expect(enchantmentNamesForCategory(kind)).toEqual(expect.arrayContaining(['Venomous', 'Eldritch', 'Vorpal', 'Crystal']))
    }
  })

  it('offers the curses v4.0.0 added and classifies them as curses', () => {
    expect(curseNamesForCategory('weapon')).toEqual(expect.arrayContaining(['Pressurized', 'Wondrous']))
    for (const name of ['Pressurized', 'Wondrous']) expect(isCurseForCategory('weapon', name)).toBe(true)
    for (const name of ['Venomous', 'Eldritch', 'Vorpal', 'Crystal']) expect(isCurseForCategory('weapon', name)).toBe(false)
  })

  it('counts 17 weapon enchantments and 10 weapon curses', () => {
    expect(enchantmentNamesForCategory('weapon')).toHaveLength(17)
    expect(curseNamesForCategory('weapon')).toHaveLength(10)
    expect(effectNamesForCategory('weapon')).toHaveLength(27)
    // The armor tables are unchanged by v4.0.0.
    expect(effectNamesForCategory('armor')).toHaveLength(21)
  })
})

describe('item sources', () => {
  it('labels the vault treasure source v4.0.0 added', () => {
    expect(sourceLabel('vault_treasure')).toBe('Vault Treasure')
    expect(sources.map((source) => source.value)).toContain('vault_treasure')
  })
})
