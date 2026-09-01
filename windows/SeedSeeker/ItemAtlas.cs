// SPDX-License-Identifier: GPL-3.0-or-later
using System.Runtime.InteropServices.WindowsRuntime;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.UI;

namespace SeedSeeker;

/// <summary>
/// The upstream Shattered Pixel Dungeon item atlases, decoded once into pixel
/// buffers and composed into nearest-neighbour scaled bitmaps.
///
/// The geometry mirrors <c>web/src/lib/sprites.ts</c>: <c>items.png</c> is a
/// 16-column grid of 16×16 cells indexed row-major by sprite index, and the art is
/// anchored to each cell's top-left, so drawing the full cell leaves small items
/// (rings, darts, seeds) hugging the corner. Each cell's art is therefore cropped
/// to its alpha bounding box — measured here at runtime, no build step — and that
/// crop is centred in the target box at the same pixel scale a full-cell render
/// would use. A ring's art is just its gem, which the run — not the ring class —
/// decides, so the two are separate inputs here: the caller passes the cell to
/// draw and, alongside it, the class's glyph from <c>item_icons.png</c> (8×8
/// cells), drawn at the same scale anchored to the sprite box's top-right. The
/// glyph cannot be recovered from the cell, since twelve classes share the twelve
/// gem cells in an order the seed picks.
///
/// WinUI 3's <c>Image</c> exposes no interpolation-mode knob, so scaling is done
/// here by hand into a <see cref="WriteableBitmap"/> sized in device pixels; the
/// caller passes the effective raster scale so sprites stay crisp at 150%/200%.
/// </summary>
internal sealed class ItemAtlas
{
    private const int Cell = 16;
    private const int Columns = 16;
    private const int IconCell = 8;
    private const int IconColumns = 16;
    /// <summary>Cached bitmaps per atlas beyond which the caches are dropped wholesale.</summary>
    private const int CacheLimit = 512;

    /// <summary>
    /// Art dimensions (width, height) of each ring glyph within its 8×8 cell,
    /// index-aligned to the ring classes (Accuracy, Arcana, … Wealth) — the
    /// catalog's <c>typeIcon</c>, not the cell the ring is drawn in.
    /// </summary>
    private static readonly (int Width, int Height)[] RingIconSizes =
    [
        (7, 7), (7, 7), (7, 7), (7, 5), (7, 7), (5, 6),
        (7, 6), (6, 6), (7, 7), (7, 7), (6, 6), (7, 6),
    ];

    private sealed record Layer(byte[] Pixels, int Width, int Height);

    private readonly Layer items;
    private readonly Layer icons;
    private readonly Dictionary<int, (int X, int Y, int Width, int Height)> bounds = new();
    private readonly Dictionary<(int Index, int TypeIcon, int Size), WriteableBitmap> sprites = new();
    private readonly Dictionary<(int Index, int Size, uint Color), WriteableBitmap> masks = new();

    private ItemAtlas(Layer items, Layer icons)
    {
        this.items = items;
        this.icons = icons;
    }

    private static Task<ItemAtlas?>? loading;

    /// <summary>
    /// The shared atlas, decoded on first use, or null when the bundled artwork
    /// could not be read. Awaiting this completes synchronously once loaded, so
    /// callers on the UI thread do not yield on the common path.
    /// </summary>
    public static Task<ItemAtlas?> GetAsync() => loading ??= LoadAsync();

    private static async Task<ItemAtlas?> LoadAsync()
    {
        try
        {
            return new ItemAtlas(await DecodeAsync("items.png"), await DecodeAsync("item_icons.png"));
        }
        catch
        {
            return null;
        }
    }

