export type ItemCategory = 'weapon' | 'armor' | 'wand' | 'ring'

/** Melee/thrown classification carried by weapon catalog entries. */
export type WeaponClass = 'melee' | 'thrown'

/**
 * Category filter of one requirement. `weapon` matches melee and thrown
 * weapons alike (the historical behavior); the two narrowed kinds restrict a
 * weapon requirement to one class.
 */
export type RequirementKind = ItemCategory | 'melee_weapon' | 'thrown_weapon'

export type ChallengeName =
  | 'on_diet'
  | 'faith_is_my_armor'
  | 'pharmacophobia'
  | 'barren_land'
  | 'swarm_intelligence'
  | 'into_darkness'
  | 'forbidden_runes'
  | 'hostile_champions'
  | 'badder_bosses'

export type ItemSource =
  | 'heap'
  | 'chest'
  | 'locked_chest'
  | 'crystal_chest'
  | 'tomb'
  | 'skeleton'
  | 'sacrificial_fire'
  | 'mimic'
  | 'golden_mimic'
  | 'crystal_mimic'
  | 'statue'
  | 'armored_statue'
  | 'shop'
  | 'ghost_reward'
  | 'wandmaker_reward'
  | 'blacksmith_reward'
  | 'imp_reward'

export interface TierFilter { mode: 'any' | 'exact' | 'at_least' | 'at_most'; value: number }

export interface UpgradeFilter { mode: 'any' | 'exact' | 'at_least'; value: number }

/**
 * Effect predicate: absent means any effect or none. `any_enchantment` wants
 * some non-curse enchantment/glyph; `one_of` wants one of the named effects.
 */
export type EffectFilter =
  | { mode: 'any_enchantment' }
  | { mode: 'one_of'; names: string[] }

/** Minimum combined upgrade total shared by requirements in one group. */
export interface UpgradeSumFilter { group: number; atLeast: number }

export interface RequirementState {
  kind?: RequirementKind
  item?: string
  tier: TierFilter
  upgrade: UpgradeFilter
  effect?: EffectFilter
  uncursed: boolean
  source?: ItemSource
  identityGroup?: number
  maxDepth?: number
  /** Requirements sharing a group are alternatives: any one match suffices. */
  alternativeGroup?: number
  upgradeSum?: UpgradeSumFilter
}

export interface QueryState {
  requirements: RequirementState[]
  maxDepth: number
  requireBlacksmith: boolean
  excludeBlacksmithRewards: boolean
  fastMode: boolean
  challenges: ChallengeName[]
}

export type TierDocument = { exact: number } | { at_least: number } | { at_most: number }
export type UpgradeDocument = number | { at_least: number }

export interface RequirementDocument {
  kind?: RequirementKind
  item?: string
  tier?: TierDocument
  upgrade?: UpgradeDocument
  /** One name, a one-of list, or the "any_enchantment" shorthand. */
  effect?: string | string[]
  uncursed?: true
  source?: ItemSource
  identity_group?: number
  max_depth?: number
  upgrade_sum?: { group: number; at_least: number }
}

/** A plain requirement, or an any_of group satisfied by any single member. */
export type RequirementEntryDocument = RequirementDocument | { any_of: RequirementDocument[] }

export interface QueryDocument {
  requirements: RequirementEntryDocument[]
  max_depth?: number
  require_blacksmith?: true
  exclude_blacksmith_rewards?: true
  fast_mode?: true
  challenges?: ChallengeName[]
}

export interface EngineInfo {
  shpdVersion: string
  shpdCommit: string
  totalSeeds: number
  maxResults: number
}

export interface ParsedSeed { code: string; value: number }

export type AnalysisResult =
  | { valid: false; error: string }
  | { valid: true; probability: number | null; impossible: boolean; notes: string[] }

export interface SearchAdvance {
  state: 'running' | 'completed'
  tested: number
  matches: ParsedSeed[]
}

export type Accessibility =
  | { type: 'independent' }
  | { type: 'choice'; group: number; option: number }
  | { type: 'scenarios'; group: number; mask: string }

export interface ScoutItem {
  id: string
  name: string
  category: ItemCategory
  spriteIndex: number
  upgrade: number
  effect: { name: string; kind: 'enchantment' | 'curse' } | null
  cursed: boolean
  depth: number
  source: ItemSource
  accessibility: Accessibility
  matched: boolean
}

export interface ScoutRequest {
  seed: string
  challenges?: ChallengeName[]
  query?: QueryDocument
}

export interface ScoutResult {
  seed: ParsedSeed
  items: ScoutItem[]
  matchedRequirements: number
  totalRequirements: number
}
