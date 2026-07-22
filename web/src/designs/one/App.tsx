import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useStore } from '@tanstack/react-store'
import { formatSeedInput } from '../../lib/format'
import { toQueryDocument, toQueryJson, validateQuery } from '../../lib/query'
import { SearchCoordinator, scoutSeed, searchStore } from '../../lib/search/coordinator'
import { queryStore, workerCountStore } from '../../lib/store'
import { analyzeQuery, getEngineInfo, parseSeedCode } from '../../lib/wasm'
import type { AnalysisResult, EngineInfo, ScoutResult } from '../../lib/wasm/types'
import { DownloadMenu } from './DownloadMenu'
import { QueryPanel } from './QueryPanel'
import { ResultsPanel } from './ResultsPanel'
import { ScoutPanel } from './ScoutPanel'
import { Sprite } from './parts'
import './styles.css'

type Tab = 'query' | 'results' | 'scout'

function useDebounced<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delay)
    return () => window.clearTimeout(timer)
  }, [value, delay])
  return debounced
}

const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.userAgent)

export default function App() {
  const query = useStore(queryStore)
  const searchState = useStore(searchStore, (state) => state.state)
  const matchCount = useStore(searchStore, (state) => state.matches.length)

  const [engine, setEngine] = useState<EngineInfo | undefined>(undefined)
  const coordinator = useRef<SearchCoordinator | undefined>(undefined)
  useEffect(() => {
    let active = true
    getEngineInfo()
      .then((info) => {
        if (!active) return
        setEngine(info)
        coordinator.current ??= new SearchCoordinator(info.totalSeeds)
      })
      .catch(() => undefined)
    return () => {
      active = false
    }
  }, [])

  // Debounced query analysis (probability / impossibility).
  const serialized = toQueryJson(query)
  const debouncedJson = useDebounced(serialized, 250)
  const hasRequirements = query.requirements.length > 0
  const [analysis, setAnalysis] = useState<AnalysisResult | undefined>(undefined)
  useEffect(() => {
    if (!hasRequirements) {
      setAnalysis(undefined)
      return
    }
    let active = true
    analyzeQuery(debouncedJson)
      .then((result) => {
        if (active) setAnalysis(result)
      })
      .catch(() => undefined)
    return () => {
      active = false
    }
  }, [debouncedJson, hasRequirements])

  const validation = useMemo(() => validateQuery(query), [query])

  const [activeTab, setActiveTab] = useState<Tab>('query')

  const toggleSearch = useCallback(() => {
    const controller = coordinator.current
    if (!controller) return
    if (searchStore.state.state === 'running') {
      controller.cancel()
      return
    }
    const state = queryStore.state
    if (!validateQuery(state).valid) return
    controller.start(toQueryDocument(state), workerCountStore.state)
    setActiveTab('results')
  }, [])

  // Ctrl/Cmd+Enter starts or cancels the search from anywhere.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
        event.preventDefault()
        toggleSearch()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [toggleSearch])

  // Warn before leaving the page while a search is running.
  useEffect(() => {
    if (searchState !== 'running') return
    const warn = (event: BeforeUnloadEvent) => {
      event.preventDefault()
    }
    window.addEventListener('beforeunload', warn)
    return () => window.removeEventListener('beforeunload', warn)
  }, [searchState])

  // Scout state, lifted so results can populate the detail pane.
  const [scoutInput, setScoutInput] = useState('')
  const [scout, setScout] = useState<{ loading: boolean; error?: string; result?: ScoutResult }>({ loading: false })
  const scoutRequest = useRef(0)
  const runScout = useCallback((seed: string) => {
    const input = formatSeedInput(seed)
    setScoutInput(input)
    setActiveTab('scout')
    if (input.length !== 11) {
      setScout((current) => ({ loading: false, result: current.result, error: 'Seed must use XXX-XXX-XXX format' }))
      return
    }
    const requestId = ++scoutRequest.current
    setScout((current) => ({ loading: true, result: current.result }))
    void (async () => {
      try {
        const parsed = await parseSeedCode(input)
        const state = queryStore.state
        const result = await scoutSeed({
          seed: parsed.code,
          challenges: state.challenges.length > 0 ? state.challenges : undefined,
          query: state.requirements.length > 0 ? toQueryDocument(state) : undefined,
        })
        if (requestId === scoutRequest.current) {
          setScout({ loading: false, result })
          setScoutInput(result.seed.code)
        }
      } catch (error) {
        if (requestId === scoutRequest.current) {
          setScout((current) => ({
            loading: false,
            result: current.result,
            error: error instanceof Error ? error.message : String(error),
          }))
        }
      }
    })()
  }, [])

  const paneClass = (tab: Tab) => `d1-pane d1-pane-${tab}${activeTab === tab ? ' d1-pane-active' : ''}`
  const running = searchState === 'running'

  return (
    <div className="d1-app">
      <header className="d1-topbar">
        <div className="d1-wordmark">
          <Sprite index={112} size={20} />
          <DownloadMenu />
        </div>
        <div className="d1-topbar-right">
          <a
            className="d1-gh-link"
            href="https://github.com/akhial/shpd-seed-seeker"
            target="_blank"
            rel="noreferrer"
            aria-label="SHPD Seed Seeker on GitHub"
            title="View source on GitHub"
          >
            <span className="d1-mono">
              <span className="d1-gh-name">SHPD Seed Seeker </span>v0.5.3
            </span>
            <span className="d1-gh-icon" aria-hidden="true" />
          </a>
        </div>
      </header>

      <nav className="d1-tabs" aria-label="Panels">
        {(
          [
            { tab: 'query' as Tab, label: 'Query' },
            { tab: 'results' as Tab, label: 'Results' },
            { tab: 'scout' as Tab, label: 'Scout' },
          ]
        ).map(({ tab, label }) => (
          <button
            key={tab}
            type="button"
            className={activeTab === tab ? 'd1-tab-on' : undefined}
            onClick={() => setActiveTab(tab)}
          >
            {label}
            {tab === 'results' && running && <span className="d1-live-dot" aria-label="Search running" />}
            {tab === 'results' && !running && matchCount > 0 && <span className="d1-count">{matchCount}</span>}
          </button>
        ))}
      </nav>

      <main className="d1-main">
        <section className={paneClass('query')} aria-label="Query builder">
          <QueryPanel
            analysis={analysis}
            validation={validation}
            running={running}
            engineReady={engine !== undefined}
            onToggleSearch={toggleSearch}
            isMac={isMac}
          />
        </section>
        <section className={paneClass('results')} aria-label="Search results">
          <ResultsPanel
            analysis={analysis}
            hasRequirements={hasRequirements}
            onScout={runScout}
            activeSeed={scout.result?.seed.code}
          />
        </section>
        <section className={paneClass('scout')} aria-label="Seed scout">
          <ScoutPanel
            input={scoutInput}
            onInput={setScoutInput}
            onScout={runScout}
            loading={scout.loading}
            error={scout.error}
            result={scout.result}
          />
        </section>
      </main>

      <footer className="d1-footer">
        <span>{engine ? `Shattered Pixel Dungeon v${engine.shpdVersion}` : 'loading engine…'}</span>
        <span className="d1-footer-sep" aria-hidden="true">·</span>
        <span>GPL-3.0-or-later</span>
        <span className="d1-footer-sep" aria-hidden="true">·</span>
        <a href="/licenses/COPYING.txt">License</a>
        <span className="d1-footer-sep" aria-hidden="true">·</span>
        <a href="/third_party/shattered-pixel-dungeon/ATTRIBUTION.md">Asset attribution</a>
      </footer>
    </div>
  )
}
