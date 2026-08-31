// SPDX-License-Identifier: GPL-3.0-or-later

//! Enchantment, glyph, and curse glow colours.
//!
//! Mirrored 1:1 from Shattered Pixel Dungeon's `ItemSprite.Glowing`
//! definitions, and kept identical to the web frontend's `web/src/lib/glow.ts`
//! so every frontend pulses items exactly as the game renders them. `period` is
//! the seconds the glow takes to fade fully in; it fades back out over the same
//! span, so one complete pulse cycle lasts `2 × period`.
//!
//! These are the only colours the app takes from the game. They are item data,
//! not app chrome: the surrounding interface stays libadwaita and follows the
//! system accent and light/dark preference.

use shpd_seedfinder_core::catalog::Effect;

/// Peak blend fraction of the glow colour, matching upstream's glow shader
/// (`rgb = texel.rgb * (1 - v) + glow * v`, with `v` peaking at 0.6).
pub const PEAK: f64 = 0.6;

/// Held glow strength when animations are disabled, matching the web
/// frontend's `prefers-reduced-motion` fallback.
pub const STATIC_VALUE: f64 = 0.3;

/// Upstream's default `Glowing(color)` period when none is given (1f).
const DEFAULT_PERIOD: f64 = 1.0;

/// One glow: the colour a sprite blends toward, and how fast it pulses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    /// Packed `0xRRGGBB` colour the sprite blends toward at the pulse peak.
    pub color: u32,
    /// Seconds to reach peak glow; a full fade-in/out cycle lasts twice this.
    pub period: f64,
}

impl Glow {
    const fn new(color: u32, period: f64) -> Self {
        Self { color, period }
    }

    /// The colour as Cairo's 0…1 red, green, and blue components.
    #[must_use]
    pub fn rgb(self) -> (f64, f64, f64) {
        let channel = |shift: u32| f64::from((self.color >> shift) & 0xff) / 255.0;
        (channel(16), channel(8), channel(0))
    }

    /// The colour as a lowercase `#rrggbb` string, the spelling the web
    /// frontend's table uses; the tests below compare against it directly.
    #[cfg(test)]
    fn hex(self) -> String {
        format!("#{:06x}", self.color & 0x00ff_ffff)
    }
}

/// Every curse glows black in the game, at the default period.
pub const CURSE: Glow = Glow::new(0x0000_0000, DEFAULT_PERIOD);

/// Every beneficial weapon enchantment and armor glyph, keyed by the wire name
/// [`Effect::wire_name`] produces and listed in the same order as the web
/// frontend's table so the two can be diffed line by line. Curses are absent;
/// they all use [`CURSE`].
const ENCHANTMENTS: &[(&str, Glow)] = &[
    // Weapon enchantments, in the catalog's journal order
    ("Blazing", Glow::new(0x00ff_4400, DEFAULT_PERIOD)),
    ("Chilling", Glow::new(0x0000_ffff, DEFAULT_PERIOD)),
    ("Kinetic", Glow::new(0x00ff_ff00, DEFAULT_PERIOD)),
    ("Shocking", Glow::new(0x00ff_ffff, 0.5)),
    ("Venomous", Glow::new(0x0044_00aa, DEFAULT_PERIOD)),
    ("Blocking", Glow::new(0x0000_00ff, DEFAULT_PERIOD)),
    ("Blooming", Glow::new(0x0000_8800, DEFAULT_PERIOD)),
    ("Eldritch", Glow::new(0x0022_2222, DEFAULT_PERIOD)),
    ("Elastic", Glow::new(0x00ff_00ff, DEFAULT_PERIOD)),
    ("Lucky", Glow::new(0x0000_ff00, DEFAULT_PERIOD)),
    ("Projecting", Glow::new(0x0088_44cc, DEFAULT_PERIOD)),
    ("Unstable", Glow::new(0x0099_9999, DEFAULT_PERIOD)),
    ("Vorpal", Glow::new(0x00aa_6666, DEFAULT_PERIOD)),
    ("Corrupting", Glow::new(0x0044_0066, DEFAULT_PERIOD)),
    ("Crystal", Glow::new(0x0000_88ff, DEFAULT_PERIOD)),
    ("Grim", Glow::new(0x0000_0000, DEFAULT_PERIOD)),
    ("Vampiric", Glow::new(0x0066_0022, DEFAULT_PERIOD)),
    // Armor glyphs
    ("Obfuscation", Glow::new(0x0088_8888, DEFAULT_PERIOD)),
    ("Swiftness", Glow::new(0x00ff_ff00, DEFAULT_PERIOD)),
    ("Viscosity", Glow::new(0x0088_44cc, DEFAULT_PERIOD)),
    ("Potential", Glow::new(0x00ff_ffff, 0.6)),
    ("Brimstone", Glow::new(0x00ff_4400, DEFAULT_PERIOD)),
    ("Stone", Glow::new(0x0022_2222, DEFAULT_PERIOD)),
    ("Entanglement", Glow::new(0x0066_3300, DEFAULT_PERIOD)),
    ("Repulsion", Glow::new(0x00ff_ffff, DEFAULT_PERIOD)),
    ("Camouflage", Glow::new(0x0044_8822, DEFAULT_PERIOD)),
    ("Flow", Glow::new(0x0000_00ff, DEFAULT_PERIOD)),
    ("Affection", Glow::new(0x00ff_4488, DEFAULT_PERIOD)),
    ("Anti-Magic", Glow::new(0x0088_eeff, DEFAULT_PERIOD)),
    ("Thorns", Glow::new(0x0066_0022, DEFAULT_PERIOD)),
];

