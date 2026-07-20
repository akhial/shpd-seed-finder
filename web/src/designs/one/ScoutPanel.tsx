import { useMemo, useState } from 'react'
import { useStore } from '@tanstack/react-store'
import { sourceLabel } from '../../lib/catalog'
import { formatSeedInput } from '../../lib/format'
import { itemGlow } from '../../lib/glow'
import { CheckIcon, CopyIcon, FlagIcon, ForkIcon } from '../../lib/icons'
import { regionForDepth } from '../../lib/region'
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
}: {
  input: string
  onInput: (value: string) => void
  onScout: (seed: string) => void
  loading: boolean
  error?: string
  result?: ScoutResult
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

            {floors.map(([depth, items]) => {
              const region = regionForDepth(depth)
              return (
                <section className="d1-floor" key={depth} style={{ ['--region' as string]: region.color }}>
                  <header className="d1-floor-head">
                    <span className="d1-floor-bar" aria-hidden="true" />
                    <span className="d1-floor-label">Floor {depth}</span>
                    <span className="d1-floor-region">{region.name}</span>
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
