import Foundation
import SeedSeekerKit
import XCTest

/// The glow table and atlas geometry must stay identical to the web reference in
/// `web/src/lib/glow.ts` and `web/src/lib/sprites.ts`; these pin the values that
/// would silently drift apart otherwise.
final class GlowTableTests: XCTestCase {
    func testEnchantmentColoursAndPeriods() {
        XCTAssertEqual(enchantmentGlows["Blazing"], ItemGlow(hex: "#ff4400", period: 1))
        XCTAssertEqual(enchantmentGlows["Chilling"], ItemGlow(hex: "#00ffff", period: 1))
        XCTAssertEqual(enchantmentGlows["Corrupting"], ItemGlow(hex: "#440066", period: 1))
        XCTAssertEqual(enchantmentGlows["Anti-Magic"], ItemGlow(hex: "#88eeff", period: 1))
        // Shocking and Potential are the only entries with a non-default period.
        XCTAssertEqual(enchantmentGlows["Shocking"], ItemGlow(hex: "#ffffff", period: 0.5))
        XCTAssertEqual(enchantmentGlows["Potential"], ItemGlow(hex: "#ffffff", period: 0.6))
        XCTAssertEqual(enchantmentGlows.count, 26)
        for (name, glow) in enchantmentGlows where name != "Shocking" && name != "Potential" {
            XCTAssertEqual(glow.period, ItemGlow.defaultPeriod, "\(name) should use the default period")
        }
    }

    func testEveryCatalogEnchantmentAndGlyphHasAGlow() {
        for name in ItemCatalog.enchantments + ItemCatalog.glyphs {
            XCTAssertNotNil(enchantmentGlows[name], "missing glow for \(name)")
        }
    }

    func testCursesGlowBlackAtTheDefaultPeriod() {
        XCTAssertEqual(curseGlow, ItemGlow(hex: "#000000", period: 1))
        for curse in ItemCatalog.weaponCurses + ItemCatalog.armorCurses {
            XCTAssertEqual(effectGlow(curse), curseGlow, "\(curse) should glow black")
        }
    }

    func testEffectGlowTreatsUnknownNamesAsCurses() {
        XCTAssertNil(effectGlow(nil))
        XCTAssertNil(effectGlow(""))
        XCTAssertEqual(effectGlow("Not An Enchantment"), curseGlow)
        XCTAssertEqual(effectGlow("Kinetic"), ItemGlow(hex: "#ffff00"))
    }

    func testEnchantmentWinsOverCurseOnTheSameItem() {
        let weapon = ItemCatalog.findById("greatsword")!
        let kinetic = ScoutItem(item: weapon, depth: 3, upgrade: 2, effect: "Kinetic",
                                cursed: true, source: .chest)
        XCTAssertEqual(itemGlow(kinetic), ItemGlow(hex: "#ffff00"))

        let cursedOnly = ScoutItem(item: weapon, depth: 3, upgrade: 0, effect: nil,
                                   cursed: true, source: .chest)
        XCTAssertEqual(itemGlow(cursedOnly), curseGlow)

        let curseEffect = ScoutItem(item: weapon, depth: 3, upgrade: 0, effect: "Wayward",
                                    cursed: true, source: .chest)
        XCTAssertEqual(itemGlow(curseEffect), curseGlow)

        let plain = ScoutItem(item: weapon, depth: 3, upgrade: 0, source: .chest)
        XCTAssertNil(itemGlow(plain))

        // Wands and rings never carry an effect and never glow.
        let ring = ScoutItem(item: ItemCatalog.findById("ring_wealth")!, depth: 9, upgrade: 3,
                             source: .tomb)
        XCTAssertNil(itemGlow(ring))
    }

    func testGlowLerpMaths() {
        // Upstream blends `texel*(1-value) + glow*value` with `value` peaking at
        // 0.6 over a `2 × period` cycle; the UI animates the masked colour
        // layer's opacity along exactly that ramp.
        let glow = enchantmentGlows["Shocking"]!
        XCTAssertEqual(glow.cycleDuration, 1.0, accuracy: 1e-9)
        XCTAssertEqual(enchantmentGlows["Blazing"]!.cycleDuration, 2.0, accuracy: 1e-9)
        XCTAssertEqual(ItemGlow.peakOpacity, 0.6, accuracy: 1e-9)
        XCTAssertEqual(ItemGlow.reducedMotionOpacity, 0.3, accuracy: 1e-9)

        let (red, green, blue) = ItemGlow(hex: "#8844cc").components
        XCTAssertEqual(red, 0x88 / 255.0, accuracy: 1e-9)
        XCTAssertEqual(green, 0x44 / 255.0, accuracy: 1e-9)
        XCTAssertEqual(blue, 0xCC / 255.0, accuracy: 1e-9)

        // A white sprite pixel at peak glow lands 60% of the way to the colour.
        let blended = 1.0 * (1 - ItemGlow.peakOpacity) + red * ItemGlow.peakOpacity
        XCTAssertEqual(blended, 0.4 + 0.6 * (0x88 / 255.0), accuracy: 1e-9)
    }
}

