import packageJson from '../../package.json'
import { effectNamesForCategory, getItem, isCurseForCategory } from './catalog'
import { fromQueryJson } from './query'
import type { ParsedSeed, QueryDocument, QueryState } from './wasm/types'

// The versioned results-export document shared by every Seed Seeker frontend.
// The canonical implementation and compatibility rules live in the Rust core
// (crates/seedfinder-core/src/results_export.rs); the schema is documented in
// docs/results-export-format.md. Keep this codec schema-compatible with it.

export const RESULTS_FILE_FORMAT = 'seed-seeker-results'
export const RESULTS_FILE_VERSION = 1
export const RESULTS_FILE_NAME = 'seed-seeker-results.json'
/** Import size cap; a maximal legal file is far below this. */
export const MAX_RESULTS_FILE_BYTES = 2 * 1024 * 1024

const SEED_CODE = /^[A-Z]{3}-[A-Z]{3}-[A-Z]{3}$/
const QUERY_KEYS = new Set(['requirements', 'max_depth', 'require_blacksmith', 'exclude_blacksmith_rewards', 'fast_mode', 'challenges'])
const REQUIREMENT_KEYS = new Set(['kind', 'item', 'tier', 'upgrade', 'effect', 'uncursed', 'source', 'identity_group', 'max_depth'])
const KIND_NAMES = new Set(['weapon', 'melee_weapon', 'thrown_weapon', 'armor', 'wand', 'ring'])
const SOURCE_NAMES = new Set([
  'heap', 'chest', 'locked_chest', 'crystal_chest', 'tomb', 'skeleton', 'sacrificial_fire', 'mimic',
  'golden_mimic', 'crystal_mimic', 'statue', 'armored_statue', 'shop', 'ghost_reward',
  'wandmaker_reward', 'blacksmith_reward', 'imp_reward',
])
const CHALLENGE_NAMES = new Set([
  'on_diet', 'faith_is_my_armor', 'pharmacophobia', 'barren_land', 'swarm_intelligence',
  'into_darkness', 'forbidden_runes', 'hostile_champions', 'badder_bosses',
])

/** Numeric value of a canonical seed code, matching the engine's base-26 form. */
export function seedCodeValue(code: string): number {
  let value = 0
  for (const digit of code.replaceAll('-', '')) value = value * 26 + (digit.charCodeAt(0) - 65)
  return value
}

/** Canonical `ParsedSeed` for one imported seed code. */
export function parsedSeedFromCode(code: string): ParsedSeed {
  return { code, value: seedCodeValue(code) }
}

/** Encodes the query document that produced `seeds` (a search-time snapshot). */
export function encodeResultsFile(query: QueryDocument, seeds: string[], shpdVersion: string): string {
  return JSON.stringify(
    {
      format: RESULTS_FILE_FORMAT,
      format_version: RESULTS_FILE_VERSION,
      app_version: packageJson.version,
      shpd_version: shpdVersion,
      query,
      results: seeds.map((seed) => ({ seed })),
    },
    null,
    2,
  )
}

