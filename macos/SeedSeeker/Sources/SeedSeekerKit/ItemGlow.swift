import Foundation

/// Enchantment / glyph glow colour and pulse period, mirrored 1:1 from
/// Shattered Pixel Dungeon's `ItemSprite.Glowing` definitions so scouted items
/// pulse exactly as the game renders them. `period` is the seconds the glow
/// takes to fade fully in — it fades back out over the same span, so a complete
/// pulse cycle lasts `2 × period`. Curses always glow black, as in the game.
///
/// This is the Swift twin of `web/src/lib/glow.ts`; keep the two in step.
public struct ItemGlow: Equatable, Hashable, Sendable {
    /// Hex colour (`#rrggbb`) the sprite blends toward at the pulse peak.
    public let hex: String
    /// Seconds to reach peak glow; the full fade-in/out cycle lasts twice this.
    public let period: Double

    /// Upstream's default `Glowing(color)` period when none is given (1f).
    public static let defaultPeriod = 1.0
    /// Peak blend factor, matching the game's maximum glow value.
    public static let peakOpacity = 0.6
    /// Static blend held instead of pulsing when the system asks for reduced
    /// motion, matching the web app's `prefers-reduced-motion` behaviour.
    public static let reducedMotionOpacity = 0.3

    public init(hex: String, period: Double = ItemGlow.defaultPeriod) {
        self.hex = hex
        self.period = period
    }

    /// Seconds for one complete fade-in/fade-out cycle.
    public var cycleDuration: Double { 2 * period }

    /// The glow colour as sRGB components in 0…1.
    public var components: (red: Double, green: Double, blue: Double) {
        var digits = hex
        if digits.hasPrefix("#") { digits.removeFirst() }
        guard digits.count == 6, let value = UInt32(digits, radix: 16) else { return (0, 0, 0) }
        return (Double((value >> 16) & 0xFF) / 255,
                Double((value >> 8) & 0xFF) / 255,
                Double(value & 0xFF) / 255)
    }
}

/// Every curse glows black in the game, at the default period.
public let curseGlow = ItemGlow(hex: "#000000")

/// Keyed by the wire names the scout emits (`WeaponEffect` / `ArmorEffect`
/// `wire_name` in seedfinder-core). Only non-curse effects live here; every
/// curse glows black and is handled by ``curseGlow``.
public let enchantmentGlows: [String: ItemGlow] = [
    // Weapon enchantments
    "Blazing": ItemGlow(hex: "#ff4400"),
    "Chilling": ItemGlow(hex: "#00ffff"),
    "Kinetic": ItemGlow(hex: "#ffff00"),
    "Shocking": ItemGlow(hex: "#ffffff", period: 0.5),
    "Venomous": ItemGlow(hex: "#4400aa"),
    "Blocking": ItemGlow(hex: "#0000ff"),
    "Blooming": ItemGlow(hex: "#008800"),
    "Eldritch": ItemGlow(hex: "#222222"),
    "Elastic": ItemGlow(hex: "#ff00ff"),
    "Lucky": ItemGlow(hex: "#00ff00"),
    "Projecting": ItemGlow(hex: "#8844cc"),
    "Unstable": ItemGlow(hex: "#999999"),
    "Vorpal": ItemGlow(hex: "#aa6666"),
    "Corrupting": ItemGlow(hex: "#440066"),
    "Crystal": ItemGlow(hex: "#0088ff"),
    "Grim": ItemGlow(hex: "#000000"),
    "Vampiric": ItemGlow(hex: "#660022"),
    // Armor glyphs
    "Obfuscation": ItemGlow(hex: "#888888"),
    "Swiftness": ItemGlow(hex: "#ffff00"),
    "Viscosity": ItemGlow(hex: "#8844cc"),
    "Potential": ItemGlow(hex: "#ffffff", period: 0.6),
    "Brimstone": ItemGlow(hex: "#ff4400"),
    "Stone": ItemGlow(hex: "#222222"),
    "Entanglement": ItemGlow(hex: "#663300"),
    "Repulsion": ItemGlow(hex: "#ffffff"),
    "Camouflage": ItemGlow(hex: "#448822"),
    "Flow": ItemGlow(hex: "#0000ff"),
    "Affection": ItemGlow(hex: "#ff4488"),
    "Anti-Magic": ItemGlow(hex: "#88eeff"),
    "Thorns": ItemGlow(hex: "#660022"),
]

/// The pulse glow for a scouted item, or nil when it carries no enchantment or
/// curse. A beneficial enchantment/glyph wins even on a cursed item (matching
/// `Weapon.glowing()`, which returns the enchantment's colour when one is
/// present — a curse-infused Kinetic weapon still glows yellow); otherwise a
/// cursed item pulses black.
public func itemGlow(_ item: ScoutItem) -> ItemGlow? {
    if let effect = item.effect, !ItemCatalog.cursesFor(item.item.kind).contains(effect) {
        return enchantmentGlows[effect]
    }
    if item.cursed { return curseGlow }
    return nil
}

/// The pulse glow for a bare effect name (as carried by a requirement), or nil
/// when there is none. Known enchantments/glyphs pulse their colour; any other
/// effect name is a curse and pulses black.
public func effectGlow(_ effect: String?) -> ItemGlow? {
    guard let effect, !effect.isEmpty else { return nil }
    return enchantmentGlows[effect] ?? curseGlow
}
