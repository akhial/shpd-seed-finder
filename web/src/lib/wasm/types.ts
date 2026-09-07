export type ItemCategory = "weapon" | "armor" | "wand" | "ring" | "trinket";

/** Melee/thrown classification carried by weapon catalog entries. */
export type WeaponClass = "melee" | "thrown";

/**
 * Category filter of one requirement. `weapon` matches melee and thrown
 * weapons alike (the historical behavior); the two narrowed kinds restrict a
 * weapon requirement to one class.
 */
export type RequirementKind = ItemCategory | "melee_weapon" | "thrown_weapon";

export type ChallengeName =
  | "on_diet"
  | "faith_is_my_armor"
  | "pharmacophobia"
  | "barren_land"
  | "swarm_intelligence"
  | "into_darkness"
  | "forbidden_runes"
  | "hostile_champions"
  | "badder_bosses";

export type ItemSource =
  | "heap"
  | "chest"
  | "locked_chest"
  | "crystal_chest"
  | "tomb"
  | "skeleton"
  | "sacrificial_fire"
  | "mimic"
  | "golden_mimic"
  | "crystal_mimic"
  | "statue"
  | "armored_statue"
  | "shop"
  | "ghost_reward"
  | "wandmaker_reward"
  | "blacksmith_reward"
  | "imp_reward"
  | "vault_treasure";

export interface TierFilter {
  mode: "any" | "exact" | "at_least" | "at_most";
  value: number;
}

export interface UpgradeFilter {
  mode: "any" | "exact" | "at_least";
  value: number;
}

/** The effect shorthand standing for every non-curse effect of the item's family. */
export const ANY_ENCHANTMENT = "any_enchantment";

/**
 * Which effects an item may carry: one wire name (e.g. `"Blazing"`), a list of
 * same-family wire names in catalog order, or `ANY_ENCHANTMENT`. Absent means
 * any effect or none. This is the document's own shape, so old presets that
 * store a bare name load unchanged.
 */
export type EffectFilter = string | string[];

/**
 * Membership in a combined-level group: some subset of the group's members,
 * filled by distinct items, must reach `atLeast` combined levels, where an
 * item counts as its upgrade plus one. Members are optional — one +2 ring
 * satisfies a two-member group asking for three levels.
 */
export interface LevelSum {
  group: number;
  atLeast: number;
}

export interface RequirementState {
  kind?: RequirementKind;
  item?: string;
  tier: TierFilter;
  upgrade: UpgradeFilter;
  effect?: EffectFilter;
  uncursed: boolean;
  source?: ItemSource;
  identityGroup?: number;
  maxDepth?: number;
  /** Requirements sharing a number form one "any of these" slot. */
  alternativeGroup?: number;
  levelSum?: LevelSum;
}

export interface QueryState {
  requirements: RequirementState[];
  maxDepth: number;
  requireBlacksmith: boolean;
  excludeBlacksmithRewards: boolean;
  /** Which Wandmaker quest a seed must roll; undefined matches any. */
  wandmakerQuest?: WandmakerQuest;
  challenges: ChallengeName[];
}

export type TierDocument = "any" | { exact: number } | { at_least: number } | { at_most: number };
export type UpgradeDocument = number | "any" | { exact: number } | { at_least: number };

export interface RequirementDocument {
  kind?: RequirementKind;
  item?: string;
  tier?: TierDocument;
  upgrade?: UpgradeDocument;
  effect?: EffectFilter;
  uncursed?: true;
  source?: ItemSource;
  identity_group?: number;
  max_depth?: number;
  level_sum?: { group: number; at_least: number };
}

/** An "any of these" slot: satisfied by any single member. Members may not carry `level_sum`. */
export interface AnyOfDocument {
  any_of: RequirementDocument[];
}

export type RequirementEntryDocument = RequirementDocument | AnyOfDocument;

/** The keys this release writes. Documents saved by older releases may carry
 * retired keys such as `fast_mode`; both the engine's codec and `fromQueryJson`
 * accept and ignore them. */
export interface QueryDocument {
  requirements: RequirementEntryDocument[];
  max_depth?: number;
  require_blacksmith?: true;
  exclude_blacksmith_rewards?: true;
  wandmaker_quest?: WandmakerQuest;
  challenges?: ChallengeName[];
}

