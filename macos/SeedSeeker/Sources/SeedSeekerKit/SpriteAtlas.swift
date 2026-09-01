import CoreGraphics
import Foundation
import ImageIO

/// A pixel size in atlas (unscaled) units.
public struct PixelSize: Equatable, Hashable, Sendable {
    public let width: Int
    public let height: Int
    public init(width: Int, height: Int) { self.width = width; self.height = height }
}

/// The art bounding box of one sprite within its 16×16 cell, in cell-local
/// pixels with the origin at the cell's top-left.
public struct SpriteBounds: Equatable, Hashable, Sendable {
    public let x: Int
    public let y: Int
    public let width: Int
    public let height: Int
    public init(x: Int, y: Int, width: Int, height: Int) {
        self.x = x; self.y = y; self.width = width; self.height = height
    }
}

/// Geometry of Shattered Pixel Dungeon's item atlases, mirroring
/// `web/src/lib/sprites.ts` and the Android client's `Components.kt` so all
/// three front-ends stay pixel-identical.
public enum SpriteSheet {
    /// `items.png` is a 16-column grid of 16×16 cells, indexed row-major by
    /// `CatalogItem.spriteIndex`.
    public static let cell = 16
    public static let columns = 16
    /// `item_icons.png` is a 16-column grid of 8×8 cells.
    public static let iconCell = 8
    public static let iconColumns = 16
    /// Cell of the first ring sprite (`ItemSpriteSheet.RINGS`). The twelve
    /// cells from here are the twelve *gems* in `Ring.gems` order, not the
    /// twelve ring classes: which of them a ring is drawn in is a property of
    /// the run, so it comes from ``RingGems`` — see
    /// ``CatalogItem/spriteIndex(in:)``.
    public static let ringSpriteBase = 224

    /// Art dimensions of each ring glyph within its 8×8 cell, indexed by ring
    /// class (`CatalogItem.typeIconIndex`): Accuracy, Arcana, Elements, …
    /// Wealth. A class keeps its glyph whatever gem the run gives it.
    public static let ringIconSizes: [PixelSize] = [
        PixelSize(width: 7, height: 7),  // Accuracy
        PixelSize(width: 7, height: 7),  // Arcana
        PixelSize(width: 7, height: 7),  // Elements
        PixelSize(width: 7, height: 5),  // Energy
        PixelSize(width: 7, height: 7),  // Evasion
        PixelSize(width: 5, height: 6),  // Force
        PixelSize(width: 7, height: 6),  // Furor
        PixelSize(width: 6, height: 6),  // Haste
        PixelSize(width: 7, height: 7),  // Might
        PixelSize(width: 7, height: 7),  // Sharpshooting
        PixelSize(width: 6, height: 6),  // Tenacity
        PixelSize(width: 7, height: 6),  // Wealth
    ]

    /// The art size of ring glyph `typeIcon`, or nil when there is no such
    /// glyph — a non-ring (nil) or an index outside the ring block.
    ///
    /// The glyph index is an input, never derived from the drawn cell: a
    /// scouted ring is drawn in its run's gem cell, which says nothing about
    /// which ring it is. Rings are told apart only by this glyph, never by
    /// colour, so the two have to travel side by side.
    public static func ringIconSize(_ typeIcon: Int?) -> PixelSize? {
        guard let typeIcon, ringIconSizes.indices.contains(typeIcon) else { return nil }
        return ringIconSizes[typeIcon]
    }
}

/// Which parts of a sprite a composed image carries.
///
/// The split exists for the enchantment glow: it tints the item's silhouette,
/// but a ring's type glyph is the only thing that says *which* ring this is, so
/// it has to stay solid. Drawing ``art`` and ``typeIcon`` as separate layers
/// with the glow between them keeps the glyph out of the glow's mask, matching
/// the web, where the glow sits inside the art element and the glyph is a
/// sibling painted after it.
public enum SpriteLayer: Hashable, Sendable {
    /// The item art plus, for rings, the type glyph — the whole icon.
    case whole
    /// The item art alone, with no ring type glyph.
    case art
    /// A ring's type glyph alone, at the same position it occupies in ``whole``.
    case typeIcon
}

