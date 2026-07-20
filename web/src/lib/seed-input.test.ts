import { describe, expect, it } from 'vitest'
import { formatSeedInput } from './format'

describe('seed input formatting', () => {
  it('uppercases partial input', () => expect(formatSeedInput('abc')).toBe('ABC'))
  it('groups and limits a full seed to nine letters', () => expect(formatSeedInput('abc def ghi extra')).toBe('ABC-DEF-GHI'))
})
