import { readFile } from 'node:fs/promises'
import { beforeAll, describe, expect, it } from 'vite-plus/test'
import { formatSeedCode } from './wasm'
import init from './wasm/pkg/seedfinder.js'

/** The masker lives in the engine, so this checks the real wasm module. */
beforeAll(async () => {
  await init({ module_or_path: await readFile(new URL('./wasm/pkg/seedfinder_bg.wasm', import.meta.url)) })
})

describe('seed input formatting', () => {
  it('uppercases partial input', () => expect(formatSeedCode('abc')).toBe('ABC'))
  it('groups and limits a full seed to nine letters', () => expect(formatSeedCode('abc def ghi extra')).toBe('ABC-DEF-GHI'))
})
