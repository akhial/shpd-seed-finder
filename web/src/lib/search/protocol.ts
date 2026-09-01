import type { ParsedSeed } from "../wasm/types";
import type { SeedRange } from "./traversal";

export type SearchWorkerRequest =
  | { type: "search:start"; queryJson: string; segments: SeedRange[]; sessionId: number }
  | { type: "search:stop"; sessionId: number }
  | { type: "scout"; requestJson: string; requestId: number }
  | { type: "filter"; queryJson: string; seeds: number[]; requestId: number };

/** Progress is reported per assigned segment: `scanned[i]` seeds at the
 * front of segment `i` have been tested. A cumulative count would misplace
 * the untested tail of a segment the session abandoned at its result cap. */
export type SearchWorkerResponse =
  | { type: "search:progress"; sessionId: number; scanned: number[]; matches: ParsedSeed[] }
  | { type: "search:stopped"; sessionId: number; scanned: number[] }
  | { type: "search:done"; sessionId: number; scanned: number[] }
  | { type: "search:error"; sessionId: number; error: string }
  | { type: "scout:result"; requestId: number; resultJson: string }
  | { type: "scout:error"; requestId: number; error: string }
  | { type: "filter:result"; requestId: number; resultJson: string }
  | { type: "filter:error"; requestId: number; error: string };
