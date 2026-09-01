import { useEffect, useState } from "react";
import {
  armorCurses,
  armorGlyphs,
  itemsForKind,
  kindFamily,
  sources,
  weaponCurses,
  weaponEnchantments,
} from "../../lib/catalog";
import {
  BOUNDED_TIER_MAX,
  BOUNDED_TIER_MIN,
  EXACT_TIER_MAX,
  EXACT_TIER_MIN,
  FLOOR_LIMIT_OPTIONS,
  STACK_MAX,
  canonicalEffect,
  clampUpgrade,
  effectNamesOf,
  isAnyEnchantment,
  maxUpgradeOf,
  validateRequirement,
} from "../../lib/query";
import { ANY_ENCHANTMENT } from "../../lib/wasm/types";
import type {
  ItemCategory,
  ItemSource,
  RequirementKind,
  RequirementState,
} from "../../lib/wasm/types";
import type { StackShape } from "./RequirementBoard";
import { Field, Segmented, SliderRow, Sprite, Stepper } from "./parts";
import { requirementArt, requirementTitle } from "./summary";

const CATEGORY_OPTIONS: { value: ItemCategory; label: string }[] = [
  { value: "weapon", label: "Weapon" },
  { value: "armor", label: "Armor" },
  { value: "wand", label: "Wand" },
  { value: "ring", label: "Ring" },
];

const WEAPON_TYPE_OPTIONS: { value: RequirementKind; label: string }[] = [
  { value: "weapon", label: "Any" },
  { value: "melee_weapon", label: "Melee" },
  { value: "thrown_weapon", label: "Thrown" },
];

const WILDCARD_LABELS: Record<RequirementKind, string> = {
  weapon: "Any weapon",
  melee_weapon: "Any melee weapon",
  thrown_weapon: "Any thrown weapon",
  armor: "Any armor",
  wand: "Any wand",
  ring: "Any ring",
};

const TIER_OPTIONS = [
  { value: "any", label: "Any" },
  { value: "exact", label: "Exactly" },
  { value: "at_least", label: "At least" },
  { value: "at_most", label: "At most" },
] as const;

const UPGRADE_OPTIONS = [
  { value: "any", label: "Any" },
  { value: "exact", label: "Exactly" },
  { value: "at_least", label: "At least" },
] as const;

type EffectMode = "any" | "any_enchantment" | "specific";
const EFFECT_MODE_OPTIONS: { value: EffectMode; label: string }[] = [
  { value: "any", label: "Any" },
  { value: "any_enchantment", label: "Any enchantment" },
  { value: "specific", label: "Specific…" },
];

/** Every integer from `first` through `last`. */
const range = (first: number, last: number): number[] =>
  Array.from({ length: last - first + 1 }, (_, index) => first + index);

const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max);

