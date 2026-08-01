import { useEffect, useRef, useState } from 'react'
import { useStore } from '@tanstack/react-store'
import { searchStore } from '../../lib/search/coordinator'
import { searchStatusNotes } from '../../lib/search/status'

/** Persistent status region on the right of the desktop footer. Hidden by CSS
 * on small layouts, where the snackbar takes over. */
export function FooterStatus() {
  const search = useStore(searchStore)
  const notes = searchStatusNotes(search)
  return (
    <span className="d1-footer-status" role="status">
      {notes.map((note, index) => (
        <span key={note.kind} className={note.kind === 'cap' ? 'd1-footer-status-cap' : undefined}>
          {index > 0 && <span className="d1-footer-sep" aria-hidden="true">{'· '}</span>}
          {note.text}
        </span>
      ))}
    </span>
  )
}

const SNACKBAR_DISMISS_MS = 4_500

/**
 * Auto-dismissing snackbar for small layouts (shown by CSS below 1000px).
 * Unlike the footer it is event-driven: it announces the refine filter's
 * outcome once and the result cap once per session, instead of mirroring
 * ongoing state the pane header already shows compactly.
 */
export function StatusSnackbar() {
  const search = useStore(searchStore)
  const [note, setNote] = useState<{ text: string; at: number } | undefined>(undefined)

  // Previous-state trackers keyed to the session, so a later run's events
  // fire again while re-renders within one run do not.
  const seen = useRef({ sessionId: 0, filterDone: false, capped: false })
  useEffect(() => {
    if (search.sessionId !== seen.current.sessionId) {
      seen.current = { sessionId: search.sessionId, filterDone: false, capped: false }
    }
    const running = search.state === 'running' || search.state === 'stopping'
    const filterDone = search.refined !== undefined && !search.filtering && (running || search.state === 'completed' || search.state === 'cancelled')
    const events: string[] = []
    if (filterDone && !seen.current.filterDone && search.refined) {
      const plural = search.refined.of === 1 ? '' : 's'
      events.push(
        running
          ? `Kept ${search.refined.kept.toLocaleString()} of ${search.refined.of.toLocaleString()} previous seed${plural} — searching the remaining range…`
          : `Refined: kept ${search.refined.kept.toLocaleString()} of ${search.refined.of.toLocaleString()} previous seed${plural}.`,
      )
    }
    if (search.capped && !seen.current.capped) events.push('Result limit reached (1,024 seeds).')
    seen.current.filterDone = filterDone
    seen.current.capped = search.capped
    if (events.length > 0) setNote({ text: events.join(' '), at: performance.now() })
  }, [search])

  useEffect(() => {
    if (!note) return
    const timer = window.setTimeout(() => setNote(undefined), SNACKBAR_DISMISS_MS)
    return () => window.clearTimeout(timer)
  }, [note])

  if (!note) return null
  return (
    <div key={note.at} className="d1-snackbar" role="status">
      {note.text}
    </div>
  )
}
