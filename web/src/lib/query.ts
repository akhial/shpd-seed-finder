import {
  effectNamesForCategory,
  enchantmentNamesForCategory,
  getItem,
  isCurseForCategory,
  kindFamily,
  kindWeaponClass,
} from "./catalog";
import { ANY_ENCHANTMENT, WANDMAKER_QUESTS } from "./wasm/types";
import type {
  EffectFilter,
  QueryDocument,
  QueryState,
  RequirementDocument,
  RequirementEntryDocument,
  RequirementState,
  TierFilter,
  UpgradeFilter,
  WandmakerQuest,
} from "./wasm/types";

// The query bounds below are local copies of the engine's own constants
// (`crates/seedfinder-core/src/engine_info.rs`). They stay local so the app
// never has to wait on the wasm module to render; `engine-constants.test.ts`
// asserts each of them against the engine's `engine_info` document instead.

/** Deepest floor a search may cover. */
export const MAX_DEPTH = 24;

/** Tiers an "exactly tier N" requirement may name (tier 1 is starting gear). */
export const EXACT_TIER_MIN = 2;
export const EXACT_TIER_MAX = 5;

/** Tiers an "at least / at most tier N" requirement may name; the ends of the
 * exact range would be redundant with "any" or "exactly". */
export const BOUNDED_TIER_MIN = 3;
export const BOUNDED_TIER_MAX = 4;

/** Highest same-item group number (groups run 1..this, shown as A..D). */
export const IDENTITY_GROUP_MAX = 4;

/** Highest combined-level group number (groups run 1..this). */
export const LEVEL_SUM_GROUP_MAX = 4;

/** The most items a stack may ask for, its anchor included. */
export const STACK_MAX = 3;

/** The highest upgrade a search may name for an item family. v4.0.0's Imp
 * vault sets the ceilings: its final-room options reach +5 on a weapon or
 * thrown weapon, +4 on armor, wands and rings. */
export const MAX_UPGRADE_DEFAULT = 4;
export const MAX_UPGRADE_RING = 4;
export const MAX_UPGRADE_WEAPON = 5;

/** The highest upgrade every ring but one can carry in a single world: ring
 * drops roll +0…+2, and the only source beyond that — the Imp vault's
 * final-room prize — appears once per run. */
export const MAX_UPGRADE_RING_STANDARD = 2;

/** The highest combined level `count` rings can reach together: one ring at
 * the vault ceiling, every other at the standard roll, each counting its
 * upgrade plus one. */
export const ringStackCapacity = (count: number): number =>
  MAX_UPGRADE_RING + 1 + (count - 1) * (MAX_UPGRADE_RING_STANDARD + 1);
export const maxUpgradeFor = (family: string | undefined): number =>
  family === "weapon"
    ? MAX_UPGRADE_WEAPON
    : family === "ring"
      ? MAX_UPGRADE_RING
      : MAX_UPGRADE_DEFAULT;

/** The highest upgrade the generator puts on any item, whatever its tier. */
export const MAX_UPGRADE_ANY_TIER = 4;

/** The one weapon tier levelled past {@link MAX_UPGRADE_ANY_TIER}, a
 * v4.0.0-BETA-3 quirk: the Imp's vault lays out one tier-4 and one tier-5
 * weapon and rolls the tier-4 one at +3…+5 while the tier-5 one stops at +4.
 * So a +5 exists only on a tier-4 weapon, melee or thrown. When upstream
 * levels the two ranges this and its callers go away and every family caps
 * at {@link MAX_UPGRADE_ANY_TIER}. */
export const EXTRA_UPGRADE_TIER = 4;

/** Whether a requirement can still land on {@link EXTRA_UPGRADE_TIER}. */
const reachesExtraUpgradeTier = (requirement: Pick<RequirementState, "item" | "tier">): boolean => {
  if (requirement.item) return getItem(requirement.item)?.tier === EXTRA_UPGRADE_TIER;
  const { mode, value } = requirement.tier;
  return mode === "exact"
    ? value === EXTRA_UPGRADE_TIER
    : mode === "at_least"
      ? value <= EXTRA_UPGRADE_TIER
      : mode === "at_most"
        ? value >= EXTRA_UPGRADE_TIER
        : true;
};

