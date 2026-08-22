import { useState } from 'react'
import { useStore } from '@tanstack/react-store'
import { LEVEL_GEN_CHALLENGES, challenges as challengeOptions, wildcardSprites } from '../../lib/catalog'
import { probabilityLabel } from '../../lib/format'
import { effectGlow } from '../../lib/glow'
import { CheckIcon, CommandIcon, ForkIcon, LinkIcon, PlusIcon, ReturnIcon, XIcon } from '../../lib/icons'
import { BLACKSMITH_LAST_FLOOR, FLOOR_LIMIT_OPTIONS, emptyRequirement, fromQueryJson, querySlots, toQueryJson, validateRequirement } from '../../lib/query'
import type { QuerySlot } from '../../lib/query'
import type { ValidationResult } from '../../lib/query'
import { questVariantLabel } from '../../lib/quests'
import { builtInPresets, loadPresets, maxWorkers, queryStore, savePresets, setWorkerCount, workerCountStore } from '../../lib/store'
import type { Preset } from '../../lib/store'
import { encodeShareLink } from '../../lib/wasm'
import { WANDMAKER_QUESTS } from '../../lib/wasm/types'
import type { AnalysisResult, ChallengeName, ItemCategory, QueryState, RequirementState, WandmakerQuest } from '../../lib/wasm/types'
import { RequirementEditor } from './RequirementEditor'
import { SliderRow, Sprite } from './parts'
import { alternativesTitle, categoryPlural, requirementDetails, requirementKind, requirementSprite, requirementTitle } from './summary'

const KIND_ORDER: ItemCategory[] = ['weapon', 'armor', 'wand', 'ring']

const patchQuery = (patch: Partial<QueryState>) => queryStore.setState((state) => ({ ...state, ...patch }))
const cloneQuery = (query: QueryState): QueryState => fromQueryJson(toQueryJson(query))

interface EditorSession {
  index: number | null
  requirement: RequirementState
  /** Set when adding an alternative to the requirement at this index. */
  alternativeOf?: number
}

/** A free alternative-group number: one above the highest in use. */
const nextAlternativeGroup = (requirements: RequirementState[]): number =>
  requirements.reduce((highest, requirement) => Math.max(highest, requirement.alternativeGroup ?? 0), 0) + 1

/** Drops alternative groups that no longer have two members, so a lone member is a plain row again. */
function collapseLoneAlternatives(requirements: RequirementState[]): RequirementState[] {
  const counts = new Map<number, number>()
  for (const requirement of requirements) {
    if (requirement.alternativeGroup !== undefined) counts.set(requirement.alternativeGroup, (counts.get(requirement.alternativeGroup) ?? 0) + 1)
  }
  return requirements.map((requirement) => {
    if (requirement.alternativeGroup === undefined || (counts.get(requirement.alternativeGroup) ?? 0) > 1) return requirement
    const { alternativeGroup: _dropped, ...rest } = requirement
    return rest
  })
}

/** Every member of a combined-upgrade group shares one total: the edited member's wins. */
function syncUpgradeSums(requirements: RequirementState[], changed: RequirementState): RequirementState[] {
  const sum = changed.upgradeSum
  if (!sum) return requirements
  return requirements.map((requirement) => (
    requirement !== changed && requirement.upgradeSum?.group === sum.group && requirement.upgradeSum.atLeast !== sum.atLeast
      ? { ...requirement, upgradeSum: { group: sum.group, atLeast: sum.atLeast } }
      : requirement
  ))
}

