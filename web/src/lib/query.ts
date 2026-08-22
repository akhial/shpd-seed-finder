import { getItem, isCurseForCategory, kindFamily, kindWeaponClass } from './catalog'
import { WANDMAKER_QUESTS } from './wasm/types'
import type {
  QueryDocument,
  QueryState,
  RequirementDocument,
  RequirementState,
  TierFilter,
  UpgradeFilter,
  WandmakerQuest,
} from './wasm/types'

// The query bounds below are local copies of the engine's own constants
// (`crates/seedfinder-core/src/engine_info.rs`). They stay local so the app
// never has to wait on the wasm module to render; `engine-constants.test.ts`
// asserts each of them against the engine's `engine_info` document instead.

/** Deepest floor a search may cover. */
export const MAX_DEPTH = 24

/** Tiers an "exactly tier N" requirement may name (tier 1 is starting gear). */
export const EXACT_TIER_MIN = 2
export const EXACT_TIER_MAX = 5

/** Tiers an "at least / at most tier N" requirement may name; the ends of the
 * exact range would be redundant with "any" or "exactly". */
export const BOUNDED_TIER_MIN = 3
export const BOUNDED_TIER_MAX = 4

/** Highest same-item group number (groups run 1..this, shown as A..D). */
export const IDENTITY_GROUP_MAX = 4

/** The highest upgrade a search may name for an item family. */
export const MAX_UPGRADE_DEFAULT = 3
export const MAX_UPGRADE_RING = 4
export const maxUpgradeFor = (family: string | undefined): number =>
  (family === 'ring' ? MAX_UPGRADE_RING : MAX_UPGRADE_DEFAULT)

/** The last floor the Blacksmith's quest can sit on: a run whose floor limit
 * reaches it always meets him, so "require Blacksmith" only matters below it. */
export const BLACKSMITH_LAST_FLOOR = 14

/**
 * Boss floors that generate no searchable items. The core treats a floor
 * limit of 5/10/15 exactly like 4/9/14, so these are useless as bounds and
 * floor-limit selectors skip them. Floor 20 stays: the Imp shop makes the
 * City boss floor carry searchable stock.
 */
export const EMPTY_BOSS_FLOORS: readonly number[] = [5, 10, 15]

/** Floors offered by floor-limit selectors: 1 through `MAX_DEPTH` minus the empty boss floors. */
export const FLOOR_LIMIT_OPTIONS: readonly number[] = Array.from({ length: MAX_DEPTH }, (_, index) => index + 1)
  .filter((floor) => !EMPTY_BOSS_FLOORS.includes(floor))

/** Snaps an empty boss-floor limit to the equivalent floor below it (5→4, 10→9, 15→14). */
export const normalizeFloorLimit = (value: number): number =>
  (EMPTY_BOSS_FLOORS.includes(value) ? value - 1 : value)

/**
 * The selector index of `value` within `options`; off-list values snap to
 * the nearest option below (or the first option). This is the snapping rule
 * every floor-limit slider uses.
 */
export const nearestOptionIndex = (options: readonly number[], value: number): number => {
  const exact = options.indexOf(value)
  if (exact >= 0) return exact
  return options.reduce((best, option, index) => (option <= value ? index : best), 0)
}

export const defaultTier = (): TierFilter => ({ mode: 'any', value: 3 })
export const defaultUpgrade = (): UpgradeFilter => ({ mode: 'any', value: 1 })

export const emptyRequirement = (kind?: RequirementState['kind']): RequirementState => ({
  kind,
  tier: defaultTier(),
  upgrade: defaultUpgrade(),
  uncursed: false,
})

export const defaultQueryState = (): QueryState => ({
  requirements: [],
  maxDepth: MAX_DEPTH,
  requireBlacksmith: false,
  excludeBlacksmithRewards: false,
  fastMode: false,
  challenges: [],
})

