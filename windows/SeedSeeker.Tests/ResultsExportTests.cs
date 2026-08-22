using System.Collections.ObjectModel;
using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The results-file codec, exercised through the engine it now delegates to
/// (<c>seedfinder_results_encode</c> / <c>seedfinder_results_decode</c>). The
/// shared fixtures under crates/seedfinder-core/tests/fixtures are the same
/// documents the core's own tests read, so a file the engine accepts imports
/// here with the same meaning — and the validation the engine applies is the
/// validation this app now gets.
/// </summary>
public sealed class ResultsExportTests
{
    private static string Fixture(string name)
    {
        var root = NativeEngineLibrary.WorkspaceRoot()
            ?? throw new InvalidOperationException("Could not locate the workspace root.");
        return File.ReadAllText(Path.Combine(root, "crates", "seedfinder-core", "tests", "fixtures", name));
    }

    [Fact]
    public void TheSharedFixtureImportsWithItsQueryAndSeeds()
    {
        var imported = ResultsExport.Decode(Fixture("results-export-v1.json"));
        Assert.Equal(["AAA-AAA-BUH", "ABC-DEF-GHI"], imported.Seeds);
        Assert.Equal(0, imported.Dropped);
        Assert.Equal("3.3.8", imported.FileShpdVersion);

        var query = imported.Query;
        Assert.Equal(12, query.MaximumDepth);
        Assert.True(query.RequireBlacksmith);
        Assert.False(query.ExcludeBlacksmithRewards);
        Assert.False(query.FastMode);
        Assert.Equal(8, query.Challenges); // barren_land
        Assert.Equal(WandmakerQuest.Any, query.WandmakerQuest);

        var ring = query.Requirements[0];
        Assert.Equal(ItemKind.Ring, ring.Kind);
        Assert.Equal("ring_tenacity", ring.Item?.Id);
        Assert.Equal(UpgradeMatch.Exactly, ring.UpgradeMatch);
        Assert.Equal(4, ring.Upgrade);
        Assert.Equal(ScoutItemSource.ImpReward, ring.Source);

        var wand = query.Requirements[1];
        Assert.Equal(ItemKind.Wand, wand.Kind);
        Assert.Equal(UpgradeMatch.AtLeast, wand.UpgradeMatch);
        Assert.Equal(2, wand.Upgrade);
        Assert.True(wand.RequireUncursed);
        Assert.Equal(1, wand.IdentityGroup);
        Assert.Equal(9, wand.MaximumDepth);
    }

    [Theory]
    [InlineData("results-export-v1.json")]
    [InlineData("results-export-v1-weapon-categories.json")]
    [InlineData("results-export-wandmaker-quest.json")]
    public void ExportingAnImportedFileRoundTripsIt(string fixture)
    {
        var imported = ResultsExport.Decode(Fixture(fixture));
        var again = ResultsExport.Decode(ResultsExport.Encode(imported.Query, imported.Seeds, "test"));

        Assert.Equal(imported.Seeds, again.Seeds);
        Assert.Equal(0, again.Dropped);
        // The engine stamps the version it targets onto everything it writes.
        Assert.Equal(EngineInfo.ShpdVersion, again.FileShpdVersion);
        Assert.Equal(ResultsExport.EncodeQueryDocument(imported.Query),
            ResultsExport.EncodeQueryDocument(again.Query));
    }

    [Fact]
    public void WeaponCategoriesAndEffectsSurviveTheRoundTrip()
    {
        var query = ResultsExport.Decode(Fixture("results-export-v1-weapon-categories.json")).Query;
        Assert.Equal(
            [ItemKind.ThrownWeapon, ItemKind.MeleeWeapon, ItemKind.Weapon],
            query.Requirements.Select(r => r.Kind));
        Assert.Equal("Projecting", query.Requirements[0].Modifier);
        Assert.Equal("sword", query.Requirements[1].Item?.Id);
        Assert.Equal(20, query.MaximumDepth);
    }

