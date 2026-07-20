/// <reference lib="webworker" />

import init, { scout, SearchSession } from '../wasm/pkg/seedfinder.js'
import type { SearchAdvance } from '../wasm/types'
import type { SearchWorkerRequest, SearchWorkerResponse } from './protocol'

const context: DedicatedWorkerGlobalScope = self as unknown as DedicatedWorkerGlobalScope
// Small enough that even a slow worker surfaces progress several times a
// second; posts are still rate-limited to one per 100ms below.
const CHUNK = 256
let activeSession = 0
let stopRequested = false
const ready = init(new URL('../wasm/pkg/seedfinder_bg.wasm', import.meta.url))
const post = (message: SearchWorkerResponse) => context.postMessage(message)
const yieldToMessages = () => new Promise<void>((resolve) => setTimeout(resolve, 0))

async function runSearch(message: Extract<SearchWorkerRequest, { type: 'search:start' }>) {
  await ready
  activeSession = message.sessionId
  stopRequested = false
  const search = new SearchSession(message.queryJson, message.startSeed, message.endSeedExclusive)
  let lastPosted = performance.now()
  let latestTested = 0
  let pendingMatches: SearchAdvance['matches'] = []
  try {
    while (!stopRequested && activeSession === message.sessionId) {
      const advance = JSON.parse(search.advance(CHUNK)) as SearchAdvance
      latestTested = advance.tested
      pendingMatches.push(...advance.matches)
      const now = performance.now()
      if (now - lastPosted >= 100 || advance.state === 'completed') {
        post({ type: 'search:progress', sessionId: message.sessionId, tested: latestTested, matches: pendingMatches })
        pendingMatches = []
        lastPosted = now
      }
      if (advance.state === 'completed') {
        post({ type: 'search:done', sessionId: message.sessionId, tested: latestTested })
        return
      }
      await yieldToMessages()
    }
    if (pendingMatches.length) post({ type: 'search:progress', sessionId: message.sessionId, tested: latestTested, matches: pendingMatches })
    post({ type: 'search:stopped', sessionId: message.sessionId, tested: latestTested })
  } catch (error) {
    post({ type: 'search:error', sessionId: message.sessionId, error: error instanceof Error ? error.message : String(error) })
  } finally {
    search.free()
  }
}

context.addEventListener('message', (event: MessageEvent<SearchWorkerRequest>) => {
  const message = event.data
  if (message.type === 'search:start') void runSearch(message)
  if (message.type === 'search:stop' && message.sessionId === activeSession) stopRequested = true
  if (message.type === 'scout') {
    void ready.then(() => {
      try {
        post({ type: 'scout:result', requestId: message.requestId, resultJson: scout(message.requestJson) })
      } catch (error) {
        post({ type: 'scout:error', requestId: message.requestId, error: error instanceof Error ? error.message : String(error) })
      }
    })
  }
})

export {}
