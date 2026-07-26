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
/// Falls back to the category's SF Symbol when there is no concrete item (a
/// wildcard requirement) and when the atlas is not bundled, which is the case
/// for a bare `swift run` outside the `.app`.
struct ItemSpriteView: View {
    /// The concrete item's atlas index, or nil for a wildcard requirement.
    var spriteIndex: Int?
    /// Category, used for the wildcard/fallback symbol and its tint.
    var kind: ItemKind
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
        }
        .frame(width: CGFloat(pointSize), height: CGFloat(pointSize))
        .accessibilityHidden(label == nil)
        .accessibilityLabel(label ?? "")
    }

    @ViewBuilder private var artwork: some View {
        if let image = sprite {
            Image(decorative: image, scale: CGFloat(SpriteAtlas.pixelScale))
                .interpolation(.none)
                .antialiased(false)
        } else {
            Image(systemName: kind.icon)
                .font(.system(size: CGFloat(pointSize) * 0.6))
                .foregroundStyle(kind.tint)
        }
    }

    private var sprite: CGImage? {
        guard let spriteIndex else { return nil }
        return SpriteAtlas.bundled?.composedSprite(spriteIndex: spriteIndex, pointSize: pointSize)
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
/// as the menu rows of the item picker. Renders nothing when the atlas is absent.
struct ItemSpriteIcon: View {
    var spriteIndex: Int
    var pointSize: Int = 16

    var body: some View {
        if let image = SpriteAtlas.bundled?.composedSprite(spriteIndex: spriteIndex,
                                                          pointSize: pointSize) {
            Image(decorative: image, scale: CGFloat(SpriteAtlas.pixelScale))
                .interpolation(.none)
                .antialiased(false)
        }
    }
}
