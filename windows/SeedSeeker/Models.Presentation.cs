using System.Text.Json.Serialization;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;

namespace SeedSeeker;

// The XAML-bound halves of the model types in Models.cs, split out so that
// file stays free of Windows App SDK types and can compile into
// SeedSeeker.Tests on any host.

public static partial class KindStyle
{
    public static Brush Tint(ItemKind kind) => new SolidColorBrush(kind.Family() switch { ItemKind.Weapon => Colors.DarkOrange, ItemKind.Armor => Colors.DodgerBlue, ItemKind.Wand => Colors.MediumPurple, _ => Colors.Goldenrod });
}

public sealed partial class ItemRequirement
{
    [JsonIgnore] public Brush Tint => KindStyle.Tint(Kind);
    [JsonIgnore] public Visibility SpriteVisibility => Item is null ? Visibility.Collapsed : Visibility.Visible;
    /// <summary>The generic glyph shows only where there is genuinely no concrete item.</summary>
    [JsonIgnore] public Visibility FallbackVisibility => Item is null ? Visibility.Visible : Visibility.Collapsed;
    /// <summary>
    /// Glow for the pinned enchantment or curse, with the bare-effect-name semantics
    /// of the web's <c>effectGlow</c>: an unrecognised effect is a curse and glows
    /// black. There is nothing to tint without a sprite, so wildcards never glow.
    /// </summary>
    [JsonIgnore] public Windows.UI.Color GlowColor => ItemGlow.ForEffect(Modifier)?.Color ?? default;
    [JsonIgnore] public double GlowPeriod => Item is null ? 0 : ItemGlow.ForEffect(Modifier)?.Period ?? 0;
}
