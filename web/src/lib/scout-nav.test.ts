import { describe, expect, it } from 'vite-plus/test'
import { resultPosition, stepResult } from './scout-nav'

const seeds = ['AAA-AAA-AAA', 'BBB-BBB-BBB', 'CCC-CCC-CCC']

describe('resultPosition', () => {
  it('locates a scouted seed inside the results', () => {
    expect(resultPosition(seeds, 'AAA-AAA-AAA')).toEqual({ index: 0, total: 3 })
    expect(resultPosition(seeds, 'CCC-CCC-CCC')).toEqual({ index: 2, total: 3 })
  })

  it('is undefined for seeds outside the results', () => {
    expect(resultPosition(seeds, 'ZZZ-ZZZ-ZZZ')).toBeUndefined()
  })

  it('is undefined without a scouted seed or without results', () => {
    expect(resultPosition(seeds, undefined)).toBeUndefined()
    expect(resultPosition(seeds, '')).toBeUndefined()
    expect(resultPosition([], 'AAA-AAA-AAA')).toBeUndefined()
  })
})

describe('stepResult', () => {
  it('moves forward and backward through the results', () => {
    expect(stepResult(seeds, 'AAA-AAA-AAA', 1)).toBe('BBB-BBB-BBB')
    expect(stepResult(seeds, 'BBB-BBB-BBB', 1)).toBe('CCC-CCC-CCC')
    expect(stepResult(seeds, 'CCC-CCC-CCC', -1)).toBe('BBB-BBB-BBB')
  })

  it('does not wrap past the ends', () => {
    expect(stepResult(seeds, 'AAA-AAA-AAA', -1)).toBeUndefined()
    expect(stepResult(seeds, 'CCC-CCC-CCC', 1)).toBeUndefined()
  })

  it('clamps larger steps to the list ends', () => {
    expect(stepResult(seeds, 'BBB-BBB-BBB', 5)).toBe('CCC-CCC-CCC')
    expect(stepResult(seeds, 'BBB-BBB-BBB', -5)).toBe('AAA-AAA-AAA')
  })

  it('is inert when the current seed is not a search result', () => {
    expect(stepResult(seeds, 'ZZZ-ZZZ-ZZZ', 1)).toBeUndefined()
    expect(stepResult(seeds, undefined, 1)).toBeUndefined()
    expect(stepResult([], 'AAA-AAA-AAA', 1)).toBeUndefined()
  })

  it('is inert on a single-result list', () => {
    expect(stepResult(['AAA-AAA-AAA'], 'AAA-AAA-AAA', 1)).toBeUndefined()
    expect(stepResult(['AAA-AAA-AAA'], 'AAA-AAA-AAA', -1)).toBeUndefined()
  })
})
