import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useStore } from '@tanstack/react-store'
import { formatSeedInput } from '../../lib/format'
import { toQueryDocument, toQueryJson, validateQuery } from '../../lib/query'
import { resultPosition, stepResult } from '../../lib/scout-nav'
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
  // Anchor for result navigation: the seed of the most recent scout request,
  // set synchronously so rapid steps chain even while a scout is in flight.
  // A failed request falls back to the seed whose manifest is still rendered,
  // keeping the indicator honest.
  const [scoutedSeed, setScoutedSeed] = useState<string | undefined>(undefined)
  const renderedSeed = useRef<string | undefined>(undefined)
  const [scout, setScout] = useState<{ loading: boolean; error?: string; result?: ScoutResult }>({ loading: false })
  const scoutRequest = useRef(0)
  const runScout = useCallback((seed: string) => {
    const input = formatSeedInput(seed)
    setScoutInput(input)
    setActiveTab('scout')
    if (input.length !== 11) {
      setScout((current) => ({ loading: false, result: current.result, error: 'Seed must use XXX-XXX-XXX format' }))
      setScoutedSeed(renderedSeed.current)
      return
    }
    setScoutedSeed(input)
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
          renderedSeed.current = result.seed.code
          setScoutedSeed(result.seed.code)
        }
      } catch (error) {
        if (requestId === scoutRequest.current) {
          setScout((current) => ({
            loading: false,
            result: current.result,
            error: error instanceof Error ? error.message : String(error),
          }))
          setScoutedSeed(renderedSeed.current)
        }
      }
    })()
  }, [])

  // Result-to-result navigation while scouting: J/K on desktop, swipe on touch.
  // The joined-string selector keeps referential stability across progress
  // updates that do not add seeds.
  const matchCodesKey = useStore(searchStore, (state) => state.matches.map((match) => match.code).join(' '))
  const resultSeeds = useMemo(() => (matchCodesKey ? matchCodesKey.split(' ') : []), [matchCodesKey])
  const scoutNav = useMemo(() => resultPosition(resultSeeds, scoutedSeed), [resultSeeds, scoutedSeed])
  const navigateResults = useCallback(
    (delta: number): boolean => {
      const next = stepResult(resultSeeds, scoutedSeed, delta)
      if (!next) return false
      runScout(next)
      return true
    },
    [resultSeeds, scoutedSeed, runScout],
  )

  // J (next) / K (previous) walk the search results while scouting. Inert
  // while typing in a field, while a modal is open, when the tabbed layout is
  // showing another pane (navigating would teleport the user to Scout), and
  // on key repeat (each step scouts a seed; repeats would queue unbounded
  // requests on the single scout worker).
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || event.repeat) return
      // Match the letter (mnemonic) or the physical key, so the shortcut
      // works on layouts without Latin letters.
      const key = event.key.toLowerCase()
      const delta = key === 'j' || event.code === 'KeyJ' ? 1 : key === 'k' || event.code === 'KeyK' ? -1 : 0
      if (delta === 0) return
      const target = event.target as HTMLElement | null
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT' || target.isContentEditable)) return
      if (document.querySelector('.d1-modal, dialog[open], [role="dialog"]')) return
      if (window.matchMedia('(max-width: 999px)').matches && activeTab !== 'scout') return
      if (navigateResults(delta)) event.preventDefault()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [navigateResults, activeTab])

  // Horizontal swipes over the scout pane step through the results on touch
  // devices; mostly-vertical gestures stay scrolls.
  const swipeStart = useRef<{ x: number; y: number } | undefined>(undefined)
  const onScoutTouchStart = (event: React.TouchEvent) => {
    const touch = event.touches[0]
    swipeStart.current = touch && event.touches.length === 1 ? { x: touch.clientX, y: touch.clientY } : undefined
  }
  const onScoutTouchEnd = (event: React.TouchEvent) => {
    const start = swipeStart.current
    swipeStart.current = undefined
    const touch = event.changedTouches[0]
    if (!start || !touch) return
    const deltaX = touch.clientX - start.x
    const deltaY = touch.clientY - start.y
    if (Math.abs(deltaX) < 60 || Math.abs(deltaX) < 1.5 * Math.abs(deltaY)) return
    navigateResults(deltaX < 0 ? 1 : -1)
  }

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
              <span className="d1-gh-name">SHPD Seed Seeker </span>v0.6.1
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
            shpdVersion={engine?.shpdVersion}
          />
        </section>
        <section
          className={paneClass('scout')}
          aria-label="Seed scout"
          onTouchStart={onScoutTouchStart}
          onTouchEnd={onScoutTouchEnd}
        >
          <ScoutPanel
            input={scoutInput}
            onInput={setScoutInput}
            onScout={runScout}
            loading={scout.loading}
            error={scout.error}
            result={scout.result}
            nav={scoutNav}
            onNavigate={navigateResults}
          />
        </section>
      </main>

      <footer className="d1-footer">
        <span>
          {engine ? (
            <>
              <span className="d1-footer-wide">Shattered Pixel Dungeon</span>
              <span className="d1-footer-narrow">SHPD</span>
              {` v${engine.shpdVersion}`}
            </>
          ) : (
            'loading engine…'
          )}
        </span>
        <span className="d1-footer-sep" aria-hidden="true">·</span>
        <span>GPL-3.0-or-later</span>
        <span className="d1-footer-sep" aria-hidden="true">·</span>
        <a href="/licenses/COPYING.txt">License</a>
        <span className="d1-footer-sep" aria-hidden="true">·</span>
        <a href="/third_party/shattered-pixel-dungeon/ATTRIBUTION.md">
          <span className="d1-footer-wide">Asset attribution</span>
          <span className="d1-footer-narrow">Attribution</span>
        </a>
      </footer>
    </div>
  )
}