/// The upstream item atlases, decoded once and diced into per-sprite images.
///
/// Sprite art is anchored to the top-left of its 16×16 cell, so drawing a full
/// cell leaves small items (rings, darts, seeds) hugging the corner. The art's
/// alpha bounding box is therefore measured at runtime on first use and centred
/// in the target box, keeping the pixel scale identical to a full-cell render —
/// exactly what `spriteBoxCss` does on the web, without a build step.
///
/// Everything is rendered nearest-neighbour at ``pixelScale`` device pixels per
/// point so the art stays crisp on Retina displays, and cached by
/// (sprite index, point size) because scout lists re-render on every keystroke.
@MainActor public final class SpriteAtlas {
    /// Device pixels per point that composed sprites are rasterised at.
    public static let pixelScale = 2

    /// The atlases installed in the app bundle's `Contents/Resources` by
    /// `scripts/build-macos-app.sh`, or nil when they are absent — a bare
    /// `swift run` or `swift test` executes outside the `.app`, and callers then
    /// fall back to an SF Symbol.
    public static let bundled: SpriteAtlas? = {
        guard let items = Bundle.main.url(forResource: "items", withExtension: "png"),
              let icons = Bundle.main.url(forResource: "item_icons", withExtension: "png")
        else { return nil }
        return SpriteAtlas(itemsURL: items, iconsURL: icons)
    }()

    private let items: CGImage
    private let icons: CGImage
    /// One alpha byte per `items.png` pixel, row-major from the top-left.
    private let itemsAlpha: [UInt8]
    private var boundsCache: [Int: SpriteBounds] = [:]
    private var imageCache: [ImageKey: CGImage] = [:]

    private struct ImageKey: Hashable {
        let spriteIndex: Int
        /// The glyph drawn over that cell, if any. Two rings of one class in
        /// runs with different gems differ here only in ``spriteIndex``, and a
        /// gem cell shared by two classes only in this — so both belong in the
        /// key.
        let typeIcon: Int?
        let pointSize: Int
        let layer: SpriteLayer
        let typeIconMargin: Int
    }

    public init?(itemsURL: URL, iconsURL: URL) {
        guard let items = Self.decode(itemsURL), let icons = Self.decode(iconsURL),
              let alpha = Self.alphaMask(items) else { return nil }
        self.items = items
        self.icons = icons
        itemsAlpha = alpha
    }