/// The glow for one beneficial weapon enchantment or armor glyph.
#[must_use]
pub fn enchantment(wire_name: &str) -> Option<Glow> {
    ENCHANTMENTS
        .iter()
        .find(|(name, _)| *name == wire_name)
        .map(|(_, glow)| *glow)
}

/// The pulse glow for a scouted item, or `None` when it carries no enchantment
/// or curse.
///
/// A beneficial enchantment or glyph wins even on a cursed item, matching
/// `Weapon.glowing()`: a curse-infused Kinetic weapon still glows yellow.
/// Otherwise a cursed item pulses black.
#[must_use]
pub fn item(cursed: bool, effect: Option<Effect>) -> Option<Glow> {
    if let Some(effect) = effect
        && !effect.is_curse()
    {
        return enchantment(effect.wire_name());
    }
    cursed.then_some(CURSE)
}

/// The pulse glow a requirement's pinned effect asks for, or `None` when the
/// requirement leaves the effect open. Known enchantments and glyphs pulse
/// their colour; every other effect is a curse and pulses black.
#[must_use]
pub fn effect(effect: Option<Effect>) -> Option<Glow> {
    let effect = effect?;
    Some(enchantment(effect.wire_name()).unwrap_or(CURSE))
}

/// The glow strength at `frame_time` microseconds, ramping linearly
/// `0 → PEAK → 0` over `2 × period` seconds. Reproduces the web frontend's
/// `d1-ench-pulse` keyframes; using the frame clock's shared monotonic time
/// keeps every visible sprite in phase.
#[must_use]
pub fn value_at(frame_time_micros: i64, period: f64) -> f64 {
    if period <= 0.0 {
        return PEAK;
    }
    #[allow(clippy::cast_precision_loss)]
    // Monotonic microseconds stay well inside f64's exact range.
    let seconds = frame_time_micros as f64 / 1_000_000.0;
    let phase = (seconds / period).rem_euclid(2.0);
    PEAK * (1.0 - (phase - 1.0).abs())
}

#[cfg(test)]
mod tests {
    use shpd_seedfinder_core::catalog::{
        ALL_ARMOR_EFFECTS, ALL_WEAPON_EFFECTS, ArmorEffect, Effect, WeaponEffect,
    };

    use super::{CURSE, PEAK, effect, enchantment, item, value_at};

    #[test]
    // Periods are exact literals copied from upstream, so exact comparison is
    // the point of the assertion.
    #[allow(clippy::float_cmp)]
    fn table_matches_the_upstream_glow_definitions() {
        assert_eq!(enchantment("Blazing").unwrap().hex(), "#ff4400");
        assert_eq!(enchantment("Blazing").unwrap().period, 1.0);
        // Shocking and Potential are the only two non-default periods.
        assert_eq!(enchantment("Shocking").unwrap().hex(), "#ffffff");
        assert_eq!(enchantment("Shocking").unwrap().period, 0.5);
        assert_eq!(enchantment("Potential").unwrap().period, 0.6);
        assert_eq!(enchantment("Anti-Magic").unwrap().hex(), "#88eeff");
        assert_eq!(enchantment("Thorns").unwrap().hex(), "#660022");
        // Grim is a beneficial enchantment that happens to glow black.
        assert_eq!(enchantment("Grim").unwrap().hex(), "#000000");
        // v4.0.0's four enchantments, all at the default period.
        assert_eq!(enchantment("Venomous").unwrap().hex(), "#4400aa");
        assert_eq!(enchantment("Eldritch").unwrap().hex(), "#222222");
        assert_eq!(enchantment("Vorpal").unwrap().hex(), "#aa6666");
        assert_eq!(enchantment("Crystal").unwrap().hex(), "#0088ff");
        assert_eq!(enchantment("Crystal").unwrap().period, 1.0);
        assert_eq!(CURSE.hex(), "#000000");
        assert_eq!(CURSE.period, 1.0);
        assert!(enchantment("Annoying").is_none());
        assert!(enchantment("Bulk").is_none());
        // v4.0.0's two curses take the shared curse glow like the rest.
        assert!(enchantment("Pressurized").is_none());
        assert!(enchantment("Wondrous").is_none());
    }

