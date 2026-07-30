import { useEffect, useRef, useState } from 'react'
import { useStore } from '@tanstack/react-store'
import { compactNumber, formatDuration, probabilityLabel } from '../../lib/format'
import { CheckIcon, CopyIcon, DownloadIcon, UploadIcon } from '../../lib/icons'
import { validateQuery } from '../../lib/query'
import {
  RESULTS_FILE_NAME,
  decodeResultsFile,
  encodeResultsFile,
  parsedSeedFromCode,
} from '../../lib/results-file'
import { loadImportedResults, searchStore } from '../../lib/search/coordinator'
import { queryStore } from '../../lib/store'
import { analyzeQuery } from '../../lib/wasm'
import type { AnalysisResult } from '../../lib/wasm/types'

/** Re-renders 10 times a second while active so stats stay live between worker updates. */
function useTicker(active: boolean): number {
  const [now, setNow] = useState(() => performance.now())
  useEffect(() => {
    if (!active) return
    const timer = window.setInterval(() => setNow(performance.now()), 100)
    return () => window.clearInterval(timer)
  }, [active])
  return now
}

function estimateDuration(milliseconds: number | undefined): string {
  if (milliseconds === undefined || !Number.isFinite(milliseconds) || milliseconds < 0) return '—'
  const seconds = milliseconds / 1_000
  if (seconds < 1) return '<1s'
  if (seconds < 60) return `${Math.round(seconds)}s`
  if (seconds < 3_600) return `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`
  const hours = Math.floor(seconds / 3_600)
  if (hours < 48) return `${hours}h ${Math.floor((seconds % 3_600) / 60)}m`
  return `${Math.floor(hours / 24)}d ${hours % 24}h`
}

function triggerJsonDownload(text: string, filename: string) {
  const url = URL.createObjectURL(new Blob([text], { type: 'application/json' }))
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filename
  document.body.appendChild(anchor)
  anchor.click()
  anchor.remove()
  URL.revokeObjectURL(url)
}