final class SpriteGeometryTests: XCTestCase {
    func testRingGlyphIndexMapping() {
        XCTAssertNil(SpriteSheet.ringIconIndex(223))
        XCTAssertEqual(SpriteSheet.ringIconIndex(224), 0)
        XCTAssertEqual(SpriteSheet.ringIconIndex(235), 11)
        XCTAssertNil(SpriteSheet.ringIconIndex(236))
        XCTAssertNil(SpriteSheet.ringIconIndex(112))
        XCTAssertEqual(SpriteSheet.ringIconSizes.count, 12)
        // The glyph atlas sizes must line up index-for-index with the rings.
        for ring in ItemCatalog.rings {
            let icon = SpriteSheet.ringIconIndex(ring.spriteIndex)
            XCTAssertNotNil(icon, "\(ring.id) has no ring glyph")
        }
        XCTAssertEqual(SpriteSheet.ringIconSizes[3], PixelSize(width: 7, height: 5))  // Energy
        XCTAssertEqual(SpriteSheet.ringIconSizes[5], PixelSize(width: 5, height: 6))  // Force
        XCTAssertEqual(SpriteSheet.ringIconSizes[11], PixelSize(width: 7, height: 6)) // Wealth
    }

    func testEveryCatalogSpriteIsInsideTheAtlas() {
        // items.png is 256×512 — 16 columns of 16×16 cells over 32 rows.
        for item in ItemCatalog.all {
            XCTAssertTrue((0..<(16 * 32)).contains(item.spriteIndex),
                          "\(item.id) sprite \(item.spriteIndex) is outside the atlas")
        }
    }
}

