import init, {
  analyze_query,
  engine_info,
  format_seed_code,
  parse_seed_code,
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
