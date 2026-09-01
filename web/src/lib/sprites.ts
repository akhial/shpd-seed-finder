import type { CSSProperties } from "react";
import spriteBounds from "../generated/sprite-bounds.json";
import type { Glow } from "./glow";

const SHEET_URL = "/third_party/shattered-pixel-dungeon/items.png";
const SHEET_COLUMNS = 16;
const CELL = 16;

const bounds: Record<string, number[]> = spriteBounds;

// Rings all share the same gemmed base sprite and are distinguished only by a
// small type glyph overlaid on top — never by colour. The glyphs live in a
// separate 8×8-cell atlas; these constants and per-glyph art sizes mirror the
// Android client's Components.kt so the two stay pixel-identical.
const ICON_SHEET_URL = "/third_party/shattered-pixel-dungeon/item_icons.png";
const ICON_COLUMNS = 16;
const ICON_CELL = 8;

/**
 * Atlas cell of the first ring sprite (`ItemSpriteSheet.RINGS`).
 *
 * The twelve cells from here on are the twelve *gems*, not the twelve ring
 * classes: the game shuffles `Ring.gems` once per run and hands each class the
 * gem at its own index, so which cell a ring is drawn in is decided by the
 * seed. The catalog still gives every class its own cell in this block, in
 * class order, because a surface with no seed has no run to ask — and that
 * offset doubles as the class's glyph index.
 */
export const RING_SPRITE_BASE = 224;

// Art dimensions (w, h) of each ring glyph within its 8×8 cell, index-aligned
// to the ring classes (Accuracy, Arcana, Elements, … Wealth).
const RING_ICON_SIZES: [number, number][] = [
  [7, 7],
  [7, 7],
  [7, 7],
  [7, 5],
  [7, 7],
  [5, 6],
  [7, 6],
  [6, 6],
  [7, 7],
  [7, 7],
  [6, 6],
  [7, 6],
];

/**
 * The gem each ring class is drawn with in one run, in catalog ring order:
 * the scout document's `ringGems`.
 */
export type RingGems = readonly number[];

/**
 * The ring-class glyph (0…11) a *catalog* sprite index names, or undefined for
 * everything that is not a ring.
 *
 * Only ever ask this of a catalog index. The catalog's ring cells are the gem
 * block read in class order, which is what makes the offset the class's glyph;
 * the cell a ring is actually drawn in is the run's gem for that class and
 * says nothing about which ring it is.
 */
export function ringGlyphIndex(catalogSpriteIndex: number): number | undefined {
  const glyph = catalogSpriteIndex - RING_SPRITE_BASE;
  return glyph >= 0 && glyph < RING_ICON_SIZES.length ? glyph : undefined;
}

/**
 * How to draw one item: the `items.png` cell the art comes from, and the ring
 * type glyph laid over it (absent for everything that is not a ring). The two
 * are separate inputs because a ring's cell belongs to the run while its glyph
 * belongs to the class — deriving either from the other recolours every seed
 * the same way. Build one with {@link itemArt}.
 */
export interface ItemArt {
  cell: number;
  ringGlyph?: number;
}

/**
 * Resolve a catalog sprite index into the art to draw.
 *
 * Pass `gems` — the scout document's `ringGems` — whenever the item belongs to
 * a specific seed, and a ring lands on the gem that run gave its class. Omit
 * them on seedless surfaces (the requirement editor, the query builder, saved
 * presets), which have no run and so keep the catalog's per-class cell. The
 * glyph is the class's either way, so the ring stays identifiable.
 */
export function itemArt(catalogSpriteIndex: number, gems?: RingGems): ItemArt {
  const ringGlyph = ringGlyphIndex(catalogSpriteIndex);
  if (ringGlyph === undefined) return { cell: catalogSpriteIndex };
  const gem = gems?.[ringGlyph];
  return {
    cell: gem === undefined ? catalogSpriteIndex : RING_SPRITE_BASE + gem,
    ringGlyph,
  };
}

/**
 * Overlay CSS for a ring's type glyph, anchored to the sprite's top-right corner
 * exactly as the Android client draws it. Takes the glyph itself rather than the
 * drawn cell, and returns undefined when there is none — that is, for non-rings.
 * Meant to sit inside a position:relative sprite box.
 */
export function ringIconCss(
  ringGlyph: number | undefined,
  size: number,
): CSSProperties | undefined {
  if (ringGlyph === undefined || ringGlyph < 0 || ringGlyph >= RING_ICON_SIZES.length)
    return undefined;
  const [width, height] = RING_ICON_SIZES[ringGlyph];
  const scale = size / CELL;
  const col = ringGlyph % ICON_COLUMNS;
  const row = Math.floor(ringGlyph / ICON_COLUMNS);
  return {
    position: "absolute",
    top: 0,
    right: 0,
    width: `${width * scale}px`,
    height: `${height * scale}px`,
    backgroundImage: `url(${ICON_SHEET_URL})`,
    backgroundPosition: `${-col * ICON_CELL * scale}px ${-row * ICON_CELL * scale}px`,
    backgroundSize: `${ICON_COLUMNS * ICON_CELL * scale}px auto`,
    imageRendering: "pixelated",
    pointerEvents: "none",
  };
}

