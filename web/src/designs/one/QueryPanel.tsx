import { Fragment, useState } from 'react'
import { useStore } from '@tanstack/react-store'
import { challenges as challengeOptions, wildcardSprites } from '../../lib/catalog'
import { probabilityLabel } from '../../lib/format'
import { effectFilterGlow } from '../../lib/glow'
import { CommandIcon, ForkIcon, PlusIcon, ReturnIcon, XIcon } from '../../lib/icons'
import { emptyRequirement, fromQueryJson, toQueryJson, validateRequirement } from '../../lib/query'
import type { ValidationResult } from '../../lib/query'
import { builtInPresets, loadPresets, maxWorkers, queryStore, savePresets, setWorkerCount, workerCountStore } from '../../lib/store'
import type { Preset } from '../../lib/store'
import type { AnalysisResult, ChallengeName, ItemCategory, QueryState, RequirementState } from '../../lib/wasm/types'
import { RequirementEditor } from './RequirementEditor'
import { SliderRow, Sprite } from './parts'
import { categoryPlural, requirementDetails, requirementKind, requirementSprite, requirementTitle } from './summary'

const KIND_ORDER: ItemCategory[] = ['weapon', 'armor', 'wand', 'ring']
const LEVEL_GEN_CHALLENGES = new Set<ChallengeName>(['barren_land', 'into_darkness', 'forbidden_runes'])

const patchQuery = (patch: Partial<QueryState>) => queryStore.setState((state) => ({ ...state, ...patch }))
const cloneQuery = (query: QueryState): QueryState => fromQueryJson(toQueryJson(query))

interface EditorSession {
  index: number | null
  requirement: RequirementState
  /** Index of the row this new requirement becomes an alternative to. */
  attachTo?: number
}

/** Groups that shrink below two members stop being alternatives. */
function normalizeAlternatives(requirements: RequirementState[]): RequirementState[] {
  const members = new Map<number, number>()
  for (const requirement of requirements) {
    if (requirement.alternativeGroup !== undefined) {
      members.set(requirement.alternativeGroup, (members.get(requirement.alternativeGroup) ?? 0) + 1)
    }
  }
  return requirements.map((requirement) =>
    requirement.alternativeGroup !== undefined && (members.get(requirement.alternativeGroup) ?? 0) < 2
      ? { ...requirement, alternativeGroup: undefined }
      : requirement,
  )
}

function nextAlternativeGroup(requirements: RequirementState[]): number {
  let highest = 0
  for (const requirement of requirements) {
    if (requirement.alternativeGroup !== undefined) highest = Math.max(highest, requirement.alternativeGroup)
  }
  return highest + 1
}

