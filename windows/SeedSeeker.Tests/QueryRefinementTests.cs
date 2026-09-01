using System.Collections.ObjectModel;
using Xunit;

namespace SeedSeeker.Tests;

public sealed class QueryRefinementTests
{
    private static QuerySettings Query(params ItemRequirement[] requirements) =>
        new() { Requirements = new ObservableCollection<ItemRequirement>(requirements) };

    private static ItemRequirement Ring(int upgrade = 0) =>
        new() { Kind = ItemKind.Ring, Upgrade = upgrade, UpgradeMatch = UpgradeMatch.AtLeast };

    private static ItemRequirement Wand() => new() { Kind = ItemKind.Wand };

    [Fact]
    public void AnUnchangedQueryContinuesTheRun()
    {
        var baseline = Query(Ring(), Wand());
        Assert.True(QueryRefinement.CanRefine(baseline.Clone(), baseline));
        // The same instance (no Clone) must qualify too: the comparison is by
        // value, on the encoded queries, never by reference.
        Assert.True(QueryRefinement.CanRefine(baseline, baseline));
    }

    [Fact]
    public void AnUnsearchableQueryContinuesNothing()
    {
        // A query with no requirements is not a query the engine will decode,
        // so it has no result set to inherit and rescanning is the only sound
        // answer — the same verdict the web frontend reaches. The UI never
        // asks: Start is disabled until a requirement exists, and importing a
        // results file whose query has none is refused outright.
        Assert.False(QueryRefinement.CanRefine(Query(), Query()));
    }

    [Fact]
    public void AddedRequirementsStillRefine()
    {
        var baseline = Query(Ring());
        Assert.True(QueryRefinement.CanRefine(Query(Ring(), Wand()), baseline));
        // Order is irrelevant; the multiset is what matters.
        Assert.True(QueryRefinement.CanRefine(Query(Wand(), Ring()), baseline));
    }

    [Fact]
    public void DuplicatesAreCountedNotDeduplicated()
    {
        var twoRings = Query(Ring(), Ring());
        Assert.True(QueryRefinement.CanRefine(Query(Ring(), Ring()), twoRings));
        Assert.True(QueryRefinement.CanRefine(Query(Ring(), Ring(), Wand()), twoRings));
        // One ring dropped for a wand is not a superset.
        Assert.False(QueryRefinement.CanRefine(Query(Ring(), Wand()), twoRings));
    }

    [Fact]
    public void ADroppedOrChangedRequirementRescans()
    {
        var baseline = Query(Ring(), Wand());
        Assert.False(QueryRefinement.CanRefine(Query(Ring()), baseline));
        Assert.False(QueryRefinement.CanRefine(Query(), baseline));
        // A loosened requirement is a different requirement, not the same one.
        Assert.False(QueryRefinement.CanRefine(Query(Ring(3), Wand()), Query(Ring(4), Wand())));
    }

    [Fact]
    public void EveryRequirementFieldParticipatesInTheComparison()
    {
        // Each variant strengthens the plain ring, so it refines the baseline;
        // the reverse direction must rescan — and if the field were dropped on
        // the wire both directions would pass, so the False half is what
        // proves the field reached the engine.
        var baseline = Query(Ring());
        var variants = new[]
        {
            new ItemRequirement
            {
                Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.AtLeast, RequireUncursed = true,
            },
            new ItemRequirement
            {
                Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.AtLeast, MaximumDepth = 5,
            },
            new ItemRequirement
            {
                Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.AtLeast, Source = ScoutItemSource.Shop,
            },
            new ItemRequirement
            {
                Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.AtLeast, IdentityGroup = 1,
            },
        };
        foreach (var variant in variants)
        {
            Assert.True(QueryRefinement.CanRefine(Query(variant), baseline));
            Assert.False(QueryRefinement.CanRefine(baseline, Query(variant)));
        }
        // Key is random per requirement and must not affect eligibility.
        var keyed = Ring(); keyed.Key = 12345;
        Assert.True(QueryRefinement.CanRefine(Query(keyed), baseline));
    }

    [Fact]
    public void AWidenedScopeRescans()
    {
        var baseline = Query(Ring());
        QuerySettings WithScope(Action<QuerySettings> mutate)
        {
            var query = Query(Ring(), Wand()); mutate(query); return query;
        }
        // The floor limit and fast mode change which world is generated, or how
        // it is searched, and so does the challenge set: the base run's coverage
        // says nothing about them, so they have to match exactly.
        Assert.False(QueryRefinement.CanRefine(WithScope(q => q.MaximumDepth = 9), baseline));
        Assert.False(QueryRefinement.CanRefine(WithScope(q => q.FastMode = true), baseline));
        Assert.False(QueryRefinement.CanRefine(WithScope(q => q.Challenges = 4), baseline));
        // Scope equal on both sides, including a non-default one.
        var challenged = Query(Ring()); challenged.Challenges = 4;
        Assert.True(QueryRefinement.CanRefine(challenged.Clone(), challenged));
    }

