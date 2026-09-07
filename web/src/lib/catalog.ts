import catalogJson from "../generated/catalog.json";
import type {
  ChallengeName,
  ItemCategory,
  ItemSource,
  RequirementKind,
  WeaponClass,
} from "./wasm/types";

export interface CatalogItem {
  id: string;
  name: string;
  type: ItemCategory;
  class?: WeaponClass;
  tier?: number;
  /** The item's catalog cell in `items.png`; for a ring, its class's own cell. */
  sprite: number;
  /** Rings only: the class's glyph cell in `item_icons.png`. */
  typeIcon?: number;
}
interface CatalogModifiers {
  weaponEnchantments: string[];
  weaponCurses: string[];
  armorGlyphs: string[];
  armorCurses: string[];
}
interface CatalogDocument {
  entries: CatalogItem[];
  /** Required: the effect tables are the game's, never restated here. */
  modifiers: CatalogModifiers;
}

const catalog = catalogJson as CatalogDocument;
export const items: CatalogItem[] = catalog.entries;
export const itemsByCategory = Object.fromEntries(
  (["weapon", "armor", "wand", "ring", "trinket"] as ItemCategory[]).map((category) => [
    category,
    items.filter((item) => item.type === category),
  ]),
) as Record<ItemCategory, CatalogItem[]>;
const lookup = new Map(items.map((item) => [item.id, item]));

/** The broad item family a requirement kind belongs to. */
export const kindFamily = (kind: RequirementKind): ItemCategory =>
  kind === "melee_weapon" || kind === "thrown_weapon" ? "weapon" : kind;

/** Weapon class a narrowed requirement kind selects, if any. */
export const kindWeaponClass = (kind: RequirementKind): WeaponClass | undefined =>
  kind === "melee_weapon" ? "melee" : kind === "thrown_weapon" ? "thrown" : undefined;

/**
 * Tipped darts are guaranteed shop stock and can be tipped by hand, so no one
 * searches for them; they stay out of the pickers but render in scout views.
 */
const isTippedDart = (item: CatalogItem): boolean => item.id.endsWith("_dart");

/** Catalog items selectable under one requirement kind. */
export const itemsForKind = (kind: RequirementKind): CatalogItem[] => {
  const weaponClass = kindWeaponClass(kind);
  const family = itemsByCategory[kindFamily(kind)];
  const selectable = family.filter((item) => !isTippedDart(item));
  return weaponClass ? selectable.filter((item) => item.class === weaponClass) : selectable;
};
export const getItem = (id: string): CatalogItem | undefined => lookup.get(id);
export const displayItemName = (id: string): string => getItem(id)?.name ?? id.replaceAll("_", " ");

// The effect tables are generated from the game itself; a catalog without
// them is a broken build, not something to paper over with a stale copy.
const modifiers: CatalogModifiers | undefined = catalog.modifiers;
if (!modifiers) {
  throw new Error(
    'src/generated/catalog.json is missing its "modifiers" effect tables — rebuild it with scripts/build-web-wasm.sh.',
  );
}

export const weaponEnchantments = modifiers.weaponEnchantments;
export const weaponCurses = modifiers.weaponCurses;
export const armorGlyphs = modifiers.armorGlyphs;
export const armorCurses = modifiers.armorCurses;
const weaponish = (category: string): boolean =>
  category === "weapon" || category === "melee_weapon" || category === "thrown_weapon";
export const effectNamesForCategory = (category: string): string[] =>
  weaponish(category)
    ? [...weaponEnchantments, ...weaponCurses]
    : category === "armor"
      ? [...armorGlyphs, ...armorCurses]
      : [];
/** The non-curse effects (enchantments or glyphs) of a category, in catalog order. */
export const enchantmentNamesForCategory = (category: string): string[] =>
  weaponish(category) ? [...weaponEnchantments] : category === "armor" ? [...armorGlyphs] : [];
/** The curses of a category, in catalog order. */
export const curseNamesForCategory = (category: string): string[] =>
  weaponish(category) ? [...weaponCurses] : category === "armor" ? [...armorCurses] : [];
export const isCurseForCategory = (category: string, effect: string): boolean =>
  weaponish(category)
    ? weaponCurses.includes(effect)
    : category === "armor"
      ? armorCurses.includes(effect)
      : false;

export const sources: { value: ItemSource; label: string }[] = [
  ["heap", "Heap"],
  ["chest", "Chest"],
  ["locked_chest", "Locked Chest"],
  ["crystal_chest", "Crystal Chest"],
  ["tomb", "Tomb"],
  ["skeleton", "Skeleton"],
  ["sacrificial_fire", "Sacrificial Fire"],
  ["mimic", "Mimic"],
  ["golden_mimic", "Golden Mimic"],
  ["crystal_mimic", "Crystal Mimic"],
  ["statue", "Statue"],
  ["armored_statue", "Armored Statue"],
  ["shop", "Shop"],
  ["ghost_reward", "Ghost Reward"],
  ["wandmaker_reward", "Wandmaker Reward"],
  ["blacksmith_reward", "Blacksmith Reward"],
  ["imp_reward", "Imp Reward"],
  ["vault_treasure", "Vault Treasure"],
].map(([value, label]) => ({ value: value as ItemSource, label }));
export const sourceLabel = (source: ItemSource): string =>
  sources.find((entry) => entry.value === source)?.label ?? source;

export const challenges: { value: ChallengeName; label: string }[] = [
  ["on_diet", "On Diet"],
  ["faith_is_my_armor", "Faith is my Armor"],
  ["pharmacophobia", "Pharmacophobia"],
  ["barren_land", "Barren Land"],
  ["swarm_intelligence", "Swarm Intelligence"],
  ["into_darkness", "Into Darkness"],
  ["forbidden_runes", "Forbidden Runes"],
  ["hostile_champions", "Hostile Champions"],
  ["badder_bosses", "Badder Bosses"],
].map(([value, label]) => ({ value: value as ChallengeName, label }));

/**
 * The challenges the level generator consults, so enabling one changes which
 * seeds a search finds. A local copy of an engine fact: `challenges` above
 * lists every challenge in the engine's mask order, and
 * `engine-constants.test.ts` checks both against `engine_info`.
 */
export const LEVEL_GEN_CHALLENGES: ReadonlySet<ChallengeName> = new Set<ChallengeName>([
  "barren_land",
  "into_darkness",
  "forbidden_runes",
]);

export const wildcardSprites: Record<ItemCategory, number> = {
  weapon: 112,
  armor: 178,
  wand: 209,
  ring: 224,
  trinket: 70,
};
/** Wildcard sprite for a requirement kind; thrown weapons show a shuriken. */
export const wildcardSpriteForKind = (kind: RequirementKind): number =>
  kind === "thrown_weapon" ? 149 : wildcardSprites[kindFamily(kind)];