/** The highest upgrade one requirement may name, its tier filter included. */
export const maxUpgradeOf = (
  requirement: Pick<RequirementState, "kind" | "item" | "tier">,
): number => {
  const ceiling = maxUpgradeFor(requirementFamily(requirement));
  return ceiling > MAX_UPGRADE_ANY_TIER && !reachesExtraUpgradeTier(requirement)
    ? MAX_UPGRADE_ANY_TIER
    : ceiling;
};

/** The requirement with its upgrade pulled back under {@link maxUpgradeOf},
 * for edits — a narrowed tier, a named item — that lower the ceiling. */
export const clampUpgrade = (requirement: RequirementState): RequirementState => {
  if (requirement.upgrade.mode === "any") return requirement;
  const maximum = maxUpgradeOf(requirement);
  const ceiling = Math.max(1, requirement.upgrade.mode === "at_least" ? maximum - 1 : maximum);
  return requirement.upgrade.value <= ceiling
    ? requirement
    : { ...requirement, upgrade: { ...requirement.upgrade, value: ceiling } };
};

/** The broad family a requirement belongs to, from its kind or its item. */
export const requirementFamily = (
  requirement: Pick<RequirementState, "kind" | "item">,
): string | undefined =>
  requirement.kind
    ? kindFamily(requirement.kind)
    : requirement.item
      ? getItem(requirement.item)?.type
      : undefined;

/**
 * The most *levels* — upgrade plus one — one requirement can contribute to a
 * combined-level total: an exact upgrade counts as itself, anything else as
 * the family cap.
 */
export const maxLevelOf = (requirement: RequirementState): number =>
  (requirement.upgrade.mode === "exact" ? requirement.upgrade.value : maxUpgradeOf(requirement)) +
  1;

/** The highest combined level a group's members can reach together: each
 * one's own ceiling, bounded by what a world generates —
 * {@link ringStackCapacity}. */
export const levelSumCapacity = (members: RequirementState[]): number =>
  Math.min(
    members.reduce((total, member) => total + maxLevelOf(member), 0),
    ringStackCapacity(members.length),
  );

/**
 * Whether a requirement constrains anything beyond its category: a stack's
 * extra copies are exactly the unconstrained requirements. A per-item floor
 * limit is a placement bound, not an item property, and does not count.
 */
export const isBareRequirement = (requirement: RequirementState): boolean =>
  requirement.item === undefined &&
  (requirement.kind === undefined || kindFamily(requirement.kind) === requirement.kind) &&
  requirement.tier.mode === "any" &&
  requirement.upgrade.mode === "any" &&
  requirement.effect === undefined &&
  !requirement.uncursed &&
  requirement.source === undefined;

/** True when the effect filter is the "some non-curse effect" shorthand. */
export const isAnyEnchantment = (effect: EffectFilter | undefined): boolean =>
  effect === ANY_ENCHANTMENT;

/** The explicit effect names a filter accepts (the shorthand expands to the family's enchantments). */
export const effectNamesOf = (
  effect: EffectFilter | undefined,
  kind: string | undefined,
): string[] => {
  if (effect === undefined) return [];
  if (effect === ANY_ENCHANTMENT) return kind ? enchantmentNamesForCategory(kind) : [];
  return typeof effect === "string" ? [effect] : effect;
};

/**
 * The canonical form of an effect selection: names in catalog order, one name
 * as a bare string, the full non-curse family set as the shorthand, and an
 * empty selection as no filter. This is the writer rule every platform shares.
 */
export function canonicalEffect(
  names: readonly string[],
  kind: string | undefined,
): EffectFilter | undefined {
  if (!kind) return names.length === 0 ? undefined : names.length === 1 ? names[0] : [...names];
  const order = effectNamesForCategory(kind);
  const known = order.filter((name) => names.includes(name));
  const unknown = names.filter((name) => !order.includes(name));
  const ordered = [...known, ...unknown];
  if (ordered.length === 0) return undefined;
  const enchantments = enchantmentNamesForCategory(kind);
  if (
    enchantments.length > 0 &&
    ordered.length === enchantments.length &&
    enchantments.every((name) => ordered.includes(name))
  )
    return ANY_ENCHANTMENT;
  return ordered.length === 1 ? ordered[0] : ordered;
}

/** One "any of these" slot: the indices of its members in requirement order. */
export interface QuerySlot {
  key: string;
  members: number[];
}

/**
 * Groups requirements into slots: an alternative group is one slot at its
 * first member's position; every other requirement is a slot of its own.
 */
