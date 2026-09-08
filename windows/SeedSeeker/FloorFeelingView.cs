// SPDX-License-Identifier: GPL-3.0-or-later
using System.Runtime.InteropServices.WindowsRuntime;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Graphics.Imaging;
using Windows.Storage;

namespace SeedSeeker;

/// <summary>The upstream 15×16 feeling frame, scaled with nearest neighbours.</summary>
public sealed class FloorFeelingView : Grid
{
    private readonly Image art = new();
    private static Task<(byte[] Pixels, int Width)>? atlas;
    private int generation;
    private XamlRoot? hookedRoot;
    public static readonly DependencyProperty FeelingProperty = DependencyProperty.Register(
        nameof(Feeling), typeof(FloorFeeling), typeof(FloorFeelingView),
        new PropertyMetadata(FloorFeeling.None, (sender, _) => ((FloorFeelingView)sender).Refresh()));
    public FloorFeeling Feeling { get => (FloorFeeling)GetValue(FeelingProperty); set => SetValue(FeelingProperty, value); }

    public FloorFeelingView()
    {
        Width = 15; Height = 16; IsHitTestVisible = false;
        Children.Add(art);
        Loaded += (_, _) =>
        {
            hookedRoot = XamlRoot;
            if (hookedRoot is not null) hookedRoot.Changed += RootChanged;
            Refresh();
        };
        Unloaded += (_, _) =>
        {
            if (hookedRoot is not null) hookedRoot.Changed -= RootChanged;
            hookedRoot = null;
            generation++;
        };
        Visibility = Visibility.Collapsed;
    }

    private void RootChanged(XamlRoot sender, XamlRootChangedEventArgs args) => Refresh();
    private void Refresh()
    {
        var token = ++generation;
        Visibility = Feeling is > FloorFeeling.None and <= FloorFeeling.Secrets ? Visibility.Visible : Visibility.Collapsed;
        art.Source = null;
        AutomationProperties.SetName(this, Feeling == FloorFeeling.None ? "" : $"{Feeling} floor");
        if (Visibility == Visibility.Visible) _ = RenderAsync(token, Feeling);
    }

    private async Task RenderAsync(int token, FloorFeeling feeling)
    {
        try
        {
            var (pixels, width) = await (atlas ??= LoadAtlasAsync());
            if (token != generation) return;
            var scale = XamlRoot?.RasterizationScale ?? 1;
            var w = Math.Max(1, (int)Math.Round(15 * scale));
            var h = Math.Max(1, (int)Math.Round(16 * scale));
            var output = new byte[w * h * 4];
            for (var y = 0; y < h; y++)
                for (var x = 0; x < w; x++)
                    Array.Copy(pixels, ((64 + y * 16 / h) * width + 16 * (int)feeling + x * 15 / w) * 4,
                        output, (y * w + x) * 4, 4);
            var bitmap = new WriteableBitmap(w, h);
            using var stream = bitmap.PixelBuffer.AsStream();
            stream.Write(output, 0, output.Length);
            bitmap.Invalidate();
            art.Source = bitmap;
        }
        catch { /* Missing artwork must not prevent scouting. */ }
    }

    private static async Task<(byte[], int)> LoadAtlasAsync()
    {
        var file = await StorageFile.GetFileFromPathAsync(Path.Combine(AppContext.BaseDirectory, "Assets", "dungeon-icons.png"));
        using var stream = await file.OpenReadAsync();
        var decoder = await BitmapDecoder.CreateAsync(stream);
        var data = await decoder.GetPixelDataAsync(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied,
            new BitmapTransform(), ExifOrientationMode.IgnoreExifOrientation, ColorManagementMode.DoNotColorManage);
        return (data.DetachPixelData(), (int)decoder.PixelWidth);
    }
}
