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

    [Fact]
    public void AlternativesEffectSetsAndSumsRoundTripThroughTheEngine()
    {
        var query = new QuerySettings
        {
            Requirements = new ObservableCollection<ItemRequirement>([
                new() { Kind = ItemKind.MeleeWeapon, Item = ItemCatalog.Find("spear"), UpgradeMatch = UpgradeMatch.Exactly, Upgrade = 3, AlternativeGroup = 1 },
                new() { Kind = ItemKind.ThrownWeapon, Item = ItemCatalog.Find("shuriken"), UpgradeMatch = UpgradeMatch.Exactly, Upgrade = 2, AlternativeGroup = 1, Effect = EffectFilter.OneOf(["Blocking", "Projecting"]) },
                new() { Kind = ItemKind.Armor, Effect = EffectFilter.Enchantment(), RequireUncursed = true },
                new() { Kind = ItemKind.Ring, Item = ItemCatalog.Find("ring_might"), LevelSum = new(2, 4) },
                new() { Kind = ItemKind.Ring, Item = ItemCatalog.Find("ring_might"), LevelSum = new(2, 4), MaximumDepth = 4 },
                // A same-item stack: a named anchor and a plain copy of its category.
                new() { Kind = ItemKind.Wand, Item = ItemCatalog.Find("wand_frost"), UpgradeMatch = UpgradeMatch.AtLeast, Upgrade = 1, IdentityGroup = 1 },
                new() { Kind = ItemKind.Wand, UpgradeMatch = UpgradeMatch.Any, IdentityGroup = 1 },
            ]),
        };
        const string expected = """{"requirements":[{"any_of":[{"kind":"melee_weapon","item":"spear","upgrade":3},{"kind":"thrown_weapon","item":"shuriken","upgrade":2,"effect":["Blocking","Projecting"]}]},{"kind":"armor","effect":"any_enchantment","uncursed":true},{"kind":"ring","item":"ring_might","level_sum":{"group":2,"at_least":4}},{"kind":"ring","item":"ring_might","max_depth":4,"level_sum":{"group":2,"at_least":4}},{"kind":"wand","item":"wand_frost","upgrade":{"at_least":1},"identity_group":1},{"kind":"wand","identity_group":1}]}""";
        var document = ResultsExport.EncodeQueryDocument(query);
        Assert.Equal(expected, document);

        // Through the real codec and back: the engine validates and re-encodes it.
        var imported = ResultsExport.Decode(ResultsExport.Encode(query, ["AAA-AAA-BUH"], "test")).Query;
        Assert.Equal(document, ResultsExport.EncodeQueryDocument(imported));
        Assert.Equal([1, 1, null, null, null, null, null], imported.Requirements.Select(r => r.AlternativeGroup));
        Assert.Equal(["Blocking", "Projecting"], imported.Requirements[1].Effect.Effects);
        Assert.True(imported.Requirements[2].Effect.AnyEnchantment);
        Assert.Equal(new LevelSum(2, 4), imported.Requirements[3].LevelSum);
        Assert.Equal(new LevelSum(2, 4), imported.Requirements[4].LevelSum);
        Assert.Equal([null, null, null, null, null, 1, 1], imported.Requirements.Select(r => r.IdentityGroup));
        // The share-link codec carries the same structures.
        var link = NativeEngine.TryEncodeShareLink(document);
        Assert.NotNull(link);
        var shared = ResultsExport.DecodeQueryDocument(NativeEngine.TryDecodeShareText(link!)!);
        Assert.Equal(document, ResultsExport.EncodeQueryDocument(shared));
    }

    [Fact]
    public void TheVaultSourceAndTheNewEffectsRoundTripThroughTheEngine()
    {
        // v4.0.0's additions, end to end: the ceilings the Imp's vault raised
        // (+5 on a weapon, +4 on everything else), the vault's own item source,
        // and the four new enchantments and two new curses.
        var query = new QuerySettings
        {
            Requirements = new ObservableCollection<ItemRequirement>([
                new() { Kind = ItemKind.MeleeWeapon, UpgradeMatch = UpgradeMatch.Exactly, Upgrade = 5 },
                new() { Kind = ItemKind.Armor, UpgradeMatch = UpgradeMatch.Exactly, Upgrade = 3, Source = ScoutItemSource.VaultTreasure },
                new() { Kind = ItemKind.Weapon, Effect = EffectFilter.OneOf(["Venomous", "Eldritch", "Vorpal", "Crystal"]) },
                new() { Kind = ItemKind.ThrownWeapon, Effect = EffectFilter.OneOf(["Pressurized", "Wondrous"]) },
                new() { Kind = ItemKind.Wand, UpgradeMatch = UpgradeMatch.Exactly, Upgrade = 4 },
            ]),
        };
        // The effect sets are written in catalog order, whatever order they were picked in.
        const string expected = """{"requirements":[{"kind":"melee_weapon","upgrade":5},{"kind":"armor","upgrade":3,"source":"vault_treasure"},{"kind":"weapon","effect":["Venomous","Eldritch","Vorpal","Crystal"]},{"kind":"thrown_weapon","effect":["Pressurized","Wondrous"]},{"kind":"wand","upgrade":4}]}""";
        var document = ResultsExport.EncodeQueryDocument(query);
        Assert.Equal(expected, document);

        // Through the real codec and back: the engine validates and re-encodes it.
        var imported = ResultsExport.Decode(ResultsExport.Encode(query, ["AAA-AAA-BUH"], "test")).Query;
        Assert.Equal(document, ResultsExport.EncodeQueryDocument(imported));
        Assert.Equal(ScoutItemSource.VaultTreasure, imported.Requirements[1].Source);
        Assert.Equal(5, imported.Requirements[0].Upgrade);
        // A share link, whose 32-bit effect masks carry the new effects.
        var link = NativeEngine.TryEncodeShareLink(document);
        Assert.NotNull(link);
        var shared = ResultsExport.DecodeQueryDocument(NativeEngine.TryDecodeShareText(link!)!);
        Assert.Equal(document, ResultsExport.EncodeQueryDocument(shared));
    }

    [Fact]
    public void TheUnreleasedUpgradeSumKeyIsRefused()
    {
        // upgrade_sum counted upgrades, not levels: a document carrying it is
        // refused rather than silently reinterpreted, as the engine does.
        var error = Assert.Throws<ResultsExportException>(() =>
            ResultsExport.DecodeQueryDocument("""{"requirements":[{"kind":"ring","upgrade_sum":{"group":1,"at_least":2}}]}"""));
        Assert.Contains("upgrade_sum", error.Message);
        var renamed = ResultsExport.DecodeQueryDocument("""{"requirements":[{"kind":"ring","level_sum":{"group":1,"at_least":2}}]}""");
        Assert.Equal(new LevelSum(1, 2), Assert.Single(renamed.Requirements).LevelSum);
    }

    [Fact]
    public void TheWriterFollowsTheEffectRules()
    {
        static string Effect(EffectFilter filter, ItemKind kind = ItemKind.Weapon) =>
            ResultsExport.EncodeQueryDocument(new QuerySettings
            {
                Requirements = new ObservableCollection<ItemRequirement>([new() { Kind = kind, Effect = filter }]),
            });
        Assert.Equal("""{"requirements":[{"kind":"weapon","effect":"Blazing"}]}""", Effect(EffectFilter.OneOf(["Blazing"])));
        // Catalog order, whatever order the set was picked in.
        Assert.Equal("""{"requirements":[{"kind":"weapon","effect":["Blazing","Vampiric"]}]}""", Effect(EffectFilter.OneOf(["Vampiric", "Blazing"])));
        // The whole non-curse family set is the shorthand, however it was expressed.
        Assert.Equal("""{"requirements":[{"kind":"weapon","effect":"any_enchantment"}]}""", Effect(EffectFilter.Enchantment()));
        Assert.Equal("""{"requirements":[{"kind":"armor","effect":"any_enchantment"}]}""", Effect(EffectFilter.OneOf(ItemCatalog.Glyphs), ItemKind.Armor));
        Assert.Equal("""{"requirements":[{"kind":"weapon"}]}""", Effect(EffectFilter.Any()));
        // A single-member alternative group is written plain.
        var lone = new QuerySettings { Requirements = new ObservableCollection<ItemRequirement>([new() { Kind = ItemKind.Wand, AlternativeGroup = 3 }]) };
        Assert.Equal("""{"requirements":[{"kind":"wand"}]}""", ResultsExport.EncodeQueryDocument(lone));
    }

    [Fact]
    public void ReadingAssignsFreshSequentialAlternativeGroups()
    {
        var decoded = ResultsExport.DecodeQueryDocument("""
            {"requirements":[
              {"any_of":[{"kind":"wand"},{"kind":"ring"}]},
              {"kind":"armor","effect":["thorns","stench"]},
              {"any_of":[{"kind":"melee_weapon","effect":"any_enchantment"},{"kind":"thrown_weapon"}]}]}
            """);
        Assert.Equal([1, 1, null, 2, 2], decoded.Requirements.Select(r => r.AlternativeGroup));
        // Effect names resolve case-insensitively to the catalog's spelling.
        Assert.Equal(["Thorns", "Stench"], decoded.Requirements[2].Effect.Effects);
        Assert.True(decoded.Requirements[3].Effect.AnyEnchantment);
        Assert.Throws<ResultsExportException>(() => ResultsExport.DecodeQueryDocument("""{"requirements":[{"kind":"weapon","effect":["Blazing","Nope"]}]}"""));
    }

    [Theory]
    // The engine's own relationship rules, reached through the results codec.
    [InlineData("""{"kind":"wand","upgrade_sum":{"group":1,"at_least":7}},{"kind":"wand","upgrade_sum":{"group":1,"at_least":7}}""")]
    [InlineData("""{"kind":"wand","upgrade_sum":{"group":1,"at_least":2}},{"kind":"wand","upgrade_sum":{"group":1,"at_least":3}}""")]
    [InlineData("""{"any_of":[{"kind":"wand","upgrade_sum":{"group":1,"at_least":1}},{"kind":"ring"}]}""")]
    [InlineData("""{"kind":"ring","effect":"any_enchantment"}""")]
    [InlineData("""{"kind":"weapon","effect":["Blazing","Thorns"]}""")]
    [InlineData("""{"kind":"weapon","uncursed":true,"effect":["Annoying","Sacrificial"]}""")]
    [InlineData("""{"any_of":[]}""")]
    public void TheEngineRefusesInvalidRelationships(string requirements) =>
        Assert.Throws<ResultsExportException>(() => ResultsExport.Decode(
            $$"""{ "format": "seed-seeker-results", "query": { "requirements": [{{requirements}}] }, "results": [] }"""));
}