export function RequirementEditor({
  requirement,
  isNew,
  stack,
  onSave,
  onCancel,
}: {
  requirement: RequirementState;
  isNew: boolean;
  /** The chip's stack shape; a cluster member's belongs to the cluster. */
  stack: StackShape;
  onSave: (
    requirement: RequirementState,
    count: number,
    total: number | undefined,
    copyDepth: number | undefined,
  ) => void;
  onCancel: () => void;
}) {
  const [draft, setDraft] = useState<RequirementState>(() => ({
    ...requirement,
    tier: { ...requirement.tier },
    upgrade: { ...requirement.upgrade },
  }));
  const [count, setCount] = useState(stack.count);
  const [total, setTotal] = useState(stack.total);
  const [copyDepth, setCopyDepth] = useState(stack.copyDepth);
  // "Specific…" with nothing ticked yet is a transient editor state, not a
  // filter, so it lives outside the draft; saving it means "any".
  const [choosingEffects, setChoosingEffects] = useState(false);

  // Every draft edit runs through the upgrade ceiling: naming an item or
  // narrowing the tier can put a +5 out of reach, since only a tier-4 weapon
  // is ever levelled that far.
  const reviseDraft = (revise: (current: RequirementState) => RequirementState) =>
    setDraft((current) => clampUpgrade(revise(current)));

  const kind = draft.kind ?? "weapon";
  const family = kindFamily(kind);
  const maxUpgrade = maxUpgradeOf(draft);
  const wildcardGear = !draft.item && (family === "weapon" || family === "armor");
  const enchantments = family === "weapon" ? weaponEnchantments : armorGlyphs;
  const curses = family === "weapon" ? weaponCurses : armorCurses;
  const errors = validateRequirement(draft);
  // A combined level is a property of a concrete stack of two or more.
  const totalable = stack.inCluster ? false : draft.item !== undefined && count > 1;
  const effectiveTotal = totalable ? total : undefined;
  const totalCapacity = count * (maxUpgrade + 1);
  const effectMode: EffectMode = isAnyEnchantment(draft.effect)
    ? "any_enchantment"
    : draft.effect !== undefined || choosingEffects
      ? "specific"
      : "any";
  const chosenEffects = effectMode === "specific" ? effectNamesOf(draft.effect, kind) : [];

  const setEffectMode = (mode: EffectMode) => {
    setChoosingEffects(mode === "specific");
    reviseDraft((current) => ({
      ...current,
      effect:
        mode === "any"
          ? undefined
          : mode === "any_enchantment"
            ? ANY_ENCHANTMENT
            : isAnyEnchantment(current.effect)
              ? undefined
              : current.effect,
    }));
  };

  const toggleEffect = (name: string) => {
    reviseDraft((current) => {
      const names = effectNamesOf(
        isAnyEnchantment(current.effect) ? undefined : current.effect,
        kind,
      );
      const next = names.includes(name)
        ? names.filter((entry) => entry !== name)
        : [...names, name];
      return { ...current, effect: canonicalEffect(next, kind) };
    });
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  const setKind = (nextKind: RequirementKind) => {
    // Re-clicking the already-selected family must not widen a narrowed
    // weapon kind or wipe the item, tier, and effect selections.
    if (kindFamily(nextKind) === family) return;
    reviseDraft((current) => ({
      ...current,
      kind: nextKind,
      item: undefined,
      tier: { mode: "any", value: 3 },
      effect: undefined,
    }));
    setChoosingEffects(false);
  };

  const setTierMode = (mode: (typeof TIER_OPTIONS)[number]["value"]) => {
    reviseDraft((current) => {
      let value = current.tier.value;
      if (mode === "exact") value = clamp(value, EXACT_TIER_MIN, EXACT_TIER_MAX);
      if (mode === "at_least" || mode === "at_most")
        value = clamp(value, BOUNDED_TIER_MIN, BOUNDED_TIER_MAX);
      return { ...current, tier: { mode, value } };
    });
  };

  const setUpgradeMode = (mode: (typeof UPGRADE_OPTIONS)[number]["value"]) => {
    reviseDraft((current) => ({ ...current, upgrade: { ...current.upgrade, mode } }));
  };

  return (
    <div
      className="d1-overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onCancel();
      }}
    >
      <div
        className="d1-modal"
        role="dialog"
        aria-modal="true"
        aria-label={isNew ? "New requirement" : "Edit requirement"}
      >
        <header className="d1-modal-head">
          <Sprite art={requirementArt(draft)} size={28} />
          <div className="d1-modal-title">
            <h2>{isNew ? "New Requirement" : "Edit Requirement"}</h2>
            <p className="d1-mono">{requirementTitle(draft)}</p>
          </div>
        </header>

        <div className="d1-modal-body">
          <section className="d1-modal-section">
            <h3>Item</h3>
            <Segmented
              value={family}
              options={CATEGORY_OPTIONS}
              onChange={setKind}
              ariaLabel="Category"
              fill
            />
            {family === "weapon" && (
              <Field label="Weapon type">
                <Segmented
                  value={kind}
                  options={WEAPON_TYPE_OPTIONS}
                  onChange={(nextKind) => {
                    reviseDraft((current) => {
                      const keepItem =
                        current.item !== undefined &&
                        itemsForKind(nextKind).some((item) => item.id === current.item);
                      return {
                        ...current,
                        kind: nextKind,
                        item: keepItem ? current.item : undefined,
                      };
                    });
                  }}
                  ariaLabel="Weapon type"
                />
              </Field>
            )}
            <Field label="Item">
              <select
                className="d1-select"
                value={draft.item ?? ""}
                onChange={(event) => {
                  const id = event.currentTarget.value || undefined;
                  if (!id) setTotal(undefined);
                  reviseDraft((current) => ({
                    ...current,
                    item: id,
                    tier: id ? { mode: "any", value: current.tier.value } : current.tier,
                  }));
                }}
              >
                <option value="">{WILDCARD_LABELS[kind]}</option>
                {family === "weapon"
                  ? range(EXACT_TIER_MIN, EXACT_TIER_MAX).map((tier) => (
                      <optgroup key={tier} label={`Tier ${tier}`}>
                        {itemsForKind(kind)
                          .filter((item) => item.tier === tier)
                          .map((item) => (
                            <option key={item.id} value={item.id}>
                              {item.name}
                            </option>
                          ))}
                      </optgroup>
                    ))
                  : itemsForKind(kind)
                      .filter((item) => item.tier !== 1)
                      .map((item) => (
                        <option key={item.id} value={item.id}>
                          {item.name}
                        </option>
                      ))}
              </select>
            </Field>
            {wildcardGear && (
              <>
                <Field label="Tier">
                  <Segmented
                    value={draft.tier.mode}
                    options={[...TIER_OPTIONS]}
                    onChange={setTierMode}
                    ariaLabel="Tier predicate"
                  />
                </Field>
                {draft.tier.mode === "exact" && (
                  <SliderRow
                    label="Exact tier"
                    valueLabel={`Tier ${draft.tier.value}`}
                    min={2}
                    max={5}
                    value={draft.tier.value}
                    onChange={(value) =>
                      reviseDraft((current) => ({ ...current, tier: { ...current.tier, value } }))
                    }
                  />
                )}
                {(draft.tier.mode === "at_least" || draft.tier.mode === "at_most") && (
                  <Field label={draft.tier.mode === "at_least" ? "Minimum tier" : "Maximum tier"}>
                    <select
                      className="d1-select"
                      value={draft.tier.value}
                      onChange={(event) => {
                        const value = Number(event.currentTarget.value);
                        reviseDraft((current) => ({
                          ...current,
                          tier: { ...current.tier, value },
                        }));
                      }}
                    >
                      {range(BOUNDED_TIER_MIN, BOUNDED_TIER_MAX).map((tier) => (
                        <option key={tier} value={tier}>
                          {draft.tier.mode === "at_least"
                            ? `Tier ${tier} or higher`
                            : `Tier ${tier} or lower`}
                        </option>
                      ))}
                    </select>
                  </Field>
                )}
              </>
            )}
          </section>

          {effectiveTotal === undefined && (
            <section className="d1-modal-section">
              <h3>Upgrade level</h3>
              <Segmented
                value={draft.upgrade.mode}
                options={[...UPGRADE_OPTIONS]}
                onChange={setUpgradeMode}
                ariaLabel="Upgrade predicate"
                fill
              />
              {draft.upgrade.mode === "exact" && (
                <SliderRow
                  label="Exactly"
                  valueLabel={`+${draft.upgrade.value}`}
                  min={1}
                  max={maxUpgrade}
                  value={draft.upgrade.value}
                  onChange={(value) =>
                    reviseDraft((current) => ({
                      ...current,
                      upgrade: { ...current.upgrade, value },
                    }))
                  }
                />
              )}
              {draft.upgrade.mode === "at_least" && (
                // Under v4.0.0's ceilings every family spans at least +1…+3
                // (weapons +1…+4), enough range to warrant a slider.
                <SliderRow
                  label="Minimum upgrade"
                  valueLabel={`+${draft.upgrade.value} or higher`}
                  min={1}
                  max={maxUpgrade - 1}
                  value={draft.upgrade.value}
                  onChange={(value) =>
                    reviseDraft((current) => ({
                      ...current,
                      upgrade: { ...current.upgrade, value },
                    }))
                  }
                />
              )}
            </section>
          )}

          {!stack.inCluster && (
            <section className="d1-modal-section">
              <div className="d1-modal-section-head">
                <h3>Total item count</h3>
                <Stepper
                  value={count}
                  min={1}
                  max={STACK_MAX}
                  format={(value) => `×${value}`}
                  onChange={(value) => {
                    setCount(value);
                    if (value < 2) setTotal(undefined);
                    else if (total !== undefined)
                      setTotal(clamp(total, 1, value * (maxUpgrade + 1)));
                  }}
                  ariaLabel="How many of this"
                />
              </div>
              {count > 1 && effectiveTotal === undefined && (
                <>
                  <label className="d1-check">
                    <input
                      type="checkbox"
                      checked={copyDepth !== undefined}
                      onChange={(event) =>
                        setCopyDepth(event.currentTarget.checked ? 4 : undefined)
                      }
                    />
                    <span>Limit the extra copies to a floor</span>
                  </label>
                  {copyDepth !== undefined && (
                    <SliderRow
                      label="Copies within first"
                      valueLabel={`${copyDepth} floor${copyDepth === 1 ? "" : "s"}`}
                      values={FLOOR_LIMIT_OPTIONS}
                      value={copyDepth}
                      fill
                      onChange={setCopyDepth}
                    />
                  )}
                </>
              )}
              {totalable && (
                <>
                  <label className="d1-check">
                    <input
                      type="checkbox"
                      checked={total !== undefined}
                      onChange={(event) =>
                        setTotal(
                          event.currentTarget.checked ? clamp(count, 1, totalCapacity) : undefined,
                        )
                      }
                    />
                    <span>Count levels together</span>
                  </label>
                  {total !== undefined && (
                    <SliderRow
                      label="Levels reach"
                      valueLabel={`≥ ${total} across up to ${count}`}
                      min={1}
                      max={totalCapacity}
                      value={clamp(total, 1, totalCapacity)}
                      fill
                      onChange={setTotal}
                    />
                  )}
                </>
              )}
            </section>
          )}

          <section className="d1-modal-section">
            <h3>Details</h3>
            {(family === "weapon" || family === "armor") && (
              <>
                <Field label={family === "weapon" ? "Enchantment" : "Glyph"} stack>
                  <Segmented
                    value={effectMode}
                    options={
                      family === "weapon"
                        ? EFFECT_MODE_OPTIONS
                        : EFFECT_MODE_OPTIONS.map((option) =>
                            option.value === "any_enchantment"
                              ? { ...option, label: "Any glyph" }
                              : option,
                          )
                    }
                    onChange={setEffectMode}
                    ariaLabel={family === "weapon" ? "Enchantment filter" : "Glyph filter"}
                  />
                </Field>
                {effectMode === "specific" && (
                  <div className="d1-effect-grid" role="group" aria-label="Effects">
                    <span className="d1-effect-grid-head">
                      {family === "weapon" ? "Enchantments" : "Glyphs"}
                    </span>
                    {enchantments.map((name) => (
                      <label className="d1-check" key={name}>
                        <input
                          type="checkbox"
                          checked={chosenEffects.includes(name)}
                          onChange={() => toggleEffect(name)}
                        />
                        <span>{name}</span>
                      </label>
                    ))}
                    {!draft.uncursed && (
                      <>
                        <span className="d1-effect-grid-head">Curses</span>
                        {curses.map((name) => (
                          <label className="d1-check" key={name}>
                            <input
                              type="checkbox"
                              checked={chosenEffects.includes(name)}
                              onChange={() => toggleEffect(name)}
                            />
                            <span>{name}</span>
                          </label>
                        ))}
                      </>
                    )}
                    <p className="d1-caption d1-effect-grid-note">
                      {chosenEffects.length === 0
                        ? "Tick the effects the item may carry; none ticked means any."
                        : `Matches any one of ${chosenEffects.length} effect${chosenEffects.length === 1 ? "" : "s"}.`}
                    </p>
                  </div>
                )}
              </>
            )}
            <label className="d1-check">
              <input
                type="checkbox"
                checked={draft.uncursed}
                onChange={(event) => {
                  const uncursed = event.currentTarget.checked;
                  // Curses leave the selection as they leave the grid.
                  reviseDraft((current) => {
                    if (
                      !uncursed ||
                      current.effect === undefined ||
                      isAnyEnchantment(current.effect)
                    )
                      return { ...current, uncursed };
                    const kept = effectNamesOf(current.effect, kind).filter(
                      (name) => !curses.includes(name),
                    );
                    return { ...current, uncursed, effect: canonicalEffect(kept, kind) };
                  });
                }}
              />
              <span>Require uncursed</span>
            </label>
            <Field label="Source">
              <select
                className="d1-select"
                value={draft.source ?? ""}
                onChange={(event) => {
                  const source = (event.currentTarget.value || undefined) as ItemSource | undefined;
                  reviseDraft((current) => ({ ...current, source }));
                }}
              >
                <option value="">Any</option>
                {sources.map((source) => (
                  <option key={source.value} value={source.value}>
                    {source.label}
                  </option>
                ))}
              </select>
            </Field>
            <label className="d1-check">
              <input
                type="checkbox"
                checked={draft.maxDepth !== undefined}
                onChange={(event) => {
                  const limited = event.currentTarget.checked;
                  reviseDraft((current) => ({ ...current, maxDepth: limited ? 4 : undefined }));
                }}
              />
              <span>Limit this item to a floor</span>
            </label>
            {draft.maxDepth !== undefined && (
              <SliderRow
                label="Within first"
                valueLabel={`${draft.maxDepth} floor${draft.maxDepth === 1 ? "" : "s"}`}
                values={FLOOR_LIMIT_OPTIONS}
                value={draft.maxDepth}
                fill
                onChange={(value) => reviseDraft((current) => ({ ...current, maxDepth: value }))}
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
          <button type="button" className="d1-btn" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="d1-btn d1-btn-primary"
            disabled={errors.length > 0}
            onClick={() =>
              onSave(
                draft,
                stack.inCluster ? 1 : count,
                effectiveTotal,
                stack.inCluster || count < 2 || effectiveTotal !== undefined
                  ? undefined
                  : copyDepth,
              )
            }
          >
            {isNew ? "Add Requirement" : "Save Changes"}
          </button>
        </footer>
      </div>
    </div>
  );
}
