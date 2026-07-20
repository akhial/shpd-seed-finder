import { describe, expect, it } from 'vitest'
import { regionForDepth } from './region'

describe('regionForDepth', () => {
  it.each([[5, 'Sewers'], [6, 'Prison'], [10, 'Prison'], [11, 'Caves'], [15, 'Caves'], [16, 'Dwarven City'], [20, 'Dwarven City'], [21, 'Demon Halls']])('maps floor %i to %s', (depth, region) => {
    expect(regionForDepth(depth).name).toBe(region)
  })
})