export function querySlots(requirements: readonly RequirementState[]): QuerySlot[] {
  const slots: QuerySlot[] = [];
  const byGroup = new Map<number, QuerySlot>();
  requirements.forEach((requirement, index) => {
    const group = requirement.alternativeGroup;
    if (group !== undefined) {
      const existing = byGroup.get(group);
      if (existing) {
        existing.members.push(index);
        return;
      }
      const slot = { key: `alt:${group}`, members: [index] };
      byGroup.set(group, slot);
      slots.push(slot);
      return;
    }
    slots.push({ key: `req:${index}`, members: [index] });
  });
  return slots;
}

/** The number of slots a requirement list fills, counting each alternative group once. */
export const slotCount = (requirements: readonly RequirementState[]): number =>
  querySlots(requirements).length;

/** The last floor the Blacksmith's quest can sit on: a run whose floor limit
 * reaches it always meets him, so "require Blacksmith" only matters below it. */
export const BLACKSMITH_LAST_FLOOR = 14;

/**
 * Boss floors that generate no searchable items. The core treats a floor
 * limit of 5/10/15 exactly like 4/9/14, so these are useless as bounds and
 * floor-limit selectors skip them. Floor 20 stays: the Imp shop makes the
 * City boss floor carry searchable stock.
 */
export const EMPTY_BOSS_FLOORS: readonly number[] = [5, 10, 15];

/** Floors offered by floor-limit selectors: 1 through `MAX_DEPTH` minus the empty boss floors. */
export const FLOOR_LIMIT_OPTIONS: readonly number[] = Array.from(
  { length: MAX_DEPTH },
  (_, index) => index + 1,
).filter((floor) => !EMPTY_BOSS_FLOORS.includes(floor));

/** Snaps an empty boss-floor limit to the equivalent floor below it (5→4, 10→9, 15→14). */
export const normalizeFloorLimit = (value: number): number =>
  EMPTY_BOSS_FLOORS.includes(value) ? value - 1 : value;

/**
 * The selector index of `value` within `options`; off-list values snap to
 * the nearest option below (or the first option). This is the snapping rule
 * every floor-limit slider uses.
 */
export const nearestOptionIndex = (options: readonly number[], value: number): number => {
  const exact = options.indexOf(value);
  if (exact >= 0) return exact;
  return options.reduce((best, option, index) => (option <= value ? index : best), 0);
};

export const defaultTier = (): TierFilter => ({ mode: "any", value: 3 });
export const defaultUpgrade = (): UpgradeFilter => ({ mode: "any", value: 1 });

export const emptyRequirement = (kind?: RequirementState["kind"]): RequirementState => ({
  kind,
  tier: defaultTier(),
  upgrade: defaultUpgrade(),
  uncursed: false,
});

export const defaultQueryState = (): QueryState => ({
  requirements: [],
  maxDepth: MAX_DEPTH,
  requireBlacksmith: false,
  excludeBlacksmithRewards: false,
  challenges: [],
});

function requirementToDocument(requirement: RequirementState): RequirementDocument {
  const output: RequirementDocument = {};
  // The category is always written, derived from the item when the editor
  // state has none: the engine's start decision compares kinds for equality,
  // so a requirement that omits its kind would share with nothing.
  const kind = requirement.kind ?? (requirement.item ? getItem(requirement.item)?.type : undefined);
  if (kind) output.kind = kind;
  if (requirement.item) output.item = requirement.item;
  if (requirement.tier.mode !== "any") {
    output.tier = { [requirement.tier.mode]: requirement.tier.value } as NonNullable<
      RequirementDocument["tier"]
    >;
  }
  if (requirement.upgrade.mode === "exact") output.upgrade = requirement.upgrade.value;
  if (requirement.upgrade.mode === "at_least")
    output.upgrade = { at_least: requirement.upgrade.value };
  if (requirement.effect !== undefined) {
    const effect = canonicalEffect(effectNamesOf(requirement.effect, kind), kind);
    if (effect !== undefined) output.effect = effect;
  }
  if (requirement.uncursed) output.uncursed = true;
  if (requirement.source) output.source = requirement.source;
  if (requirement.identityGroup) output.identity_group = requirement.identityGroup;
  if (requirement.maxDepth !== undefined) output.max_depth = requirement.maxDepth;
  if (requirement.levelSum)
    output.level_sum = {
      group: requirement.levelSum.group,
      at_least: requirement.levelSum.atLeast,
    };
  return output;
}

