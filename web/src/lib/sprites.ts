import type { CSSProperties } from 'react'
import spriteBounds from '../generated/sprite-bounds.json'

const SHEET_URL = '/third_party/shattered-pixel-dungeon/items.png'
const SHEET_COLUMNS = 16
const CELL = 16

const bounds: Record<string, number[]> = spriteBounds

// Rings all share the same gemmed base sprite and are distinguished only by a
// small type glyph overlaid on top — never by colour. The glyphs live in a
// separate 8×8-cell atlas; these constants and per-glyph art sizes mirror the
// Android client's Components.kt so the two stay pixel-identical.
const ICON_SHEET_URL = '/third_party/shattered-pixel-dungeon/item_icons.png'
const ICON_COLUMNS = 16
const ICON_CELL = 8
const RING_SPRITE_BASE = 224
// Art dimensions (w, h) of each ring glyph within its 8×8 cell, index-aligned to
// ring sprites 224…235 (Accuracy, Arcana, Elements, … Wealth).
const RING_ICON_SIZES: [number, number][] = [
  [7, 7], [7, 7], [7, 7], [7, 5], [7, 7], [5, 6],
  [7, 6], [6, 6], [7, 7], [7, 7], [6, 6], [7, 6],
]

/** The ring-type glyph index (0…11) for a base ring sprite, or undefined for non-rings. */
export function ringIconIndex(spriteIndex: number): number | undefined {
  const icon = spriteIndex - RING_SPRITE_BASE
  return icon >= 0 && icon < RING_ICON_SIZES.length ? icon : undefined
}

/**
 * Overlay CSS for a ring's type glyph, anchored to the sprite's top-right corner
 * exactly as the Android client draws it. Returns undefined when the sprite is
 * not a ring. Meant to sit inside a position:relative sprite box.
 */
export function ringIconCss(spriteIndex: number, size: number): CSSProperties | undefined {
  const icon = ringIconIndex(spriteIndex)
  if (icon === undefined) return undefined
  const [width, height] = RING_ICON_SIZES[icon]
  const scale = size / CELL
  const col = icon % ICON_COLUMNS
  const row = Math.floor(icon / ICON_COLUMNS)
  return {
    position: 'absolute',
    top: 0,
    right: 0,
    width: `${width * scale}px`,
    height: `${height * scale}px`,
    backgroundImage: `url(${ICON_SHEET_URL})`,
    backgroundPosition: `${-col * ICON_CELL * scale}px ${-row * ICON_CELL * scale}px`,
    backgroundSize: `${ICON_COLUMNS * ICON_CELL * scale}px auto`,
    imageRendering: 'pixelated',
    pointerEvents: 'none',
  }
}

export function spriteCss(index: number, size: number): CSSProperties {
  const scale = size / CELL
  const col = index % SHEET_COLUMNS
  const row = Math.floor(index / SHEET_COLUMNS)
  return {
    width: `${size}px`,
    height: `${size}px`,
    backgroundImage: `url(${SHEET_URL})`,
    backgroundPosition: `${-col * CELL * scale}px ${-row * CELL * scale}px`,
    backgroundSize: `${SHEET_COLUMNS * CELL * scale}px auto`,
    imageRendering: 'pixelated',
    flex: '0 0 auto',
  }
}

export interface SpriteBoxCss { outer: CSSProperties; inner: CSSProperties }

/**
 * Sprite art is anchored to the top-left of its 16x16 sheet cell, so rendering
 * the full cell leaves small items (rings, seeds) hugging the top-left corner.
 * This crops to the art's measured bounding box and centers it in a size×size
 * box, keeping the pixel scale identical to a full-cell render.
 */
export function spriteBoxCss(index: number, size: number): SpriteBoxCss {
  const [x, y, width, height] = bounds[String(index)] ?? [0, 0, CELL, CELL]
  const scale = size / CELL
  const col = index % SHEET_COLUMNS
  const row = Math.floor(index / SHEET_COLUMNS)
  return {
    outer: {
      position: 'relative',
      width: `${size}px`,
      height: `${size}px`,
      display: 'inline-flex',
      alignItems: 'center',
      justifyContent: 'center',
      flex: '0 0 auto',
    },
    inner: {
      position: 'relative',
      width: `${width * scale}px`,
      height: `${height * scale}px`,
      backgroundImage: `url(${SHEET_URL})`,
      backgroundPosition: `${-(col * CELL + x) * scale}px ${-(row * CELL + y) * scale}px`,
      backgroundSize: `${SHEET_COLUMNS * CELL * scale}px auto`,
      imageRendering: 'pixelated',
    },
  }
}

/**
 * Overlay CSS for an enchantment/curse glow, sized to sit exactly on top of the
 * `inner` sprite art (as its child, inset 0). A solid colour layer is masked to
 * the sprite's opaque pixels so only the art tints; animating the layer's
 * opacity from 0 to 0.6 blends the sprite toward `color`, reproducing upstream's
 * `texel*(1-value) + glow*value` glow shader. The pulse period is supplied via
 * the `.d1-sprite-glow` animation; `2 × period` seconds per full cycle.
 */
export function spriteGlowCss(index: number, size: number, color: string, period: number): CSSProperties {
  const scale = size / CELL
  const col = index % SHEET_COLUMNS
  const row = Math.floor(index / SHEET_COLUMNS)
  const [x, y] = bounds[String(index)] ?? [0, 0, CELL, CELL]
  const maskPosition = `${-(col * CELL + x) * scale}px ${-(row * CELL + y) * scale}px`
  const maskSize = `${SHEET_COLUMNS * CELL * scale}px auto`
  return {
    position: 'absolute',
    inset: 0,
    backgroundColor: color,
    WebkitMaskImage: `url(${SHEET_URL})`,
    maskImage: `url(${SHEET_URL})`,
    WebkitMaskPosition: maskPosition,
    maskPosition,
    WebkitMaskSize: maskSize,
    maskSize,
    WebkitMaskRepeat: 'no-repeat',
    maskRepeat: 'no-repeat',
    animationDuration: `${2 * period}s`,
    pointerEvents: 'none',
  }
}
