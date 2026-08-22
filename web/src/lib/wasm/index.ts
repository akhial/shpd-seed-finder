import init, {
  analyze_query,
  decode_results_file,
  decode_share_text,
  encode_results_file,
  encode_share_link,
  engine_info,
  format_seed_code,
  parse_seed_code,
  query_continues,
} from './pkg/seedfinder.js'
import type { AnalysisResult, EngineInfo, ParsedSeed } from './types'

let enginePromise: Promise<void> | undefined

export function initEngine(): Promise<void> {
  enginePromise ??= init(new URL('./pkg/seedfinder_bg.wasm', import.meta.url)).then(() => undefined)
  return enginePromise
}

export async function getEngineInfo(): Promise<EngineInfo> {
  await initEngine()
  return JSON.parse(engine_info()) as EngineInfo
}

export async function formatSeedCode(input: string): Promise<string> {
  await initEngine()
  return format_seed_code(input)
}

export async function parseSeedCode(input: string): Promise<ParsedSeed> {
  await initEngine()
  return JSON.parse(parse_seed_code(input)) as ParsedSeed
}

export async function analyzeQuery(queryJson: string): Promise<AnalysisResult> {
  await initEngine()
  return JSON.parse(analyze_query(queryJson)) as AnalysisResult
}

/**
 * The engine's refine-soundness predicate: whether a run of `candidateJson`
 * can continue one of `baseJson`. Single-sourced here rather than restated in
 * TypeScript, so the browser agrees with every other frontend about when
 * filter-and-resume is safe. Throws when either query fails to decode.
 *
 * Synchronous, unlike everything else in this module, because the refine
 * decision sits on the synchronous Start path. Callers must have awaited
 * `initEngine()` — the app builds its `SearchCoordinator` only once
 * `getEngineInfo()` has resolved, so nothing can reach this before then.
 */
export function queryContinues(candidateJson: string, baseJson: string): boolean {
  return query_continues(candidateJson, baseJson)
}

/** Encodes a canonical query document as a full shareable web link. */
export async function encodeShareLink(queryJson: string): Promise<string> {
  await initEngine()
  return encode_share_link(queryJson)
}

/** Decodes share-link text (full link or bare code) into the canonical query document. */
export async function decodeShareText(text: string): Promise<string> {
  await initEngine()
  return decode_share_text(text)
}

/**
 * Parses a canonical seed code into `{code, value}` using the engine's own
 * base-26 seed semantics.
 *
 * Synchronous, like `queryContinues`: the results-import path turns a whole
 * decoded file into seeds in one synchronous step. Callers must have awaited
 * `initEngine()`.
 */
export function parseSeedCodeSync(input: string): ParsedSeed {
  return JSON.parse(parse_seed_code(input)) as ParsedSeed
}

/**
 * The engine's results-file encoder. `requestJson` is
 * `{"query", "seeds", "app_version"}`; the answer is the file text. Throws
 * with the codec's own message for an invalid query or seed code.
 *
 * Synchronous, like `queryContinues`, because export and import both sit on
 * synchronous UI paths. Callers must have awaited `initEngine()`.
 */
export function encodeResultsFileText(requestJson: string): string {
  return encode_results_file(requestJson)
}

/**
 * The engine's results-file decoder, answering
 * `{"query", "seeds", "dropped", "app_version", "shpd_version"}` as JSON. The
 * seeds arrive deduplicated and capped, and the 2 MiB import limit, the
 * envelope rules and the query validation are all the engine's. Throws with
 * the codec's own message for an unusable file.
 *
 * Synchronous, like `queryContinues`. Callers must have awaited
 * `initEngine()`.
 */
export function decodeResultsFileText(contents: string): string {
  return decode_results_file(contents)
}