export function toQueryDocument(state: QueryState): QueryDocument {
  // An alternative group is one any_of entry at its first member's position;
  // a group of one is a plain requirement.
  const entries: RequirementEntryDocument[] = querySlots(state.requirements).map((slot) => {
    const members = slot.members.map((index) => requirementToDocument(state.requirements[index]));
    return members.length === 1 ? members[0] : { any_of: members };
  });
  const output: QueryDocument = { requirements: entries };
  if (state.maxDepth !== MAX_DEPTH) output.max_depth = state.maxDepth;
  if (state.requireBlacksmith) output.require_blacksmith = true;
  if (state.excludeBlacksmithRewards) output.exclude_blacksmith_rewards = true;
  if (state.wandmakerQuest) output.wandmaker_quest = state.wandmakerQuest;
  if (state.challenges.length) output.challenges = [...state.challenges];
  return output;
}

export function toQueryJson(state: QueryState): string {
  return JSON.stringify(toQueryDocument(state));
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** Decodes the wire tier forms: absent, "any", or a single-key filter object. */
function tierFromDocument(value: unknown): TierFilter {
  if (value === undefined) return defaultTier();
  if (typeof value === "string") {
    if (value.toLowerCase() === "any") return defaultTier();
    throw new Error(`unknown tier mode "${value}"`);
  }
  if (isRecord(value) && Object.keys(value).length === 1) {
    if (typeof value.exact === "number") return { mode: "exact", value: value.exact };
    if (typeof value.at_least === "number") return { mode: "at_least", value: value.at_least };
    if (typeof value.at_most === "number") return { mode: "at_most", value: value.at_most };
  }
  throw new Error("unrecognized tier filter");
}

/** Decodes the wire upgrade forms: absent, "any", a number, or a single-key filter object. */
function upgradeFromDocument(value: unknown): UpgradeFilter {
  if (value === undefined) return defaultUpgrade();
  if (typeof value === "number") return { mode: "exact", value };
  if (typeof value === "string") {
    if (value.toLowerCase() === "any") return defaultUpgrade();
    throw new Error(`unknown upgrade mode "${value}"`);
  }
  if (isRecord(value) && Object.keys(value).length === 1) {
    if (typeof value.exact === "number") return { mode: "exact", value: value.exact };
    if (typeof value.at_least === "number") return { mode: "at_least", value: value.at_least };
  }
  throw new Error("unrecognized upgrade filter");
}

/** Rejects unknown quest names rather than silently widening the filter. */
function wandmakerQuestFromDocument(value: unknown): WandmakerQuest | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value === "string") {
    if ((WANDMAKER_QUESTS as readonly string[]).includes(value)) return value as WandmakerQuest;
    throw new Error(`unknown Wandmaker quest "${value}"`);
  }
  throw new Error("unrecognized Wandmaker quest filter");
}

/** Decodes the wire effect forms: absent, a bare name, or a list of names; anything else is an error. */
function effectFromDocument(value: unknown, kind: string | undefined): EffectFilter | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value === "string") return value;
  if (Array.isArray(value) && value.every((name) => typeof name === "string")) {
    // Lists are stored canonically so a round trip is the identity.
    return canonicalEffect(value as string[], kind);
  }
  throw new Error("unrecognized effect filter");
}

function levelSumFromDocument(value: unknown): RequirementState["levelSum"] {
  if (value === undefined || value === null) return undefined;
  if (isRecord(value) && typeof value.group === "number" && typeof value.at_least === "number") {
    return { group: value.group, atLeast: value.at_least };
  }
  throw new Error("unrecognized level_sum");
}

function requirementFromDocument(
  value: RequirementDocument,
  alternativeGroup?: number,
): RequirementState {
  const raw = value as Record<string, unknown>;
  // Same rule as the encoder: an item-only requirement gets its item's
  // category, so the state a share link or a results file restores carries
  // the kind the start decision needs.
  const kind = value.kind ?? (value.item ? getItem(value.item)?.type : undefined);
  const requirement: RequirementState = {
    kind,
    item: value.item,
    tier: tierFromDocument(raw.tier),
    upgrade: upgradeFromDocument(raw.upgrade),
    effect: effectFromDocument(raw.effect, kind),
    uncursed: value.uncursed ?? false,
    source: value.source,
    identityGroup: value.identity_group,
    maxDepth: value.max_depth === undefined ? undefined : normalizeFloorLimit(value.max_depth),
  };
  if (alternativeGroup !== undefined) requirement.alternativeGroup = alternativeGroup;
  // The unreleased upgrade_sum key is refused rather than reinterpreted.
  if (raw.upgrade_sum !== undefined)
    throw new Error("upgrade_sum is no longer supported; use level_sum");
  const levelSum = levelSumFromDocument(raw.level_sum);
  if (levelSum) requirement.levelSum = levelSum;
  return requirement;
}

