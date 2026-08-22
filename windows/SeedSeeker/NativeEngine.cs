using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace SeedSeeker;

internal static partial class Native
{
    private const string Library = "shpd_seedfinder_ffi";
    [LibraryImport(Library)] internal static partial long seedfinder_start_search(byte[] request, nuint length);
    [LibraryImport(Library)] internal static partial long seedfinder_start_resumed_search(byte[] request, nuint length, ulong resumeFrom, ulong scanLength);
    [LibraryImport(Library)] internal static partial int seedfinder_poll(long handle, uint maximum, out nint packet, out nuint length);
    [LibraryImport(Library)] internal static partial int seedfinder_status(long handle, [Out] long[] status);
    [LibraryImport(Library)] internal static partial int seedfinder_resume_hint(long handle, [Out] long[] hint);
    [LibraryImport(Library)] internal static partial void seedfinder_cancel(long handle);
    [LibraryImport(Library)] internal static partial void seedfinder_close(long handle);
    [LibraryImport(Library)] internal static partial int seedfinder_scout(byte[] request, nuint length, out nint packet, out nuint outputLength);
    [LibraryImport(Library)] internal static partial int seedfinder_scout_matches(byte[] request, nuint length, byte[] query, nuint queryLength, out nint packet, out nuint outputLength);
    [LibraryImport(Library)] internal static partial int seedfinder_filter_seeds(byte[] request, nuint length, ulong[] seeds, nuint seedsLength, out nint packet, out nuint outputLength);
    [LibraryImport(Library)] internal static partial int seedfinder_query_continues(byte[] candidate, nuint candidateLength, byte[] baseline, nuint baselineLength);
    [LibraryImport(Library)] internal static partial int seedfinder_decide_start(byte[] candidate, nuint candidateLength, byte[]? target, nuint targetLength, int targetSetEmpty, int targetHasUncoveredSeeds, byte[]? detachedBase, nuint detachedBaseLength, out nint packet, out nuint outputLength);
    [LibraryImport(Library)] internal static partial int seedfinder_share_encode(byte[] queryJson, nuint length, out nint packet, out nuint outputLength);
    [LibraryImport(Library)] internal static partial int seedfinder_share_decode(byte[] text, nuint length, out nint packet, out nuint outputLength);
    [LibraryImport(Library)] internal static partial int seedfinder_results_encode(byte[] request, nuint length, out nint packet, out nuint outputLength);
    [LibraryImport(Library)] internal static partial int seedfinder_results_decode(byte[] contents, nuint length, out nint packet, out nuint outputLength);
    [LibraryImport(Library)] internal static partial int seedfinder_engine_info(out nint packet, out nuint outputLength);
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
    public IReadOnlyList<ScoutQuest> Quests() => ScoutQuests.Parse(data, ref offset);
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
    /// <summary>The numeric seed a canonical code names: nine base-26 letters, A = 0, dashes ignored.</summary>
    public static ulong Value(string value)
    {
        ulong result = 0; var letters = 0;
        foreach (var c in value)
        {
            if (c == '-') continue;
            if (c is < 'A' or > 'Z' || letters == 9) throw new ArgumentException($"Seed must use XXX-XXX-XXX format: {value}");
            result = result * 26 + (ulong)(c - 'A'); letters++;
        }
        if (letters != 9) throw new ArgumentException($"Seed must use XXX-XXX-XXX format: {value}");
        return result;
    }
}

public sealed class NativeEngine
{
    public NativeSearch Start(QuerySettings query)
    {
        var packet = EncodeQuery(query); var handle = Native.seedfinder_start_search(packet, (nuint)packet.Length);
        if (handle == 0) throw new InvalidOperationException("The native engine rejected the query.");
        return new NativeSearch(handle);
    }

    public NativeSearch StartResumed(QuerySettings query, long resumeFrom, long scanLength)
    {
        var packet = EncodeQuery(query); var handle = Native.seedfinder_start_resumed_search(packet, (nuint)packet.Length, (ulong)resumeFrom, (ulong)scanLength);
        if (handle == 0) throw new InvalidOperationException("The native engine rejected the query.");
        return new NativeSearch(handle);
    }

    public IReadOnlyList<string> FilterSeeds(QuerySettings query, IReadOnlyList<string> seeds)
    {
        if (seeds.Count == 0) return [];
        var packet = EncodeQuery(query); var values = seeds.Select(SeedCode.Value).ToArray();
        var code = Native.seedfinder_filter_seeds(packet, (nuint)packet.Length, values, (nuint)values.Length, out var ptr, out var len);
        if (code != 0) throw new InvalidOperationException($"Native filter failed ({code}).");
        return ReadSeedList(CopyAndFree(ptr, len));
    }

