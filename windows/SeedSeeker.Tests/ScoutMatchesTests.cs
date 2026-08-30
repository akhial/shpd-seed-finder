using System.Collections.ObjectModel;
using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The scout pane's match marks, which the engine now computes
/// (<c>seedfinder_scout_matches</c>) over the same SSQ2 request the manifest
/// came from. Pinned on seed AAA-AAA-BUH without challenges, whose only +3 wand
/// is the Wand of Frost in the floor-17 Imp vault.
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
        Assert.Equal("wand_frost", only.Item.Id);
        Assert.Equal(17, only.Depth);
        Assert.Equal(3, only.Upgrade);
        // v4.0.0's vault treasure, which the scout decodes as its own source.
        Assert.Equal(ScoutItemSource.VaultTreasure, only.Source);
    }

    [Fact]
    public void EachRequirementClaimsADistinctItem()
    {
        // Not a second vault item: the Imp lets exactly one item leave, so the
        // +3 vault wand and a vault ring would be one mutually exclusive pick.
        var crystalRing = new ItemRequirement { Kind = ItemKind.Ring, Source = ScoutItemSource.CrystalChest };
        var marks = Matches(Query(Wand(3), crystalRing));
        Assert.Equal(2, marks.TotalRequirements);
        Assert.Equal(2, marks.MatchedRequirements);
        Assert.Equal(2, marks.Matched.Count);
        var items = Manifest();
        Assert.Contains(marks.Matched, index => items[index].Item.Id == "wand_frost");
        Assert.Contains(marks.Matched, index => items[index].Source == ScoutItemSource.CrystalChest
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
        // The one matching wand sits on floor 17, outside both limits.
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

    [Fact]
    public void AnAlternativeGroupIsOneSlot()
    {
        // Any member satisfies the slot: the +3 wand does, a vault ring does too.
        var wand = Wand(3); wand.AlternativeGroup = 1;
        var vaultRing = new ItemRequirement { Kind = ItemKind.Ring, Source = ScoutItemSource.VaultTreasure, AlternativeGroup = 1 };
        var marks = Matches(Query(wand, vaultRing));
        Assert.Equal(1, marks.TotalRequirements);
        Assert.Equal(1, marks.MatchedRequirements);
        Assert.Single(marks.Matched);
        // A slot no member satisfies counts once too: this world holds no +4
        // wand at all, and no +1 wand within the first eight floors.
        var missing = Wand(4); missing.AlternativeGroup = 2; var missingToo = Wand(1); missingToo.AlternativeGroup = 2; missingToo.MaximumDepth = 8;
        marks = Matches(Query(wand, vaultRing, missing, missingToo));
        Assert.Equal(2, marks.TotalRequirements);
        Assert.Equal(1, marks.MatchedRequirements);
    }

    [Fact]
    public void ACombinedLevelGroupIsOneConditionAnySubsetMayMeet()
    {
        static ItemRequirement AnyWand(int atLeast) => new() { Kind = ItemKind.Wand, LevelSum = new(1, atLeast) };
        // The group is one condition however many members it has. The +3 vault
        // wand alone carries four levels (its upgrade plus one), so it meets a
        // total of 5 with any other wand, and every contributing item is marked.
        var marks = Matches(Query(AnyWand(5), AnyWand(5)));
        Assert.Equal(1, marks.TotalRequirements);
        Assert.Equal(1, marks.MatchedRequirements);
        var items = Manifest();
        Assert.Contains(marks.Matched, index => items[index].Item.Id == "wand_frost");
        Assert.True(marks.Matched.Sum(index => items[index].Upgrade + 1) >= 5);
        // Members are optional: the +3 wand meets a total of 4 by itself.
        marks = Matches(Query(AnyWand(4), AnyWand(4)));
        Assert.Equal(1, marks.MatchedRequirements);
        Assert.NotEmpty(marks.Matched);
        // Eight levels is attainable (two +4 wands would carry ten) but this
        // world's best pair is the +3 and a +2, seven levels: nothing is
        // marked, not even the +3 that serves the short group.
        marks = Matches(Query(AnyWand(8), AnyWand(8)));
        Assert.Equal(1, marks.TotalRequirements);
        Assert.Equal(0, marks.MatchedRequirements);
        Assert.Empty(marks.Matched);
    }

    [Fact]
    public void EffectSetsAndAnyEnchantmentReachTheMatcher()
    {
        var items = Manifest();
        // Floor 3 has a Venomous dirk — one of v4.0.0's enchantments — and a
        // Lucky katana appears later. Either satisfies the set.
        var set = new ItemRequirement { Kind = ItemKind.Weapon, Effect = EffectFilter.OneOf(["Lucky", "Venomous"]) };
        var marks = Matches(Query(set));
        Assert.Equal(1, marks.MatchedRequirements);
        var marked = items[Assert.Single(marks.Matched)];
        Assert.Contains(marked.Effect, new[] { "Lucky", "Venomous" });
        // Any enchantment on uncursed armor: an enchanted, uncursed armor is marked.
        var armor = new ItemRequirement { Kind = ItemKind.Armor, Effect = EffectFilter.Enchantment(), RequireUncursed = true };
        marks = Matches(Query(armor));
        Assert.Equal(1, marks.MatchedRequirements);
        marked = items[Assert.Single(marks.Matched)];
        Assert.Equal(ItemKind.Armor, marked.Item.Kind);
        Assert.NotNull(marked.Effect); Assert.False(marked.Cursed);
        // An effect the world lacks matches nothing.
        var grim = new ItemRequirement { Kind = ItemKind.Weapon, Effect = EffectFilter.OneOf(["Grim"]) };
        Assert.Equal(0, Matches(Query(grim)).MatchedRequirements);
        // v4.0.0's curses reach the matcher too: floor 17 drops a Wondrous javelin.
        var wondrous = new ItemRequirement { Kind = ItemKind.ThrownWeapon, Effect = EffectFilter.OneOf(["Wondrous"]) };
        marked = items[Assert.Single(Matches(Query(wondrous)).Matched)];
        Assert.Equal("Wondrous", marked.Effect); Assert.True(marked.Cursed);
    }
}
