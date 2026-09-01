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
        // The four enchantments v4.0.0 added, at upstream's own colours.
        XCTAssertEqual(enchantmentGlows["Venomous"], ItemGlow(hex: "#4400aa", period: 1))
        XCTAssertEqual(enchantmentGlows["Eldritch"], ItemGlow(hex: "#222222", period: 1))
        XCTAssertEqual(enchantmentGlows["Vorpal"], ItemGlow(hex: "#aa6666", period: 1))
        XCTAssertEqual(enchantmentGlows["Crystal"], ItemGlow(hex: "#0088ff", period: 1))
        XCTAssertEqual(enchantmentGlows.count, 30)
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
    /// The glyph is the ring *class*, carried by the catalog — never read back
    /// out of the cell the sprite is drawn in, which a run's gems decide.
    func testRingGlyphIndexComesFromTheCatalogNotTheDrawnCell() {
        XCTAssertEqual(SpriteSheet.ringIconSizes.count, 12)
        XCTAssertNil(SpriteSheet.ringIconSize(nil))
        XCTAssertNil(SpriteSheet.ringIconSize(-1))
        XCTAssertNil(SpriteSheet.ringIconSize(12))
        XCTAssertEqual(SpriteSheet.ringIconSize(3), PixelSize(width: 7, height: 5))  // Energy
        XCTAssertEqual(SpriteSheet.ringIconSize(5), PixelSize(width: 5, height: 6))  // Force
        XCTAssertEqual(SpriteSheet.ringIconSize(11), PixelSize(width: 7, height: 6)) // Wealth

        // Every ring class carries a glyph, in catalog order; nothing else has
        // one, so no other sprite can accidentally acquire a ring's mark.
        XCTAssertEqual(ItemCatalog.rings.compactMap(\.typeIconIndex), Array(0..<12))
        for ring in ItemCatalog.rings {
            XCTAssertNotNil(SpriteSheet.ringIconSize(ring.typeIconIndex), "\(ring.id) has no glyph")
        }
        for item in ItemCatalog.all where item.kind != .ring {
            XCTAssertNil(item.typeIconIndex, item.id)
        }
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
        let wealth = 235, wealthGlyph = 11
        let whole = try XCTUnwrap(atlas.composedSprite(spriteIndex: wealth, typeIcon: wealthGlyph,
                                                       pointSize: 32))
        let art = try XCTUnwrap(atlas.composedSprite(spriteIndex: wealth, typeIcon: wealthGlyph,
                                                     pointSize: 32, layer: .art))
        let glyph = try XCTUnwrap(atlas.composedSprite(spriteIndex: wealth, typeIcon: wealthGlyph,
                                                       pointSize: 32, layer: .typeIcon))
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

    /// The cell and the glyph are independent inputs: a run's gems move a ring
    /// onto another cell without touching the mark that says which ring it is,
    /// and the mark never bleeds back into the art.
    func testTheDrawnCellAndTheGlyphMoveIndependently() throws {
        let atlas = try loadAtlas()
        // Ring of haste in YKH-LGJ-WDQ: the diamond's cell (gem 11), glyph 7.
        let hasteCell = 235, hasteGlyph = 7
        let catalogHasteCell = 231, wealthGlyph = 11
        func ink(_ spriteIndex: Int, _ typeIcon: Int, _ layer: SpriteLayer) throws -> [UInt8] {
            pixelBytes(try XCTUnwrap(atlas.composedSprite(spriteIndex: spriteIndex,
                                                          typeIcon: typeIcon,
                                                          pointSize: 32, layer: layer)))
        }
        // Same cell, different glyph: identical art.
        XCTAssertEqual(try ink(hasteCell, hasteGlyph, .art),
                       try ink(hasteCell, wealthGlyph, .art))
        // Same glyph, different cell: identical mark.
        XCTAssertEqual(try ink(hasteCell, hasteGlyph, .typeIcon),
                       try ink(catalogHasteCell, hasteGlyph, .typeIcon))
        // And the gem really did change the artwork — a diamond ring is not
        // the sapphire the catalog cell would have drawn.
        XCTAssertNotEqual(try ink(hasteCell, hasteGlyph, .art),
                          try ink(catalogHasteCell, hasteGlyph, .art))
    }

    /// The picker needs clearance to the right of a ring's glyph, but the scout
    /// list must keep the geometry it has: the margin may only add transparent
    /// width, never move the art or the glyph.
    func testTypeIconMarginOnlyAddsTrailingTransparency() throws {
        let atlas = try loadAtlas()
        let wealth = 235, wealthGlyph = 11
        let bare = try XCTUnwrap(atlas.composedSprite(spriteIndex: wealth, typeIcon: wealthGlyph,
                                                      pointSize: 16))
        let margined = try XCTUnwrap(atlas.composedSprite(spriteIndex: wealth,
                                                          typeIcon: wealthGlyph, pointSize: 16,
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

    /// Every RGBA byte of an image, so two renders can be compared exactly —
    /// colour included, which is the whole point of a ring's gem.
    private func pixelBytes(_ image: CGImage) -> [UInt8] {
        let width = image.width, height = image.height
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        pixels.withUnsafeMutableBytes { buffer in
            let context = CGContext(data: buffer.baseAddress, width: width, height: height,
                                    bitsPerComponent: 8, bytesPerRow: width * 4,
                                    space: CGColorSpaceCreateDeviceRGB(),
                                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            context?.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        }
        return pixels
    }

    /// The bounding box of an image's non-transparent pixels, origin top-left.
    private func opaqueBounds(_ image: CGImage) -> SpriteBounds {
        let width = image.width, height = image.height
        let pixels = pixelBytes(image)
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
        let image = try XCTUnwrap(atlas.composedSprite(spriteIndex: 235, typeIcon: 11,
                                                       pointSize: 32))
        XCTAssertEqual(image.width, 32 * SpriteAtlas.pixelScale)
        XCTAssertEqual(image.height, 32 * SpriteAtlas.pixelScale)
        // Cached, so the same call returns the identical bitmap.
        XCTAssertTrue(image === atlas.composedSprite(spriteIndex: 235, typeIcon: 11, pointSize: 32))
        // The glyph is part of that identity: the same cell under another
        // class's mark is a different picture and must not be served from it.
        XCTAssertFalse(image === atlas.composedSprite(spriteIndex: 235, typeIcon: 7, pointSize: 32))
    }
}

/// Which gem — and so which colour — a ring shows is the run's, not the
/// catalog's: `Dungeon.init()` shuffles `Ring.gems` once per seed and gives
/// each ring class the gem at its own index. The class's glyph never moves.
final class RingGemsTests: XCTestCase {
    /// The seed the engine pins for this: it gives the ring of haste a
    /// diamond, where the catalog cell would draw a sapphire.
    private static let hasteDiamondSeed = "YKH-LGJ-WDQ"
    /// Its table, in ring-class order (Accuracy, Arcana, … Wealth).
    private static let hasteDiamondGems = [7, 8, 3, 5, 4, 6, 2, 11, 10, 1, 0, 9]

    func testATableIsAPermutationOfTheTwelveGems() {
        XCTAssertNil(RingGems(ordinals: []))
        XCTAssertNil(RingGems(ordinals: Array(0..<11)))
        XCTAssertNil(RingGems(ordinals: Array(0..<13)))
        // Twelve entries, but a class left without a gem of its own.
        XCTAssertNil(RingGems(ordinals: Array(0..<11) + [0]))
        XCTAssertNil(RingGems(ordinals: Array(1...12)))
        XCTAssertNotNil(RingGems(ordinals: Self.hasteDiamondGems))
        XCTAssertEqual(RingGems.catalogDefault.ordinals, Array(0..<12))
        XCTAssertEqual(RingGems.catalogDefault.gem(forRingClass: 7), 7)
        XCTAssertNil(RingGems.catalogDefault.gem(forRingClass: -1))
        XCTAssertNil(RingGems.catalogDefault.gem(forRingClass: 12))
    }

    /// The catalog's per-ring cell is the table before any run shuffles it,
    /// which is what a seedless surface draws and what makes the class index
    /// double as its glyph index.
    func testCatalogCellsAreTheUnshuffledTable() throws {
        for ring in ItemCatalog.rings {
            let glyph = try XCTUnwrap(ring.typeIconIndex, ring.id)
            XCTAssertEqual(ring.spriteIndex, SpriteSheet.ringSpriteBase + glyph, ring.id)
            XCTAssertEqual(ring.spriteIndex(in: RingGems.catalogDefault), ring.spriteIndex, ring.id)
            // No run in hand — the requirement board — keeps that same cell.
            XCTAssertEqual(ring.spriteIndex(in: nil), ring.spriteIndex, ring.id)
        }
    }

    func testARunsGemsMoveEveryRingAndNothingElse() throws {
        let gems = try XCTUnwrap(RingGems(ordinals: Self.hasteDiamondGems))
        let haste = try XCTUnwrap(ItemCatalog.findById("ring_haste"))
        XCTAssertEqual(haste.typeIconIndex, 7)
        XCTAssertEqual(haste.spriteIndex, SpriteSheet.ringSpriteBase + 7)
        // Haste wears gem 11, the diamond: another cell, the very same glyph.
        XCTAssertEqual(haste.spriteIndex(in: gems), SpriteSheet.ringSpriteBase + 11)
        XCTAssertEqual(haste.spriteIndex(in: gems), 235)
        XCTAssertEqual(haste.typeIconIndex, 7)

        // Every class still lands on a cell of its own inside the ring block.
        let drawn = ItemCatalog.rings.map { $0.spriteIndex(in: gems) }
        XCTAssertEqual(Set(drawn).count, 12)
        XCTAssertEqual(drawn.sorted(), (0..<12).map { SpriteSheet.ringSpriteBase + $0 })
        // Nothing that is not a ring has an appearance to shuffle.
        for item in ItemCatalog.all where item.kind != .ring {
            XCTAssertEqual(item.spriteIndex(in: gems), item.spriteIndex, item.id)
        }
    }

    /// The table rides in the scout packet, between the seed and the quests, so
    /// a manifest whose gem block is missing or is not a whole permutation is a
    /// corrupt packet rather than an unusual run.
    func testScoutPacketsMustCarryAWholeGemTable() throws {
        let head = Array("SSC3".utf8) + [11] + Array("AAA-AAA-AAA".utf8)
        // Gems, then no quests and no items.
        func packet(_ gems: [UInt8]) -> Data { Data(head + gems + [0, 0, 0]) }

        // The block is missing entirely: the quest count is read off its bytes.
        XCTAssertThrowsError(try ScoutCodec.decode(Data(head + [0, 0, 0])))
        XCTAssertThrowsError(try ScoutCodec.decode(packet(Array(repeating: 0, count: 12))))
        // Twelve entries, but a class left without a gem of its own.
        XCTAssertThrowsError(try ScoutCodec.decode(packet((0..<11).map { UInt8($0) } + [0])))
        // SSC2 said nothing about gems, so its magic no longer names a packet
        // this can read at all.
        XCTAssertThrowsError(try ScoutCodec.decode(Data(
            Array("SSC2".utf8) + Array(head.dropFirst(4))
                + Self.hasteDiamondGems.map { UInt8($0) } + [0, 0, 0])))

        let world = try ScoutCodec.decode(packet(Self.hasteDiamondGems.map { UInt8($0) }))
        XCTAssertEqual(world.ringGems.ordinals, Self.hasteDiamondGems)
    }

    func testAScoutedWorldArrivesWithTheRunsGems() async throws {
        let engine = ProductionSeedFinderEngine()
        // A scouted world arrives with its own table, so every surface showing
        // its items has the run's colours without asking again.
        let world = try await engine.scoutSeed(Self.hasteDiamondSeed, challenges: 0)
        XCTAssertEqual(world.ringGems.ordinals, Self.hasteDiamondGems)
        XCTAssertNotEqual(world.ringGems, RingGems.catalogDefault)
        // The gems are drawn before any challenge is read, so the mask that
        // rides along in the request cannot move them.
        let challenged = try await engine.scoutSeed(Self.hasteDiamondSeed, challenges: 1)
        XCTAssertEqual(challenged.ringGems, world.ringGems)

        let haste = try XCTUnwrap(ItemCatalog.findById("ring_haste"))
        XCTAssertEqual(haste.spriteIndex(in: world.ringGems), 235)
        // The gem moves the cell drawn; the class's glyph stays where it is.
        XCTAssertEqual(haste.typeIconIndex, 7)
    }
}
