import type { ParsedSeed } from '../wasm/types'
import type { SeedRange } from './traversal'

export type SearchWorkerRequest =
  | { type: 'search:start'; queryJson: string; segments: SeedRange[]; sessionId: number }
  | { type: 'search:stop'; sessionId: number }
  | { type: 'scout'; requestJson: string; requestId: number }

export type SearchWorkerResponse =
  | { type: 'search:progress'; sessionId: number; tested: number; matches: ParsedSeed[] }
  | { type: 'search:stopped'; sessionId: number; tested: number }
  | { type: 'search:done'; sessionId: number; tested: number }
  | { type: 'search:error'; sessionId: number; error: string }
  | { type: 'scout:result'; requestId: number; resultJson: string }
  | { type: 'scout:error'; requestId: number; error: string }