function requirementToDocument(requirement: RequirementState): RequirementDocument {
  const output: RequirementDocument = {}
  // The category is always written, derived from the item when the editor
  // state has none: the engine's start decision compares kinds for equality,
  // so a requirement that omits its kind would share with nothing.
  const kind = requirement.kind ?? (requirement.item ? getItem(requirement.item)?.type : undefined)
  if (kind) output.kind = kind
  if (requirement.item) output.item = requirement.item
  if (requirement.tier.mode !== 'any') {
    output.tier = { [requirement.tier.mode]: requirement.tier.value } as NonNullable<RequirementDocument['tier']>
  }
  if (requirement.upgrade.mode === 'exact') output.upgrade = requirement.upgrade.value
  if (requirement.upgrade.mode === 'at_least') output.upgrade = { at_least: requirement.upgrade.value }
  if (requirement.effect) output.effect = requirement.effect
  if (requirement.uncursed) output.uncursed = true
  if (requirement.source) output.source = requirement.source
  if (requirement.identityGroup) output.identity_group = requirement.identityGroup
  if (requirement.maxDepth !== undefined) output.max_depth = requirement.maxDepth
  return output
}

export function toQueryDocument(state: QueryState): QueryDocument {
  const output: QueryDocument = { requirements: state.requirements.map(requirementToDocument) }
  if (state.maxDepth !== MAX_DEPTH) output.max_depth = state.maxDepth
  if (state.requireBlacksmith) output.require_blacksmith = true
  if (state.excludeBlacksmithRewards) output.exclude_blacksmith_rewards = true
  if (state.wandmakerQuest) output.wandmaker_quest = state.wandmakerQuest
  if (state.fastMode) output.fast_mode = true
  if (state.challenges.length) output.challenges = [...state.challenges]
  return output
}

