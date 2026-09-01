using System.Text.Json;

namespace SeedSeeker;

/// <summary>
/// How many search threads the engine spawns, as a device-local preference.
///
/// It describes this machine, not the hunt: it never enters a query document,
/// so it cannot reach presets, results files, share links or the continuation
/// predicate — two runs differing only in worker count still find the same
/// seeds, just at different speeds. It therefore lives in its own small file
/// beside the app's other local state (see MainWindow's paths) rather than in
/// the saved query.
///
/// The ceiling is the engine's own <see cref="NativeEngine.AvailableWorkers"/>,
/// never below one; an unset or unreadable preference means "every core", and
/// a saved one is clamped into [1, ceiling] on load so a file written on a
/// bigger machine (or by hand) can never over-subscribe this one.
///
/// Pure but for the two file calls, so the logic stays testable off-Windows
/// (see SeedSeeker.Tests).
/// </summary>
public static class WorkerPreference
{
    /// <summary>Logical processors available to search workers, never less than one.</summary>
    public static int Ceiling => NativeEngine.AvailableWorkers;

    /// <summary>The preference file's shape; an absent count means every core.</summary>
    private sealed class Stored { public int? Workers { get; set; } }

    /// <summary><paramref name="count"/> brought into the [1, ceiling] range the engine accepts.</summary>
    public static int Clamp(int count, int ceiling) => Math.Clamp(count, 1, Math.Max(1, ceiling));

    /// <summary>The selector's value text, as the web client words it.</summary>
    public static string Describe(int count, int ceiling) =>
        $"{Clamp(count, ceiling)} of {Math.Max(1, ceiling)} cores";

    /// <summary>
    /// The saved worker count clamped into [1, <paramref name="ceiling"/>], or
    /// the ceiling itself when nothing is saved yet or the file cannot be read:
    /// the default is always every core.
    /// </summary>
    public static int Load(string path, int ceiling)
    {
        try
        {
            if (File.Exists(path) && JsonSerializer.Deserialize<Stored>(File.ReadAllText(path))?.Workers is int saved)
                return Clamp(saved, ceiling);
        }
        catch { }
        return Math.Max(1, ceiling);
    }

    /// <summary>
    /// Records the clamped count. Failing to write a preference must never
    /// interrupt a search, so an unwritable path is ignored — exactly how the
    /// update state beside it is saved.
    /// </summary>
    public static void Save(string path, int count, int ceiling)
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, JsonSerializer.Serialize(new Stored { Workers = Clamp(count, ceiling) }));
        }
        catch { }
    }
}
