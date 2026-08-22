import { useEffect, useRef, useState } from 'react'
import { useStore } from '@tanstack/react-store'
import { compactNumber, formatDuration, probabilityLabel } from '../../lib/format'
import { CheckIcon, CopyIcon, DownloadIcon, TrashIcon, UploadIcon } from '../../lib/icons'
import {
  RESULTS_FILE_NAME,
  decodeResultsFile,
  encodeResultsFile,
  parsedSeedFromCode,
} from '../../lib/results-file'
import { clearResults, loadImportedResults, searchStore } from '../../lib/search/coordinator'
import { canClearResults, resultCap } from '../../lib/search/coordinator-state'
import { queryStore } from '../../lib/store'
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
  const [fileError, setFileError] = useState<string | undefined>(undefined)
  const [fileInfo, setFileInfo] = useState<string | undefined>(undefined)
  const fileInput = useRef<HTMLInputElement>(null)

  // Keep the scouted seed's row in view while J/K/swipe move it.
  const activeRow = useRef<HTMLLIElement | null>(null)
  useEffect(() => {
    activeRow.current?.scrollIntoView({ block: 'nearest' })
  }, [activeSeed])

  const copySeed = (code: string) => {
    void navigator.clipboard.writeText(code).then(() => {
      setCopied(code)
      window.setTimeout(() => setCopied((current) => (current === code ? undefined : current)), 1_200)
    })
  }

  const running = search.state === 'running' || search.state === 'stopping'
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
        : search.state === 'failed'
          ? 'Failed'
          : search.state === 'imported'
            ? 'Imported'
            : undefined
  // The store keeps every delivered match for refine soundness; the panel
  // lists at most the advertised cap, while the counts report the full
  // collection — an accumulated set is the user's real result.
  const shownMatches = search.matches.slice(0, resultCap())
  const foundCount = search.matches.length

  // Returns the panel to its idle empty state, banners included. Dropping the
  // finished run also drops what a start would have refined from, which is
  // the point: the next search rescans the whole seed space.
  const discardResults = () => {
    clearResults()
    setFileError(undefined)
    setFileInfo(undefined)
  }

  const exportResults = () => {
    // Export the query snapshot captured when the results were produced (at
    // search start or import), never the live editor state.
    const query = search.query
    if (!query || search.matches.length === 0) {
      setFileError('Run a search first — there are no results to export yet.')
      return
    }
    triggerJsonDownload(
      encodeResultsFile(query, search.matches.map((match) => match.code)),
      RESULTS_FILE_NAME,
    )
    setFileError(undefined)
  }

  const importResults = async (file: File) => {
    try {
      // The engine's codec owns the size limit, the envelope rules, the query
      // validation and dedupe-and-cap, and reports its own message on failure.
      const decoded = decodeResultsFile(await file.text())
      // A search may have started while the picker or the read were pending.
      if (searchStore.state.state === 'running') {
        throw new Error('A search is running — stop it before importing results.')
      }
      queryStore.setState(() => decoded.query)
      loadImportedResults(decoded.seeds.map(parsedSeedFromCode), decoded.queryDocument, decoded.dropped)
      setFileError(undefined)
      setFileInfo(
        decoded.shpdVersion !== undefined && shpdVersion !== undefined && decoded.shpdVersion !== shpdVersion
          ? `This file was made for Shattered Pixel Dungeon v${decoded.shpdVersion}; this app targets v${shpdVersion}. The seeds may generate differently.`
          : undefined,
      )
    } catch (error) {
      setFileError(error instanceof Error ? error.message : String(error))
    }
  }

  return (
    <>
      <div className="d1-pane-head">
        <span>Results</span>
        <span className="d1-pane-head-side">
          <span className="d1-pane-head-info">
            {running && <span className="d1-live-dot" aria-hidden="true" />}
            {foundCount > 0
              ? `${foundCount.toLocaleString()} seed${foundCount === 1 ? '' : 's'}`
              : running
                ? search.filtering
                  ? 'refining…'
                  : 'searching…'
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
            <DownloadIcon size={13} />
            <span className="d1-io-label">Import</span>
          </button>
          <button
            type="button"
            className="d1-io-btn"
            title="Export results to a file"
            aria-label="Export results to a file"
            disabled={running || search.matches.length === 0 || !search.query}
            onClick={exportResults}
          >
            <UploadIcon size={13} />
            <span className="d1-io-label">Export</span>
          </button>
          <button
            type="button"
            className="d1-io-btn"
            title="Clear these results, so the next search starts from scratch"
            aria-label="Clear results"
            disabled={!canClearResults(search)}
            onClick={discardResults}
          >
            <TrashIcon size={13} />
            <span className="d1-io-label">Clear</span>
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
        {fileError && <div className="d1-banner d1-banner-error" role="alert">{fileError}</div>}
        {fileInfo && <div className="d1-banner d1-banner-info" role="status">{fileInfo}</div>}

        {running && search.filtering && (
          <div className="d1-progress" role="progressbar" aria-label="Verifying previous results">
            <div className="d1-progress-sweep" />
          </div>
        )}

        {running && !search.filtering && (
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
            <p className="d1-caption">{search.state === 'stopping' ? 'Stopping…' : probabilityLabel(probability)}</p>
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
                ? `${foundCount.toLocaleString()} seed${foundCount === 1 ? '' : 's'} loaded from file${
                    search.importedDropped ? ` · ${search.importedDropped.toLocaleString()} entr${search.importedDropped === 1 ? 'y' : 'ies'} dropped (duplicates or beyond the ${resultCap().toLocaleString()}-seed limit)` : ''
                  }`
                : `${foundCount.toLocaleString()} seed${foundCount === 1 ? '' : 's'} · tested ${compactNumber(search.tested)} in ${formatDuration(search.elapsed)}`}
            </span>
          </div>
        )}
      </div>

      <div className="d1-pane-body">
        {shownMatches.length === 0 ? (
          <div className="d1-results-empty">
            {search.state === 'completed'
              ? <p>No seeds matched this query in the searched range.</p>
              : search.state === 'imported'
                ? <p>The imported file contained no seeds.</p>
                : <p>Matching seeds will appear here as they're found.</p>}
          </div>
        ) : (
          <ol className="d1-result-list">
            {shownMatches.map((match, index) => (
              <li
                key={match.code}
                ref={activeSeed === match.code ? activeRow : undefined}
                className={activeSeed === match.code ? 'd1-result-active' : undefined}
              >
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
