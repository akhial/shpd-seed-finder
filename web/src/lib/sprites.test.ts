import { describe, expect, it } from 'vitest'
import spriteBounds from '../generated/sprite-bounds.json'
import { getItem, items } from './catalog'
import { RING_SPRITE_BASE, itemArt, ringGlyphIndex } from './sprites'
import type { RingGems } from './sprites'

/**
 * The `ringGems` the engine reports for seed YKH-LGJ-WDQ, which the game draws
 * with a diamond ring of haste. Kept literal so this test fails if the wire
 * order of the table ever changes under the app.
 */
const YKH_LGJ_WDQ: RingGems = [7, 8, 3, 5, 4, 6, 2, 11, 10, 1, 0, 9]

const bounds = spriteBounds as Record<string, number[]>
const rings = items.filter((item) => item.type === 'ring')

describe('itemArt', () => {
  it("draws a seed's ring with the gem that seed gave it, keeping the class glyph", () => {
    const haste = getItem('ring_haste')
    expect(haste?.sprite).toBe(RING_SPRITE_BASE + 7)
    // The whole bug in one assertion: the art moves onto the diamond, eleven
    // cells along, while the glyph that names the ring stays put.
    expect(itemArt(haste!.sprite, YKH_LGJ_WDQ)).toEqual({
      cell: RING_SPRITE_BASE + 11,
      ringGlyph: 7,
    })
  })

  it('gives a seedless surface the catalog cell', () => {
    for (const ring of rings) {
      expect(itemArt(ring.sprite)).toEqual({ cell: ring.sprite, ringGlyph: ring.typeIcon })
    }
  })

  it("permutes the ring block, so a run's twelve rings stay twelve distinct colours", () => {
    const cells = rings.map((ring) => itemArt(ring.sprite, YKH_LGJ_WDQ).cell)
    expect(new Set(cells).size).toBe(rings.length)
    expect([...cells].sort((a, b) => a - b)).toEqual(
      rings.map((_, offset) => RING_SPRITE_BASE + offset),
    )
  })

  it("reads each ring's glyph as the one the catalog states outright", () => {
    expect(rings).toHaveLength(12)
    for (const ring of rings) {
      expect(ring.typeIcon).toBe(ringGlyphIndex(ring.sprite))
      expect(itemArt(ring.sprite, YKH_LGJ_WDQ).ringGlyph).toBe(ring.typeIcon)
    }
  })

  it('leaves everything that is not a ring where it was', () => {
    for (const item of items.filter((entry) => entry.type !== 'ring')) {
      expect(ringGlyphIndex(item.sprite)).toBeUndefined()
      expect(itemArt(item.sprite, YKH_LGJ_WDQ)).toEqual({ cell: item.sprite })
    }
  })

  it('lands only on cells with measured art bounds, so the box still centres', () => {
    // `spriteBoxCss` crops to the drawn cell's bounds; a gem the catalog never
    // points at would otherwise fall back to the whole 16x16 cell.
    for (let gem = 0; gem < 12; gem += 1) {
      expect(bounds[String(RING_SPRITE_BASE + gem)]).toBeDefined()
    }
  })
})