    private static func decode(_ url: URL) -> CGImage? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }

    /// Reads the image's alpha channel into a byte buffer indexed row-major from
    /// the top-left. A bitmap context's memory is stored top row first, and
    /// `draw(_:in:)` renders an image upright, so no flip is needed here.
    private static func alphaMask(_ image: CGImage) -> [UInt8]? {
        let width = image.width, height = image.height
        guard width > 0, height > 0 else { return nil }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { return nil }
        var alpha = [UInt8](repeating: 0, count: width * height)
        for index in 0..<(width * height) { alpha[index] = pixels[index * 4 + 3] }
        return alpha
    }

    /// The art bounding box of a sprite, measured once and cached. Empty or
    /// out-of-range cells fall back to the full cell, as the web does.
    public func bounds(forSprite spriteIndex: Int) -> SpriteBounds {
        if let cached = boundsCache[spriteIndex] { return cached }
        let measured = measureBounds(spriteIndex)
        boundsCache[spriteIndex] = measured
        return measured
    }

    private func measureBounds(_ spriteIndex: Int) -> SpriteBounds {
        let cell = SpriteSheet.cell
        let fullCell = SpriteBounds(x: 0, y: 0, width: cell, height: cell)
        let originX = (spriteIndex % SpriteSheet.columns) * cell
        let originY = (spriteIndex / SpriteSheet.columns) * cell
        guard spriteIndex >= 0, originX + cell <= items.width, originY + cell <= items.height
        else { return fullCell }
        var minX = cell, minY = cell, maxX = -1, maxY = -1
        for y in 0..<cell {
            let row = (originY + y) * items.width + originX
            for x in 0..<cell where itemsAlpha[row + x] > 8 {
                minX = min(minX, x); minY = min(minY, y)
                maxX = max(maxX, x); maxY = max(maxY, y)
            }
        }
        guard maxX >= 0 else { return fullCell }
        return SpriteBounds(x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1)
    }

    /// A sprite composed for display: the `items.png` cell `spriteIndex` centred
    /// in a `pointSize`-square box, with ring glyph `typeIcon` anchored to the
    /// box's top-right at the same pixel scale, rasterised nearest-neighbour at
    /// ``pixelScale``× and cached.
    ///
    /// The two are separate inputs and neither follows from the other. A
    /// scouted ring's cell is its run's gem, which two ring classes never share
    /// but which says nothing about the class; the glyph is the class, and is
    /// the same in every run. Pass ``CatalogItem/spriteIndex(in:)`` and
    /// ``CatalogItem/typeIconIndex``.
    ///
    /// `layer` selects which parts to draw; every layer uses the same geometry
    /// and box size, so they stack pixel-for-pixel back into ``SpriteLayer/whole``.
    ///
    /// `typeIconMargin` widens the image by that many points of transparency on
    /// the right, leaving the art and glyph exactly where they are in the square
    /// box. A ring's glyph is anchored flush to that box's edge, so without it
    /// the glyph butts against whatever sits beside the icon; SwiftUI's popup
    /// button ignores a view's outer padding when it draws the selected row, so
    /// the space has to be inside the bitmap to survive. Ignored for sprites
    /// with no glyph, which keep their square box.
    ///
    /// Returns nil when the bitmap cannot be allocated, and for
    /// ``SpriteLayer/typeIcon`` without a glyph — there is nothing to draw, and
    /// an empty layer is not worth caching.
    public func composedSprite(spriteIndex: Int, typeIcon: Int? = nil, pointSize: Int,
                               layer: SpriteLayer = .whole,
                               typeIconMargin: Int = 0) -> CGImage? {
        // A glyph index outside the ring block draws nothing, so it must not
        // reach the cache key either: that image is the one with no glyph.
        let icon = SpriteSheet.ringIconSize(typeIcon) == nil ? nil : typeIcon
        guard layer != .typeIcon || icon != nil else { return nil }
        let margin = icon == nil ? 0 : max(0, typeIconMargin)
        let key = ImageKey(spriteIndex: spriteIndex, typeIcon: icon, pointSize: pointSize,
                           layer: layer, typeIconMargin: margin)
        if let cached = imageCache[key] { return cached }
        guard let rendered = render(spriteIndex: spriteIndex, typeIcon: icon, pointSize: pointSize,
                                    layer: layer, typeIconMargin: margin) else { return nil }
        imageCache[key] = rendered
        return rendered
    }

    private func render(spriteIndex: Int, typeIcon: Int?, pointSize: Int, layer: SpriteLayer,
                        typeIconMargin: Int) -> CGImage? {
        let pixels = pointSize * Self.pixelScale
        guard pixels > 0, let context = CGContext(
            data: nil, width: pixels + typeIconMargin * Self.pixelScale, height: pixels,
            bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        context.interpolationQuality = .none
        context.setShouldAntialias(false)
        let scale = CGFloat(pixels) / CGFloat(SpriteSheet.cell)
        let box = CGFloat(pixels)

        let art = bounds(forSprite: spriteIndex)
        let cell = SpriteSheet.cell
        let source = CGRect(x: (spriteIndex % SpriteSheet.columns) * cell + art.x,
                            y: (spriteIndex / SpriteSheet.columns) * cell + art.y,
                            width: art.width, height: art.height)
        if layer != .typeIcon, let cropped = items.cropping(to: source) {
            // The art is centred, so the same inset applies top and bottom and
            // CoreGraphics' bottom-up destination needs no further flipping.
            let width = CGFloat(art.width) * scale, height = CGFloat(art.height) * scale
            context.draw(cropped, in: CGRect(x: ((box - width) / 2).rounded(),
                                             y: ((box - height) / 2).rounded(),
                                             width: width, height: height))
        }

        if layer != .art, let icon = typeIcon, let glyph = SpriteSheet.ringIconSize(icon) {
            let iconCell = SpriteSheet.iconCell
            let glyphSource = CGRect(x: (icon % SpriteSheet.iconColumns) * iconCell,
                                     y: (icon / SpriteSheet.iconColumns) * iconCell,
                                     width: glyph.width, height: glyph.height)
            if let cropped = icons.cropping(to: glyphSource) {
                let width = CGFloat(glyph.width) * scale, height = CGFloat(glyph.height) * scale
                context.draw(cropped, in: CGRect(x: box - width, y: box - height,
                                                 width: width, height: height))
            }
        }
        return context.makeImage()
    }
}
