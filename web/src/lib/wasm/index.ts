import init, {
  analyze_query,
  decode_share_text,
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