    /// <summary>
    /// Whether <paramref name="candidate"/> continues <paramref name="baseline"/>:
    /// an identical floor limit, challenge set and fast mode, world conditions
    /// (the blacksmith flags and the Wandmaker quest) at least as strict as the
    /// baseline's, and every baseline requirement covered by a distinct
    /// candidate requirement at least as strict (equal or strengthened).
    /// The engine owns this predicate — the same
    /// <c>SearchQuery::continues</c> that decides which seeds a resumed pass may
    /// skip — so the decision is made on the encoded SSF8 packets rather than
    /// re-derived here, and the two can never drift.
    /// </summary>
    public static bool QueryContinues(QuerySettings candidate, QuerySettings baseline)
    {
        var left = EncodeQuery(candidate); var right = EncodeQuery(baseline);
        // A query the engine cannot decode continues nothing, matching the web
        // frontend: an unsearchable query — one with no requirements, say — has
        // no result set to inherit, so the only sound answer is a fresh scan.
        // The UI never asks about one anyway: Start stays disabled until a
        // requirement exists, and imports reject a query without them.
        return Native.seedfinder_query_continues(left, (nuint)left.Length, right, (nuint)right.Length) == 1;
    }

    private static byte[] EncodeQuery(QuerySettings query)
    {
        var w = new Writer(); w.Bytes("SSF8"u8.ToArray()); w.U8(query.MaximumDepth);
        w.U8((query.RequireBlacksmith ? 1 : 0) | (query.FastMode ? 2 : 0) | (query.ExcludeBlacksmithRewards ? 4 : 0));
        w.U16Le(query.Challenges); w.U8((int)query.WandmakerQuest); w.U16(query.Requirements.Count);
        foreach (var r in query.Requirements)
        {
            w.U8((int)r.Kind); w.Text(r.Item?.Id ?? ""); w.U8((int)r.TierMatch); w.U8(r.Tier);
            w.U8((int)r.UpgradeMatch); w.U8(r.Upgrade); w.Text(r.Modifier ?? "");
            w.U8(r.Source is null ? 0 : (int)r.Source + 1); w.U8(r.IdentityGroup ?? 0); w.U8(r.MaximumDepth ?? 0); w.U8(r.RequireUncursed ? 1 : 0);
        }
        return w.Finish();
    }

    /// <summary>
    /// What pressing Start Search must do with <paramref name="query"/>, per
    /// docs/search-semantics.md. The Target Set is the anchor: a continuation
    /// of the Target Query refines it, a query sharing an item filters it, and
    /// anything else scans the full range without touching it — continuing the
    /// previous detached scan when that is sound.
    ///
    /// The engine decides. <c>seedfinder_decide_start</c> is handed both
    /// encoded queries, whether the Target Set is empty and whether the target
    /// still has uncovered seeds, so the continuation predicate that gates a
    /// resumed scan and the dispatch built on it can never disagree.
    /// </summary>
    /// <param name="target">The session's Target, if one has been established.</param>
    /// <param name="lastDetachedQuery">The query of the previous run when that
    /// run was a detached scan that concluded (completed or cancelled), null
    /// otherwise. Only such a run may be continued by a query unrelated to the
    /// Target; a failed run is never a continuation base.</param>
    public static StartMode DecideStart(QuerySettings query, TargetRun? target, QuerySettings? lastDetachedQuery = null)
    {
        var candidate = EncodeQuery(query);
        var targetPacket = target is null ? null : EncodeQuery(target.Query);
        var detachedPacket = lastDetachedQuery is null ? null : EncodeQuery(lastDetachedQuery);
        var code = Native.seedfinder_decide_start(
            candidate, (nuint)candidate.Length,
            targetPacket, (nuint)(targetPacket?.Length ?? 0),
            target is { Seeds.Count: 0 } ? 1 : 0,
            target is { Remaining: > 0 } ? 1 : 0,
            detachedPacket, (nuint)(detachedPacket?.Length ?? 0),
            out var ptr, out var len);
        if (code != 0) throw new InvalidOperationException($"Native start decision failed ({code}).");
        var decision = Encoding.UTF8.GetString(CopyAndFree(ptr, len));
        return decision switch
        {
            "anchor" => StartMode.Anchor,
            "target-refine" => StartMode.TargetRefine,
            "target-filter" => StartMode.TargetFilter,
            "continue-detached" => StartMode.ContinueDetached,
            "detached" => StartMode.Detached,
            _ => throw new InvalidDataException($"Unknown start decision \"{decision}\""),
        };
    }