/** The query bounds `SearchQuery::validate` itself applies, plus the
 * results-file limit every frontend must agree on. */
export interface EngineLimits {
  maxDepth: number;
  exactTierMin: number;
  exactTierMax: number;
  boundedTierMin: number;
  boundedTierMax: number;
  identityGroupMax: number;
  levelSumGroupMax: number;
  maxUpgradeDefault: number;
  maxUpgradeRing: number;
  maxUpgradeRingStandard: number;
  maxUpgradeWeapon: number;
  maxUpgradeAnyTier: number;
  extraUpgradeTier: number;
  resultsFileMaxBytes: number;
}

/** One challenge as the engine lists it, in mask order. */
export interface EngineChallenge {
  name: ChallengeName;
  mask: number;
  /** True for the challenges the level generator itself consults. */
  changesLevelGeneration: boolean;
}

/**
 * The engine's constants document (`engine_info`). The app only reads the
 * first four at runtime; the rest exist so `engine-constants.test.ts` can
 * check the app's local copies of them against the engine.
 */
export interface EngineInfo {
  shpdVersion: string;
  shpdCommit: string;
  totalSeeds: number;
  maxResults: number;
  limits: EngineLimits;
  emptyBossFloors: number[];
  /** Inclusive `[first, last]` depth window per quest. */
  questWindows: Record<QuestName, [number, number]>;
  challenges: EngineChallenge[];
  searchStartStride: number;
}

export interface ParsedSeed {
  code: string;
  value: number;
}

export type AnalysisResult =
  | { valid: false; error: string }
  | { valid: true; probability: number | null; impossible: boolean; notes: string[] };

export interface SearchAdvance {
  state: "running" | "completed";
  tested: number;
  matches: ParsedSeed[];
}

export type Accessibility =
  | { type: "independent" }
  | { type: "choice"; group: number; option: number }
  | { type: "scenarios"; group: number; mask: string };

export interface ScoutItem {
  id: string;
  name: string;
  category: ItemCategory;
  /**
   * The item's *catalog* cell, which names the item and never varies by seed.
   * For a ring it is the class's own cell, whose offset from the ring block is
   * the class's glyph; the cell to actually draw comes from `ringGems` below.
   * Resolve both through `itemArt`.
   */
  spriteIndex: number;
  upgrade: number;
  effect: { name: string; kind: "enchantment" | "curse" } | null;
  cursed: boolean;
  secret: boolean;
  depth: number;
  source: ItemSource;
  accessibility: Accessibility;
  matched: boolean;
}

export interface ScoutRequest {
  seed: string;
  challenges?: ChallengeName[];
  query?: QueryDocument;
}

export type QuestName = "ghost" | "wandmaker" | "blacksmith" | "imp";

/** The three quests the Prison's Wandmaker can roll, in wire-id order. */
export const WANDMAKER_QUESTS = ["corpse_dust", "elemental_embers", "rotberry"] as const;

export type WandmakerQuest = (typeof WANDMAKER_QUESTS)[number];

export type QuestVariant =
  | "fetid_rat"
  | "gnoll_trickster"
  | "great_crab"
  | "corpse_dust"
  | "elemental_embers"
  | "rotberry"
  | "crystal"
  | "gnoll"
  // v4.0.0 replaced the Imp's Monk/Golem token hunts with one vault expedition.
  | "vault";

export interface ScoutQuest {
  quest: QuestName;
  variant: QuestVariant;
  depth: number;
}

export interface TrinketOffer {
  id: string;
  name: string;
  spriteIndex: number;
}

export interface ScoutResult {
  /** Full private-deck order; only entries 0..3 are initial catalyst offers. */
  trinketOrder?: TrinketOffer[];
  seed: ParsedSeed;
  items: ScoutItem[];
  /**
   * The gem this run draws each ring class with, in catalog ring order. The
   * game shuffles `Ring.gems` once per run, so a seed decides what colour each
   * ring is: an item's ring cell is `RING_SPRITE_BASE` plus its class's entry
   * here. Only items from this scout may be resolved against it.
   */
  ringGems: number[];
  quests: ScoutQuest[];
  matchedRequirements: number;
  totalRequirements: number;
}
