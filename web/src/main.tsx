import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { getEngineInfo } from './lib/wasm'

const root = createRoot(document.getElementById('root')!)

// The validation bounds, the floor lists, the quest windows and the challenge
// list are all read from `engine_info` through one holder, and the query store
// reads them the moment its module evaluates, so the app is imported only once
// that holder is populated.
getEngineInfo()
  .then(async () => {
    const { default: App } = await import('./designs/one/App')
    root.render(
      <StrictMode>
        <App />
      </StrictMode>,
    )
  })
  .catch((error: unknown) => {
    root.render(<p>Could not load the search engine: {error instanceof Error ? error.message : String(error)}</p>)
  })
