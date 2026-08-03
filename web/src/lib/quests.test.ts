import { describe, expect, it } from 'vitest'
import { questLabel, questVariantLabel } from './quests'

describe('questLabel', () => {
  it.each([
    ['ghost', 'Sad Ghost'],
    ['wandmaker', 'Wandmaker'],
    ['blacksmith', 'Blacksmith'],
    ['imp', 'Imp'],
  ] as const)('labels %s as %s', (quest, label) => {
    expect(questLabel(quest)).toBe(label)
  })
})

describe('questVariantLabel', () => {
  it.each([
    ['fetid_rat', 'Fetid Rat'],
    ['gnoll_trickster', 'Gnoll Trickster'],
    ['great_crab', 'Great Crab'],
    ['corpse_dust', 'Corpse Dust'],
    ['elemental_embers', 'Elemental Embers'],
    ['rotberry', 'Rotberry'],
    ['crystal', 'Crystal Spire'],
    ['gnoll', 'Gnoll Geomancer'],
    ['monk', 'Monks'],
    ['golem', 'Golems'],
  ] as const)('labels %s as %s', (variant, label) => {
    expect(questVariantLabel(variant)).toBe(label)
  })
})
