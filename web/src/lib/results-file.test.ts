import { describe, expect, it } from 'vitest'
// The canonical frozen fixture, imported verbatim from the Rust core's test
// data so this codec can never silently drift from it.
import VERSION_1_FIXTURE from '../../../crates/seedfinder-core/tests/fixtures/results-export-v1.json?raw'
import WEAPON_CATEGORIES_FIXTURE from '../../../crates/seedfinder-core/tests/fixtures/results-export-v1-weapon-categories.json?raw'
import { defaultQueryState, toQueryDocument } from './query'
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

const file = (query: unknown, results: unknown[] = []) =>
  JSON.stringify({ format: 'seed-seeker-results', format_version: 1, query, results })

describe('results file', () => {
  it('computes seed values in the engine base-26 form', () => {
    expect(seedCodeValue('AAA-AAA-AAA')).toBe(0)
    expect(seedCodeValue('AAA-AAA-AAB')).toBe(1)
    expect(seedCodeValue('ZZZ-ZZZ-ZZZ')).toBe(26 ** 9 - 1)
    expect(parsedSeedFromCode('AAA-AAA-AAB')).toEqual({ code: 'AAA-AAA-AAB', value: 1 })
  })

  it('round-trips the query and seeds through encode and decode', () => {
    const text = encodeResultsFile(toQueryDocument(loadedQuery), ['AAA-AAA-BUH', 'ABC-DEF-GHI'], '3.3.8')
    const decoded = decodeResultsFile(text)
    expect(decoded.formatVersion).toBe(RESULTS_FILE_VERSION)
    expect(decoded.appVersion).toBeDefined()
    expect(decoded.shpdVersion).toBe('3.3.8')
    expect(decoded.query).toEqual(loadedQuery)
    expect(decoded.seeds).toEqual(['AAA-AAA-BUH', 'ABC-DEF-GHI'])
  })

  it('emits the documented envelope fields', () => {
    const parsed = JSON.parse(encodeResultsFile(toQueryDocument(loadedQuery), ['AAA-AAA-AAB'], '3.3.8')) as Record<string, unknown>
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

  it('always decodes the canonical frozen version-1 fixture', () => {
    const decoded = decodeResultsFile(VERSION_1_FIXTURE)
    expect(decoded.formatVersion).toBe(1)
    expect(decoded.appVersion).toBe('0.6.1')
    expect(decoded.shpdVersion).toBe('3.3.8')
    expect(decoded.query).toEqual(loadedQuery)
    expect(decoded.seeds).toEqual(['AAA-AAA-BUH', 'ABC-DEF-GHI'])
  })

  it('accepts the narrowed weapon kinds and keeps them through a round-trip', () => {
    // "melee_weapon"/"thrown_weapon" are additive within format version 1;
    // widening them to "weapon" on either side would silently change the query.
    const decoded = decodeResultsFile(WEAPON_CATEGORIES_FIXTURE)
    expect(decoded.query.requirements.map((requirement) => requirement.kind))
      .toEqual(['thrown_weapon', 'melee_weapon', 'weapon'])
    expect(decoded.query.requirements[1].item).toBe('sword')
    expect(decoded.seeds).toEqual(['AAA-AAA-ACO'])
    const reEncoded = decodeResultsFile(encodeResultsFile(decoded.queryDocument, decoded.seeds, '3.3.8'))
    expect(reEncoded.query).toEqual(decoded.query)
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
      query: { requirements: [{ item: 'sword' }] },
      results: [],
    })
    expect(() => decodeResultsFile(text)).toThrowError(/format version 2.*Update Seed Seeker/s)
  })

  it('requires the format version to be a positive whole number', () => {
    for (const version of [0, 1.5, true, '1', -1, null]) {
      const text = JSON.stringify({
        format: 'seed-seeker-results',
        format_version: version,
        query: { requirements: [{ item: 'sword' }] },
        results: [],
      })
      expect(() => decodeResultsFile(text), JSON.stringify(version)).toThrowError(/format version/)
    }
  })

  it('rejects foreign and malformed files clearly', () => {
    for (const text of ['not json', '[]', '{}', '{"format":"other"}']) {
      expect(() => decodeResultsFile(text)).toThrowError(/not a Seed Seeker results file/)
    }
    expect(() =>
      decodeResultsFile(JSON.stringify({ format: 'seed-seeker-results', query: { requirements: [{ item: 'sword' }] }, results: [] })),
    ).toThrowError(/format version/)
  })

  it('accepts all core tier and upgrade forms', () => {
    const decoded = decodeResultsFile(file({
      requirements: [
        { kind: 'weapon', tier: 'any', upgrade: 'any' },
        { kind: 'weapon', tier: { exact: 2 }, upgrade: { exact: 3 } },
        { kind: 'armor', tier: { at_least: 3 }, upgrade: { at_least: 1 } },
        { kind: 'armor', tier: { at_most: 4 }, effect: 'anti-magic' },
      ],
    }))
    const requirements = decoded.query.requirements
    expect(requirements[0].tier.mode).toBe('any')
    expect(requirements[0].upgrade.mode).toBe('any')
    expect(requirements[1].tier).toEqual({ mode: 'exact', value: 2 })
    expect(requirements[1].upgrade).toEqual({ mode: 'exact', value: 3 })
    expect(requirements[2].tier).toEqual({ mode: 'at_least', value: 3 })
    expect(requirements[2].upgrade).toEqual({ mode: 'at_least', value: 1 })
    expect(requirements[3].tier).toEqual({ mode: 'at_most', value: 4 })
  })

  it('rejects unknown query content instead of changing its meaning', () => {
    expect(() => decodeResultsFile(file({ requirements: [{ item: 'item_from_the_future' }] })))
      .toThrowError(/item_from_the_future/)
    expect(() => decodeResultsFile(file({ requirements: [{ item: 'sword' }], wished_luck: 7 })))
      .toThrowError(/wished_luck/)
    expect(() => decodeResultsFile(file({ requirements: [{ item: 'sword', teleports: true }] })))
      .toThrowError(/teleports/)
    expect(() => decodeResultsFile(file({ requirements: [{ kind: 'RING' }] })))
      .toThrowError(/unknown category/)
    expect(() => decodeResultsFile(file({ requirements: [{ kind: 'weapon', effect: 'Sparkling' }] })))
      .toThrowError(/Sparkling/)
    expect(() => decodeResultsFile(file({ requirements: [{ kind: 'wand', identity_group: 5 }] })))
      .toThrowError(/same-item group/)
    expect(() => decodeResultsFile(file({ requirements: [{ kind: 'weapon', upgrade: {} }] })))
      .toThrowError(/upgrade/)
  })

  it('rejects wrong-typed query fields instead of coercing or dropping them', () => {
    expect(() => decodeResultsFile(file({ requirements: [{ item: 'sword' }], max_depth: '12' })))
      .toThrowError(/max_depth/)
    expect(() => decodeResultsFile(file({ requirements: [{ item: 42 }] })))
      .toThrowError(/item/)
    expect(() => decodeResultsFile(file({ requirements: [{ item: 'sword' }], challenges: 'barren_land' })))
      .toThrowError(/challenges/)
    expect(() => decodeResultsFile(file({ requirements: [{ item: 'sword', upgrade: true }] })))
      .toThrowError(/upgrade/)
    expect(() => decodeResultsFile(file({ requirements: [{ item: 'sword', uncursed: 'yes' }] })))
      .toThrowError(/uncursed/)
  })

  it('rejects non-canonical seed codes so files behave the same on every platform', () => {
    for (const seed of ['aaa-aaa-aab', 'AAAAAAAAB', 'AAA AAA AAB', ' AAA-AAA-AAB']) {
      expect(() => decodeResultsFile(file({ requirements: [{ item: 'sword' }] }, [{ seed }])), seed)
        .toThrowError(/Result 1/)
    }
    expect(() => decodeResultsFile(file({ requirements: [{ item: 'sword' }] }, [{ seed: 'AAA-AAA-AAB' }, { seed: 'AAA-AAA-AA0' }])))
      .toThrowError(/Result 2/)
  })

  it('round-trips a minimal query with no results', () => {
    const query: QueryState = { ...defaultQueryState(), requirements: [{ kind: 'wand', tier: { mode: 'any', value: 3 }, upgrade: { mode: 'any', value: 1 }, uncursed: false }] }
    const decoded = decodeResultsFile(encodeResultsFile(toQueryDocument(query), [], '3.3.8'))
    expect(decoded.query).toEqual(query)
    expect(decoded.seeds).toEqual([])
  })
})