    /// <summary>The SSQ2 request naming one scouted world; scouting it is deterministic.</summary>
    private static byte[] EncodeScoutRequest(string seed, int challenges)
    {
        if (!SeedCode.IsCanonical(seed)) throw new ArgumentException("Seed must use XXX-XXX-XXX format");
        var w = new Writer(); w.Bytes("SSQ2"u8.ToArray()); w.U16Le(challenges); w.Bytes(Encoding.ASCII.GetBytes(seed));
        return w.Finish();
    }

    public ScoutWorld Scout(string seed, int challenges)
    {
        var request = EncodeScoutRequest(seed, challenges);
        var code = Native.seedfinder_scout(request, (nuint)request.Length, out var ptr, out var len);
        if (code != 0) throw new InvalidOperationException($"Native scout failed ({code}).");
        var bytes = CopyAndFree(ptr, len); var r = new Reader(bytes); r.Magic("SSC2");
        var returnedSeed = r.Text(r.U8()); var quests = r.Quests(); var items = new List<ScoutItem>(); var count = r.U16();
        for (var i = 0; i < count; i++)
        {
            var item = ItemCatalog.Find(r.Text()) ?? throw new InvalidDataException("Unknown item in scout packet");
            var depth = r.U8(); var upgrade = r.U8(); var flags = r.U8(); var effect = r.Text();
            var source = (ScoutItemSource)r.U8(); var tag = r.U8(); var group = 0; ulong value = 0;
            if (tag == 1) { group = r.U16(); value = r.U8(); } else if (tag == 2) { group = r.U16(); value = r.U64(); } else if (tag != 0) throw new InvalidDataException("Unknown accessibility tag");
            items.Add(new(item, depth, upgrade, effect.Length == 0 ? null : effect, (flags & 1) != 0, source, tag, group, value, Secret: (flags & 2) != 0));
        }
        if (r.Remaining != 0) throw new InvalidDataException("Trailing native data");
        return new(returnedSeed, quests, items);
    }

    /// <summary>
    /// Which items of the world scouted by <paramref name="seed"/> and
    /// <paramref name="challenges"/> satisfy <paramref name="query"/>, as
    /// indices into the item list <see cref="Scout"/> returns for the same
    /// request. The engine owns the selection — the very matcher the search
    /// runs, so a marked manifest can never disagree with the result list —
    /// and it is asked over the same SSQ2 request bytes, which name the world
    /// exactly.
    /// </summary>
    public static ScoutMatches ScoutMatches(string seed, int challenges, QuerySettings query)
    {
        var request = EncodeScoutRequest(seed, challenges); var packet = EncodeQuery(query);
        var code = Native.seedfinder_scout_matches(request, (nuint)request.Length, packet, (nuint)packet.Length, out var ptr, out var len);
        // A query the engine cannot decode — one with no requirements, which
        // the scout pane shows a manifest for anyway — marks nothing.
        if (code == -1) return new(new HashSet<int>(), 0, query.Requirements.Count);
        if (code != 0) throw new InvalidOperationException($"Native scout matches failed ({code}).");
        var document = JsonNode.Parse(Encoding.UTF8.GetString(CopyAndFree(ptr, len))) as JsonObject
            ?? throw new InvalidDataException("Unreadable scout match document");
        var matched = new HashSet<int>();
        foreach (var index in document["matched"] as JsonArray ?? [])
            if (index is JsonValue value && value.TryGetValue(out int number)) matched.Add(number);
        return new(matched, (int?)document["matched_requirements"] ?? matched.Count,
            (int?)document["total_requirements"] ?? query.Requirements.Count);
    }

    /// <summary>The full web share link for a canonical JSON query document, or null when the engine rejects the query.</summary>
    public static string? TryEncodeShareLink(string queryJson)
    {
        var bytes = Encoding.UTF8.GetBytes(queryJson);
        return Native.seedfinder_share_encode(bytes, (nuint)bytes.Length, out var ptr, out var len) == 0
            ? Encoding.UTF8.GetString(CopyAndFree(ptr, len)) : null;
    }

    /// <summary>The canonical JSON query document carried by share-link text (web link, seedseeker:// link, or bare code), or null when there is none.</summary>
    public static string? TryDecodeShareText(string text)
    {
        var bytes = Encoding.UTF8.GetBytes(text);
        return Native.seedfinder_share_decode(bytes, (nuint)bytes.Length, out var ptr, out var len) == 0
            ? Encoding.UTF8.GetString(CopyAndFree(ptr, len)) : null;
    }