/** Flattens the entries: any_of groups get fresh sequential group ids in document order. */
function requirementsFromDocument(entries: RequirementEntryDocument[]): RequirementState[] {
  const requirements: RequirementState[] = [];
  let nextGroup = 0;
  for (const entry of entries) {
    if (isRecord(entry) && "any_of" in entry) {
      const members = (entry as { any_of: unknown }).any_of;
      if (!Array.isArray(members) || members.length === 0)
        throw new Error("any_of needs at least one alternative");
      nextGroup += 1;
      for (const member of members) {
        if (!isRecord(member) || "any_of" in member) throw new Error("any_of groups cannot nest");
        requirements.push(requirementFromDocument(member as RequirementDocument, nextGroup));
      }
      continue;
    }
    requirements.push(requirementFromDocument(entry as RequirementDocument));
  }
  return requirements;
}

/**
 * Decodes a stored query document. Keys this release no longer writes are
 * ignored rather than rejected, matching the engine's codec: a saved query, a
 * preset or a share link written before fast mode was removed still loads, and
 * its `fast_mode` flag simply has no effect.
 */
export function fromQueryJson(json: string): QueryState {
  const document = JSON.parse(json) as QueryDocument;
  if (!isRecord(document) || !Array.isArray(document.requirements))
    throw new Error("a query needs a requirements list");
  if (document.challenges !== undefined && !Array.isArray(document.challenges))
    throw new Error("challenges must be a list of challenge names");
  return {
    requirements: requirementsFromDocument(document.requirements),
    maxDepth: normalizeFloorLimit(document.max_depth ?? MAX_DEPTH),
    requireBlacksmith: document.require_blacksmith ?? false,
    excludeBlacksmithRewards: document.exclude_blacksmith_rewards ?? false,
    wandmakerQuest: wandmakerQuestFromDocument(document.wandmaker_quest),
    challenges: document.challenges ? [...document.challenges] : [],
  };
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
}

export function validateRequirement(requirement: RequirementState): string[] {
  const errors: string[] = [];
  const item = requirement.item ? getItem(requirement.item) : undefined;
  const kind = requirement.kind ?? item?.type;
  const family = kind ? kindFamily(kind) : undefined;
  const weaponClass = kind ? kindWeaponClass(kind) : undefined;
  if (!kind) errors.push("Choose an item category.");
  if (item && requirement.kind && item.type !== family)
    errors.push("The item does not belong to this category.");
  else if (item && weaponClass && item.class !== weaponClass)
    errors.push(`The item is not a ${weaponClass} weapon.`);
  if (requirement.tier.mode !== "any") {
    if (requirement.item || (family !== "weapon" && family !== "armor"))
      errors.push("Tier filters require a wildcard weapon or armor.");
    const { mode, value } = requirement.tier;
    if (mode === "exact" && (value < EXACT_TIER_MIN || value > EXACT_TIER_MAX))
      errors.push(`Exact tier must be ${EXACT_TIER_MIN} through ${EXACT_TIER_MAX}.`);
    if (
      (mode === "at_least" || mode === "at_most") &&
      (value < BOUNDED_TIER_MIN || value > BOUNDED_TIER_MAX)
    )
      errors.push(`Tier bounds must be ${BOUNDED_TIER_MIN} or ${BOUNDED_TIER_MAX}.`);
  }
  if (requirement.upgrade.mode !== "any") {
    const maximum = maxUpgradeOf(requirement);
    const minimum = requirement.upgrade.mode === "exact" ? 1 : 0;
    if (requirement.upgrade.value < minimum || requirement.upgrade.value > maximum) {
      errors.push(
        maximum < maxUpgradeFor(family)
          ? `Upgrade must be ${minimum} through +${maximum}; only a tier-${EXTRA_UPGRADE_TIER} weapon reaches +${MAX_UPGRADE_WEAPON}.`
          : `Upgrade must be ${minimum} through +${maximum}.`,
      );
    }
  }
  if (
    requirement.maxDepth !== undefined &&
    (requirement.maxDepth < 1 || requirement.maxDepth > MAX_DEPTH)
  )
    errors.push(`Requirement floor must be 1 through ${MAX_DEPTH}.`);
  if (requirement.effect !== undefined) {
    if (family !== "weapon" && family !== "armor")
      errors.push("Effects require a weapon or armor category.");
    else if (kind) {
      const names = effectNamesOf(requirement.effect, kind);
      const known = effectNamesForCategory(kind);
      const unknown = names.filter((name) => !known.includes(name));
      if (unknown.length > 0)
        errors.push(`The effect ${unknown.join(", ")} does not belong to this category.`);
      else if (names.length === 0) errors.push("Choose at least one effect.");
      else if (requirement.uncursed && names.every((name) => isCurseForCategory(kind, name))) {
        errors.push(
          names.length === 1
            ? "An uncursed item cannot have a curse effect."
            : "An uncursed item cannot have only curse effects.",
        );
      }
    }
  }
  if (requirement.levelSum) {
    const { group, atLeast } = requirement.levelSum;
    if (group < 1 || group > LEVEL_SUM_GROUP_MAX)
      errors.push(`A combined-level group must be 1 through ${LEVEL_SUM_GROUP_MAX}.`);
    if (atLeast < 1) errors.push("A combined level must be at least 1.");
    if (requirementFamily(requirement) !== "ring")
      errors.push("Only rings can count levels together.");
    if (requirement.alternativeGroup !== undefined)
      errors.push("An either/or alternative cannot count a combined level.");
  }
  if (
    requirement.identityGroup !== undefined &&
    (requirement.identityGroup < 1 || requirement.identityGroup > IDENTITY_GROUP_MAX)
  ) {
    errors.push(`A stack group must be 1 through ${IDENTITY_GROUP_MAX}.`);
  }
  return errors;
}

