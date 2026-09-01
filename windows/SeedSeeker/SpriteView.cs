// SPDX-License-Identifier: GPL-3.0-or-later
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Windows.Foundation;
using Windows.UI;
using Windows.UI.ViewManagement;

namespace SeedSeeker;

/// <summary>
/// One Shattered Pixel Dungeon item sprite, scaled nearest-neighbour, optionally
/// pulsed by its enchantment or curse glow.
///
/// The glow is a solid colour layer masked to the sprite's opaque pixels whose
/// opacity animates linearly 0 → 0.6 → 0 over <c>2 × GlowPeriod</c> seconds. That
/// reproduces upstream's shader (<c>rgb = texel.rgb*(1-v) + glow*v</c>) with <c>v</c>
/// peaking at 0.6, matching <c>web/src/designs/one/styles.css</c>; alpha is
/// untouched, so only the art tints — no bloom, no halo. When the system has
/// animations turned off the layer is held at 0.3, the web's reduced-motion value.
/// </summary>
public sealed class SpriteView : Grid
{
    /// <summary>Peak opacity of the glow layer, matching upstream's shader peak.</summary>
    private const double PeakOpacity = 0.6;
    /// <summary>Held opacity when the system has animations turned off.</summary>
    private const double StaticOpacity = PeakOpacity / 2;

    private static readonly UISettings SystemSettings = new();

    private readonly Image art = new() { Stretch = Stretch.Fill };
    private readonly Image glow = new() { Stretch = Stretch.Fill, Opacity = 0 };

    private Storyboard? pulse;
    private bool live;
    private XamlRoot? hookedRoot;
    private TypedEventHandler<XamlRoot, XamlRootChangedEventArgs>? rootChanged;
    private (int Index, int TypeIcon, int Size, uint Glow) rendered = (int.MinValue, int.MinValue, 0, 0);
    private int generation;

    public SpriteView()
    {
        IsHitTestVisible = false;
        Children.Add(art);
        Children.Add(glow);
        Width = SpriteSize;
        Height = SpriteSize;
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
    }

    /// <summary>
    /// Row-major index into <c>items.png</c>; negative renders nothing. For a
    /// ring drawn as part of a scouted seed this is the cell that run's gems
    /// give it, which is not the ring class's catalog cell — hence
    /// <see cref="TypeIconIndex"/> beside it.
    /// </summary>
    public static readonly DependencyProperty SpriteIndexProperty = DependencyProperty.Register(
        nameof(SpriteIndex), typeof(int), typeof(SpriteView), new PropertyMetadata(-1, OnVisualChanged));

    /// <summary>
    /// Row-major index of the ring glyph in <c>item_icons.png</c> overlaid on the
    /// sprite, or -1 for an item that carries no glyph. It names the ring class
    /// and so cannot be derived from <see cref="SpriteIndex"/>.
    /// </summary>
    public static readonly DependencyProperty TypeIconIndexProperty = DependencyProperty.Register(
        nameof(TypeIconIndex), typeof(int), typeof(SpriteView), new PropertyMetadata(-1, OnVisualChanged));

    /// <summary>Edge of the square sprite box, in DIPs.</summary>
    public static readonly DependencyProperty SpriteSizeProperty = DependencyProperty.Register(
        nameof(SpriteSize), typeof(double), typeof(SpriteView), new PropertyMetadata(24.0, OnVisualChanged));

    /// <summary>Colour the art blends toward at the pulse peak.</summary>
    public static readonly DependencyProperty GlowColorProperty = DependencyProperty.Register(
        nameof(GlowColor), typeof(Color), typeof(SpriteView), new PropertyMetadata(default(Color), OnVisualChanged));

    /// <summary>Seconds to reach peak glow; zero or less means no glow at all.</summary>
    public static readonly DependencyProperty GlowPeriodProperty = DependencyProperty.Register(
        nameof(GlowPeriod), typeof(double), typeof(SpriteView), new PropertyMetadata(0.0, OnVisualChanged));

    public int SpriteIndex
    {
        get => (int)GetValue(SpriteIndexProperty);
        set => SetValue(SpriteIndexProperty, value);
    }

    public int TypeIconIndex
    {
        get => (int)GetValue(TypeIconIndexProperty);
        set => SetValue(TypeIconIndexProperty, value);
    }

    public double SpriteSize
    {
        get => (double)GetValue(SpriteSizeProperty);
        set => SetValue(SpriteSizeProperty, value);
    }

