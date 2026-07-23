import { useEffect, useState } from 'react'
import {
  armorCurses,
  armorGlyphs,
  itemsByCategory,
  sources,
  weaponCurses,
  weaponEnchantments,
} from '../../lib/catalog'
import { validateRequirement } from '../../lib/query'
import type { ItemCategory, ItemSource, RequirementState } from '../../lib/wasm/types'
import { Field, Segmented, SliderRow, Sprite } from './parts'
import { requirementSprite, requirementTitle } from './summary'

const CATEGORY_OPTIONS: { value: ItemCategory; label: string }[] = [
  { value: 'weapon', label: 'Weapon' },
  { value: 'armor', label: 'Armor' },
  { value: 'wand', label: 'Wand' },
  { value: 'ring', label: 'Ring' },
]

const TIER_OPTIONS = [
  { value: 'any', label: 'Any' },
  { value: 'exact', label: 'Exactly' },
  { value: 'at_least', label: 'At least' },
  { value: 'at_most', label: 'At most' },
] as const

const UPGRADE_OPTIONS = [
  { value: 'any', label: 'Any' },
  { value: 'exact', label: 'Exactly' },
  { value: 'at_least', label: 'At least' },
] as const

const GROUP_OPTIONS = [
  { value: 0, label: 'None' },
  { value: 1, label: 'A' },
  { value: 2, label: 'B' },
  { value: 3, label: 'C' },
  { value: 4, label: 'D' },
]

const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max)