export function validateQuery(state: QueryState): ValidationResult {
  const errors: string[] = [];
  if (!state.requirements.length) errors.push("Add at least one requirement.");
  if (state.maxDepth < 1 || state.maxDepth > MAX_DEPTH)
    errors.push(`Maximum floor must be 1 through ${MAX_DEPTH}.`);
  state.requirements.forEach((requirement, index) => {
    for (const error of validateRequirement(requirement))
      errors.push(`Requirement ${index + 1}: ${error}`);
  });
  // A stack (identity group) has one anchor unit — a lone requirement or
  // one alternative group — that may constrain the item it binds to; every
  // other member is a bare copy of the same category.
  const identityMembers = new Map<number, number[]>();
  state.requirements.forEach((requirement, index) => {
    if (!requirement.identityGroup) return;
    identityMembers.set(requirement.identityGroup, [
      ...(identityMembers.get(requirement.identityGroup) ?? []),
      index,
    ]);
  });
  for (const members of identityMembers.values()) {
    const families = new Set(members.map((index) => requirementFamily(state.requirements[index])));
    if (families.size > 1) {
      errors.push("The copies of a stack must share its category.");
      continue;
    }
    // The constrained members must all live in one unit.
    const units = new Set(
      members
        .filter((index) => !isBareRequirement(state.requirements[index]))
        .map((index) =>
          state.requirements[index].alternativeGroup === undefined
            ? `req:${index}`
            : `alt:${state.requirements[index].alternativeGroup}`,
        ),
    );
    if (units.size > 1)
      errors.push("Only one item of a stack can carry constraints; the extra copies are plain.");
  }
  // Combined-level groups: one shared, reachable total, counted in levels
  // (upgrade plus one per item).
  const sumMembers = new Map<number, RequirementState[]>();
  for (const requirement of state.requirements) {
    if (!requirement.levelSum) continue;
    sumMembers.set(requirement.levelSum.group, [
      ...(sumMembers.get(requirement.levelSum.group) ?? []),
      requirement,
    ]);
  }
  for (const members of sumMembers.values()) {
    const totals = new Set(members.map((member) => member.levelSum?.atLeast));
    if (totals.size > 1) {
      errors.push("A stack must share one combined level.");
      continue;
    }
    const needed = members[0].levelSum?.atLeast ?? 0;
    const capacity = levelSumCapacity(members);
    if (needed > capacity)
      errors.push(
        `A combined level of ${needed} needs more items: these ${members.length} can reach ${capacity}.`,
      );
  }
  return { valid: errors.length === 0, errors };
}