    public Color GlowColor
    {
        get => (Color)GetValue(GlowColorProperty);
        set => SetValue(GlowColorProperty, value);
    }

    public double GlowPeriod
    {
        get => (double)GetValue(GlowPeriodProperty);
        set => SetValue(GlowPeriodProperty, value);
    }

    private static void OnVisualChanged(DependencyObject sender, DependencyPropertyChangedEventArgs e) =>
        ((SpriteView)sender).Invalidate(force: false);

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        live = true;
        // Re-render when the raster scale changes so the bitmap stays crisp; rows
        // are recycled, so the subscription is released again in OnUnloaded.
        if (hookedRoot is null && XamlRoot is XamlRoot root)
        {
            rootChanged ??= (_, _) => Invalidate(force: false);
            hookedRoot = root;
            root.Changed += rootChanged;
        }
        Invalidate(force: true);
    }

    private void OnUnloaded(object sender, RoutedEventArgs e)
    {
        live = false;
        StopPulse();
        if (hookedRoot is not null && rootChanged is not null) hookedRoot.Changed -= rootChanged;
        hookedRoot = null;
    }

    private void Invalidate(bool force)
    {
        var size = SpriteSize;
        Width = size;
        Height = size;
        art.Width = size;
        art.Height = size;
        glow.Width = size;
        glow.Height = size;
        var period = GlowPeriod;
        var color = GlowColor;
        // The high bit distinguishes "no glow" from "glow that happens to be black".
        var tint = period > 0 ? 0x1000000u | (uint)((color.R << 16) | (color.G << 8) | color.B) : 0u;
        var pixels = (int)Math.Max(1, Math.Round(size * EffectiveScale()));
        var key = (SpriteIndex, TypeIconIndex, pixels, tint);
        if (!force && key == rendered) return;
        rendered = key;
        StopPulse();
        if (SpriteIndex < 0)
        {
            art.Source = null;
            glow.Source = null;
            return;
        }
        _ = ApplyAsync(++generation, pixels, period);
    }

    /// <summary>
    /// The raster scale to size the bitmap from, so it stays crisp at 150%/200%.
    /// Prefers the XamlRoot's scale and falls back to the element's own suggestion
    /// before the control is in a tree.
    /// </summary>
    private double EffectiveScale()
    {
        var scale = XamlRoot?.RasterizationScale ?? 0;
        if (scale <= 0) scale = RasterizationScale;
        return scale > 0 ? scale : 1.0;
    }

    private async Task ApplyAsync(int token, int pixels, double period)
    {
        // Completes synchronously once the atlas has been decoded.
        var atlas = await ItemAtlas.GetAsync();
        if (atlas is null || token != generation) return;
        art.Source = atlas.Sprite(SpriteIndex, TypeIconIndex, pixels);
        if (period > 0)
        {
            glow.Source = atlas.Mask(SpriteIndex, pixels, GlowColor);
            StartPulse(period);
        }
        else
        {
            glow.Source = null;
        }
    }

    private void StartPulse(double period)
    {
        StopPulse();
        if (!live || glow.Source is null) return;
        if (!AnimationsEnabled())
        {
            glow.Opacity = StaticOpacity;
            return;
        }
        var animation = new DoubleAnimationUsingKeyFrames
        {
            EnableDependentAnimation = true,
            RepeatBehavior = RepeatBehavior.Forever,
        };
        animation.KeyFrames.Add(new LinearDoubleKeyFrame { KeyTime = KeyTime.FromTimeSpan(TimeSpan.Zero), Value = 0 });
        animation.KeyFrames.Add(new LinearDoubleKeyFrame { KeyTime = KeyTime.FromTimeSpan(TimeSpan.FromSeconds(period)), Value = PeakOpacity });
        animation.KeyFrames.Add(new LinearDoubleKeyFrame { KeyTime = KeyTime.FromTimeSpan(TimeSpan.FromSeconds(2 * period)), Value = 0 });
        Storyboard.SetTarget(animation, glow);
        Storyboard.SetTargetProperty(animation, "Opacity");
        var storyboard = new Storyboard();
        storyboard.Children.Add(animation);
        pulse = storyboard;
        storyboard.Begin();
    }

    private void StopPulse()
    {
        if (pulse is not null)
        {
            pulse.Stop();
            pulse.Children.Clear();
            pulse = null;
        }
        glow.Opacity = 0;
    }

    private static bool AnimationsEnabled()
    {
        try { return SystemSettings.AnimationsEnabled; }
        catch { return true; }
    }
}