    [Fact]
    public void ANarrowedWorldConditionContinuesAndARelaxedOneRescans()
    {
        // The blacksmith flags and the Wandmaker filter are conditions on an
        // unchanged world: switching one on can only drop seeds the base run
        // already matched, so the run continues. Switching it back off asks for
        // seeds the base never delivered, and rescans.
        var baseline = Query(Ring());
        QuerySettings Narrowed(Action<QuerySettings> mutate)
        {
            var query = Query(Ring(), Wand()); mutate(query); return query;
        }
        QuerySettings BaselineWith(Action<QuerySettings> mutate)
        {
            var query = Query(Ring()); mutate(query); return query;
        }
        var conditions = new (string Name, Action<QuerySettings> Apply)[]
        {
            ("require_blacksmith", q => q.RequireBlacksmith = true),
            ("exclude_blacksmith_rewards", q => q.ExcludeBlacksmithRewards = true),
            ("wandmaker_quest", q => q.WandmakerQuest = WandmakerQuest.Rotberry),
        };
        foreach (var (name, apply) in conditions)
        {
            Assert.True(QueryRefinement.CanRefine(Narrowed(apply), baseline), name);
            Assert.True(QueryRefinement.CanRefine(Narrowed(apply), BaselineWith(apply)), name);
            Assert.False(QueryRefinement.CanRefine(Query(Ring(), Wand()), BaselineWith(apply)), name);
        }
        // A different quest is not a narrowing of the one the base ran.
        Assert.False(QueryRefinement.CanRefine(
            Narrowed(q => q.WandmakerQuest = WandmakerQuest.CorpseDust),
            BaselineWith(q => q.WandmakerQuest = WandmakerQuest.Rotberry)));
    }

    [Fact]
    public void PinnedItemsAreComparedByIdentity()
    {
        Assert.True(QueryRefinement.CanRefine(Query(Pinned("ring_wealth")), Query(Pinned("ring_wealth"))));
        Assert.False(QueryRefinement.CanRefine(Query(Pinned("ring_force")), Query(Pinned("ring_wealth"))));
    }

    private static ItemRequirement Pinned(string id) => new()
    {
        Kind = ItemKind.Ring, Item = new CatalogItem(id, id, ItemKind.Ring, 0, 3),
    };

    [Fact]
    public void AlternativesSumsAndEffectSetsReachTheEngine()
    {
        // An "any of these" slot continues itself, and a narrower set (one
        // member dropped) continues the wider one; not the reverse.
        static ItemRequirement Alternative(ItemKind kind, int group) => new() { Kind = kind, AlternativeGroup = group };
        var either = Query(Alternative(ItemKind.Ring, 1), Alternative(ItemKind.Wand, 1));
        Assert.True(QueryRefinement.CanRefine(either.Clone(), either));
        Assert.True(QueryRefinement.CanRefine(Query(Ring()), either));
        Assert.False(QueryRefinement.CanRefine(either, Query(Ring())));

        // A higher combined total strengthens the group; a lower one loosens it.
        static QuerySettings Sum(int atLeast) => Query(
            new ItemRequirement { Kind = ItemKind.Ring, LevelSum = new(1, atLeast) },
            new ItemRequirement { Kind = ItemKind.Ring, LevelSum = new(1, atLeast) });
        Assert.True(QueryRefinement.CanRefine(Sum(4), Sum(3)));
        Assert.False(QueryRefinement.CanRefine(Sum(3), Sum(4)));

        // A subset of effects is stricter than the set; "any enchantment" is
        // wider than one enchantment.
        static QuerySettings Effect(EffectFilter filter) => Query(new ItemRequirement { Kind = ItemKind.Weapon, Effect = filter });
        Assert.True(QueryRefinement.CanRefine(Effect(EffectFilter.OneOf(["Blazing"])), Effect(EffectFilter.OneOf(["Blazing", "Chilling"]))));
        Assert.False(QueryRefinement.CanRefine(Effect(EffectFilter.OneOf(["Blazing", "Chilling"])), Effect(EffectFilter.OneOf(["Blazing"]))));
        Assert.True(QueryRefinement.CanRefine(Effect(EffectFilter.OneOf(["Blazing"])), Effect(EffectFilter.Enchantment())));
        Assert.False(QueryRefinement.CanRefine(Effect(EffectFilter.Enchantment()), Effect(EffectFilter.OneOf(["Blazing"]))));
    }
}