    [Fact]
    public void TheWandmakerQuestFilterSurvivesTheRoundTrip()
    {
        var imported = ResultsExport.Decode(Fixture("results-export-wandmaker-quest.json"));
        Assert.Equal(WandmakerQuest.Rotberry, imported.Query.WandmakerQuest);
        var again = ResultsExport.Decode(ResultsExport.Encode(imported.Query, imported.Seeds, "test"));
        Assert.Equal(WandmakerQuest.Rotberry, again.Query.WandmakerQuest);
    }

    [Theory]
    // Not JSON, not a results file, no query, no results, and a seed code the
    // file format does not allow.
    [InlineData("not json at all")]
    [InlineData("{}")]
    [InlineData("""{ "format": "seed-seeker-results", "results": [] }""")]
    [InlineData("""{ "format": "seed-seeker-results", "query": { "requirements": [{ "kind": "ring" }] } }""")]
    [InlineData("""{ "format": "seed-seeker-results", "query": { "requirements": [{ "kind": "ring" }] }, "results": [{ "seed": "aaa-aaa-aaa" }] }""")]
    // A query with no requirements is not a searchable query.
    [InlineData("""{ "format": "seed-seeker-results", "query": { "requirements": [] }, "results": [] }""")]
    // An unknown query field means a newer writer; importing it would change
    // the query's meaning silently.
    [InlineData("""{ "format": "seed-seeker-results", "query": { "requirements": [{ "kind": "ring" }], "future": 1 }, "results": [] }""")]
    public void AMalformedFileIsRefused(string contents) =>
        Assert.Throws<ResultsExportException>(() => ResultsExport.Decode(contents));

    [Theory]
    // The four validation gaps the hand-written C# decoder used to let past:
    // an out-of-range exact tier, an over-cap upgrade, "uncursed" alongside a
    // curse, and a same-item group whose members cannot be the same item.
    [InlineData("""{ "kind": "weapon", "tier": { "exact": 9 } }""")]
    [InlineData("""{ "kind": "wand", "upgrade": 99 }""")]
    [InlineData("""{ "kind": "weapon", "effect": "Wayward", "uncursed": true }""")]
    [InlineData("""{ "kind": "wand", "identity_group": 1 }, { "kind": "ring", "identity_group": 1 }""")]
    public void TheEngineRefusesQueriesTheOldDecoderAccepted(string requirements) =>
        Assert.Throws<ResultsExportException>(() => ResultsExport.Decode(
            $$"""{ "format": "seed-seeker-results", "query": { "requirements": [{{requirements}}] }, "results": [] }"""));

    [Fact]
    public void ImportedSeedsAreDeduplicatedAndCappedByTheEngine()
    {
        var seeds = Enumerable.Range(0, 3).Select(_ => """{ "seed": "AAA-AAA-BUH" }""");
        var imported = ResultsExport.Decode(
            $$"""
            { "format": "seed-seeker-results",
              "query": { "requirements": [{ "kind": "ring" }] },
              "results": [{{string.Join(",", seeds)}}] }
            """);
        Assert.Equal(["AAA-AAA-BUH"], imported.Seeds);
        Assert.Equal(2, imported.Dropped);
    }

    [Fact]
    public void AQueryDocumentRoundTripsThroughTheShareCodecShape()
    {
        var query = new QuerySettings
        {
            Requirements = new ObservableCollection<ItemRequirement>([
                new() { Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.AtLeast, Upgrade = 2, RequireUncursed = true },
                new() { Kind = ItemKind.Armor, TierMatch = TierMatch.AtLeast, Tier = 4, Modifier = "Brimstone" },
            ]),
            MaximumDepth = 14,
            ExcludeBlacksmithRewards = true,
            Challenges = 1 | 32,
        };
        var document = ResultsExport.EncodeQueryDocument(query);
        var decoded = ResultsExport.DecodeQueryDocument(document);
        Assert.Equal(document, ResultsExport.EncodeQueryDocument(decoded));
        Assert.Equal(14, decoded.MaximumDepth);
        Assert.True(decoded.ExcludeBlacksmithRewards);
        Assert.Equal(1 | 32, decoded.Challenges);
        Assert.Equal("Brimstone", decoded.Requirements[1].Modifier);
    }
}
