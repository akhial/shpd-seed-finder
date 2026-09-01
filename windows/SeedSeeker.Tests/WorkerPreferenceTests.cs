using System.Collections.ObjectModel;
using System.Text.Json;
using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The worker count: a device-local preference with the engine's core count as
/// its ceiling, which must never leak into anything that carries a query.
/// </summary>
public sealed class WorkerPreferenceTests : IDisposable
{
    private readonly string directory =
        Path.Combine(Path.GetTempPath(), $"seedseeker-workers-{Guid.NewGuid():N}");

    private string At(string name = "workers.json") => Path.Combine(directory, name);

    public void Dispose() { try { Directory.Delete(directory, recursive: true); } catch { } }

    [Fact]
    public void TheCeilingIsTheEnginesOwnCoreCount()
    {
        // The FFI promises at least one; the selector's ceiling is that value
        // verbatim, so it can never offer threads the search will not spawn.
        Assert.True(NativeEngine.AvailableWorkers >= 1);
        Assert.Equal(NativeEngine.AvailableWorkers, WorkerPreference.Ceiling);
    }

    [Fact]
    public void AnUnsetPreferenceMeansEveryCore()
    {
        // No file at all, and a file that carries no count: both default to the
        // ceiling rather than to some fixed number.
        Assert.Equal(6, WorkerPreference.Load(At(), 6));
        Directory.CreateDirectory(directory);
        File.WriteAllText(At("empty.json"), "{}");
        Assert.Equal(6, WorkerPreference.Load(At("empty.json"), 6));
    }

    [Fact]
    public void AnUnreadablePreferenceFallsBackToEveryCore()
    {
        Directory.CreateDirectory(directory);
        File.WriteAllText(At(), "not json at all");
        Assert.Equal(4, WorkerPreference.Load(At(), 4));
    }

    [Theory]
    // Saved on a machine with more cores than this one has, or edited by hand.
    [InlineData(99, 4, 4)]
    [InlineData(5, 4, 4)]
    // Nonsense low values land on one worker, never zero or negative: a search
    // with no workers would never finish.
    [InlineData(0, 4, 1)]
    [InlineData(-3, 4, 1)]
    // In range, kept exactly.
    [InlineData(3, 4, 3)]
    [InlineData(1, 4, 1)]
    // A single-core host can only ever mean one.
    [InlineData(8, 1, 1)]
    public void APersistedCountIsClampedIntoTheEnginesRange(int saved, int ceiling, int expected)
    {
        Assert.Equal(expected, WorkerPreference.Clamp(saved, ceiling));
        Directory.CreateDirectory(directory);
        File.WriteAllText(At(), $$"""{ "Workers": {{saved}} }""");
        Assert.Equal(expected, WorkerPreference.Load(At(), ceiling));
    }

    [Fact]
    public void ASavedCountComesBack()
    {
        WorkerPreference.Save(At(), 3, 8);
        Assert.Equal(3, WorkerPreference.Load(At(), 8));
        // Saving clamps too, so a later load on a smaller ceiling still agrees
        // with what the slider was allowed to show.
        WorkerPreference.Save(At(), 99, 8);
        Assert.Equal(8, WorkerPreference.Load(At(), 8));
        Assert.Equal(2, WorkerPreference.Load(At(), 2));
    }

    [Fact]
    public void SavingSurvivesAnUnwritablePath()
    {
        // A preference that cannot be written must never interrupt a search;
        // the in-memory count stands for the session.
        var blocked = Path.Combine(At("file.json"), "nested", "workers.json");
        Directory.CreateDirectory(directory);
        File.WriteAllText(At("file.json"), "{}");
        WorkerPreference.Save(blocked, 2, 8);
        Assert.Equal(8, WorkerPreference.Load(blocked, 8));
    }

    [Fact]
    public void TheValueLabelReadsAsTheWebClientWordsIt()
    {
        Assert.Equal("3 of 8 cores", WorkerPreference.Describe(3, 8));
        // The label shows what will actually run, so it clamps like the slider.
        Assert.Equal("8 of 8 cores", WorkerPreference.Describe(99, 8));
        Assert.Equal("1 of 1 cores", WorkerPreference.Describe(1, 1));
    }

    /// <summary>
    /// The preference describes this machine, not the hunt: it must be absent
    /// from the query document (and so from share links and results files),
    /// from the saved query, and from presets — two runs differing only in
    /// worker count find exactly the same seeds.
    /// </summary>
    [Fact]
    public void ThePreferenceNeverEntersAQueryDocumentOrExport()
    {
        var query = new QuerySettings
        {
            Requirements = new ObservableCollection<ItemRequirement>([
                new() { Kind = ItemKind.Ring, Upgrade = 3, UpgradeMatch = UpgradeMatch.AtLeast }]),
            MaximumDepth = 12,
            RequireBlacksmith = true,
        };
        var document = ResultsExport.EncodeQueryDocument(query);
        Assert.DoesNotContain("worker", document, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("thread", document, StringComparison.OrdinalIgnoreCase);
        var file = ResultsExport.Encode(query, ["AAA-AAA-BUH"], "1.0.0");
        Assert.DoesNotContain("worker", file, StringComparison.OrdinalIgnoreCase);
        // query.json and presets serialize QuerySettings itself, so the type
        // must carry no worker member at all.
        Assert.DoesNotContain("worker", JsonSerializer.Serialize(query), StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("worker",
            JsonSerializer.Serialize(new QueryPreset { Name = "p", Query = query }),
            StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain(typeof(QuerySettings).GetProperties(),
            property => property.Name.Contains("Worker", StringComparison.OrdinalIgnoreCase));
    }

    /// <summary>
    /// The count reaches the engine instead: both start calls take it, and the
    /// engine clamps it — including the 0 that means every core.
    /// </summary>
    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(int.MaxValue)]
    public void BothStartCallsAcceptAWorkerCount(int workers)
    {
        var query = new QuerySettings
        {
            Requirements = new ObservableCollection<ItemRequirement>([
                new() { Kind = ItemKind.Ring, Upgrade = 2, UpgradeMatch = UpgradeMatch.AtLeast }]),
        };
        var engine = new NativeEngine();
        using (var search = engine.Start(query, workers))
        {
            search.Cancel();
            Assert.True(search.Status().Scanned >= 0);
        }
        using (var resumed = engine.StartResumed(query, 0, 1024, workers))
        {
            resumed.Cancel();
            Assert.True(resumed.Status().Scanned >= 0);
        }
    }
}
