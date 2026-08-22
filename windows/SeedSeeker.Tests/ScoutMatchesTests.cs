using System.Collections.ObjectModel;
using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The scout pane's match marks, which the engine now computes
/// (<c>seedfinder_scout_matches</c>) over the same SSQ2 request the manifest
/// came from. Pinned on seed AAA-AAA-BUH without challenges, whose floor-9
/// Wandmaker reward is a +3 Wand of Corrosion.
/// </summary>
public sealed class ScoutMatchesTests
{
    private const string Seed = "AAA-AAA-BUH";

    private static QuerySettings Query(params ItemRequirement[] requirements) =>
        new() { Requirements = new ObservableCollection<ItemRequirement>(requirements) };

    private static ItemRequirement Wand(int upgrade) =>
        new() { Kind = ItemKind.Wand, UpgradeMatch = UpgradeMatch.Exactly, Upgrade = upgrade };

    private static ScoutMatches Matches(QuerySettings query) => NativeEngine.ScoutMatches(Seed, 0, query);

    /// <summary>The manifest the marks index into, fetched over the same request.</summary>
    private static IReadOnlyList<ScoutItem> Manifest() => new NativeEngine().Scout(Seed, 0).Items;

    [Fact]
    public void TheMarksIndexTheManifestOfTheSameWorld()
    {
        var marks = Matches(Query(Wand(3)));
        Assert.Equal(1, marks.TotalRequirements);
        Assert.Equal(1, marks.MatchedRequirements);
        var items = Manifest();
        var marked = marks.Matched.Select(index => items[index]).ToList();
        var only = Assert.Single(marked);
        Assert.Equal("wand_corrosion", only.Item.Id);
        Assert.Equal(9, only.Depth);
        Assert.Equal(3, only.Upgrade);
        Assert.Equal(ScoutItemSource.WandmakerReward, only.Source);
    }

    [Fact]
    public void EachRequirementClaimsADistinctItem()
    {
        var shopRing = new ItemRequirement { Kind = ItemKind.Ring, Source = ScoutItemSource.Shop };
        var marks = Matches(Query(Wand(3), shopRing));
        Assert.Equal(2, marks.TotalRequirements);
        Assert.Equal(2, marks.MatchedRequirements);
        Assert.Equal(2, marks.Matched.Count);
        var items = Manifest();
        Assert.Contains(marks.Matched, index => items[index].Item.Id == "wand_corrosion");
        Assert.Contains(marks.Matched, index => items[index].Source == ScoutItemSource.Shop
            && items[index].Item.Kind == ItemKind.Ring);
    }

    [Fact]
    public void APartialMatchMarksOnlyTheItemsItCanExplain()
    {
        // Three copies of a requirement only one item in the world satisfies.
        var marks = Matches(Query(Wand(3), Wand(3), Wand(3)));
        Assert.Equal(3, marks.TotalRequirements);
        Assert.Equal(1, marks.MatchedRequirements);
        Assert.Single(marks.Matched);
    }

    [Fact]
    public void TheQueryScopeNarrowsTheMarks()
    {
        // The one matching wand sits on floor 9, outside both limits.
        var byFloorLimit = Query(Wand(3));
        byFloorLimit.MaximumDepth = 8;
        Assert.Empty(Matches(byFloorLimit).Matched);

        var perItem = Wand(3); perItem.MaximumDepth = 8;
        Assert.Empty(Matches(Query(perItem)).Matched);

        // A different world: seed and challenge set together name the world,
        // so the marks index the manifest scouted with the same challenges.
        var challenged = Query(Wand(3)); challenged.Challenges = 8; // barren_land
        var challengedItems = new NativeEngine().Scout(Seed, 8).Items;
        foreach (var index in NativeEngine.ScoutMatches(Seed, 8, challenged).Matched)
        {
            Assert.InRange(index, 0, challengedItems.Count - 1);
            Assert.Equal(ItemKind.Wand, challengedItems[index].Item.Kind);
            Assert.Equal(3, challengedItems[index].Upgrade);
        }
    }

    [Fact]
    public void AQueryWithNoRequirementsMarksNothing()
    {
        // The scout pane shows a manifest before any requirement exists; the
        // engine cannot decode such a query, and nothing is marked.
        var marks = Matches(Query());
        Assert.Empty(marks.Matched);
        Assert.Equal(0, marks.MatchedRequirements);
        Assert.Equal(0, marks.TotalRequirements);
    }
}
