import { readFile } from 'node:fs/promises'
import { beforeAll, describe, expect, it } from 'vitest'
import { defaultQueryState, toQueryJson } from './query'
import { hasShareCode, withoutFragment } from './share-link'
import init, { decode_share_text, encode_share_link } from './wasm/pkg/seedfinder.js'
import type { QueryState } from './wasm/types'

describe('share link fragments', () => {
  it('detects share codes in fragments', () => {
    expect(hasShareCode('#q=EAGWhMA')).toBe(true)
    expect(hasShareCode('#other&q=EAGWhMA')).toBe(true)
    expect(hasShareCode('')).toBe(false)
    expect(hasShareCode('#')).toBe(false)
    expect(hasShareCode('#squire')).toBe(false)
    expect(hasShareCode('#faq=1')).toBe(false)
  })

  it('strips the fragment from an href', () => {
    expect(withoutFragment('https://x.app/#q=EAGWhMA')).toBe('https://x.app/')
    expect(withoutFragment('https://x.app/')).toBe('https://x.app/')
  })
})

/**
 * The codec itself is the engine's, so these are conformance tests against
 * the real wasm module. Node has no `fetch` for `file:` URLs, so the module is
 * instantiated from bytes.
 */
describe('share link codes', () => {
  beforeAll(async () => {
    await init({ module_or_path: await readFile(new URL('./wasm/pkg/seedfinder_bg.wasm', import.meta.url)) })
  })

  const query = (...patch: Partial<QueryState>[]): QueryState =>
    Object.assign({ ...defaultQueryState() }, ...patch)
  const base = { tier: { mode: 'any' as const, value: 3 }, upgrade: { mode: 'any' as const, value: 1 }, uncursed: false }

  it('round-trips a query through a shareable link', () => {
    const state = query({ requirements: [{ ...base, kind: 'weapon', item: 'sword', upgrade: { mode: 'exact', value: 5 } }] })
    const link = encode_share_link(toQueryJson(state))
    expect(hasShareCode(new URL(link).hash)).toBe(true)
    expect(JSON.parse(decode_share_text(link))).toEqual(JSON.parse(toQueryJson(state)))
  })

  it('carries the effects v4.0.0 appended, which need format version 3', () => {
    // Their codes sit above the 24-bit effect mask versions 1 and 2 use, so
    // the engine writes version 3 for them; the app only has to keep the
    // round trip lossless.
    const state = query({ requirements: [
      { ...base, kind: 'weapon', effect: ['Blazing', 'Venomous', 'Eldritch', 'Vorpal', 'Crystal'] },
      { ...base, kind: 'weapon', effect: ['Pressurized', 'Wondrous'] },
    ] })
    expect(JSON.parse(decode_share_text(encode_share_link(toQueryJson(state))))).toEqual(JSON.parse(toQueryJson(state)))
  })

  it('carries a vault-treasure source pin', () => {
    const state = query({ requirements: [{ ...base, kind: 'armor', source: 'vault_treasure' }] })
    expect(JSON.parse(decode_share_text(encode_share_link(toQueryJson(state))))).toEqual(JSON.parse(toQueryJson(state)))
  })
})
