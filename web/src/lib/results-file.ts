import packageJson from '../../package.json'
import { fromQueryJson, toQueryDocument } from './query'
import type { ParsedSeed, QueryDocument, QueryState } from './wasm/types'

// The versioned results-export document shared by every Seed Seeker frontend.
// The canonical implementation and compatibility rules live in the Rust core
// (crates/seedfinder-core/src/results_export.rs); the schema is documented in
// docs/results-export-format.md. Keep this codec byte-compatible with it.

export const RESULTS_FILE_FORMAT = 'seed-seeker-results'
export const RESULTS_FILE_VERSION = 1
export const RESULTS_FILE_NAME = 'seed-seeker-results.json'

const SEED_CODE = /^[A-Z]{3}-[A-Z]{3}-[A-Z]{3}$/

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

export function encodeResultsFile(query: QueryState, seeds: string[], shpdVersion: string | undefined): string {
  return JSON.stringify(
    {
      format: RESULTS_FILE_FORMAT,
      format_version: RESULTS_FILE_VERSION,
      app_version: packageJson.version,
      ...(shpdVersion !== undefined && { shpd_version: shpdVersion }),
      query: toQueryDocument(query),
      results: seeds.map((seed) => ({ seed })),
    },
    null,
    2,
  )
}

export interface DecodedResultsFile {
  formatVersion: number
  appVersion?: string
  /** The raw query document, for engine-side (strict) validation. */
  queryDocument: QueryDocument
  /** The query decoded into editor state. */
  query: QueryState
  /** Canonical seed codes in their exported order. */
  seeds: string[]
}

/**
 * Decodes and validates a results-export document.
 *
 * Unknown envelope and per-result fields are ignored so files written by
 * future releases of format version 1 keep importing; files declaring a newer
 * `format_version` are rejected with an "update the app" message. Callers
 * should additionally validate `queryDocument` with the engine
 * (`analyzeQuery`), which rejects unknown query fields, items, effects, and
 * challenges instead of silently changing the query's meaning.
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
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    throw new Error('This is not a Seed Seeker results file.')
  }
  const document = parsed as Record<string, unknown>
  if (document.format !== RESULTS_FILE_FORMAT) {
    throw new Error('This is not a Seed Seeker results file.')
  }
  const version = document.format_version
  if (typeof version !== 'number' || !Number.isInteger(version) || version < 1) {
    throw new Error('This results file is missing its format version.')
  }
  if (version > RESULTS_FILE_VERSION) {
    throw new Error(
      `This results file uses format version ${version}, but this app understands up to version ${RESULTS_FILE_VERSION}. Update Seed Seeker to import it.`,
    )
  }
  const queryValue = document.query
  if (typeof queryValue !== 'object' || queryValue === null || Array.isArray(queryValue)) {
    throw new Error('This results file is missing its query.')
  }
  const resultsValue = document.results
  if (!Array.isArray(resultsValue)) {
    throw new Error('This results file is missing its results list.')
  }
  const seeds = resultsValue.map((entry, index) => {
    const seed =
      typeof entry === 'object' && entry !== null ? (entry as Record<string, unknown>).seed : undefined
    if (typeof seed !== 'string' || !SEED_CODE.test(seed)) {
      throw new Error(`Result ${index + 1} does not have a valid seed code.`)
    }
    return seed
  })
  let query: QueryState
  try {
    query = fromQueryJson(JSON.stringify(queryValue))
  } catch {
    throw new Error('The query in this results file is not usable.')
  }
  return {
    formatVersion: version,
    appVersion: typeof document.app_version === 'string' ? document.app_version : undefined,
    queryDocument: queryValue as QueryDocument,
    query,
    seeds,
  }
}