export function toQueryJson(state: QueryState): string {
  return JSON.stringify(toQueryDocument(state))
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

/** Decodes the wire tier forms: absent, "any", or a single-key filter object. */
function tierFromDocument(value: unknown): TierFilter {
  if (value === undefined) return defaultTier()
  if (typeof value === 'string') {
    if (value.toLowerCase() === 'any') return defaultTier()
    throw new Error(`unknown tier mode "${value}"`)
  }
  if (isRecord(value) && Object.keys(value).length === 1) {
    if (typeof value.exact === 'number') return { mode: 'exact', value: value.exact }
    if (typeof value.at_least === 'number') return { mode: 'at_least', value: value.at_least }
    if (typeof value.at_most === 'number') return { mode: 'at_most', value: value.at_most }
  }
  throw new Error('unrecognized tier filter')
}

/** Decodes the wire upgrade forms: absent, "any", a number, or a single-key filter object. */
function upgradeFromDocument(value: unknown): UpgradeFilter {
  if (value === undefined) return defaultUpgrade()
  if (typeof value === 'number') return { mode: 'exact', value }
  if (typeof value === 'string') {
    if (value.toLowerCase() === 'any') return defaultUpgrade()
    throw new Error(`unknown upgrade mode "${value}"`)
  }
  if (isRecord(value) && Object.keys(value).length === 1) {
    if (typeof value.exact === 'number') return { mode: 'exact', value: value.exact }
    if (typeof value.at_least === 'number') return { mode: 'at_least', value: value.at_least }
  }
  throw new Error('unrecognized upgrade filter')
}

/** Rejects unknown quest names rather than silently widening the filter. */
function wandmakerQuestFromDocument(value: unknown): WandmakerQuest | undefined {
  if (value === undefined || value === null) return undefined
  if (typeof value === 'string' && (WANDMAKER_QUESTS as readonly string[]).includes(value)) return value as WandmakerQuest
  throw new Error(`unknown Wandmaker quest "${String(value)}"`)
}

function requirementFromDocument(value: RequirementDocument): RequirementState {
  const raw = value as Record<string, unknown>
  return {
    // Same rule as the encoder: an item-only requirement gets its item's
    // category, so the state a share link or a results file restores carries
    // the kind the start decision needs.
    kind: value.kind ?? (value.item ? getItem(value.item)?.type : undefined),
    item: value.item,
    tier: tierFromDocument(raw.tier),
    upgrade: upgradeFromDocument(raw.upgrade),
    effect: value.effect,
    uncursed: value.uncursed ?? false,
    source: value.source,
    identityGroup: value.identity_group,
    maxDepth: value.max_depth === undefined ? undefined : normalizeFloorLimit(value.max_depth),
  }
}

export function fromQueryJson(json: string): QueryState {
  const document = JSON.parse(json) as QueryDocument
  if (!isRecord(document) || !Array.isArray(document.requirements)) throw new Error('a query needs a requirements list')
  if (document.challenges !== undefined && !Array.isArray(document.challenges)) throw new Error('challenges must be a list of challenge names')
  return {
    requirements: document.requirements.map(requirementFromDocument),
    maxDepth: normalizeFloorLimit(document.max_depth ?? MAX_DEPTH),
    requireBlacksmith: document.require_blacksmith ?? false,
    excludeBlacksmithRewards: document.exclude_blacksmith_rewards ?? false,
    wandmakerQuest: wandmakerQuestFromDocument(document.wandmaker_quest),
    fastMode: document.fast_mode ?? false,
    challenges: document.challenges ? [...document.challenges] : [],
  }
}

export interface ValidationResult { valid: boolean; errors: string[] }

export function validateRequirement(requirement: RequirementState): string[] {
  const errors: string[] = []
  const item = requirement.item ? getItem(requirement.item) : undefined
  const kind = requirement.kind ?? item?.type
  const family = kind ? kindFamily(kind) : undefined
  const weaponClass = kind ? kindWeaponClass(kind) : undefined
  if (!kind) errors.push('Choose an item category.')
  if (item && requirement.kind && item.type !== family) errors.push('The item does not belong to this category.')
  else if (item && weaponClass && item.class !== weaponClass) errors.push(`The item is not a ${weaponClass} weapon.`)
  if (requirement.tier.mode !== 'any') {
    if (requirement.item || (family !== 'weapon' && family !== 'armor')) errors.push('Tier filters require a wildcard weapon or armor.')
    const { mode, value } = requirement.tier
    if (mode === 'exact' && (value < EXACT_TIER_MIN || value > EXACT_TIER_MAX)) errors.push(`Exact tier must be ${EXACT_TIER_MIN} through ${EXACT_TIER_MAX}.`)
    if ((mode === 'at_least' || mode === 'at_most') && (value < BOUNDED_TIER_MIN || value > BOUNDED_TIER_MAX)) errors.push(`Tier bounds must be ${BOUNDED_TIER_MIN} or ${BOUNDED_TIER_MAX}.`)
  }
  if (requirement.upgrade.mode !== 'any') {
    const maximum = maxUpgradeFor(family)
    const minimum = requirement.upgrade.mode === 'exact' ? 1 : 0
    if (requirement.upgrade.value < minimum || requirement.upgrade.value > maximum) errors.push(`Upgrade must be ${minimum} through +${maximum}.`)
  }
  if (requirement.maxDepth !== undefined && (requirement.maxDepth < 1 || requirement.maxDepth > MAX_DEPTH)) errors.push(`Requirement floor must be 1 through ${MAX_DEPTH}.`)
  if (requirement.effect && family !== 'weapon' && family !== 'armor') errors.push('Effects require a weapon or armor category.')
  if (requirement.effect && kind && !isCurseForCategory(kind, requirement.effect) && !getEffectNames(kind).includes(requirement.effect)) errors.push('The effect does not belong to this category.')
  if (requirement.uncursed && requirement.effect && kind && isCurseForCategory(kind, requirement.effect)) errors.push('An uncursed item cannot have a curse effect.')
  return errors
}

function getEffectNames(kind: string): string[] {
  const { effectNamesForCategory } = catalogHelpers
  return effectNamesForCategory(kind)
}

// Kept indirect so validation remains straightforward to mock in component tests.
import { effectNamesForCategory } from './catalog'
const catalogHelpers = { effectNamesForCategory }

export function validateQuery(state: QueryState): ValidationResult {
  const errors: string[] = []
  if (!state.requirements.length) errors.push('Add at least one requirement.')
  if (state.maxDepth < 1 || state.maxDepth > MAX_DEPTH) errors.push(`Maximum floor must be 1 through ${MAX_DEPTH}.`)
  state.requirements.forEach((requirement, index) => {
    for (const error of validateRequirement(requirement)) errors.push(`Requirement ${index + 1}: ${error}`)
  })
  const groups = new Map<number, { kind?: string; item?: string }>()
  state.requirements.forEach((requirement) => {
    if (!requirement.identityGroup) return
    const current = {
      kind: requirement.kind ? kindFamily(requirement.kind) : getItem(requirement.item ?? '')?.type,
      item: requirement.item,
    }
    const previous = groups.get(requirement.identityGroup)
    if (previous && (previous.kind !== current.kind || (previous.item && current.item && previous.item !== current.item))) {
      errors.push(`Identity group ${requirement.identityGroup} has incompatible category or item requirements.`)
    } else if (!previous || (!previous.item && current.item)) {
      groups.set(requirement.identityGroup, current)
    }
  })
  return { valid: errors.length === 0, errors }
}