    private static async Task<Layer> DecodeAsync(string name)
    {
        var file = await StorageFile.GetFileFromPathAsync(Path.Combine(AppContext.BaseDirectory, "Assets", name));
        using var stream = await file.OpenReadAsync();
        var decoder = await BitmapDecoder.CreateAsync(stream);
        var data = await decoder.GetPixelDataAsync(
            BitmapPixelFormat.Bgra8,
            BitmapAlphaMode.Premultiplied,
            new BitmapTransform(),
            ExifOrientationMode.IgnoreExifOrientation,
            ColorManagementMode.DoNotColorManage);
        return new Layer(data.DetachPixelData(), (int)decoder.PixelWidth, (int)decoder.PixelHeight);
    }

    /// <summary>True when the atlas actually holds a cell for this sprite index.</summary>
    public bool Contains(int index) =>
        index >= 0 && (index / Columns + 1) * Cell <= items.Height && Columns * Cell <= items.Width;

    /// <summary>
    /// The sprite for <paramref name="index"/>, cropped, centred and scaled into a
    /// <paramref name="size"/>×<paramref name="size"/> device-pixel bitmap, with
    /// ring glyph <paramref name="typeIcon"/> overlaid when one is given.
    /// </summary>
    /// <param name="index">The <c>items.png</c> cell to draw. For a scouted ring
    /// this is the run's gem, not the class's catalog cell.</param>
    /// <param name="typeIcon">The ring class's cell in <c>item_icons.png</c>, or
    /// -1 for an item that carries no glyph.</param>
    public WriteableBitmap? Sprite(int index, int typeIcon, int size)
    {
        if (!Contains(index) || size <= 0) return null;
        if (sprites.TryGetValue((index, typeIcon, size), out var cached)) return cached;
        if (sprites.Count >= CacheLimit) sprites.Clear();
        var bitmap = Bitmap(Compose(index, size, null, typeIcon), size);
        sprites[(index, typeIcon, size)] = bitmap;
        return bitmap;
    }

    /// <summary>
    /// A solid <paramref name="color"/> layer masked to the sprite's opaque pixels,
    /// matched pixel-for-pixel to <see cref="Sprite"/>. Stacking it over the sprite
    /// and animating its opacity to 0.6 reproduces upstream's glow shader
    /// (<c>rgb = texel.rgb*(1-v) + glow*v</c>); alpha outside the silhouette stays
    /// zero, so there is no bloom or halo. The ring glyph is deliberately excluded,
    /// matching the web.
    /// </summary>
    public WriteableBitmap? Mask(int index, int size, Color color)
    {
        if (!Contains(index) || size <= 0) return null;
        var key = (index, size, (uint)((color.R << 16) | (color.G << 8) | color.B));
        if (masks.TryGetValue(key, out var cached)) return cached;
        if (masks.Count >= CacheLimit) masks.Clear();
        var bitmap = Bitmap(Compose(index, size, color, -1), size);
        masks[key] = bitmap;
        return bitmap;
    }

    private static WriteableBitmap Bitmap(byte[] pixels, int size)
    {
        var bitmap = new WriteableBitmap(size, size);
        WindowsRuntimeBufferExtensions.CopyTo(pixels, bitmap.PixelBuffer);
        bitmap.Invalidate();
        return bitmap;
    }

    /// <summary>
    /// Premultiplied BGRA for one sprite. With <paramref name="tint"/> null this is
    /// the art itself; otherwise it is the tinted alpha mask used for the glow.
    /// </summary>
    private byte[] Compose(int index, int size, Color? tint, int typeIcon)
    {
        var buffer = new byte[size * size * 4];
        var (boundsX, boundsY, boundsWidth, boundsHeight) = Bounds(index);
        var scale = size / (double)Cell;
        var width = Math.Clamp((int)Math.Round(boundsWidth * scale), 1, size);
        var height = Math.Clamp((int)Math.Round(boundsHeight * scale), 1, size);
        var left = (size - width) / 2;
        var top = (size - height) / 2;
        var column = index % Columns;
        var row = index / Columns;
        for (var y = 0; y < height; y++)
        {
            var sourceY = row * Cell + boundsY + Math.Min(boundsHeight - 1, y * boundsHeight / height);
            for (var x = 0; x < width; x++)
            {
                var sourceX = column * Cell + boundsX + Math.Min(boundsWidth - 1, x * boundsWidth / width);
                var source = (sourceY * items.Width + sourceX) * 4;
                var target = ((top + y) * size + left + x) * 4;
                var alpha = items.Pixels[source + 3];
                if (tint is Color color)
                {
                    buffer[target] = (byte)(color.B * alpha / 255);
                    buffer[target + 1] = (byte)(color.G * alpha / 255);
                    buffer[target + 2] = (byte)(color.R * alpha / 255);
                }
                else
                {
                    buffer[target] = items.Pixels[source];
                    buffer[target + 1] = items.Pixels[source + 1];
                    buffer[target + 2] = items.Pixels[source + 2];
                }
                buffer[target + 3] = alpha;
            }
        }
        if (tint is null) DrawRingIcon(buffer, typeIcon, size, scale);
        return buffer;
    }

