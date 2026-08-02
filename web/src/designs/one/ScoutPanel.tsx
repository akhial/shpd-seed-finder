import { useMemo, useState } from 'react'
import { useStore } from '@tanstack/react-store'
import { sourceLabel } from '../../lib/catalog'
import { formatSeedInput } from '../../lib/format'
import { itemGlow } from '../../lib/glow'
import { CheckIcon, CopyIcon, FlagIcon, ForkIcon } from '../../lib/icons'
import { questLabel, questVariantLabel } from '../../lib/quests'
import { regionForDepth } from '../../lib/region'
import type { ResultPosition } from '../../lib/scout-nav'
import { queryStore } from '../../lib/store'
import type { ScoutItem, ScoutResult } from '../../lib/wasm/types'
import { Sprite } from './parts'

const groupLetter = (group: number) => 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'[group % 26]

function accessibilityNote(item: ScoutItem): string | undefined {
  if (item.accessibility.type === 'choice') {
    return `One reward of choice group ${groupLetter(item.accessibility.group)} (option ${item.accessibility.option + 1})`
  }
  if (item.accessibility.type === 'scenarios') {
    return `Only in some outcomes of scenario group ${groupLetter(item.accessibility.group)}`
  }
  return undefined
}

export function ScoutPanel({
  input,
  onInput,
  onScout,
  loading,
  error,
  result,
  nav,
  onNavigate,
}: {
  input: string
  onInput: (value: string) => void
  onScout: (seed: string) => void
  loading: boolean
  error?: string
  result?: ScoutResult
  /** Position of the scouted seed within the search results, when it is one. */
  nav?: ResultPosition
  onNavigate?: (delta: number) => void
}) {
  const challengeCount = useStore(queryStore, (state) => state.challenges.length)
  const [copied, setCopied] = useState(false)

  const floors = useMemo(() => {
    const byDepth = new Map<number, ScoutItem[]>()
    for (const item of result?.items ?? []) {
      byDepth.set(item.depth, [...(byDepth.get(item.depth) ?? []), item])
    }
    return [...byDepth.entries()].sort(([left], [right]) => left - right)
  }, [result])

  // `?? []` guards against cached worker responses from before quests existed.
  const quests = result?.quests ?? []
  const questByDepth = new Map(quests.map((quest) => [quest.depth, quest]))

  const copySeed = () => {
    if (!result) return
    void navigator.clipboard.writeText(result.seed.code).then(() => {
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1_200)
    })
  }

  return (
    <>
      <div className="d1-pane-head">
        <span>Seed Scout</span>
        <span className="d1-pane-head-info">
          {challengeCount > 0 && (
            <>
              <FlagIcon size={12} />
              {challengeCount} challenge{challengeCount === 1 ? '' : 's'}
            </>
          )}
        </span>
      </div>

      <div className="d1-scout-input-row">
        <input
          className="d1-seed-field d1-mono"
          value={input}
          placeholder="AAA-AAA-AAA"
          autoComplete="off"
          autoCapitalize="characters"
          spellCheck={false}
          aria-label="Seed code"
          onChange={(event) => onInput(formatSeedInput(event.currentTarget.value))}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && input.length === 11) onScout(input)
          }}
        />
        <button
          type="button"
          className="d1-btn d1-btn-primary"
          disabled={input.length !== 11 || loading}
          onClick={() => onScout(input)}
        >
          {loading ? 'Scouting…' : 'Scout'}
        </button>
      </div>
      {error && <p className="d1-inline-error d1-scout-error" role="alert">{error}</p>}

      {nav && (
        <div className="d1-scout-nav" role="navigation" aria-label="Search result navigation">
          <button
            type="button"
            className="d1-scout-nav-btn"
            disabled={nav.index === 0}
            onClick={() => onNavigate?.(-1)}
            aria-label="Previous result"
            title="Previous result (K)"
          >
            ‹
          </button>
          {/* Only the index is a live region: a running search grows the
              total ~1,000 times and must not re-announce each change. */}
          <span className="d1-scout-nav-pos">
            <span aria-live="polite">Result <b className="d1-mono">{nav.index + 1}</b></span>
            {' of '}
            <b className="d1-mono">{nav.total}</b>
          </span>
          <button
            type="button"
            className="d1-scout-nav-btn"
            disabled={nav.index + 1 >= nav.total}
            onClick={() => onNavigate?.(1)}
            aria-label="Next result"
            title="Next result (J)"
          >
            ›
          </button>
          <span className="d1-scout-nav-hint d1-scout-nav-hint-keys" aria-hidden="true">
            <kbd className="d1-keycap">J</kbd>
            <span>next</span>
            <kbd className="d1-keycap">K</kbd>
            <span>prev</span>
          </span>
          <span className="d1-scout-nav-hint d1-scout-nav-hint-swipe" aria-hidden="true">swipe to browse</span>
        </div>
      )}

      <div className="d1-pane-body">
        {!result && !loading && (
          <div className="d1-scout-empty">
            <div className="d1-scout-empty-art" aria-hidden="true">
              <Sprite index={112} size={32} />
              <Sprite index={178} size={32} />
              <Sprite index={209} size={32} />
              <Sprite index={224} size={32} />
            </div>
            <h4>No seed scouted</h4>
            <p>Enter a seed, or select a search result, to scout its contents.</p>
          </div>
        )}

        {!result && loading && <p className="d1-empty">Scouting seed…</p>}

        {result && (
          <div className={loading ? 'd1-manifest d1-manifest-loading' : 'd1-manifest'}>
            <div className="d1-manifest-head">
              <div className="d1-manifest-seed">
                <span className="d1-mono d1-manifest-code">{result.seed.code}</span>
                <button type="button" className="d1-result-copy" aria-label="Copy seed" title="Copy seed" onClick={copySeed}>
                  {copied ? <CheckIcon size={14} /> : <CopyIcon size={14} />}
                </button>
              </div>
              <p className="d1-caption">
                {result.items.length} item{result.items.length === 1 ? '' : 's'} across {floors.length} floor{floors.length === 1 ? '' : 's'}
                {result.totalRequirements > 0 && (
                  <>
                    {' · '}
                    <span className={result.matchedRequirements === result.totalRequirements ? 'd1-match-full' : undefined}>
                      {result.matchedRequirements} of {result.totalRequirements} requirement match{result.matchedRequirements === 1 ? '' : 'es'}
                    </span>
                  </>
                )}
              </p>
            </div>

            {quests.length > 0 && (
              <ul className="d1-quest-strip" aria-label="Quests">
                {quests.map((quest) => {
                  const region = regionForDepth(quest.depth)
                  return (
                    <li
                      className="d1-quest-chip"
                      key={quest.quest}
                      style={{ ['--region' as string]: region.color }}
                      title={`${questLabel(quest.quest)} quest on floor ${quest.depth} (${region.name})`}
                    >
                      <span className="d1-quest-chip-variant">{questVariantLabel(quest.variant)}</span>
                      <span className="d1-quest-chip-giver">{questLabel(quest.quest)}</span>
                      <span className="d1-quest-chip-floor d1-mono">F{quest.depth}</span>
                    </li>
                  )
                })}
              </ul>
            )}

            {floors.map(([depth, items]) => {
              const region = regionForDepth(depth)
              const quest = questByDepth.get(depth)
              return (
                <section className="d1-floor" key={depth} style={{ ['--region' as string]: region.color }}>
                  <header className="d1-floor-head">
                    <span className="d1-floor-bar" aria-hidden="true" />
                    <span className="d1-floor-label">Floor {depth}</span>
                    <span className="d1-floor-region">{region.name}</span>
                    {quest && (
                      <span className="d1-floor-quest" title={`${questLabel(quest.quest)} quest`}>
                        {questVariantLabel(quest.variant)}
                      </span>
                    )}
                  </header>
                  <ul className="d1-item-list">
                    {items.map((item, index) => {
                      const note = accessibilityNote(item)
                      return (
                        <li className={item.matched ? 'd1-item d1-item-matched' : 'd1-item'} key={`${item.id}-${index}`}>
                          <Sprite index={item.spriteIndex} size={32} label={item.name} glow={itemGlow(item)} />
                          <div className="d1-item-body">
                            <div className="d1-item-name">
                              <span>{item.name}</span>
                              {item.upgrade > 0 && <b className="d1-badge d1-badge-up">+{item.upgrade}</b>}
                              {item.cursed && <b className="d1-badge d1-badge-curse">cursed</b>}
                              {item.secret && <b className="d1-badge d1-badge-secret" title="Hidden in a secret room — search to reveal it">secret</b>}
                            </div>
                            <div className="d1-item-meta">
                              {item.effect && (
                                <span className={item.effect.kind === 'curse' ? 'd1-fx-curse' : 'd1-fx'}>{item.effect.name}</span>
                              )}
                              <span>{sourceLabel(item.source)}</span>
                            </div>
                            {note && (
                              <p className="d1-item-note">
                                <ForkIcon size={12} />
                                {note}
                              </p>
                            )}
                          </div>
                          {item.matched && <span className="d1-badge d1-badge-match" title="Selected as part of a jointly obtainable requirement match">✓ match</span>}
                        </li>
                      )
                    })}
                  </ul>
                </section>
              )
            })}
          </div>
        )}
      </div>
    </>
  )
}
