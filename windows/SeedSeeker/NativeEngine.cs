using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;

namespace SeedSeeker;

internal static partial class Native
{
    private const string Library = "shpd_seedfinder_ffi";
    [LibraryImport(Library)] internal static partial long seedfinder_start_search(byte[] request, nuint length);
    [LibraryImport(Library)] internal static partial int seedfinder_poll(long handle, uint maximum, out nint packet, out nuint length);
    [LibraryImport(Library)] internal static partial int seedfinder_status(long handle, [Out] long[] status);
    [LibraryImport(Library)] internal static partial void seedfinder_cancel(long handle);
    [LibraryImport(Library)] internal static partial void seedfinder_close(long handle);
    [LibraryImport(Library)] internal static partial int seedfinder_scout(byte[] request, nuint length, out nint packet, out nuint outputLength);
    [LibraryImport(Library)] internal static partial void seedfinder_buffer_free(nint packet, nuint length);
}

internal sealed class Writer
{
    private readonly MemoryStream stream = new();
    public void Bytes(IEnumerable<byte> bytes) { foreach (var b in bytes) stream.WriteByte(b); }
    public void U8(int value) => stream.WriteByte((byte)value);
    public void U16(int value) { U8(value >> 8); U8(value); }
    public void U16Le(int value) { U8(value); U8(value >> 8); }
    public void Text(string value) { var b = Encoding.UTF8.GetBytes(value); U16(b.Length); Bytes(b); }
    public byte[] Finish() => stream.ToArray();
}

internal ref struct Reader
{
    private readonly ReadOnlySpan<byte> data; private int offset;
    public Reader(byte[] bytes) { data = bytes; offset = 0; }
    public int Remaining => data.Length - offset;
    public byte U8() { if (Remaining < 1) throw new InvalidDataException("Truncated native packet"); return data[offset++]; }
    public int U16() => U8() << 8 | U8();
    public ulong U64() { ulong v = 0; for (var i = 0; i < 8; i++) v = v << 8 | U8(); return v; }
    public string Text(int count) { if (count < 0 || Remaining < count) throw new InvalidDataException("Truncated native packet"); var s = Encoding.UTF8.GetString(data.Slice(offset, count)); offset += count; return s; }
    public string Text() => Text(U16());
    public void Magic(string expected) { if (Text(4) != expected) throw new InvalidDataException("Unexpected native packet"); }
}

public static partial class SeedCode
{
    [GeneratedRegex("^[A-Z]{3}-[A-Z]{3}-[A-Z]{3}$")] private static partial Regex CanonicalRegex();
    public static bool IsCanonical(string value) => CanonicalRegex().IsMatch(value);
    public static string Format(string value)
    {
        var letters = new string(value.ToUpperInvariant().Where(c => c is >= 'A' and <= 'Z').Take(9).ToArray());
        return string.Join('-', Enumerable.Range(0, (letters.Length + 2) / 3).Select(i => letters.Substring(i * 3, Math.Min(3, letters.Length - i * 3))));
    }
}

public sealed class NativeEngine
{
    public NativeSearch Start(QuerySettings query)
    {
        var w = new Writer(); w.Bytes("SSF7"u8.ToArray()); w.U8(query.MaximumDepth);
        w.U8((query.RequireBlacksmith ? 1 : 0) | (query.FastMode ? 2 : 0) | (query.ExcludeBlacksmithRewards ? 4 : 0));
        w.U16Le(query.Challenges); w.U16(query.Requirements.Count);
        foreach (var r in query.Requirements)
        {
            w.U8((int)r.Kind); w.Text(r.Item?.Id ?? ""); w.U8((int)r.TierMatch); w.U8(r.Tier);
            w.U8((int)r.UpgradeMatch); w.U8(r.Upgrade); w.Text(r.Modifier ?? "");
            w.U8(r.Source is null ? 0 : (int)r.Source + 1); w.U8(r.IdentityGroup ?? 0); w.U8(r.MaximumDepth ?? 0); w.U8(r.RequireUncursed ? 1 : 0);
        }
        var packet = w.Finish(); var handle = Native.seedfinder_start_search(packet, (nuint)packet.Length);
        if (handle == 0) throw new InvalidOperationException("The native engine rejected the query.");
        return new NativeSearch(handle);
    }

    public ScoutWorld Scout(string seed, int challenges)
    {
        if (!SeedCode.IsCanonical(seed)) throw new ArgumentException("Seed must use XXX-XXX-XXX format");
        var w = new Writer(); w.Bytes("SSQ2"u8.ToArray()); w.U16Le(challenges); w.Bytes(Encoding.ASCII.GetBytes(seed));
        var request = w.Finish(); var code = Native.seedfinder_scout(request, (nuint)request.Length, out var ptr, out var len);
        if (code != 0) throw new InvalidOperationException($"Native scout failed ({code}).");
        var bytes = CopyAndFree(ptr, len); var r = new Reader(bytes); r.Magic("SSC1");
        var returnedSeed = r.Text(r.U8()); var items = new List<ScoutItem>(); var count = r.U16();
        for (var i = 0; i < count; i++)
        {
            var item = ItemCatalog.Find(r.Text()) ?? throw new InvalidDataException("Unknown item in scout packet");
            var depth = r.U8(); var upgrade = r.U8(); var flags = r.U8(); var effect = r.Text();
            var source = (ScoutItemSource)r.U8(); var tag = r.U8(); var group = 0; ulong value = 0;
            if (tag == 1) { group = r.U16(); value = r.U8(); } else if (tag == 2) { group = r.U16(); value = r.U64(); } else if (tag != 0) throw new InvalidDataException("Unknown accessibility tag");
            items.Add(new(item, depth, upgrade, effect.Length == 0 ? null : effect, (flags & 1) != 0, source, tag, group, value));
        }
        if (r.Remaining != 0) throw new InvalidDataException("Trailing native data");
        return new(returnedSeed, items);
    }

    internal static byte[] CopyAndFree(nint ptr, nuint len)
    {
        try { var bytes = new byte[(int)len]; Marshal.Copy(ptr, bytes, 0, bytes.Length); return bytes; }
        finally { if (ptr != 0) Native.seedfinder_buffer_free(ptr, len); }
    }
}

public sealed class NativeSearch : IDisposable
{
    private long handle;
    internal NativeSearch(long value) => handle = value;
    public IReadOnlyList<string> Poll(int maximum)
    {
        var code = Native.seedfinder_poll(handle, (uint)maximum, out var ptr, out var len);
        if (code != 0) throw new InvalidOperationException($"Native poll failed ({code}).");
        var r = new Reader(NativeEngine.CopyAndFree(ptr, len)); r.Magic("SSR1");
        var result = new List<string>(); var count = r.U16(); for (var i = 0; i < count; i++) result.Add(r.Text(r.U8()));
        return result;
    }
    public SearchStatus Status()
    {
        var raw = new long[5]; var code = Native.seedfinder_status(handle, raw); if (code != 0) throw new InvalidOperationException($"Native status failed ({code}).");
        return new((SearchState)raw[0], raw[1], raw[2], raw[3], BitConverter.Int64BitsToDouble(raw[4]));
    }
    public void Cancel() { if (handle != 0) Native.seedfinder_cancel(handle); }
    public void Dispose() { var old = Interlocked.Exchange(ref handle, 0); if (old != 0) Native.seedfinder_close(old); GC.SuppressFinalize(this); }
    ~NativeSearch() => Dispose();
}