    /// <summary>
    /// The results-file text for the UTF-8 JSON request
    /// <c>{"query", "seeds", "app_version"}</c>, or null when the engine
    /// rejects it. The file schema and every validation rule are the core
    /// codec's (crates/seedfinder-core/src/results_export.rs).
    /// </summary>
    public static string? TryEncodeResultsFile(string requestJson)
    {
        var bytes = Encoding.UTF8.GetBytes(requestJson);
        return Native.seedfinder_results_encode(bytes, (nuint)bytes.Length, out var ptr, out var len) == 0
            ? Encoding.UTF8.GetString(CopyAndFree(ptr, len)) : null;
    }

    /// <summary>
    /// The UTF-8 JSON <c>{"query", "seeds", "dropped", "app_version",
    /// "shpd_version"}</c> a results file carries — seeds already deduplicated
    /// and capped by the engine — or null when the text is not an importable
    /// results file.
    /// </summary>
    public static string? TryDecodeResultsFile(string contents)
    {
        var bytes = Encoding.UTF8.GetBytes(contents);
        return Native.seedfinder_results_decode(bytes, (nuint)bytes.Length, out var ptr, out var len) == 0
            ? Encoding.UTF8.GetString(CopyAndFree(ptr, len)) : null;
    }

    /// <summary>The engine's own constants document, as UTF-8 JSON.</summary>
    public static string EngineInfoJson()
    {
        var code = Native.seedfinder_engine_info(out var ptr, out var len);
        if (code != 0) throw new InvalidOperationException($"Native engine info failed ({code}).");
        return Encoding.UTF8.GetString(CopyAndFree(ptr, len));
    }

    internal static byte[] CopyAndFree(nint ptr, nuint len)
    {
        try { var bytes = new byte[(int)len]; Marshal.Copy(ptr, bytes, 0, bytes.Length); return bytes; }
        finally { if (ptr != 0) Native.seedfinder_buffer_free(ptr, len); }
    }

    internal static IReadOnlyList<string> ReadSeedList(byte[] bytes)
    {
        var r = new Reader(bytes); r.Magic("SSR1");
        var result = new List<string>(); var count = r.U16(); for (var i = 0; i < count; i++) result.Add(r.Text(r.U8()));
        return result;
    }
}

/// <summary>
/// The engine's own constants, read once from <c>seedfinder_engine_info</c>
/// instead of mirrored in C#: the pinned upstream version and commit, and the
/// limits the shared codecs enforce.
/// </summary>
public static class EngineInfo
{
    private static readonly JsonObject Document =
        JsonNode.Parse(NativeEngine.EngineInfoJson()) as JsonObject
        ?? throw new InvalidOperationException("The engine returned an unreadable info document.");

    /// <summary>The upstream Shattered Pixel Dungeon version this engine reproduces.</summary>
    public static string ShpdVersion { get; } = Text("shpdVersion");

    /// <summary>The upstream commit the reproduction was ported from.</summary>
    public static string ShpdCommit { get; } = Text("shpdCommit");

    /// <summary>The import cap the results codec enforces on file text.</summary>
    public static int ResultsFileMaxBytes { get; } = Limit("resultsFileMaxBytes");

    private static string Text(string key) => (string?)Document[key]
        ?? throw new InvalidOperationException($"The engine info document has no \"{key}\".");

    private static int Limit(string key) => (int?)(Document["limits"] as JsonObject)?[key]
        ?? throw new InvalidOperationException($"The engine info document has no limit \"{key}\".");
}

public sealed class NativeSearch : IDisposable
{
    private long handle;
    internal NativeSearch(long value) => handle = value;
    public IReadOnlyList<string> Poll(int maximum)
    {
        var code = Native.seedfinder_poll(handle, (uint)maximum, out var ptr, out var len);
        if (code != 0) throw new InvalidOperationException($"Native poll failed ({code}).");
        return NativeEngine.ReadSeedList(NativeEngine.CopyAndFree(ptr, len));
    }
    public SearchStatus Status()
    {
        var raw = new long[5]; var code = Native.seedfinder_status(handle, raw); if (code != 0) throw new InvalidOperationException($"Native status failed ({code}).");
        return new((SearchState)raw[0], raw[1], raw[2], raw[3], BitConverter.Int64BitsToDouble(raw[4]));
    }
    public (long ResumeFrom, long Remaining) ResumeHint()
    {
        var hint = new long[2]; var code = Native.seedfinder_resume_hint(handle, hint); if (code != 0) throw new InvalidOperationException($"Native resume hint failed ({code}).");
        return (hint[0], hint[1]);
    }
    public void Cancel() { if (handle != 0) Native.seedfinder_cancel(handle); }
    public void Dispose() { var old = Interlocked.Exchange(ref handle, 0); if (old != 0) Native.seedfinder_close(old); GC.SuppressFinalize(this); }
    ~NativeSearch() => Dispose();
}
