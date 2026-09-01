/// <reference lib="webworker" />

import init, { filter_seeds, scout, SearchSession } from "../wasm/pkg/seedfinder.js";
import type { SearchAdvance } from "../wasm/types";
import type { SearchWorkerRequest, SearchWorkerResponse } from "./protocol";

const context: DedicatedWorkerGlobalScope = self as unknown as DedicatedWorkerGlobalScope;
// Small enough that even a slow worker surfaces progress several times a
// second; posts are still rate-limited to one per 100ms below.
const CHUNK = 256;
let activeSession = 0;
let stopRequested = false;
const ready = init(new URL("../wasm/pkg/seedfinder_bg.wasm", import.meta.url));
const post = (message: SearchWorkerResponse) => context.postMessage(message);
const yieldToMessages = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

async function runSearch(message: Extract<SearchWorkerRequest, { type: "search:start" }>) {
  // activeSession/stopRequested are assigned synchronously in the message
  // handler, so a stop that arrives while the wasm module is still
  // initializing is honored instead of dropped.
  const sessionId = message.sessionId;
  await ready;
  const scanned = message.segments.map(() => 0);
  let lastPosted = performance.now();
  let pendingMatches: SearchAdvance["matches"] = [];
  const flush = () => {
    post({ type: "search:progress", sessionId, scanned: [...scanned], matches: pendingMatches });
    pendingMatches = [];
  };
  try {
    for (const [segmentIndex, segment] of message.segments.entries()) {
      if (stopRequested || activeSession !== sessionId) break;
      const search = new SearchSession(
        message.queryJson,
        segment.startSeed,
        segment.endSeedExclusive,
      );
      try {
        while (!stopRequested && activeSession === sessionId) {
          const advance = JSON.parse(search.advance(CHUNK)) as SearchAdvance;
          scanned[segmentIndex] = advance.tested;
          pendingMatches.push(...advance.matches);
          const now = performance.now();
          if (now - lastPosted >= 100) {
            flush();
            lastPosted = now;
          }
          // "completed" also fires when the session hits its own result cap
          // before reaching the end of the segment; the per-segment scanned
          // count keeps the untested tail attributable either way.
          if (advance.state === "completed") break;
          await yieldToMessages();
        }
      } finally {
        search.free();
      }
    }
    if (activeSession !== sessionId) return;
    flush();
    if (stopRequested) post({ type: "search:stopped", sessionId, scanned });
    else post({ type: "search:done", sessionId, scanned });
  } catch (error) {
    post({
      type: "search:error",
      sessionId,
      error: error instanceof Error ? error.message : String(error),
    });
  }
}

context.addEventListener("message", (event: MessageEvent<SearchWorkerRequest>) => {
  const message = event.data;
  if (message.type === "search:start") {
    activeSession = message.sessionId;
    stopRequested = false;
    void runSearch(message);
  }
  if (message.type === "search:stop" && message.sessionId === activeSession) stopRequested = true;
  if (message.type === "scout") {
    void ready.then(() => {
      try {
        post({
          type: "scout:result",
          requestId: message.requestId,
          resultJson: scout(message.requestJson),
        });
      } catch (error) {
        post({
          type: "scout:error",
          requestId: message.requestId,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    });
  }
  if (message.type === "filter") {
    void ready.then(() => {
      try {
        post({
          type: "filter:result",
          requestId: message.requestId,
          resultJson: filter_seeds(message.queryJson, new Float64Array(message.seeds)),
        });
      } catch (error) {
        post({
          type: "filter:error",
          requestId: message.requestId,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    });
  }
});

export {};
