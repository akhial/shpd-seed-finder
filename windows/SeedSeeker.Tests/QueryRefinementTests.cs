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
        // The same instance (no Clone) must qualify too: Signature is by value.
        Assert.True(QueryRefinement.CanRefine(baseline, baseline));
    }

    [Fact]
    public void AnEmptyQueryContinuesAnEmptyBaseline()
    {
        Assert.True(QueryRefinement.CanRefine(Query(), Query()));
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
        var baseline = Query(Ring());
        Assert.False(QueryRefinement.CanRefine(Query(new ItemRequirement
        {
            Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.AtLeast, RequireUncursed = true,
        }), baseline));
        Assert.False(QueryRefinement.CanRefine(Query(new ItemRequirement
        {
            Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.AtLeast, MaximumDepth = 5,
        }), baseline));
        Assert.False(QueryRefinement.CanRefine(Query(new ItemRequirement
        {
            Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.AtLeast, Source = ScoutItemSource.Shop,
        }), baseline));
        Assert.False(QueryRefinement.CanRefine(Query(new ItemRequirement
        {
            Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.AtLeast, IdentityGroup = 1,
        }), baseline));
        // Key is random per requirement and must not affect eligibility.
        var keyed = Ring(); keyed.Key = 12345;
        Assert.True(QueryRefinement.CanRefine(Query(keyed), baseline));
    }

    [Fact]
    public void ADifferentScopeAlwaysRescans()
    {
        var baseline = Query(Ring());
        QuerySettings WithScope(Action<QuerySettings> mutate)
        {
            var query = Query(Ring(), Wand()); mutate(query); return query;
        }
        Assert.False(QueryRefinement.CanRefine(WithScope(q => q.MaximumDepth = 9), baseline));
        Assert.False(QueryRefinement.CanRefine(WithScope(q => q.FastMode = true), baseline));
        Assert.False(QueryRefinement.CanRefine(WithScope(q => q.RequireBlacksmith = true), baseline));
        Assert.False(QueryRefinement.CanRefine(WithScope(q => q.ExcludeBlacksmithRewards = true), baseline));
        Assert.False(QueryRefinement.CanRefine(WithScope(q => q.Challenges = 4), baseline));
        // Scope equal on both sides, including a non-default one.
        var challenged = Query(Ring()); challenged.Challenges = 4;
        Assert.True(QueryRefinement.CanRefine(challenged.Clone(), challenged));
    }

    [Fact]
    public void PinnedItemsAreComparedByIdentity()
    {
        ItemRequirement Pinned(string id) => new()
        {
            Kind = ItemKind.Ring, Item = new CatalogItem(id, id, ItemKind.Ring, 0, 3),
        };
        Assert.True(QueryRefinement.CanRefine(Query(Pinned("ring_wealth")), Query(Pinned("ring_wealth"))));
        Assert.False(QueryRefinement.CanRefine(Query(Pinned("ring_force")), Query(Pinned("ring_wealth"))));
    }
}
