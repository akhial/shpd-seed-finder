import { describe, expect, it } from 'vitest'
import type { CoordinatorState } from './coordinator-state'
import { initialCoordinatorState } from './coordinator-state'
import { searchStatusNotes } from './status'

const state = (overrides: Partial<CoordinatorState>): CoordinatorState => ({
  ...initialCoordinatorState(1_000),
  ...overrides,
})

describe('searchStatusNotes', () => {
  it('is empty for an idle session', () => {
    expect(searchStatusNotes(state({}))).toEqual([])
  })

  it('is empty for a plain running search', () => {
    expect(searchStatusNotes(state({ state: 'running' }))).toEqual([])
  })

  it('reports the filter phase while a refine verifies previous results', () => {
    const notes = searchStatusNotes(state({ state: 'running', filtering: true, refined: { kept: 0, of: 128 } }))
    expect(notes).toEqual([
      { kind: 'refine', text: 'Verifying 128 previously found seeds against the combined requirements…' },
    ])
  })

  it('reports the kept count while a refined search scans for more', () => {
    const notes = searchStatusNotes(state({ state: 'running', refined: { kept: 96, of: 128 } }))
    expect(notes).toEqual([
      { kind: 'refine', text: 'Kept 96 of 128 previous seeds — searching for more…' },
    ])
  })

  it('summarizes a finished refine', () => {
    const notes = searchStatusNotes(state({ state: 'completed', refined: { kept: 1, of: 1 } }))
    expect(notes).toEqual([{ kind: 'refine', text: 'Refined: kept 1 of 1 previous seed.' }])
  })

  it('appends the cap notice after the refine note', () => {
    const notes = searchStatusNotes(state({ state: 'completed', capped: true, refined: { kept: 1_024, of: 2_000 } }))
    expect(notes.map((note) => note.kind)).toEqual(['refine', 'cap'])
    expect(notes[1]?.text).toBe('Result limit reached (1,024 seeds).')
  })

  it('reports only the cap for an unrefined capped run', () => {
    expect(searchStatusNotes(state({ state: 'completed', capped: true }))).toEqual([
      { kind: 'cap', text: 'Result limit reached (1,024 seeds).' },
    ])
  })

  it('holds the previous run\'s cap notice while the filter phase runs', () => {
    const notes = searchStatusNotes(state({ state: 'running', filtering: true, capped: true, refined: { kept: 0, of: 1_024 } }))
    expect(notes.map((note) => note.kind)).toEqual(['refine'])
  })
})