export function QueryPanel({
  analysis,
  validation,
  running,
  engineReady,
  onToggleSearch,
  isMac,
}: {
  analysis: AnalysisResult | undefined
  validation: ValidationResult
  running: boolean
  engineReady: boolean
  onToggleSearch: () => void
  isMac: boolean
}) {
  const query = useStore(queryStore)
  const workerCount = useStore(workerCountStore)
  const workerCeiling = maxWorkers()
  const [userPresets, setUserPresets] = useState<Preset[]>(() => loadPresets())
  const [namingPreset, setNamingPreset] = useState(false)
  const [presetName, setPresetName] = useState('')
  const [editor, setEditor] = useState<EditorSession | null>(null)

  const applyPreset = (preset: Preset) => {
    queryStore.setState(() => cloneQuery(preset.query))
  }

  const currentQueryJson = JSON.stringify(toQueryJson(query))
  const presetFingerprint = (preset: Preset) => JSON.stringify(toQueryJson(preset.query))
  const builtInMatch = builtInPresets.findIndex((preset) => presetFingerprint(preset) === currentQueryJson)
  const userMatch = builtInMatch >= 0 ? -1 : userPresets.findIndex((preset) => presetFingerprint(preset) === currentQueryJson)
  const selectedPreset = builtInMatch >= 0 ? `b:${builtInMatch}` : userMatch >= 0 ? `u:${userMatch}` : ''

  const saveCurrentPreset = () => {
    const name = presetName.trim()
    if (!name) return
    const snapshot = cloneQuery(query)
    const next = [...userPresets]
    const existing = next.findIndex((preset) => preset.name.toLowerCase() === name.toLowerCase())
    if (existing >= 0) next[existing] = { name: next[existing].name, query: snapshot }
    else next.push({ name, query: snapshot })
    setUserPresets(next)
    savePresets(next)
    setNamingPreset(false)
    setPresetName('')
  }

  const deletePreset = (name: string) => {
    const next = userPresets.filter((preset) => preset.name !== name)
    setUserPresets(next)
    savePresets(next)
  }

  const removeRequirement = (index: number) => {
    queryStore.setState((state) => ({
      ...state,
      requirements: normalizeAlternatives(state.requirements.filter((_, i) => i !== index)),
    }))
  }

  const addAlternative = (index: number) => {
    const source = query.requirements[index]
    if (!source) return
    setEditor({ index: null, requirement: emptyRequirement(source.kind ?? requirementKind(source) ?? 'weapon'), attachTo: index })
  }

  const commitRequirement = (session: EditorSession, requirement: RequirementState) => {
    queryStore.setState((state) => {
      let requirements = [...state.requirements]
      const next = { ...requirement }
      if (session.attachTo !== undefined) {
        const source = requirements[session.attachTo]
        if (source) {
          let group = source.alternativeGroup
          if (group === undefined) {
            group = nextAlternativeGroup(requirements)
            // Joining a group forfeits the source's combined-upgrade link.
            requirements[session.attachTo] = { ...source, alternativeGroup: group, upgradeSum: undefined }
          }
          next.alternativeGroup = group
          next.upgradeSum = undefined
        }
      }
      if (session.index === null) {
        if (next.alternativeGroup !== undefined) {
          // Keep alternatives together, right after their group.
          let insertAt = requirements.length
          for (let i = requirements.length - 1; i >= 0; i -= 1) {
            if (requirements[i].alternativeGroup === next.alternativeGroup) {
              insertAt = i + 1
              break
            }
          }
          requirements.splice(insertAt, 0, next)
        } else {
          requirements.push(next)
        }
      } else {
        requirements[session.index] = next
      }
      const sum = next.upgradeSum
      if (sum) {
        // Members of a combined-upgrade group always agree on one total.
        requirements = requirements.map((current) =>
          current.upgradeSum && current.upgradeSum.group === sum.group
            ? { ...current, upgradeSum: { ...sum } }
            : current,
        )
      }
      return { ...state, requirements: normalizeAlternatives(requirements) }
    })
    setEditor(null)
  }

  const toggleChallenge = (name: ChallengeName) => {
    const active = query.challenges.includes(name)
    patchQuery({
      challenges: active ? query.challenges.filter((value) => value !== name) : [...query.challenges, name],
    })
  }

  const indexed = query.requirements.map((requirement, index) => ({ requirement, index }))
  const plain = indexed.filter(({ requirement }) => requirement.alternativeGroup === undefined)
  const alternativeGroups: { id: number; entries: typeof indexed }[] = []
  for (const entry of indexed) {
    const group = entry.requirement.alternativeGroup
    if (group === undefined) continue
    const existing = alternativeGroups.find((candidate) => candidate.id === group)
    if (existing) existing.entries.push(entry)
    else alternativeGroups.push({ id: group, entries: [entry] })
  }
  const groups = KIND_ORDER.map((kind) => ({
    kind,
    entries: plain.filter(({ requirement }) => requirementKind(requirement) === kind),
  })).filter((group) => group.entries.length > 0)
  const ungrouped = plain.filter(({ requirement }) => requirementKind(requirement) === undefined)
  const challengeCount = query.challenges.length
  const blacksmithCount = Number(query.requireBlacksmith) + Number(query.excludeBlacksmithRewards)
  const performanceCount = Number(query.fastMode)
  const hasRequirements = query.requirements.length > 0
  // Count each alternative group as one requirement slot, matching the
  // scout header's "N of M" denominator.
  const slotCount = new Set(
    query.requirements.map((requirement, index) =>
      requirement.alternativeGroup === undefined ? `r${index}` : `g${requirement.alternativeGroup}`,
    ),
  ).size
  const impossible = Boolean(analysis?.valid && analysis.impossible)
  const startDisabled = !running && (!engineReady || !validation.valid || impossible)

  return (
    <>
      <div className="d1-pane-head">
        <span>Query</span>
        <span className="d1-pane-head-info">
          {hasRequirements ? `${slotCount} requirement${slotCount === 1 ? '' : 's'}` : ''}
        </span>
      </div>
      <div className="d1-pane-body">
        <section className="d1-section">
          <div className="d1-section-head"><h3>Presets</h3></div>
          <div className="d1-preset-row">
            <select
              className="d1-select d1-grow"
              value={selectedPreset}
              aria-label="Load preset"
              onChange={(event) => {
                const value = event.currentTarget.value
                if (!value) return
                const [scope, indexText] = value.split(':')
                const index = Number(indexText)
                const preset = scope === 'b' ? builtInPresets[index] : userPresets[index]
                if (preset) applyPreset(preset)
              }}
            >
              <option value="">Load preset…</option>
              <optgroup label="Included">
                {builtInPresets.map((preset, index) => (
                  <option key={preset.name} value={`b:${index}`}>{preset.name}</option>
                ))}
              </optgroup>
              {userPresets.length > 0 && (
                <optgroup label="Saved">
                  {userPresets.map((preset, index) => (
                    <option key={preset.name} value={`u:${index}`}>{preset.name}</option>
                  ))}
                </optgroup>
              )}
            </select>
            <button
              type="button"
              className="d1-btn"
              onClick={() => {
                setNamingPreset((value) => !value)
                setPresetName('')
              }}
            >
              Save…
            </button>
          </div>
          {namingPreset && (
            <div className="d1-preset-row">
              <input
                className="d1-input d1-grow"
                autoFocus
                placeholder="Preset name"
                value={presetName}
                aria-label="Preset name"
                onChange={(event) => setPresetName(event.currentTarget.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') saveCurrentPreset()
                  if (event.key === 'Escape') setNamingPreset(false)
                }}
              />
              <button type="button" className="d1-btn d1-btn-primary" disabled={!presetName.trim()} onClick={saveCurrentPreset}>
                Save
              </button>
            </div>
          )}
          {userPresets.length > 0 && (
            <ul className="d1-preset-chips">
              {userPresets.map((preset) => (
                <li key={preset.name}>
                  <button type="button" className="d1-chip-name" title="Apply preset" onClick={() => applyPreset(preset)}>
                    {preset.name}
                  </button>
                  <button
                    type="button"
                    className="d1-chip-delete"
                    aria-label={`Delete preset ${preset.name}`}
                    title="Delete preset"
                    onClick={() => deletePreset(preset.name)}
                  >
                    <XIcon size={14} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="d1-section">
          <div className="d1-section-head">
            <h3>Requirements</h3>
            <button
              type="button"
              className="d1-btn d1-btn-sm d1-btn-primary"
              onClick={() => setEditor({ index: null, requirement: emptyRequirement('weapon') })}
            >
              <PlusIcon size={14} />
              Add
            </button>
          </div>
          {!hasRequirements ? (
            <p className="d1-empty">No requirements yet. Add one to describe the item you're hunting for.</p>
          ) : (
            <>
              {alternativeGroups.map((group) => (
                <div className="d1-req-group d1-req-alt" key={`alt-${group.id}`}>
                  <div className="d1-req-group-head d1-req-alt-head">
                    <ForkIcon size={14} />
                    <span>Any of these</span>
                  </div>
                  <ul className="d1-req-list">
                    {group.entries.map(({ requirement, index }, position) => (
                      <Fragment key={index}>
                        {position > 0 && (
                          <li className="d1-req-or" aria-hidden="true">
                            or
                          </li>
                        )}
                        <RequirementRow
                          requirement={requirement}
                          onEdit={() => setEditor({ index, requirement })}
                          onRemove={() => removeRequirement(index)}
                        />
                      </Fragment>
                    ))}
                  </ul>
                  <button
                    type="button"
                    className="d1-btn d1-btn-sm d1-req-alt-add"
                    onClick={() => addAlternative(group.entries[0].index)}
                  >
                    <PlusIcon size={13} />
                    Add alternative
                  </button>
                </div>
              ))}
              {groups.map((group) => (
                <div className="d1-req-group" key={group.kind}>
                  <div className="d1-req-group-head" style={{ color: 'rgb(234, 234, 234)' }}>
                    <Sprite index={wildcardSprites[group.kind]} size={16} />
                    <span>{categoryPlural[group.kind]}</span>
                  </div>
                  <ul className="d1-req-list">
                    {group.entries.map(({ requirement, index }) => (
                      <RequirementRow
                        key={index}
                        requirement={requirement}
                        onEdit={() => setEditor({ index, requirement })}
                        onRemove={() => removeRequirement(index)}
                        onAddAlternative={() => addAlternative(index)}
                      />
                    ))}
                  </ul>
                </div>
              ))}
              {ungrouped.length > 0 && (
                <ul className="d1-req-list">
                  {ungrouped.map(({ requirement, index }) => (
                    <RequirementRow
                      key={index}
                      requirement={requirement}
                      onEdit={() => setEditor({ index, requirement })}
                      onRemove={() => removeRequirement(index)}
                    />
                  ))}
                </ul>
              )}
            </>
          )}
        </section>

        <section className="d1-section">
          <div className="d1-section-head"><h3>Search scope</h3></div>
          <SliderRow
            label="Floor limit"
            valueLabel={`first ${query.maxDepth} floor${query.maxDepth === 1 ? '' : 's'}`}
            min={1}
            max={24}
            value={query.maxDepth}
            fill
            onChange={(value) => patchQuery({ maxDepth: value })}
          />
        </section>

        <section className="d1-section">
          <details className="d1-details">
            <summary>
              <span>Blacksmith</span>
              {blacksmithCount > 0 && <span className="d1-count">{blacksmithCount}</span>}
            </summary>
            <div className="d1-details-body">
              <label className={`d1-check${query.maxDepth >= 14 ? ' d1-check-disabled' : ''}`}>
                <input
                  type="checkbox"
                  checked={query.requireBlacksmith}
                  disabled={query.maxDepth >= 14}
                  onChange={(event) => patchQuery({ requireBlacksmith: event.currentTarget.checked })}
                />
                <span>Require accessible blacksmith</span>
              </label>
              <label className="d1-check">
                <input
                  type="checkbox"
                  checked={query.excludeBlacksmithRewards}
                  onChange={(event) => patchQuery({ excludeBlacksmithRewards: event.currentTarget.checked })}
                />
                <span>Exclude Smith rewards</span>
              </label>
              <p className="d1-caption">
                Required items cannot come from the 2,000-favor Smith choice, leaving favor available for reforging.
              </p>
            </div>
          </details>
        </section>

        <section className="d1-section">
          <details className="d1-details">
            <summary>
              <span>Performance</span>
              {performanceCount > 0 && <span className="d1-count">{performanceCount}</span>}
            </summary>
            <div className="d1-details-body">
              {workerCeiling > 1 && (
                <>
                  <SliderRow
                    label="Workers"
                    valueLabel={`${workerCount} of ${workerCeiling} core${workerCeiling === 1 ? '' : 's'}`}
                    min={1}
                    max={workerCeiling}
                    value={Math.min(workerCount, workerCeiling)}
                    fill
                    onChange={setWorkerCount}
                  />
                  <p className="d1-caption d1-caption-spaced">
                    Number of search threads to spawn.
                  </p>
                </>
              )}
              <label className="d1-check">
                <input
                  type="checkbox"
                  checked={query.fastMode}
                  onChange={(event) => patchQuery({ fastMode: event.currentTarget.checked })}
                />
                <span>Fast search</span>
              </label>
              <p className="d1-caption">
                Treats +3 weapons and armor as quest rewards only, skipping the rare Crypt and Sacrificial-fire prizes.
              </p>
            </div>
          </details>
        </section>

        <section className="d1-section">
          <details className="d1-details">
            <summary>
              <span>Challenges</span>
              {challengeCount > 0 && <span className="d1-count">{challengeCount}</span>}
            </summary>
            <p className="d1-caption">Searches simulate runs with the selected challenges enabled.</p>
            <div className="d1-challenge-list">
              {challengeOptions.map((challenge) => (
                <label className="d1-check" key={challenge.value}>
                  <input
                    type="checkbox"
                    checked={query.challenges.includes(challenge.value)}
                    onChange={() => toggleChallenge(challenge.value)}
                  />
                  <span>
                    {challenge.label}
                    <em>{LEVEL_GEN_CHALLENGES.has(challenge.value) ? 'changes level generation' : 'no effect on seed content'}</em>
                  </span>
                </label>
              ))}
            </div>
          </details>
        </section>
      </div>

      <div className="d1-query-foot">
        {hasRequirements && (
          // The status line always reserves one line so the foot height (and the scroll body) never changes
          // as the query toggles between possible and impossible. The warning is anchored to this line's
          // bottom and grows upward over the form, so its bottom edge sits one gap above the button — matching
          // the button's bottom spacing — instead of pushing the button down.
          <div className="d1-query-status">
            {impossible && (
              <div className="d1-impossible">
                <strong className="d1-impossible-title">Impossible query</strong>
                <p>
                  No seed can satisfy these requirements within the current floor limit. Quest-reward-only items
                  need their quest floors in range: +3 wands floors 7–9, +3/+4 rings floors 17–19.
                </p>
              </div>
            )}
            {!validation.valid ? (
              <span className="d1-inline-error">{validation.errors[0]}</span>
            ) : (
              <span className="d1-analysis-line">
                {analysis?.valid && !analysis.impossible ? probabilityLabel(analysis.probability) : ''}
              </span>
            )}
          </div>
        )}
        <button
          type="button"
          className={`d1-btn d1-btn-big ${running ? 'd1-btn-danger' : impossible ? '' : 'd1-btn-primary'}`}
          disabled={startDisabled}
          onClick={onToggleSearch}
        >
          <span>{running ? 'Cancel Search' : 'Start Search'}</span>
          <kbd>
            {isMac ? <CommandIcon size={13} /> : <span className="d1-kbd-text">Ctrl</span>}
            <ReturnIcon size={13} />
          </kbd>
        </button>
      </div>

      {editor && (
        <RequirementEditor
          key={editor.index ?? 'new'}
          requirement={editor.requirement}
          isNew={editor.index === null}
          isAlternative={editor.attachTo !== undefined || editor.requirement.alternativeGroup !== undefined}
          onSave={(requirement) => commitRequirement(editor, requirement)}
          onCancel={() => setEditor(null)}
        />
      )}
    </>
  )
}

function RequirementRow({
  requirement,
  onEdit,
  onRemove,
  onAddAlternative,
}: {
  requirement: RequirementState
  onEdit: () => void
  onRemove: () => void
  onAddAlternative?: () => void
}) {
  const errors = validateRequirement(requirement)
  const details = requirementDetails(requirement)
  return (
    <li className="d1-req">
      <button type="button" className="d1-req-main" onClick={onEdit} title="Edit requirement">
        <Sprite index={requirementSprite(requirement)} size={28} glow={effectFilterGlow(requirement.effect)} />
        <span className="d1-req-text">
          <span className="d1-req-title">{requirementTitle(requirement)}</span>
          <span className="d1-req-sub">{details.length > 0 ? details.join(' · ') : 'any upgrade · any source'}</span>
          {errors.length > 0 && <span className="d1-req-error">{errors[0]}</span>}
        </span>
      </button>
      {onAddAlternative && (
        <button
          type="button"
          className="d1-req-remove d1-req-fork"
          aria-label="Add an alternative to this requirement"
          title="Add alternative (either this or another item)"
          onClick={onAddAlternative}
        >
          <ForkIcon size={15} />
        </button>
      )}
      <button type="button" className="d1-req-remove" aria-label="Remove requirement" title="Remove" onClick={onRemove}>
        <XIcon size={15} />
      </button>
    </li>
  )
}
