using System.Text.Json;
using Xunit;

namespace SeedSeeker.Tests;

public sealed class ArtifactTests
{
    private static ItemRequirement Sandals() => new()
    {
        Kind = ItemKind.Artifact, Item = ItemCatalog.Find("sandals_of_nature"),
        UpgradeMatch = UpgradeMatch.Exactly, Upgrade = 5,
        Source = ScoutItemSource.ImpReward, MaximumDepth = 19, RequireUncursed = true,
    };

    [Fact]
    public void ArtifactCatalogAndEditorBoundsMatchNamedArtifacts()
    {
        var items = ItemCatalog.For(ItemKind.Artifact).ToList();
        Assert.Equal(11, items.Count);
        Assert.Equal(11, items.Select(item => item.Id).Distinct().Count());
        Assert.All(items, item => Assert.Null(item.Tier));
        Assert.True(ItemKind.Artifact.RequiresNamedItem());
        Assert.Equal(5, Sandals().UpgradeCeiling);
        Assert.Empty(ItemCatalog.Modifiers(ItemKind.Artifact));
        Assert.Contains("+5", Sandals().Description);
        Assert.Contains("by floor 19", Sandals().Description);
    }

    [Fact]
    public void ArtifactsCannotBeWildcardsOrStacks()
    {
        var unnamed = new QuerySettings { Requirements = [new() { Kind = ItemKind.Artifact }] };
        Assert.Equal("Choose a named artifact.", QueryRelationships.Validate(unnamed));
        Assert.Null(NativeEngine.TryEncodeShareLink(ResultsExport.EncodeQueryDocument(unnamed)));
        var query = new QuerySettings { Requirements = [Sandals()] };
        var item = Assert.Single(QueryRelationships.BoardItems(query.Requirements));
        Assert.False(QueryRelationships.CanStack(query.Requirements, item));
        Assert.Single(QueryRelationships.SetStackCount(query.Requirements, item, 3));
        var plain = new ItemRequirement { Kind = ItemKind.Artifact, Item = ItemCatalog.Find("dried_rose") };
        var repeated = new[] { plain, plain.Clone(), Sandals() };
        Assert.Equal(3, QueryRelationships.BoardItems(repeated).Count);
        var joined = QueryRelationships.JoinAlternatives(repeated, 0, 2);
        Assert.All(joined, requirement => Assert.Null(requirement.IdentityGroup));
    }

    [Fact]
    public void ArtifactConstraintsAndAlternativesSurviveDocumentsLinksAndSettings()
    {
        var alternative = new ItemRequirement { Kind = ItemKind.Artifact, Item = ItemCatalog.Find("dried_rose"), MaximumDepth = 9 };
        var query = new QuerySettings { Requirements = new(QueryRelationships.JoinAlternatives([Sandals(), alternative], 1, 0)) };
        var document = ResultsExport.EncodeQueryDocument(query);
        Assert.Contains("\"kind\":\"artifact\"", document);
        var link = NativeEngine.TryEncodeShareLink(document);
        Assert.NotNull(link);
        var linkDocument = NativeEngine.TryDecodeShareText(link!);
        Assert.NotNull(linkDocument);
        var restored = ResultsExport.DecodeQueryDocument(linkDocument!);
        Assert.Equal(1, QueryRelationships.SlotCount(restored.Requirements));
        Assert.All(restored.Requirements, requirement => Assert.Equal(ItemKind.Artifact, requirement.Kind));
        var sandals = restored.Requirements.Single(requirement => requirement.Item?.Id == "sandals_of_nature");
        Assert.Equal(5, sandals.Upgrade);
        Assert.Equal(19, sandals.MaximumDepth);
        Assert.True(sandals.RequireUncursed);
        Assert.Equal(ScoutItemSource.ImpReward, sandals.Source);
        var file = ResultsExport.Encode(query, ["AAA-AAA-AAA"], "test");
        Assert.Equal(document, ResultsExport.EncodeQueryDocument(ResultsExport.Decode(file).Query));
        var settings = JsonSerializer.Deserialize<QuerySettings>(JsonSerializer.Serialize(query))!;
        Assert.Equal(document, ResultsExport.EncodeQueryDocument(settings));
    }

    [Fact]
    public void NativeScoutKeepsArtifactUpgradesAndMatchIndices()
    {
        var world = new NativeEngine().Scout("AAA-AAA-AAA", 0);
        Assert.Equal(4, world.Items.Count(item => item.Item.Kind == ItemKind.Artifact));
        var sandals = Assert.Single(world.Items, item => item.Item.Id == "sandals_of_nature");
        Assert.Equal(5, sandals.Upgrade);
        Assert.Equal(19, sandals.Depth);
        Assert.Equal(ScoutItemSource.ImpReward, sandals.Source);
        var query = new QuerySettings { Requirements = [Sandals()] };
        var matches = NativeEngine.ScoutMatches(world.Seed, 0, query);
        Assert.Equal(1, matches.MatchedRequirements);
        Assert.Equal(sandals, world.Items[Assert.Single(matches.Matched)]);
        query.Requirements[0].MaximumDepth = 18;
        Assert.Empty(NativeEngine.ScoutMatches(world.Seed, 0, query).Matched);
    }

    [Theory]
    [InlineData(double.NaN)]
    [InlineData(double.PositiveInfinity)]
    public void UnavailableProbabilityHasAStableDisplay(double probability)
    {
        var status = new SearchStatus(SearchState.Running, 1, 10, 0, probability);
        Assert.True(status.ProbabilityUnavailable);
        Assert.Equal("unavailable", status.ProbabilityDescription);
    }
}
