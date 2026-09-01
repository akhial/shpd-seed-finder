import SeedSeekerKit
import SwiftUI

/// An item's real Shattered Pixel Dungeon sprite, pulsing with its
/// enchantment/curse glow when it carries one.
///
/// `SpriteAtlas` crops the art to its alpha bounding box, centres it and draws a
/// ring's type glyph, all nearest-neighbour at 2× so it stays crisp on Retina.
/// The glow is a solid colour layer masked to the sprite's opaque pixels whose
/// opacity fades linearly 0 → 0.6 → 0 over `2 × period` seconds — the same
/// reproduction of upstream's `texel*(1-v) + glow*v` shader the web app uses, so
/// only the silhouette tints and there is no halo outside it.
///
/// Falls back to a question mark when there is no concrete item (a wildcard
/// requirement) and when the atlas is not bundled, which is the case for a bare
/// `swift run` outside the `.app`.
struct ItemSpriteView: View {
    /// The concrete item, or nil for a wildcard requirement.
    var item: CatalogItem?
    /// The ring gems of the run this item belongs to, when it belongs to one:
    /// a ring is drawn in its run's gem cell, so a scouted item must pass its
    /// world's table. Nil keeps the catalog's own cell, which is what a
    /// seedless surface — the requirement board — has to draw.
    var ringGems: RingGems?
    /// Enchantment or curse glow to pulse with, if any.
    var glow: ItemGlow?
    /// Box edge in points. Multiples of 8 keep the pixel scale integral.
    var pointSize: Int = 32
    /// Accessibility label; the sprite is decorative when nil.
    var label: String?

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ZStack {
            artwork
            if let glow {
                SpriteGlowLayer(glow: glow, reduceMotion: reduceMotion) { artwork }
                    // Restart the pulse from zero whenever the effect changes,
                    // since list rows are reused as the manifest re-renders.
                    .id(glow)
            }
            // Stacked above the glow: the glyph is the only thing distinguishing
            // one ring from another, so it must stay solid rather than pulse
            // with a curse. The web draws it as a sibling after the glow too.
            typeIcon
        }
        .frame(width: CGFloat(pointSize), height: CGFloat(pointSize))
        .accessibilityHidden(label == nil)
        .accessibilityLabel(label ?? "")
    }

    /// The item art alone — also the glow's mask, hence excluding the glyph.
    @ViewBuilder private var artwork: some View {
        if let image = sprite(.art) {
            pixels(image)
        } else {
            Image(systemName: "questionmark")
                .font(.system(size: CGFloat(pointSize) * 0.6, weight: .semibold))
                .foregroundStyle(Color.shatteredGreen)
        }
    }

    /// A ring's type glyph, in the same box so it lands where it does in
    /// ``SpriteLayer/whole``. Empty for every other item.
    @ViewBuilder private var typeIcon: some View {
        if let image = sprite(.typeIcon) { pixels(image) }
    }

    private func pixels(_ image: CGImage) -> some View {
        Image(decorative: image, scale: CGFloat(SpriteAtlas.pixelScale))
            .interpolation(.none)
            .antialiased(false)
    }

    private func sprite(_ layer: SpriteLayer) -> CGImage? {
        guard let item else { return nil }
        return SpriteAtlas.bundled?.composedSprite(spriteIndex: item.spriteIndex(in: ringGems),
                                                   typeIcon: item.typeIconIndex,
                                                   pointSize: pointSize, layer: layer)
    }
}

/// The pulsing glow overlay: `mask`'s opaque pixels tinted with the glow colour,
/// its opacity animating linearly between 0 and ``ItemGlow/peakOpacity``. Held
/// at a static blend instead when the system asks for reduced motion, matching
/// the web app under `prefers-reduced-motion`.
private struct SpriteGlowLayer<Mask: View>: View {
    let glow: ItemGlow
    let reduceMotion: Bool
    @ViewBuilder let mask: Mask
    @State private var atPeak = false

    var body: some View {
        let (red, green, blue) = glow.components
        return Color(.sRGB, red: red, green: green, blue: blue, opacity: 1)
            .mask { mask }
            .opacity(reduceMotion ? ItemGlow.reducedMotionOpacity
                                  : (atPeak ? ItemGlow.peakOpacity : 0))
            .allowsHitTesting(false)
            .onAppear {
                guard !reduceMotion else { return }
                // One `period` in, one back out: a full cycle is 2 × period.
                withAnimation(.linear(duration: glow.period).repeatForever(autoreverses: true)) {
                    atPeak = true
                }
            }
    }
}

/// A bare, non-glowing sprite image for places that only accept an `Image`, such
/// as the item picker. Renders nothing when the atlas is absent.
///
/// The picker sets its label hard against the icon, and a ring's type glyph runs
/// to the very edge of its box, so ring sprites are drawn with a trailing margin
/// baked in. It has to be in the bitmap rather than applied as padding: the
/// popup button honours the icon's own width but drops its outer padding when it
/// draws the selected row, so padding moves the open menu's rows and leaves the
/// collapsed control untouched. Everything else keeps its square box, and the
/// scout list — which asks for no margin — is unaffected either way.
///
/// The picker names item *classes*, not the contents of any one run, so a ring
/// keeps the catalog's cell here — there is no seed to ask for a gem.
struct ItemSpriteIcon: View {
    var item: CatalogItem
    var pointSize: Int = 16
    /// Points of transparency added to the right of a ring's glyph.
    var typeIconMargin: Int = 2

    var body: some View {
        if let image = SpriteAtlas.bundled?.composedSprite(spriteIndex: item.spriteIndex,
                                                           typeIcon: item.typeIconIndex,
                                                           pointSize: pointSize,
                                                           typeIconMargin: typeIconMargin) {
            Image(decorative: image, scale: CGFloat(SpriteAtlas.pixelScale))
                .interpolation(.none)
                .antialiased(false)
        }
    }
}
