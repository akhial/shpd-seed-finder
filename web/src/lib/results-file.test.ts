import { describe, expect, it } from 'vitest'
import { defaultQueryState } from './query'
import {
  RESULTS_FILE_VERSION,
  decodeResultsFile,
  encodeResultsFile,
  parsedSeedFromCode,
  seedCodeValue,
} from './results-file'
import type { QueryState } from './wasm/types'

const loadedQuery: QueryState = {
  requirements: [
    {
      kind: 'ring',
      item: 'ring_tenacity',
      tier: { mode: 'any', value: 3 },
      upgrade: { mode: 'exact', value: 4 },
      uncursed: false,
      source: 'imp_reward',
    },
    {
      kind: 'wand',
      tier: { mode: 'any', value: 3 },
      upgrade: { mode: 'at_least', value: 2 },
      uncursed: true,
      identityGroup: 1,
      maxDepth: 9,
    },
  ],
  maxDepth: 12,
  requireBlacksmith: true,
  excludeBlacksmithRewards: false,
  fastMode: false,
  challenges: ['barren_land'],
}

/**
 * The canonical version-1 fixture, byte-for-byte the same schema as
 * crates/seedfinder-core/tests/fixtures/results-export-v1.json. Files
 * exported today must always stay readable; never edit this fixture.
 */
const VERSION_1_FIXTURE = `{
  "format": "seed-seeker-results",
  "format_version": 1,
  "app_version": "0.6.1",
  "shpd_version": "3.3.8",
  "query": {
    "requirements": [
      {
        "kind": "ring",
        "item": "ring_tenacity",
        "upgrade": 4,
        "source": "imp_reward"
      },
      {
        "kind": "wand",
        "upgrade": { "at_least": 2 },
        "uncursed": true,
        "identity_group": 1,
        "max_depth": 9
      }
    ],
    "max_depth": 12,
    "require_blacksmith": true,
    "challenges": ["barren_land"]
  },
  "results": [
    { "seed": "AAA-AAA-BUH" },
    { "seed": "ABC-DEF-GHI" }
  ]
}`

describe('results file', () => {
  it('computes seed values in the engine base-26 form', () => {
    expect(seedCodeValue('AAA-AAA-AAA')).toBe(0)
    expect(seedCodeValue('AAA-AAA-AAB')).toBe(1)
    expect(seedCodeValue('ZZZ-ZZZ-ZZZ')).toBe(26 ** 9 - 1)
    expect(parsedSeedFromCode('AAA-AAA-AAB')).toEqual({ code: 'AAA-AAA-AAB', value: 1 })
  })

  it('round-trips the query and seeds through encode and decode', () => {
    const text = encodeResultsFile(loadedQuery, ['AAA-AAA-BUH', 'ABC-DEF-GHI'], '3.3.8')
    const decoded = decodeResultsFile(text)
    expect(decoded.formatVersion).toBe(RESULTS_FILE_VERSION)
    expect(decoded.appVersion).toBeDefined()
    expect(decoded.query).toEqual(loadedQuery)
    expect(decoded.seeds).toEqual(['AAA-AAA-BUH', 'ABC-DEF-GHI'])
  })

  it('emits the documented envelope fields', () => {
    const parsed = JSON.parse(encodeResultsFile(loadedQuery, ['AAA-AAA-AAB'], '3.3.8')) as Record<string, unknown>
    expect(parsed.format).toBe('seed-seeker-results')
    expect(parsed.format_version).toBe(1)
    expect(typeof parsed.app_version).toBe('string')
    expect(parsed.shpd_version).toBe('3.3.8')
    expect(parsed.results).toEqual([{ seed: 'AAA-AAA-AAB' }])
    expect(parsed.query).toEqual({
      requirements: [
        { kind: 'ring', item: 'ring_tenacity', upgrade: 4, source: 'imp_reward' },
        { kind: 'wand', upgrade: { at_least: 2 }, uncursed: true, identity_group: 1, max_depth: 9 },
      ],
      max_depth: 12,
      require_blacksmith: true,
      challenges: ['barren_land'],
    })
  })

  it('always decodes the frozen version-1 fixture', () => {
    const decoded = decodeResultsFile(VERSION_1_FIXTURE)
    expect(decoded.formatVersion).toBe(1)
    expect(decoded.appVersion).toBe('0.6.1')
    expect(decoded.query).toEqual(loadedQuery)
    expect(decoded.seeds).toEqual(['AAA-AAA-BUH', 'ABC-DEF-GHI'])
  })

  it('ignores unknown envelope and per-result fields from future releases', () => {
    const decoded = decodeResultsFile(JSON.stringify({
      format: 'seed-seeker-results',
      format_version: 1,
      exported_at: '2031-01-01T00:00:00Z',
      future_minor_field: { nested: true },
      query: { requirements: [{ item: 'sword' }] },
      results: [{ seed: 'AAA-AAA-AAB', future_note: 'still fine' }],
    }))
    expect(decoded.seeds).toEqual(['AAA-AAA-AAB'])
    expect(decoded.query.maxDepth).toBe(24)
  })

  it('rejects files from a newer format version with an update message', () => {
    const text = JSON.stringify({
      format: 'seed-seeker-results',
      format_version: 2,
      query: { requirements: [] },
      results: [],
    })
    expect(() => decodeResultsFile(text)).toThrowError(/format version 2.*Update Seed Seeker/s)
  })

  it('rejects foreign and malformed files clearly', () => {
    for (const text of ['not json', '[]', '{}', '{"format":"other"}']) {
      expect(() => decodeResultsFile(text)).toThrowError(/not a Seed Seeker results file/)
    }
    expect(() =>
      decodeResultsFile(JSON.stringify({ format: 'seed-seeker-results', query: {}, results: [] })),
    ).toThrowError(/format version/)
  })

  it('rejects invalid seed codes and names the offending result', () => {
    const text = JSON.stringify({
      format: 'seed-seeker-results',
      format_version: 1,
      query: { requirements: [] },
      results: [{ seed: 'AAA-AAA-AAB' }, { seed: 'AAA-AAA-AA0' }],
    })
    expect(() => decodeResultsFile(text)).toThrowError(/Result 2/)
  })

  it('round-trips a default query with no results', () => {
    const decoded = decodeResultsFile(encodeResultsFile(defaultQueryState(), [], undefined))
    expect(decoded.query).toEqual(defaultQueryState())
    expect(decoded.seeds).toEqual([])
  })
})
