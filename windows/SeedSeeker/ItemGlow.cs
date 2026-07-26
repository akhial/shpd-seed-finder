// SPDX-License-Identifier: GPL-3.0-or-later
using Windows.UI;

namespace SeedSeeker;

/// <summary>
/// A colour the sprite blends toward at the pulse peak, and the seconds it takes
/// to get there. A complete fade-in/out cycle lasts <c>2 × Period</c>.
/// </summary>
public sealed record SpriteGlow(Color Color, double Period);

/// <summary>
/// Enchantment / glyph glow colours and pulse periods, mirrored 1:1 from
/// Shattered Pixel Dungeon's <c>ItemSprite.Glowing</c> definitions (and from
/// <c>web/src/lib/glow.ts</c>, which is the reference implementation) so the item
/// sprites pulse exactly as the game renders them. Curses always glow black.
/// These are item data straight from the game, not app chrome: the Fluent palette
/// is untouched.
/// </summary>
public static class ItemGlow
{
    /// <summary>Upstream's default <c>Glowing(color)</c> period when none is given (1f).</summary>
    private const double DefaultPeriod = 1.0;

    private static Color Rgb(uint value) =>
        Color.FromArgb(255, (byte)(value >> 16), (byte)(value >> 8), (byte)value);

    // Keyed by the wire names the scout emits (WeaponEffect / ArmorEffect
    // wire_name in seedfinder-core). Only non-curse effects live here; every
    // curse glows black and is handled by Curse below.
    private static readonly Dictionary<string, SpriteGlow> Enchantments = new(StringComparer.Ordinal)
    {
        // Weapon enchantments
        ["Blazing"] = new(Rgb(0xff4400), DefaultPeriod),
        ["Chilling"] = new(Rgb(0x00ffff), DefaultPeriod),
        ["Kinetic"] = new(Rgb(0xffff00), DefaultPeriod),
        ["Shocking"] = new(Rgb(0xffffff), 0.5),
        ["Blocking"] = new(Rgb(0x0000ff), DefaultPeriod),
        ["Blooming"] = new(Rgb(0x008800), DefaultPeriod),
        ["Elastic"] = new(Rgb(0xff00ff), DefaultPeriod),
        ["Lucky"] = new(Rgb(0x00ff00), DefaultPeriod),
        ["Projecting"] = new(Rgb(0x8844cc), DefaultPeriod),
        ["Unstable"] = new(Rgb(0x999999), DefaultPeriod),
        ["Corrupting"] = new(Rgb(0x440066), DefaultPeriod),
        ["Grim"] = new(Rgb(0x000000), DefaultPeriod),
        ["Vampiric"] = new(Rgb(0x660022), DefaultPeriod),
        // Armor glyphs
        ["Obfuscation"] = new(Rgb(0x888888), DefaultPeriod),
        ["Swiftness"] = new(Rgb(0xffff00), DefaultPeriod),
        ["Viscosity"] = new(Rgb(0x8844cc), DefaultPeriod),
        ["Potential"] = new(Rgb(0xffffff), 0.6),
        ["Brimstone"] = new(Rgb(0xff4400), DefaultPeriod),
        ["Stone"] = new(Rgb(0x222222), DefaultPeriod),
        ["Entanglement"] = new(Rgb(0x663300), DefaultPeriod),
        ["Repulsion"] = new(Rgb(0xffffff), DefaultPeriod),
        ["Camouflage"] = new(Rgb(0x448822), DefaultPeriod),
        ["Flow"] = new(Rgb(0x0000ff), DefaultPeriod),
        ["Affection"] = new(Rgb(0xff4488), DefaultPeriod),
        ["Anti-Magic"] = new(Rgb(0x88eeff), DefaultPeriod),
        ["Thorns"] = new(Rgb(0x660022), DefaultPeriod),
    };

    /// <summary>Every curse glows black in the game, at the default period.</summary>
    public static SpriteGlow Curse { get; } = new(Rgb(0x000000), DefaultPeriod);

    /// <summary>
    /// The pulse glow for a scouted item, or null when it carries no enchantment
    /// or curse. A beneficial enchantment/glyph wins even on a cursed item
    /// (matching <c>Weapon.glowing()</c>, which returns the enchantment's colour
    /// when one is present — e.g. a curse-infused Kinetic weapon still glows
    /// yellow); otherwise a cursed item pulses black.
    /// </summary>
    public static SpriteGlow? ForItem(ScoutItem item)
    {
        if (item.Effect is string effect && !ItemCatalog.IsCurse(item.Item.Kind, effect))
            return Enchantments.GetValueOrDefault(effect);
        return item.Cursed ? Curse : null;
    }

    /// <summary>
    /// The pulse glow for a bare effect name (as carried by a requirement), or
    /// null when there is none. Known enchantments/glyphs pulse their colour; any
    /// other effect name is a curse and pulses black.
    /// </summary>
    public static SpriteGlow? ForEffect(string? effect) =>
        effect is null ? null : Enchantments.GetValueOrDefault(effect) ?? Curse;
}