    #[test]
    fn colours_convert_to_cairo_components() {
        let (red, green, blue) = enchantment("Kinetic").unwrap().rgb();
        assert!((red - 1.0).abs() < 1e-9);
        assert!((green - 1.0).abs() < 1e-9);
        assert!(blue.abs() < 1e-9);
        let (red, green, blue) = enchantment("Corrupting").unwrap().rgb();
        assert!((red - 68.0 / 255.0).abs() < 1e-9);
        assert!(green.abs() < 1e-9);
        assert!((blue - 102.0 / 255.0).abs() < 1e-9);
    }

    #[test]
    fn every_effect_resolves_to_a_glow() {
        for weapon_effect in ALL_WEAPON_EFFECTS {
            let effect = Effect::Weapon(*weapon_effect);
            let expected = enchantment(effect.wire_name()).is_some();
            assert_eq!(
                !weapon_effect.is_curse(),
                expected,
                "{} is missing from the glow table",
                effect.wire_name()
            );
            // A requirement pinning any effect always has something to pulse.
            assert!(super::effect(Some(effect)).is_some());
        }
        for armor_effect in ALL_ARMOR_EFFECTS {
            let effect = Effect::Armor(*armor_effect);
            let expected = enchantment(effect.wire_name()).is_some();
            assert_eq!(
                !armor_effect.is_curse(),
                expected,
                "{} is missing from the glow table",
                effect.wire_name()
            );
            assert!(super::effect(Some(effect)).is_some());
        }
    }

    #[test]
    fn enchantment_wins_over_a_curse_on_the_same_item() {
        let kinetic = Effect::Weapon(WeaponEffect::Kinetic);
        assert_eq!(item(true, Some(kinetic)), enchantment("Kinetic"));
        assert_eq!(item(false, Some(kinetic)), enchantment("Kinetic"));

        let camouflage = Effect::Armor(ArmorEffect::Camouflage);
        assert_eq!(item(true, Some(camouflage)), enchantment("Camouflage"));
    }

    #[test]
    fn cursed_items_and_curse_effects_pulse_black() {
        assert_eq!(item(true, None), Some(CURSE));
        assert_eq!(
            item(true, Some(Effect::Weapon(WeaponEffect::Wayward))),
            Some(CURSE)
        );
        assert_eq!(
            item(true, Some(Effect::Armor(ArmorEffect::Overgrowth))),
            Some(CURSE)
        );
        assert_eq!(item(false, None), None);
        // Generation always sets `cursed` alongside a curse effect, so this
        // case cannot occur in a scouted world; it mirrors the web's `itemGlow`.
        assert_eq!(
            item(false, Some(Effect::Weapon(WeaponEffect::Wayward))),
            None
        );
    }

    #[test]
    fn requirement_effects_fall_back_to_the_curse_glow() {
        assert_eq!(effect(None), None);
        assert_eq!(
            effect(Some(Effect::Armor(ArmorEffect::Swiftness))),
            enchantment("Swiftness")
        );
        assert_eq!(
            effect(Some(Effect::Weapon(WeaponEffect::Crystal))),
            enchantment("Crystal")
        );
        assert_eq!(
            effect(Some(Effect::Armor(ArmorEffect::Stench))),
            Some(CURSE)
        );
        assert_eq!(
            effect(Some(Effect::Weapon(WeaponEffect::Wondrous))),
            Some(CURSE)
        );
    }

    #[test]
    fn the_pulse_ramps_linearly_between_zero_and_the_peak() {
        // One full cycle of the default period is two seconds.
        assert!(value_at(0, 1.0).abs() < 1e-9);
        assert!((value_at(500_000, 1.0) - PEAK / 2.0).abs() < 1e-9);
        assert!((value_at(1_000_000, 1.0) - PEAK).abs() < 1e-9);
        assert!((value_at(1_500_000, 1.0) - PEAK / 2.0).abs() < 1e-9);
        assert!(value_at(2_000_000, 1.0).abs() < 1e-9);
        // Shocking's half-second period peaks twice as often.
        assert!((value_at(500_000, 0.5) - PEAK).abs() < 1e-9);
        // The ramp never leaves 0…PEAK, including before the clock's origin.
        for micros in (-4_000_000..4_000_000).step_by(9_973) {
            let value = value_at(micros, 0.6);
            assert!((0.0..=PEAK).contains(&value), "{value} out of range");
        }
    }
}
