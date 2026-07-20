import type { ScoutItem } from './wasm/types'

/**
 * Enchantment / glyph glow colours and pulse periods, mirrored 1:1 from
 * Shattered Pixel Dungeon's `ItemSprite.Glowing` definitions so the scout icons
 * pulse exactly as the game renders them. `period` is the seconds the glow takes
 * to fade fully in — it fades back out over the same span, so a complete pulse
 * cycle lasts `2 × period`. Curses always glow black, matching the game.
 */
export interface Glow {
  /** Hex colour the sprite blends toward at the pulse peak. */
  color: string
  /** Seconds to reach peak glow; the full fade-in/out cycle lasts twice this. */
  period: number
}

/** Upstream's default `Glowing(color)` period when none is given (1f). */
const DEFAULT_PERIOD = 1

// Keyed by the wire names the scout emits (WeaponEffect / ArmorEffect
// `wire_name` in seedfinder-core). Only non-curse effects live here; every curse
// glows black and is handled by CURSE_GLOW below.
const ENCHANT_GLOW: Record<string, Glow> = {
  // Weapon enchantments
  Blazing: { color: '#ff4400', period: DEFAULT_PERIOD },
  Chilling: { color: '#00ffff', period: DEFAULT_PERIOD },
  Kinetic: { color: '#ffff00', period: DEFAULT_PERIOD },
  Shocking: { color: '#ffffff', period: 0.5 },
  Blocking: { color: '#0000ff', period: DEFAULT_PERIOD },
  Blooming: { color: '#008800', period: DEFAULT_PERIOD },
  Elastic: { color: '#ff00ff', period: DEFAULT_PERIOD },
  Lucky: { color: '#00ff00', period: DEFAULT_PERIOD },
  Projecting: { color: '#8844cc', period: DEFAULT_PERIOD },
  Unstable: { color: '#999999', period: DEFAULT_PERIOD },
  Corrupting: { color: '#440066', period: DEFAULT_PERIOD },
  Grim: { color: '#000000', period: DEFAULT_PERIOD },
  Vampiric: { color: '#660022', period: DEFAULT_PERIOD },
  // Armor glyphs
  Obfuscation: { color: '#888888', period: DEFAULT_PERIOD },
  Swiftness: { color: '#ffff00', period: DEFAULT_PERIOD },
  Viscosity: { color: '#8844cc', period: DEFAULT_PERIOD },
  Potential: { color: '#ffffff', period: 0.6 },
  Brimstone: { color: '#ff4400', period: DEFAULT_PERIOD },
  Stone: { color: '#222222', period: DEFAULT_PERIOD },
  Entanglement: { color: '#663300', period: DEFAULT_PERIOD },
  Repulsion: { color: '#ffffff', period: DEFAULT_PERIOD },
  Camouflage: { color: '#448822', period: DEFAULT_PERIOD },
  Flow: { color: '#0000ff', period: DEFAULT_PERIOD },
  Affection: { color: '#ff4488', period: DEFAULT_PERIOD },
  'Anti-Magic': { color: '#88eeff', period: DEFAULT_PERIOD },
  Thorns: { color: '#660022', period: DEFAULT_PERIOD },
}

/** Every curse glows black in the game, at the default period. */
const CURSE_GLOW: Glow = { color: '#000000', period: DEFAULT_PERIOD }

/**
 * The pulse glow for a scouted item, or null when it carries no enchantment or
 * curse. A beneficial enchantment/glyph wins even on a cursed item (matching
 * `Weapon.glowing()`, which returns the enchantment's colour when one is
 * present — e.g. a curse-infused Kinetic weapon still glows yellow); otherwise a
 * cursed item pulses black.
 */
export function itemGlow(item: Pick<ScoutItem, 'cursed' | 'effect'>): Glow | null {
  if (item.effect?.kind === 'enchantment') return ENCHANT_GLOW[item.effect.name] ?? null
  if (item.cursed) return CURSE_GLOW
  return null
}

/**
 * The pulse glow for a bare effect name (as carried by a requirement), or null
 * when there is none. Known enchantments/glyphs pulse their colour; any other
 * effect name is a curse and pulses black.
 */
export function effectGlow(effect: string | undefined): Glow | null {
  if (!effect) return null
  return ENCHANT_GLOW[effect] ?? CURSE_GLOW
}