export function RequirementEditor({
  requirement,
  isNew,
  onSave,
  onCancel,
}: {
  requirement: RequirementState
  isNew: boolean
  onSave: (requirement: RequirementState) => void
  onCancel: () => void
}) {
  const [draft, setDraft] = useState<RequirementState>(() => ({
    ...requirement,
    tier: { ...requirement.tier },
    upgrade: { ...requirement.upgrade },
  }))

  const kind = draft.kind ?? 'weapon'
  const maxUpgrade = kind === 'ring' ? 4 : 3
  const wildcardGear = !draft.item && (kind === 'weapon' || kind === 'armor')
  const enchantments = kind === 'weapon' ? weaponEnchantments : armorGlyphs
  const curses = kind === 'weapon' ? weaponCurses : armorCurses
  const errors = validateRequirement(draft)

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onCancel()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onCancel])

  const setKind = (nextKind: ItemCategory) => {
    setDraft((current) => {
      const nextMax = nextKind === 'ring' ? 4 : 3
      let upgrade = { ...current.upgrade }
      if (upgrade.mode === 'exact') upgrade = { ...upgrade, value: clamp(upgrade.value, 1, nextMax) }
      if (upgrade.mode === 'at_least') upgrade = { ...upgrade, value: clamp(upgrade.value, 1, nextMax - 1) }
      return {
        ...current,
        kind: nextKind,
        item: undefined,
        tier: { mode: 'any', value: 3 },
        effect: undefined,
        upgrade,
      }
    })
  }

  const setTierMode = (mode: (typeof TIER_OPTIONS)[number]['value']) => {
    setDraft((current) => {
      let value = current.tier.value
      if (mode === 'exact') value = clamp(value, 2, 5)
      if (mode === 'at_least' || mode === 'at_most') value = clamp(value, 3, 4)
      return { ...current, tier: { mode, value } }
    })
  }

  const setUpgradeMode = (mode: (typeof UPGRADE_OPTIONS)[number]['value']) => {
    setDraft((current) => {
      const max = (current.kind ?? 'weapon') === 'ring' ? 4 : 3
      let value = current.upgrade.value
      if (mode === 'exact') value = clamp(value, 1, max)
      if (mode === 'at_least') value = clamp(value, 1, max - 1)
      return { ...current, upgrade: { mode, value } }
    })
  }

  return (
    <div
      className="d1-overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onCancel()
      }}
    >
      <div className="d1-modal" role="dialog" aria-modal="true" aria-label={isNew ? 'New requirement' : 'Edit requirement'}>
        <header className="d1-modal-head">
          <Sprite index={requirementSprite(draft)} size={28} />
          <div className="d1-modal-title">
            <h2>{isNew ? 'New Requirement' : 'Edit Requirement'}</h2>
            <p className="d1-mono">{requirementTitle(draft)}</p>
          </div>
        </header>

        <div className="d1-modal-body">
          <section className="d1-modal-section">
            <h3>Item</h3>
            <Segmented value={kind} options={CATEGORY_OPTIONS} onChange={setKind} ariaLabel="Category" fill />
            <Field label="Item">
              <select
                className="d1-select"
                value={draft.item ?? ''}
                onChange={(event) => {
                  const id = event.currentTarget.value || undefined
                  setDraft((current) => ({
                    ...current,
                    item: id,
                    tier: id ? { mode: 'any', value: current.tier.value } : current.tier,
                  }))
                }}
              >
                <option value="">Any {kind}</option>
                {kind === 'weapon'
                  ? [2, 3, 4, 5].map((tier) => (
                      <optgroup key={tier} label={`Tier ${tier}`}>
                        {itemsByCategory.weapon
                          .filter((item) => item.tier === tier)
                          .map((item) => (
                            <option key={item.id} value={item.id}>{item.name}</option>
                          ))}
                      </optgroup>
                    ))
                  : itemsByCategory[kind]
                      .filter((item) => item.tier !== 1)
                      .map((item) => (
                        <option key={item.id} value={item.id}>{item.name}</option>
                      ))}
              </select>
            </Field>
            {wildcardGear && (
              <>
                <Field label="Tier">
                  <Segmented value={draft.tier.mode} options={[...TIER_OPTIONS]} onChange={setTierMode} ariaLabel="Tier predicate" />
                </Field>
                {draft.tier.mode === 'exact' && (
                  <SliderRow
                    label="Exact tier"
                    valueLabel={`Tier ${draft.tier.value}`}
                    min={2}
                    max={5}
                    value={draft.tier.value}
                    onChange={(value) => setDraft((current) => ({ ...current, tier: { ...current.tier, value } }))}
                  />
                )}
                {(draft.tier.mode === 'at_least' || draft.tier.mode === 'at_most') && (
                  <Field label={draft.tier.mode === 'at_least' ? 'Minimum tier' : 'Maximum tier'}>
                    <select
                      className="d1-select"
                      value={draft.tier.value}
                      onChange={(event) => {
                        const value = Number(event.currentTarget.value)
                        setDraft((current) => ({ ...current, tier: { ...current.tier, value } }))
                      }}
                    >
                      {[3, 4].map((tier) => (
                        <option key={tier} value={tier}>
                          {draft.tier.mode === 'at_least' ? `Tier ${tier} or higher` : `Tier ${tier} or lower`}
                        </option>
                      ))}
                    </select>
                  </Field>
                )}
              </>
            )}
          </section>

          <section className="d1-modal-section">
            <h3>Upgrade level</h3>
            <Segmented value={draft.upgrade.mode} options={[...UPGRADE_OPTIONS]} onChange={setUpgradeMode} ariaLabel="Upgrade predicate" fill />
            {draft.upgrade.mode === 'exact' && (
              <SliderRow
                label="Exactly"
                valueLabel={`+${draft.upgrade.value}`}
                min={1}
                max={maxUpgrade}
                value={draft.upgrade.value}
                onChange={(value) => setDraft((current) => ({ ...current, upgrade: { ...current.upgrade, value } }))}
              />
            )}
            {draft.upgrade.mode === 'at_least' &&
              // Rings span +1…+3, enough range to warrant a slider; other kinds
              // have just +1/+2, which read more clearly as a dropdown.
              (kind === 'ring' ? (
                <SliderRow
                  label="Minimum upgrade"
                  valueLabel={`+${draft.upgrade.value} or higher`}
                  min={1}
                  max={maxUpgrade - 1}
                  value={draft.upgrade.value}
                  onChange={(value) => setDraft((current) => ({ ...current, upgrade: { ...current.upgrade, value } }))}
                />
              ) : (
                <Field label="Minimum upgrade">
                  <select
                    className="d1-select"
                    value={draft.upgrade.value}
                    onChange={(event) => {
                      const value = Number(event.currentTarget.value)
                      setDraft((current) => ({ ...current, upgrade: { ...current.upgrade, value } }))
                    }}
                  >
                    {Array.from({ length: maxUpgrade - 1 }, (_, index) => index + 1).map((value) => (
                      <option key={value} value={value}>+{value} or higher</option>
                    ))}
                  </select>
                </Field>
              ))}
          </section>

          <section className="d1-modal-section">
            <h3>Details</h3>
            {(kind === 'weapon' || kind === 'armor') && (
              <Field label={kind === 'weapon' ? 'Enchantment' : 'Glyph'}>
                <select
                  className="d1-select"
                  value={draft.effect ?? ''}
                  onChange={(event) => {
                    const effect = event.currentTarget.value || undefined
                    setDraft((current) => ({ ...current, effect }))
                  }}
                >
                  <option value="">None</option>
                  <optgroup label={kind === 'weapon' ? 'Enchantments' : 'Glyphs'}>
                    {enchantments.map((name) => (
                      <option key={name} value={name}>{name}</option>
                    ))}
                  </optgroup>
                  {!draft.uncursed && (
                    <optgroup label="Curses">
                      {curses.map((name) => (
                        <option key={name} value={name}>{name}</option>
                      ))}
                    </optgroup>
                  )}
                </select>
              </Field>
            )}
            <label className="d1-check">
              <input
                type="checkbox"
                checked={draft.uncursed}
                onChange={(event) => {
                  const uncursed = event.currentTarget.checked
                  setDraft((current) => ({
                    ...current,
                    uncursed,
                    effect: uncursed && current.effect && curses.includes(current.effect) ? undefined : current.effect,
                  }))
                }}
              />
              <span>Require uncursed</span>
            </label>
            <Field label="Source">
              <select
                className="d1-select"
                value={draft.source ?? ''}
                onChange={(event) => {
                  const source = (event.currentTarget.value || undefined) as ItemSource | undefined
                  setDraft((current) => ({ ...current, source }))
                }}
              >
                <option value="">Any</option>
                {sources.map((source) => (
                  <option key={source.value} value={source.value}>{source.label}</option>
                ))}
              </select>
            </Field>
            <Field label="Same-item group">
              <Segmented
                value={draft.identityGroup ?? 0}
                options={GROUP_OPTIONS}
                onChange={(group) => setDraft((current) => ({ ...current, identityGroup: group === 0 ? undefined : group }))}
                ariaLabel="Same-item group"
              />
            </Field>
            <label className="d1-check">
              <input
                type="checkbox"
                checked={draft.maxDepth !== undefined}
                onChange={(event) => {
                  const limited = event.currentTarget.checked
                  setDraft((current) => ({ ...current, maxDepth: limited ? 5 : undefined }))
                }}
              />
              <span>Limit this item to a floor</span>
            </label>
            {draft.maxDepth !== undefined && (
              <SliderRow
                label="Within first"
                valueLabel={`${draft.maxDepth} floor${draft.maxDepth === 1 ? '' : 's'}`}
                min={1}
                max={24}
                value={draft.maxDepth}
                fill
                onChange={(value) => setDraft((current) => ({ ...current, maxDepth: value }))}
              />
            )}
          </section>

          {errors.length > 0 && (
            <ul className="d1-editor-errors" role="alert">
              {errors.map((error) => (
                <li key={error}>{error}</li>
              ))}
            </ul>
          )}
        </div>

        <footer className="d1-modal-foot">
          <button type="button" className="d1-btn" onClick={onCancel}>Cancel</button>
          <button type="button" className="d1-btn d1-btn-primary" disabled={errors.length > 0} onClick={() => onSave(draft)}>
            {isNew ? 'Add Requirement' : 'Save Changes'}
          </button>
        </footer>
      </div>
    </div>
  )
}