/// Bounding boxes measured from the real atlas. The PNGs are not bundled with
/// the test binary, so these load the canonical copy from the repository and
/// skip when it is unavailable.
@MainActor final class SpriteAtlasTests: XCTestCase {
    private static var atlasDirectory: URL {
        URL(fileURLWithPath: #filePath)                       // …/Tests/SeedSeekerKitTests/x.swift
            .deletingLastPathComponent()                      // …/Tests/SeedSeekerKitTests
            .deletingLastPathComponent()                      // …/Tests
            .deletingLastPathComponent()                      // …/macos/SeedSeeker
            .deletingLastPathComponent()                      // …/macos
            .deletingLastPathComponent()                      // repository root
            .appendingPathComponent("android/app/src/main/assets/third_party/shattered-pixel-dungeon")
    }

    private func loadAtlas() throws -> SpriteAtlas {
        let directory = Self.atlasDirectory
        let items = directory.appendingPathComponent("items.png")
        let icons = directory.appendingPathComponent("item_icons.png")
        try XCTSkipUnless(FileManager.default.fileExists(atPath: items.path),
                          "atlas not available at \(items.path)")
        return try XCTUnwrap(SpriteAtlas(itemsURL: items, iconsURL: icons))
    }

    /// Expected values produced by `scripts/generate-sprite-bounds.py`, the same
    /// generator the web app's `sprite-bounds.json` comes from.
    func testMeasuredBoundsMatchTheWebGenerator() throws {
        let atlas = try loadAtlas()
        let expected: [Int: SpriteBounds] = [
            96: SpriteBounds(x: 0, y: 0, width: 13, height: 13),   // worn shortsword
            // One of only two cells whose art is inset from the cell origin —
            // it catches a vertically flipped alpha read.
            101: SpriteBounds(x: 0, y: 2, width: 15, height: 14),
            112: SpriteBounds(x: 0, y: 0, width: 14, height: 14),  // sword
            147: SpriteBounds(x: 0, y: 0, width: 12, height: 10),  // throwing stone
            161: SpriteBounds(x: 0, y: 0, width: 15, height: 15),  // rot dart
            178: SpriteBounds(x: 0, y: 0, width: 14, height: 12),  // mail armor
            209: SpriteBounds(x: 0, y: 0, width: 14, height: 14),  // wand of fireblast
            224: SpriteBounds(x: 0, y: 0, width: 8, height: 10),   // ring of accuracy
            235: SpriteBounds(x: 0, y: 0, width: 8, height: 10),   // ring of wealth
        ]
        for (index, bounds) in expected.sorted(by: { $0.key < $1.key }) {
            XCTAssertEqual(atlas.bounds(forSprite: index), bounds, "sprite \(index)")
        }
    }

    func testBoundsFallBackToTheFullCell() throws {
        let atlas = try loadAtlas()
        let fullCell = SpriteBounds(x: 0, y: 0, width: 16, height: 16)
        // Out-of-range indices and empty cells fall back to the whole cell, the
        // same fallback `spriteBoxCss` uses on the web.
        XCTAssertEqual(atlas.bounds(forSprite: -1), fullCell)
        XCTAssertEqual(atlas.bounds(forSprite: 100_000), fullCell)
    }

    /// The glow masks the art layer, so a cursed ring's type glyph must not be
    /// in it — otherwise the glyph telling Wealth from Energy pulses black too.
    func testRingLayersSplitTheGlyphOutOfTheArt() throws {
        let atlas = try loadAtlas()
        let wealth = 235
        let whole = try XCTUnwrap(atlas.composedSprite(spriteIndex: wealth, pointSize: 32))
        let art = try XCTUnwrap(atlas.composedSprite(spriteIndex: wealth, pointSize: 32, layer: .art))
        let glyph = try XCTUnwrap(atlas.composedSprite(spriteIndex: wealth, pointSize: 32,
                                                       layer: .typeIcon))
        // Ring of Wealth's art is 8×10 of a 16×16 cell, centred; its glyph is
        // 7×6 anchored top-right. At 32 pt (64 px, scale 4) that is x 16…47 for
        // the art and x 36…63, y 0…23 for the glyph.
        XCTAssertEqual(opaqueBounds(art), SpriteBounds(x: 16, y: 12, width: 32, height: 40))
        XCTAssertEqual(opaqueBounds(glyph), SpriteBounds(x: 36, y: 0, width: 28, height: 24))
        XCTAssertEqual(opaqueBounds(whole), SpriteBounds(x: 16, y: 0, width: 48, height: 52))

        // Non-rings have no glyph layer at all, and their art is the whole icon.
        let sword = 112
        XCTAssertNil(atlas.composedSprite(spriteIndex: sword, pointSize: 32, layer: .typeIcon))
        XCTAssertEqual(opaqueBounds(try XCTUnwrap(atlas.composedSprite(spriteIndex: sword,
                                                                      pointSize: 32, layer: .art))),
                       opaqueBounds(try XCTUnwrap(atlas.composedSprite(spriteIndex: sword,
                                                                      pointSize: 32))))
    }

    /// The picker needs clearance to the right of a ring's glyph, but the scout
    /// list must keep the geometry it has: the margin may only add transparent
    /// width, never move the art or the glyph.
    func testTypeIconMarginOnlyAddsTrailingTransparency() throws {
        let atlas = try loadAtlas()
        let wealth = 235
        let bare = try XCTUnwrap(atlas.composedSprite(spriteIndex: wealth, pointSize: 16))
        let margined = try XCTUnwrap(atlas.composedSprite(spriteIndex: wealth, pointSize: 16,
                                                          typeIconMargin: 2))
        XCTAssertEqual(bare.width, 16 * SpriteAtlas.pixelScale)
        XCTAssertEqual(margined.width, (16 + 2) * SpriteAtlas.pixelScale)
        XCTAssertEqual(margined.height, bare.height)
        // Identical ink: the extra width is empty, so nothing shifted.
        XCTAssertEqual(opaqueBounds(margined), opaqueBounds(bare))

        // A margin asked for on a non-ring is ignored — no glyph, no crowding —
        // so those sprites stay square and share one cache entry.
        let sword = 112
        let square = try XCTUnwrap(atlas.composedSprite(spriteIndex: sword, pointSize: 16,
                                                        typeIconMargin: 2))
        XCTAssertEqual(square.width, 16 * SpriteAtlas.pixelScale)
        XCTAssertTrue(square === atlas.composedSprite(spriteIndex: sword, pointSize: 16))
    }

    /// The bounding box of an image's non-transparent pixels, origin top-left.
    private func opaqueBounds(_ image: CGImage) -> SpriteBounds {
        let width = image.width, height = image.height
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        pixels.withUnsafeMutableBytes { buffer in
            let context = CGContext(data: buffer.baseAddress, width: width, height: height,
                                    bitsPerComponent: 8, bytesPerRow: width * 4,
                                    space: CGColorSpaceCreateDeviceRGB(),
                                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            context?.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        }
        var minX = width, minY = height, maxX = -1, maxY = -1
        for y in 0..<height {
            for x in 0..<width where pixels[(y * width + x) * 4 + 3] > 8 {
                minX = min(minX, x); minY = min(minY, y)
                maxX = max(maxX, x); maxY = max(maxY, y)
            }
        }
        guard maxX >= 0 else { return SpriteBounds(x: 0, y: 0, width: 0, height: 0) }
        return SpriteBounds(x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1)
    }

    func testComposedSpriteIsRasterisedAtTheRetinaPixelScale() throws {
        let atlas = try loadAtlas()
        let image = try XCTUnwrap(atlas.composedSprite(spriteIndex: 235, pointSize: 32))
        XCTAssertEqual(image.width, 32 * SpriteAtlas.pixelScale)
        XCTAssertEqual(image.height, 32 * SpriteAtlas.pixelScale)
        // Cached, so the same call returns the identical bitmap.
        XCTAssertTrue(image === atlas.composedSprite(spriteIndex: 235, pointSize: 32))
    }
}