export function spriteCss(index: number, size: number): CSSProperties {
  const scale = size / CELL;
  const col = index % SHEET_COLUMNS;
  const row = Math.floor(index / SHEET_COLUMNS);
  return {
    width: `${size}px`,
    height: `${size}px`,
    backgroundImage: `url(${SHEET_URL})`,
    backgroundPosition: `${-col * CELL * scale}px ${-row * CELL * scale}px`,
    backgroundSize: `${SHEET_COLUMNS * CELL * scale}px auto`,
    imageRendering: "pixelated",
    flex: "0 0 auto",
  };
}

export interface SpriteBoxCss {
  outer: CSSProperties;
  inner: CSSProperties;
}

/**
 * Sprite art is anchored to the top-left of its 16x16 sheet cell, so rendering
 * the full cell leaves small items (rings, seeds) hugging the top-left corner.
 * This crops to the art's measured bounding box and centers it in a size×size
 * box, keeping the pixel scale identical to a full-cell render.
 *
 * `cell` is the cell actually drawn ({@link ItemArt.cell}), not the catalog
 * index it came from: the bounds are measured per cell, so a ring resolved onto
 * another run's gem is cropped to that gem's own art.
 */
export function spriteBoxCss(cell: number, size: number): SpriteBoxCss {
  const [x, y, width, height] = bounds[String(cell)] ?? [0, 0, CELL, CELL];
  const scale = size / CELL;
  const col = cell % SHEET_COLUMNS;
  const row = Math.floor(cell / SHEET_COLUMNS);
  return {
    outer: {
      position: "relative",
      width: `${size}px`,
      height: `${size}px`,
      display: "inline-flex",
      alignItems: "center",
      justifyContent: "center",
      flex: "0 0 auto",
    },
    inner: {
      position: "relative",
      width: `${width * scale}px`,
      height: `${height * scale}px`,
      backgroundImage: `url(${SHEET_URL})`,
      backgroundPosition: `${-(col * CELL + x) * scale}px ${-(row * CELL + y) * scale}px`,
      backgroundSize: `${SHEET_COLUMNS * CELL * scale}px auto`,
      imageRendering: "pixelated",
    },
  };
}

/** How much gradient one glow's turn occupies, and its blend into the next. */
const GLOW_BAND = 160;
const GLOW_FADE = 12;

/**
 * Overlay CSS for an enchantment/curse glow, sized to sit exactly on top of the
 * `inner` sprite art (as its child, inset 0). A solid colour layer is masked to
 * the sprite's opaque pixels so only the art tints; animating the layer's
 * opacity from 0 to 0.6 blends the sprite toward the glow colour, reproducing
 * upstream's `texel*(1-value) + glow*value` glow shader. The pulse period is
 * supplied via the `.d1-sprite-glow` animation; `2 x period` seconds per cycle.
 *
 * Given several glows the sprite takes them in turn, one pulse each: the layer
 * is painted with a wide band per colour and scrolled by exactly one band per
 * pulse, so the colour under the sprite only ever changes as the pulse passes
 * through its trough and the swap is never seen. Turns are equal and the whole
 * round lasts the sum of the glows' cycles, matching the many-effects badge, so
 * a chip's sprite and badge move through the colours together.
 */
export function spriteGlowCss(cell: number, size: number, glows: Glow[]): CSSProperties {
  const scale = size / CELL;
  const col = cell % SHEET_COLUMNS;
  const row = Math.floor(cell / SHEET_COLUMNS);
  const [x, y] = bounds[String(cell)] ?? [0, 0, CELL, CELL];
  const maskPosition = `${-(col * CELL + x) * scale}px ${-(row * CELL + y) * scale}px`;
  const maskSize = `${SHEET_COLUMNS * CELL * scale}px auto`;
  const base: CSSProperties = {
    position: "absolute",
    inset: 0,
    WebkitMaskImage: `url(${SHEET_URL})`,
    maskImage: `url(${SHEET_URL})`,
    WebkitMaskPosition: maskPosition,
    maskPosition,
    WebkitMaskSize: maskSize,
    maskSize,
    WebkitMaskRepeat: "no-repeat",
    maskRepeat: "no-repeat",
    pointerEvents: "none",
  };
  if (glows.length < 2) {
    return {
      ...base,
      backgroundColor: glows[0]?.color,
      animationDuration: `${2 * (glows[0]?.period ?? 1)}s`,
    };
  }
  const round = glows.reduce((total, glow) => total + glow.period * 2, 0);
  const width = glows.length * GLOW_BAND;
  const stops = glows.flatMap((glow, band) => [
    `${glow.color} ${band * GLOW_BAND}px`,
    `${glow.color} ${(band + 1) * GLOW_BAND - GLOW_FADE}px`,
  ]);
  stops.push(`${glows[0].color} ${width}px`);
  return {
    ...base,
    backgroundImage: `linear-gradient(90deg, ${stops.join(", ")})`,
    backgroundSize: `${width}px 100%`,
    backgroundRepeat: "repeat",
    // Start with a band boundary halfway across the sprite: that is where the
    // pulse begins, at zero opacity, so every later swap lands there too.
    "--d1-glow-from": `${size / 2}px`,
    "--d1-glow-to": `${size / 2 - width}px`,
    animationDuration: `${round / glows.length}s, ${round}s`,
  } as CSSProperties;
}
