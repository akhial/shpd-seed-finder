import { readFile } from 'node:fs/promises'
import { beforeAll, describe, expect, it } from 'vitest'
import { LEVEL_GEN_CHALLENGES, challenges } from './catalog'
import {
  BLACKSMITH_LAST_FLOOR,
  BOUNDED_TIER_MAX,
  BOUNDED_TIER_MIN,
  EMPTY_BOSS_FLOORS,
  EXACT_TIER_MAX,
  EXACT_TIER_MIN,
  IDENTITY_GROUP_MAX,
  MAX_DEPTH,
  MAX_UPGRADE_DEFAULT,
  MAX_UPGRADE_RING,
} from './query'
import { RESULT_CAP } from './search/coordinator-state'
import { SEARCH_START_STRIDE, TOTAL_SEEDS } from './search/traversal'
import init, { engine_info } from './wasm/pkg/seedfinder.js'
import type { EngineInfo } from './wasm/types'

/**
 * The app keeps local copies of the engine's scalar constants so nothing has
 * to wait on the wasm module to render. This is the one place they meet the
 * engine: every local is asserted against the `engine_info` document, so a
 * change on either side fails here rather than as an editor offering a query
 * the search refuses. Node has no `fetch` for `file:` URLs, so the module is
 * instantiated from bytes.
 */
let info: EngineInfo

beforeAll(async () => {
  await init({ module_or_path: await readFile(new URL('./wasm/pkg/seedfinder_bg.wasm', import.meta.url)) })
  info = JSON.parse(engine_info()) as EngineInfo
})

describe('local constants match the engine document', () => {
  it('query bounds', () => {
    expect(MAX_DEPTH).toBe(info.limits.maxDepth)
    expect(EXACT_TIER_MIN).toBe(info.limits.exactTierMin)
    expect(EXACT_TIER_MAX).toBe(info.limits.exactTierMax)
    expect(BOUNDED_TIER_MIN).toBe(info.limits.boundedTierMin)
    expect(BOUNDED_TIER_MAX).toBe(info.limits.boundedTierMax)
    expect(IDENTITY_GROUP_MAX).toBe(info.limits.identityGroupMax)
    expect(MAX_UPGRADE_DEFAULT).toBe(info.limits.maxUpgradeDefault)
    expect(MAX_UPGRADE_RING).toBe(info.limits.maxUpgradeRing)
  })

  it('result cap and seed space', () => {
    expect(RESULT_CAP).toBe(info.maxResults)
    expect(TOTAL_SEEDS).toBe(info.totalSeeds)
    expect(SEARCH_START_STRIDE).toBe(info.searchStartStride)
    // The import byte cap is applied by the engine's own decoder; the app
    // keeps no copy of it, so there is nothing local to compare.
    expect(info.limits.resultsFileMaxBytes).toBeGreaterThan(0)
  })

  it('empty boss floors and the Blacksmith window', () => {
    expect([...EMPTY_BOSS_FLOORS]).toEqual(info.emptyBossFloors)
    // The app has no quest-window table of its own; the only window it
    // depends on is the Blacksmith's last floor, which gates "require
    // Blacksmith".
    expect(BLACKSMITH_LAST_FLOOR).toBe(info.questWindows.blacksmith[1])
  })

  it('challenge list, mask order, and generation relevance', () => {
    expect(challenges.map((challenge) => challenge.value)).toEqual(info.challenges.map((challenge) => challenge.name))
    info.challenges.forEach((challenge, index) => expect(challenge.mask).toBe(1 << index))
    expect([...LEVEL_GEN_CHALLENGES].sort()).toEqual(
      info.challenges.filter((challenge) => challenge.changesLevelGeneration).map((challenge) => challenge.name).sort(),
    )
  })
})