export interface DecodedResultsFile {
  formatVersion: number
  appVersion?: string
  shpdVersion?: string
  /** The raw query document, for engine-side validation and re-serialization. */
  queryDocument: QueryDocument
  /** The query decoded into editor state. */
  query: QueryState
  /** Canonical seed codes in their exported order (not yet deduplicated or capped). */
  seeds: string[]
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

// null is treated like an absent optional field, matching the core decoder.
function stringField(entry: Record<string, unknown>, key: string): string | undefined {
  const value = entry[key]
  if (value === undefined || value === null) return undefined
  if (typeof value !== 'string') throw new Error(`"${key}" must be a string`)
  return value
}

function intField(entry: Record<string, unknown>, key: string): number | undefined {
  const value = entry[key]
  if (value === undefined || value === null) return undefined
  if (typeof value !== 'number' || !Number.isInteger(value)) throw new Error(`"${key}" must be a whole number`)
  return value
}

function boolField(entry: Record<string, unknown>, key: string): boolean {
  const value = entry[key]
  if (value === undefined) return false
  if (typeof value !== 'boolean') throw new Error(`"${key}" must be true or false`)
  return value
}

/**
 * Rejects unknown query fields, items, effects, sources, and challenges, and
 * wrong-typed field values, instead of silently changing the query's meaning.
 * Mirrors the core's strict `json_query` decoding; the import flow
 * additionally re-validates through the wasm engine.
 */
function validateQueryDocument(query: Record<string, unknown>): void {
  for (const key of Object.keys(query)) {
    if (!QUERY_KEYS.has(key)) {
      throw new Error(`The query in this results file uses an unknown field "${key}". Update Seed Seeker to import it.`)
    }
  }
  if (!Array.isArray(query.requirements) || query.requirements.length === 0) {
    throw new Error('The query in this results file has no requirements.')
  }
  const maxDepth = intField(query, 'max_depth') ?? 24
  if (maxDepth < 1 || maxDepth > 24) throw new Error('Maximum floor must be 1 through 24.')
  boolField(query, 'require_blacksmith')
  boolField(query, 'exclude_blacksmith_rewards')
  boolField(query, 'fast_mode')
  if (query.challenges !== undefined) {
    if (!Array.isArray(query.challenges)) throw new Error('"challenges" must be a list of challenge names.')
    for (const name of query.challenges as unknown[]) {
      if (typeof name !== 'string' || !CHALLENGE_NAMES.has(name)) {
        throw new Error(`The query in this results file uses an unknown challenge "${String(name)}".`)
      }
    }
  }
  query.requirements.forEach((entry, index) => {
    try {
      validateRequirementDocument(entry)
    } catch (error) {
      throw new Error(`Requirement ${index + 1}: ${error instanceof Error ? error.message : String(error)}`)
    }
  })
}

function validateRequirementDocument(entry: unknown): void {
  if (!isRecord(entry)) throw new Error('not a JSON object')
  for (const key of Object.keys(entry)) {
    if (!REQUIREMENT_KEYS.has(key)) throw new Error(`unknown field "${key}" — update Seed Seeker to import it`)
  }
  const item = stringField(entry, 'item')
  if (item !== undefined && !getItem(item)) throw new Error(`unknown item "${item}"`)
  const kind = stringField(entry, 'kind') ?? (item ? getItem(item)?.type : undefined)
  if (kind === undefined) throw new Error('a category is required when no item is set')
  if (!KIND_NAMES.has(kind)) throw new Error(`unknown category "${kind}"`)
  const effect = stringField(entry, 'effect')
  if (effect !== undefined) {
    const known = effectNamesForCategory(kind).some((name) => name.toLowerCase() === effect.toLowerCase())
      || isCurseForCategory(kind, effect)
    if (!known) throw new Error(`unknown effect "${effect}"`)
  }
  const source = stringField(entry, 'source')
  if (source !== undefined && !SOURCE_NAMES.has(source)) throw new Error(`unknown source "${source}"`)
  boolField(entry, 'uncursed')
  const identityGroup = intField(entry, 'identity_group')
  if (identityGroup !== undefined && (identityGroup < 1 || identityGroup > 4)) {
    throw new Error('same-item group must be between 1 and 4 (A..D)')
  }
  const maxDepth = intField(entry, 'max_depth')
  if (maxDepth !== undefined && (maxDepth < 1 || maxDepth > 24)) throw new Error('item floor limit must be 1..24')
}

/**
 * Decodes and validates a results-export document.
 *
 * Unknown envelope and per-result fields are ignored so files written by
 * future releases of format version 1 keep importing; files declaring a newer
 * `format_version` are rejected with an "update the app" message. Unknown or
 * wrong-typed query content fails instead of silently changing the query's
 * meaning. Callers should additionally validate `queryDocument` with the
 * engine (`analyzeQuery`).
 *
 * @throws Error with a user-facing message for unusable files.
 */
export function decodeResultsFile(text: string): DecodedResultsFile {
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    throw new Error('This is not a Seed Seeker results file (not valid JSON).')
  }
  if (!isRecord(parsed) || parsed.format !== RESULTS_FILE_FORMAT) {
    throw new Error('This is not a Seed Seeker results file.')
  }
  const document = parsed
  const version = document.format_version
  if (version === undefined) throw new Error('This results file is missing its format version.')
  if (typeof version !== 'number' || !Number.isInteger(version) || version < 1) {
    throw new Error('This results file does not declare a valid format version (a positive whole number).')
  }
  if (version > RESULTS_FILE_VERSION) {
    throw new Error(
      `This results file uses format version ${version}, but this app understands up to version ${RESULTS_FILE_VERSION}. Update Seed Seeker to import it.`,
    )
  }
  const queryValue = document.query
  if (!isRecord(queryValue)) {
    throw new Error('This results file is missing its query.')
  }
  validateQueryDocument(queryValue)
  const resultsValue = document.results
  if (!Array.isArray(resultsValue)) {
    throw new Error('This results file is missing its results list.')
  }
  const seeds = resultsValue.map((entry, index) => {
    const seed = isRecord(entry) ? entry.seed : undefined
    if (typeof seed !== 'string' || !SEED_CODE.test(seed)) {
      throw new Error(`Result ${index + 1} does not have a valid seed code (canonical XXX-XXX-XXX form).`)
    }
    return seed
  })
  let query: QueryState
  try {
    query = fromQueryJson(JSON.stringify(queryValue))
  } catch (error) {
    throw new Error(`The query in this results file is not usable: ${error instanceof Error ? error.message : String(error)}`)
  }
  return {
    formatVersion: version,
    appVersion: typeof document.app_version === 'string' ? document.app_version : undefined,
    shpdVersion: typeof document.shpd_version === 'string' ? document.shpd_version : undefined,
    queryDocument: queryValue as unknown as QueryDocument,
    query,
    seeds,
  }
}