    /// <summary>
    /// Overlays ring glyph <paramref name="icon"/>, doing nothing when the item
    /// has none. The glyph is the ring class's own and is passed in: the drawn
    /// cell is the run's gem, which says nothing about which ring this is.
    /// </summary>
    private void DrawRingIcon(byte[] buffer, int icon, int size, double scale)
    {
        if (icon < 0 || icon >= RingIconSizes.Length) return;
        var (artWidth, artHeight) = RingIconSizes[icon];
        var width = Math.Clamp((int)Math.Round(artWidth * scale), 1, size);
        var height = Math.Clamp((int)Math.Round(artHeight * scale), 1, size);
        var left = size - width;
        var column = icon % IconColumns;
        var row = icon / IconColumns;
        if ((row + 1) * IconCell > icons.Height) return;
        for (var y = 0; y < height; y++)
        {
            var sourceY = row * IconCell + Math.Min(artHeight - 1, y * artHeight / height);
            for (var x = 0; x < width; x++)
            {
                var sourceX = column * IconCell + Math.Min(artWidth - 1, x * artWidth / width);
                var source = (sourceY * icons.Width + sourceX) * 4;
                var alpha = icons.Pixels[source + 3];
                if (alpha == 0) continue;
                // Premultiplied source-over: out = src + dst × (1 − srcAlpha).
                var target = (y * size + left + x) * 4;
                var remainder = 255 - alpha;
                buffer[target] = (byte)Math.Min(255, icons.Pixels[source] + buffer[target] * remainder / 255);
                buffer[target + 1] = (byte)Math.Min(255, icons.Pixels[source + 1] + buffer[target + 1] * remainder / 255);
                buffer[target + 2] = (byte)Math.Min(255, icons.Pixels[source + 2] + buffer[target + 2] * remainder / 255);
                buffer[target + 3] = (byte)Math.Min(255, alpha + buffer[target + 3] * remainder / 255);
            }
        }
    }

    /// <summary>
    /// The art's alpha bounding box (x, y, width, height) within the sprite's 16×16
    /// cell, measured on first use. Fully transparent cells fall back to the whole
    /// cell so the geometry stays well defined.
    /// </summary>
    private (int X, int Y, int Width, int Height) Bounds(int index)
    {
        if (bounds.TryGetValue(index, out var cached)) return cached;
        var column = index % Columns;
        var row = index / Columns;
        int minimumX = Cell, minimumY = Cell, maximumX = -1, maximumY = -1;
        for (var y = 0; y < Cell; y++)
        {
            for (var x = 0; x < Cell; x++)
            {
                if (items.Pixels[((row * Cell + y) * items.Width + column * Cell + x) * 4 + 3] == 0) continue;
                if (x < minimumX) minimumX = x;
                if (x > maximumX) maximumX = x;
                if (y < minimumY) minimumY = y;
                if (y > maximumY) maximumY = y;
            }
        }
        (int X, int Y, int Width, int Height) result = maximumX < 0
            ? (0, 0, Cell, Cell)
            : (minimumX, minimumY, maximumX - minimumX + 1, maximumY - minimumY + 1);
        bounds[index] = result;
        return result;
    }
}
