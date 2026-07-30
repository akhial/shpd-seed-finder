import { useEffect, useState } from 'react'
import { useStore } from '@tanstack/react-store'
import { compactNumber, formatDuration, probabilityLabel } from '../../lib/format'
import { CheckIcon, CopyIcon } from '../../lib/icons'
import { searchStore } from '../../lib/search/coordinator'
import { RESULT_CAP } from '../../lib/search/coordinator-state'
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

export function ResultsPanel({
  analysis,
  hasRequirements,
  onScout,
  activeSeed,
}: {
  analysis: AnalysisResult | undefined
  hasRequirements: boolean
  onScout: (code: string) => void
  activeSeed?: string
}) {
  const search = useStore(searchStore)
  const [copied, setCopied] = useState<string | undefined>(undefined)

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
    search.state === 'completed' ? 'Completed' : search.state === 'cancelled' ? 'Cancelled' : search.state === 'failed' ? 'Failed' : undefined
  // The store keeps every delivered match for refine soundness; the panel
  // shows at most the advertised cap.
  const shownMatches = search.matches.slice(0, RESULT_CAP)

  return (
    <>
      <div className="d1-pane-head">
        <span>Results</span>
        <span className="d1-pane-head-info">
          {running && <span className="d1-live-dot" aria-hidden="true" />}
          {shownMatches.length > 0
            ? `${shownMatches.length.toLocaleString()} seed${shownMatches.length === 1 ? '' : 's'}`
            : running
              ? search.filtering
                ? 'refining…'
                : 'searching…'
              : ''}
        </span>
      </div>

      <div className="d1-results-status">
        {search.error && <div className="d1-banner d1-banner-error" role="alert">{search.error}</div>}

        {running && search.filtering && (
          <>
            <div className="d1-progress" role="progressbar" aria-label="Verifying previous results">
              <div className="d1-progress-sweep" />
            </div>
            <p className="d1-caption">
              Verifying {search.refined?.of.toLocaleString() ?? ''} previously found seed{search.refined?.of === 1 ? '' : 's'} against the combined requirements…
            </p>
          </>
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
            {search.refined && (
              <p className="d1-caption">
                Refining: kept {search.refined.kept.toLocaleString()} of {search.refined.of.toLocaleString()} previous seed{search.refined.of === 1 ? '' : 's'}, scanning the remaining range…
              </p>
            )}
            <p className="d1-caption">{search.state === 'stopping' ? 'Stopping…' : probabilityLabel(probability)}</p>
          </>
        )}

        {!running && !impossible && search.state === 'idle' && (
          <p className="d1-empty">Add requirements, then press Start Search.</p>
        )}

        {!running && !impossible && statusChip && (
          <div className="d1-done-row">
            <span className={`d1-state-chip${search.state === 'completed' ? ' d1-state-ok' : ''}`}>{statusChip}</span>
            <span className="d1-caption">
              {shownMatches.length.toLocaleString()} seed{shownMatches.length === 1 ? '' : 's'} · tested {compactNumber(search.tested)} in {formatDuration(search.elapsed)}
            </span>
          </div>
        )}

        {!running && search.refined && (
          <p className="d1-caption">
            Refined: kept {search.refined.kept.toLocaleString()} of {search.refined.of.toLocaleString()} previous seed{search.refined.of === 1 ? '' : 's'}.
          </p>
        )}

        {search.capped && <p className="d1-caption d1-capped">Result limit reached (1,024 seeds).</p>}
      </div>

      <div className="d1-pane-body">
        {shownMatches.length === 0 ? (
          <div className="d1-results-empty">
            {search.state === 'completed'
              ? <p>No seeds matched this query in the searched range.</p>
              : <p>Matching seeds will appear here as they're found.</p>}
          </div>
        ) : (
          <ol className="d1-result-list">
            {shownMatches.map((match, index) => (
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