export function QueryPanel({
  analysis,
  validation,
  running,
  engineReady,
  onToggleSearch,
  isMac,
  shareNotice,
  onDismissShareNotice,
}: {
  analysis: AnalysisResult | undefined
  validation: ValidationResult
  running: boolean
  engineReady: boolean
  onToggleSearch: () => void
  isMac: boolean
  shareNotice: string | undefined
  onDismissShareNotice: () => void
}) {
  const query = useStore(queryStore)
  const workerCount = useStore(workerCountStore)
  const workerCeiling = maxWorkers()
  const [userPresets, setUserPresets] = useState<Preset[]>(() => loadPresets())
  const [namingPreset, setNamingPreset] = useState(false)
  const [presetName, setPresetName] = useState('')
  const [editor, setEditor] = useState<EditorSession | null>(null)
  const [linkCopied, setLinkCopied] = useState(false)

  const shareQuery = () => {
    void encodeShareLink(toQueryJson(query))
      .then((link) => navigator.clipboard.writeText(link))
      .then(() => {
        setLinkCopied(true)
        window.setTimeout(() => setLinkCopied(false), 1_200)
      })
      .catch(() => undefined)
  }

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
      requirements: collapseLoneAlternatives(state.requirements.filter((_, i) => i !== index)),
    }))
  }

  const removeSlot = (slot: QuerySlot) => {
    queryStore.setState((state) => ({
      ...state,
      requirements: state.requirements.filter((_, i) => !slot.members.includes(i)),
    }))
  }

  /** Opens the editor on a fresh alternative for the requirement at `index`. */
  const addAlternative = (index: number) => {
    const source = query.requirements[index]
    const group = source.alternativeGroup ?? nextAlternativeGroup(query.requirements)
    setEditor({
      index: null,
      alternativeOf: index,
      requirement: { ...emptyRequirement(source.kind), alternativeGroup: group },
    })
  }

  const commitRequirement = (session: EditorSession, requirement: RequirementState) => {
    queryStore.setState((state) => {
      let requirements: RequirementState[]
      if (session.alternativeOf !== undefined) {
        // The source joins the new member's group (dropping any combined-upgrade
        // sum, which alternatives cannot carry) and the member lands right
        // after the group's last row so the card reads in order.
        const group = requirement.alternativeGroup ?? nextAlternativeGroup(state.requirements)
        const member = { ...requirement, alternativeGroup: group, upgradeSum: undefined }
        requirements = state.requirements.map((current, i) => (
          i === session.alternativeOf ? { ...current, alternativeGroup: group, upgradeSum: undefined } : current
        ))
        const last = requirements.reduce((found, current, i) => (current.alternativeGroup === group ? i : found), session.alternativeOf)
        requirements = [...requirements.slice(0, last + 1), member, ...requirements.slice(last + 1)]
      } else if (session.index === null) {
        requirements = [...state.requirements, requirement]
      } else {
        requirements = state.requirements.map((current, i) => (i === session.index ? requirement : current))
      }
      return { ...state, requirements: syncUpgradeSums(requirements, requirement) }
    })
    setEditor(null)
  }

  const toggleChallenge = (name: ChallengeName) => {
    const active = query.challenges.includes(name)
    patchQuery({
      challenges: active ? query.challenges.filter((value) => value !== name) : [...query.challenges, name],
    })
  }

  // Rows are "any of these" slots. A slot sits under its members' family
  // when they share one; mixed or kind-less slots list after the families.
  const slots = querySlots(query.requirements)
  const slotFamily = (slot: QuerySlot): ItemCategory | undefined => {
    const families = new Set(slot.members.map((index) => requirementKind(query.requirements[index])))
    const [only] = families
    return families.size === 1 ? only : undefined
  }
  const groups = KIND_ORDER.map((kind) => ({
    kind,
    entries: slots.filter((slot) => slotFamily(slot) === kind),
  })).filter((group) => group.entries.length > 0)
  const ungrouped = slots.filter((slot) => slotFamily(slot) === undefined)
  const slotTotal = slots.length
  const challengeCount = query.challenges.length
  const wandmakerCount = Number(Boolean(query.wandmakerQuest))
  const blacksmithCount = Number(query.requireBlacksmith) + Number(query.excludeBlacksmithRewards)
  const performanceCount = Number(query.fastMode)
  const hasRequirements = query.requirements.length > 0
  const impossible = Boolean(analysis?.valid && analysis.impossible)
  const startDisabled = !running && (!engineReady || !validation.valid || impossible)

  return (
    <>
      <div className="d1-pane-head">
        <span>Query</span>
        <span className="d1-pane-head-side">
          <span className="d1-pane-head-info">
            {hasRequirements ? `${slotTotal} requirement${slotTotal === 1 ? '' : 's'}` : ''}
          </span>
          <button
            type="button"
            className="d1-io-btn"
            title="Copy a shareable link to this search"
            aria-label="Copy a shareable link to this search"
            disabled={!engineReady || !validation.valid}
            onClick={shareQuery}
          >
            {linkCopied ? <CheckIcon size={13} /> : <LinkIcon size={13} />}
            {linkCopied ? 'Copied' : 'Share'}
          </button>
        </span>
      </div>
      <div className="d1-pane-body">
        {shareNotice && (
          <div className="d1-banner d1-banner-bleed" role="alert">
            <span className="d1-grow">This share link couldn't be loaded: {shareNotice}</span>
            <button type="button" className="d1-banner-dismiss" aria-label="Dismiss" title="Dismiss" onClick={onDismissShareNotice}>
              <XIcon size={14} />
            </button>
          </div>
        )}
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
          {!hasRequirements && ungrouped.length === 0 ? (
            <p className="d1-empty">No requirements yet. Add one to describe the item you're hunting for.</p>
          ) : (
            <>
              {groups.map((group) => (
                <div className="d1-req-group" key={group.kind}>
                  <div className="d1-req-group-head" style={{ color: 'rgb(234, 234, 234)' }}>
                    <Sprite index={wildcardSprites[group.kind]} size={16} />
                    <span>{categoryPlural[group.kind]}</span>
                  </div>
                  <ul className="d1-req-list">
                    {group.entries.map((slot) => (
                      <SlotRow
                        key={slot.key}
                        slot={slot}
                        requirements={query.requirements}
                        onEdit={(index) => setEditor({ index, requirement: query.requirements[index] })}
                        onAddAlternative={addAlternative}
                        onRemove={removeRequirement}
                        onRemoveSlot={() => removeSlot(slot)}
                      />
                    ))}
                  </ul>
                </div>
              ))}
              {ungrouped.length > 0 && (
                <ul className="d1-req-list">
                  {ungrouped.map((slot) => (
                    <SlotRow
                      key={slot.key}
                      slot={slot}
                      requirements={query.requirements}
                      onEdit={(index) => setEditor({ index, requirement: query.requirements[index] })}
                      onAddAlternative={addAlternative}
                      onRemove={removeRequirement}
                      onRemoveSlot={() => removeSlot(slot)}
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
            values={FLOOR_LIMIT_OPTIONS}
            value={query.maxDepth}
            fill
            onChange={(value) => patchQuery({ maxDepth: value })}
          />
        </section>

        <section className="d1-section">
          <details className="d1-details">
            <summary>
              <span>Wandmaker</span>
              {wandmakerCount > 0 && <span className="d1-count">{wandmakerCount}</span>}
            </summary>
            <div className="d1-details-body">
              <label className="d1-field">
                <span className="d1-field-label">Quest</span>
                <span className="d1-field-control">
                  <select
                    className="d1-select"
                    value={query.wandmakerQuest ?? ''}
                    onChange={(event) => patchQuery({ wandmakerQuest: (event.currentTarget.value || undefined) as WandmakerQuest | undefined })}
                  >
                    <option value="">Any</option>
                    {WANDMAKER_QUESTS.map((variant) => (
                      <option key={variant} value={variant}>{questVariantLabel(variant)}</option>
                    ))}
                  </select>
                </span>
              </label>
            </div>
          </details>
        </section>

        <section className="d1-section">
          <details className="d1-details">
            <summary>
              <span>Blacksmith</span>
              {blacksmithCount > 0 && <span className="d1-count">{blacksmithCount}</span>}
            </summary>
            <div className="d1-details-body">
              <label className={`d1-check${query.maxDepth >= BLACKSMITH_LAST_FLOOR ? ' d1-check-disabled' : ''}`}>
                <input
                  type="checkbox"
                  checked={query.requireBlacksmith}
                  disabled={query.maxDepth >= BLACKSMITH_LAST_FLOOR}
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
                  need their quest floors in range: +3 wands and the Wandmaker's quest floors 7–9,
                  +3/+4 rings floors 17–19.
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
          others={query.requirements.filter((_, index) => index !== editor.index)}
          onSave={(requirement) => commitRequirement(editor, requirement)}
          onCancel={() => setEditor(null)}
        />
      )}
    </>
  )
}

/** One slot: a plain requirement row, or an "any of these" card when it has several members. */
function SlotRow({
  slot,
  requirements,
  onEdit,
  onAddAlternative,
  onRemove,
  onRemoveSlot,
}: {
  slot: QuerySlot
  requirements: RequirementState[]
  onEdit: (index: number) => void
  onAddAlternative: (index: number) => void
  onRemove: (index: number) => void
  onRemoveSlot: () => void
}) {
  if (slot.members.length === 1) {
    const index = slot.members[0]
    return (
      <RequirementRow
        requirement={requirements[index]}
        onEdit={() => onEdit(index)}
        onAddAlternative={() => onAddAlternative(index)}
        onRemove={() => onRemove(index)}
      />
    )
  }
  return (
    <li className="d1-alt-card">
      <div className="d1-alt-head">
        <span className="d1-alt-title">{alternativesTitle(slot.members.length)}</span>
        <span className="d1-alt-hint">one of these satisfies the slot</span>
        <button type="button" className="d1-req-remove" aria-label="Remove this group" title="Remove group" onClick={onRemoveSlot}>
          <XIcon size={15} />
        </button>
      </div>
      <ul className="d1-req-list d1-alt-list">
        {slot.members.map((index, position) => (
          <RequirementRow
            key={index}
            requirement={requirements[index]}
            separator={position > 0 ? 'OR' : undefined}
            onEdit={() => onEdit(index)}
            onAddAlternative={() => onAddAlternative(index)}
            onRemove={() => onRemove(index)}
          />
        ))}
      </ul>
    </li>
  )
}

function RequirementRow({
  requirement,
  separator,
  onEdit,
  onAddAlternative,
  onRemove,
}: {
  requirement: RequirementState
  /** Text shown above the row inside an "any of these" card. */
  separator?: string
  onEdit: () => void
  onAddAlternative: () => void
  onRemove: () => void
}) {
  const errors = validateRequirement(requirement)
  const details = requirementDetails(requirement)
  return (
    <li className="d1-req-slot">
      {separator && <span className="d1-alt-or" aria-hidden="true">{separator}</span>}
      <div className="d1-req">
        <button type="button" className="d1-req-main" onClick={onEdit} title="Edit requirement">
          <Sprite index={requirementSprite(requirement)} size={28} glow={effectGlow(requirement.effect)} />
          <span className="d1-req-text">
            <span className="d1-req-title">{requirementTitle(requirement)}</span>
            <span className="d1-req-sub">{details.length > 0 ? details.join(' · ') : 'any upgrade · any source'}</span>
            {errors.length > 0 && <span className="d1-req-error">{errors[0]}</span>}
          </span>
        </button>
        <button type="button" className="d1-req-fork" aria-label="Add an alternative" title="Add an alternative (any of these)" onClick={onAddAlternative}>
          <ForkIcon size={14} />
        </button>
        <button type="button" className="d1-req-remove" aria-label="Remove requirement" title="Remove" onClick={onRemove}>
          <XIcon size={15} />
        </button>
      </div>
    </li>
  )
}