export function ResultsPanel({
  analysis,
  hasRequirements,
  onScout,
  activeSeed,
  shpdVersion,
}: {
  analysis: AnalysisResult | undefined
  hasRequirements: boolean
  onScout: (code: string) => void
  activeSeed?: string
  shpdVersion?: string
}) {
  const search = useStore(searchStore)
  const [copied, setCopied] = useState<string | undefined>(undefined)
  const [fileNotice, setFileNotice] = useState<string | undefined>(undefined)
  const fileInput = useRef<HTMLInputElement>(null)

  const copySeed = (code: string) => {
    void navigator.clipboard.writeText(code).then(() => {
      setCopied(code)
      window.setTimeout(() => setCopied((current) => (current === code ? undefined : current)), 1_200)
    })
  }

  const running = search.state === 'running'
  const now = useTicker(running)
  const elapsed = running ? now - search.startedAt : search.elapsed
  const probability = analysis?.valid ? analysis.probability : null
  const impossible = Boolean(hasRequirements && analysis?.valid && analysis.impossible)
  const timeToSeed = probability && probability > 0 && search.rate > 0 ? 1_000 / (probability * search.rate) : undefined

  const statusChip =
    search.state === 'completed'
      ? 'Completed'
      : search.state === 'cancelled'
        ? 'Cancelled'
        : search.state === 'imported'
          ? 'Imported'
          : undefined

  const exportResults = () => {
    const query = queryStore.state
    const validation = validateQuery(query)
    if (!validation.valid) {
      setFileNotice(`Cannot export — fix the query first: ${validation.errors[0]}`)
      return
    }
    triggerJsonDownload(
      encodeResultsFile(query, search.matches.map((match) => match.code), shpdVersion),
      RESULTS_FILE_NAME,
    )
    setFileNotice(undefined)
  }

  const importResults = async (file: File) => {
    try {
      const decoded = decodeResultsFile(await file.text())
      // The engine validates the query strictly: unknown items, effects,
      // challenges, or query fields fail here instead of being dropped.
      const verdict = await analyzeQuery(JSON.stringify(decoded.queryDocument))
      if (!verdict.valid) throw new Error(`The query in this results file is not usable: ${verdict.error}`)
      queryStore.setState(() => decoded.query)
      loadImportedResults(decoded.seeds.map(parsedSeedFromCode))
      setFileNotice(undefined)
    } catch (error) {
      setFileNotice(error instanceof Error ? error.message : String(error))
    }
  }

  return (
    <>
      <div className="d1-pane-head">
        <span>Results</span>
        <span className="d1-pane-head-side">
          <span className="d1-pane-head-info">
            {running && <span className="d1-live-dot" aria-hidden="true" />}
            {search.matches.length > 0
              ? `${search.matches.length.toLocaleString()} seed${search.matches.length === 1 ? '' : 's'}`
              : running
                ? 'searching…'
                : ''}
          </span>
          <button
            type="button"
            className="d1-io-btn"
            title="Import results from a file"
            aria-label="Import results from a file"
            disabled={running}
            onClick={() => fileInput.current?.click()}
          >
            <UploadIcon size={13} />
          </button>
          <button
            type="button"
            className="d1-io-btn"
            title="Export results to a file"
            aria-label="Export results to a file"
            disabled={running || search.matches.length === 0}
            onClick={exportResults}
          >
            <DownloadIcon size={13} />
          </button>
          <input
            ref={fileInput}
            type="file"
            accept="application/json,.json"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0]
              event.target.value = ''
              if (file) void importResults(file)
            }}
          />
        </span>
      </div>

      <div className="d1-results-status">
        {search.error && <div className="d1-banner d1-banner-error" role="alert">{search.error}</div>}
        {fileNotice && <div className="d1-banner d1-banner-error" role="alert">{fileNotice}</div>}

        {running && (
          <>
            <div className="d1-progress" role="progressbar" aria-label="Search running">
              <div className="d1-progress-sweep" />
            </div>
            <div className="d1-stat-grid">
              <div className="d1-stat">
                <span className="d1-stat-label">Tested</span>
                <span className="d1-stat-value d1-mono">{compactNumber(search.tested)}</span>
              </div>
              <div className="d1-stat">
                <span className="d1-stat-label">Rate</span>
                <span className="d1-stat-value d1-mono">{search.rate > 0 ? `${compactNumber(search.rate)}/s` : '—'}</span>
              </div>
              <div className="d1-stat">
                <span className="d1-stat-label">Elapsed</span>
                <span className="d1-stat-value d1-mono">{formatDuration(elapsed)}</span>
              </div>
              <div className="d1-stat">
                <span className="d1-stat-label">First seed ≈</span>
                <span className="d1-stat-value d1-mono">{estimateDuration(timeToSeed)}</span>
              </div>
            </div>
            <p className="d1-caption">{probabilityLabel(probability)}</p>
          </>
        )}

        {!running && !impossible && search.state === 'idle' && (
          <p className="d1-empty">Add requirements, then press Start Search.</p>
        )}

        {!running && !impossible && statusChip && (
          <div className="d1-done-row">
            <span className={`d1-state-chip${search.state === 'completed' || search.state === 'imported' ? ' d1-state-ok' : ''}`}>{statusChip}</span>
            <span className="d1-caption">
              {search.state === 'imported'
                ? `${search.matches.length.toLocaleString()} seed${search.matches.length === 1 ? '' : 's'} loaded from file`
                : `${search.matches.length.toLocaleString()} seed${search.matches.length === 1 ? '' : 's'} · tested ${compactNumber(search.tested)} in ${formatDuration(search.elapsed)}`}
            </span>
          </div>
        )}

        {search.capped && <p className="d1-caption d1-capped">Result limit reached (1,024 seeds).</p>}
      </div>

      <div className="d1-pane-body">
        {search.matches.length === 0 ? (
          <div className="d1-results-empty">
            {search.state === 'completed'
              ? <p>No seeds matched this query in the searched range.</p>
              : <p>Matching seeds will appear here as they're found.</p>}
          </div>
        ) : (
          <ol className="d1-result-list">
            {search.matches.map((match, index) => (
              <li key={match.code} className={activeSeed === match.code ? 'd1-result-active' : undefined}>
                <button type="button" className="d1-result-main" onClick={() => onScout(match.code)} title="Scout this seed">
                  <span className="d1-result-index">{index + 1}</span>
                  <span className="d1-result-code d1-mono">{match.code}</span>
                </button>
                <button
                  type="button"
                  className="d1-result-copy"
                  aria-label={`Copy seed ${match.code}`}
                  title="Copy seed"
                  onClick={() => copySeed(match.code)}
                >
                  {copied === match.code ? <CheckIcon size={14} /> : <CopyIcon size={14} />}
                </button>
              </li>
            ))}
          </ol>
        )}
      </div>
    </>
  )
}
