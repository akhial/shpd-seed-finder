import catalogJson from '../generated/catalog.json'
import type { ChallengeName, ItemCategory, ItemSource, RequirementKind, WeaponClass } from './wasm/types'

export interface CatalogItem { id: string; name: string; type: ItemCategory; class?: WeaponClass; tier?: number; sprite: number }
interface CatalogDocument {
  entries: CatalogItem[]
  modifiers?: {
    weaponEnchantments?: string[]
    weaponCurses?: string[]
    armorGlyphs?: string[]
    armorCurses?: string[]
  }
}

const catalog = catalogJson as CatalogDocument
export const items = catalog.entries
export const itemsByCategory = Object.fromEntries(
  (['weapon', 'armor', 'wand', 'ring'] as ItemCategory[]).map((category) => [category, items.filter((item) => item.type === category)]),
) as Record<ItemCategory, CatalogItem[]>
const lookup = new Map(items.map((item) => [item.id, item]))

/** The broad item family a requirement kind belongs to. */
export const kindFamily = (kind: RequirementKind): ItemCategory =>
  kind === 'melee_weapon' || kind === 'thrown_weapon' ? 'weapon' : kind

/** Weapon class a narrowed requirement kind selects, if any. */
export const kindWeaponClass = (kind: RequirementKind): WeaponClass | undefined =>
  kind === 'melee_weapon' ? 'melee' : kind === 'thrown_weapon' ? 'thrown' : undefined

/** Catalog items selectable under one requirement kind. */
export const itemsForKind = (kind: RequirementKind): CatalogItem[] => {
  const weaponClass = kindWeaponClass(kind)
  const family = itemsByCategory[kindFamily(kind)]
  return weaponClass ? family.filter((item) => item.class === weaponClass) : family
}
export const getItem = (id: string): CatalogItem | undefined => lookup.get(id)
export const displayItemName = (id: string): string => getItem(id)?.name ?? id.replaceAll('_', ' ')

const fallback = {
  weaponEnchantments: ['Blazing', 'Chilling', 'Kinetic', 'Shocking', 'Blocking', 'Blooming', 'Elastic', 'Lucky', 'Projecting', 'Unstable', 'Corrupting', 'Grim', 'Vampiric'],
  weaponCurses: ['Annoying', 'Displacing', 'Dazzling', 'Explosive', 'Sacrificial', 'Wayward', 'Polarized', 'Friendly'],
  armorGlyphs: ['Obfuscation', 'Swiftness', 'Viscosity', 'Potential', 'Brimstone', 'Stone', 'Entanglement', 'Repulsion', 'Camouflage', 'Flow', 'Affection', 'Anti-Magic', 'Thorns'],
  armorCurses: ['Anti-Entropy', 'Corrosion', 'Displacement', 'Metabolism', 'Multiplicity', 'Stench', 'Overgrowth', 'Bulk'],
}

export const weaponEnchantments = catalog.modifiers?.weaponEnchantments ?? fallback.weaponEnchantments
export const weaponCurses = catalog.modifiers?.weaponCurses ?? fallback.weaponCurses
export const armorGlyphs = catalog.modifiers?.armorGlyphs ?? fallback.armorGlyphs
export const armorCurses = catalog.modifiers?.armorCurses ?? fallback.armorCurses
const weaponish = (category: string): boolean =>
  category === 'weapon' || category === 'melee_weapon' || category === 'thrown_weapon'
export const effectNamesForCategory = (category: string): string[] => weaponish(category)
  ? [...weaponEnchantments, ...weaponCurses]
  : category === 'armor' ? [...armorGlyphs, ...armorCurses] : []
export const isCurseForCategory = (category: string, effect: string): boolean =>
  weaponish(category) ? weaponCurses.includes(effect) : category === 'armor' ? armorCurses.includes(effect) : false

export const sources: { value: ItemSource; label: string }[] = [
  ['heap', 'Heap'], ['chest', 'Chest'], ['locked_chest', 'Locked Chest'], ['crystal_chest', 'Crystal Chest'],
  ['tomb', 'Tomb'], ['skeleton', 'Skeleton'], ['sacrificial_fire', 'Sacrificial Fire'], ['mimic', 'Mimic'],
  ['golden_mimic', 'Golden Mimic'], ['crystal_mimic', 'Crystal Mimic'], ['statue', 'Statue'],
  ['armored_statue', 'Armored Statue'], ['shop', 'Shop'], ['ghost_reward', 'Ghost Reward'],
  ['wandmaker_reward', 'Wandmaker Reward'], ['blacksmith_reward', 'Blacksmith Reward'], ['imp_reward', 'Imp Reward'],
].map(([value, label]) => ({ value: value as ItemSource, label }))
export const sourceLabel = (source: ItemSource): string => sources.find((entry) => entry.value === source)?.label ?? source

export const challenges: { value: ChallengeName; label: string }[] = [
  ['on_diet', 'On Diet'], ['faith_is_my_armor', 'Faith is my Armor'], ['pharmacophobia', 'Pharmacophobia'],
  ['barren_land', 'Barren Land'], ['swarm_intelligence', 'Swarm Intelligence'], ['into_darkness', 'Into Darkness'],
  ['forbidden_runes', 'Forbidden Runes'], ['hostile_champions', 'Hostile Champions'], ['badder_bosses', 'Badder Bosses'],
].map(([value, label]) => ({ value: value as ChallengeName, label }))

export const wildcardSprites: Record<ItemCategory, number> = { weapon: 112, armor: 178, wand: 209, ring: 224 }
/** Wildcard sprite for a requirement kind; thrown weapons show a shuriken. */
export const wildcardSpriteForKind = (kind: RequirementKind): number =>
  kind === 'thrown_weapon' ? 149 : wildcardSprites[kindFamily(kind)]
