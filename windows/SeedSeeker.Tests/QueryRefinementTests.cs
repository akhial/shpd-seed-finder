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
        Assert.True(QueryRefinement.CanRefine(Query(Pinned("ring_wealth")), Query(Pinned("ring_wealth"))));
        Assert.False(QueryRefinement.CanRefine(Query(Pinned("ring_force")), Query(Pinned("ring_wealth"))));
    }

    private static ItemRequirement Pinned(string id) => new()
    {
        Kind = ItemKind.Ring, Item = new CatalogItem(id, id, ItemKind.Ring, 0, 3),
    };

    [Fact]
    public void SharingNeedsOnlyOneCommonKind()
    {
        Assert.True(QueryRefinement.SharesRequirement(Query(Wand()), Query(Wand(), Ring())));
        Assert.True(QueryRefinement.SharesRequirement(Query(Ring(), Wand()), Query(Wand())));
        Assert.False(QueryRefinement.SharesRequirement(Query(Wand()), Query(Ring())));
        Assert.False(QueryRefinement.SharesRequirement(Query(), Query(Wand())));
        Assert.False(QueryRefinement.SharesRequirement(Query(Wand()), Query()));
    }

    [Fact]
    public void AKindLevelRequirementSubsumesEveryItemOfItsKind()
    {
        Assert.True(QueryRefinement.SharesRequirement(Query(Pinned("ring_wealth")), Query(Ring())));
        Assert.True(QueryRefinement.SharesRequirement(Query(Ring()), Query(Pinned("ring_wealth"))));
        Assert.True(QueryRefinement.SharesRequirement(Query(Pinned("ring_wealth")), Query(Pinned("ring_wealth"))));
        Assert.False(QueryRefinement.SharesRequirement(Query(Pinned("ring_force")), Query(Pinned("ring_wealth"))));
    }

    [Fact]
    public void SharingIgnoresScopeChallengesAndOtherPredicates()
    {
        // A filter re-verifies seeds from scratch under the candidate query,
        // so only the kind/item overlap matters — never scope or predicates.
        var narrowed = Query(Ring(4)); narrowed.MaximumDepth = 5; narrowed.Challenges = 4; narrowed.FastMode = true;
        Assert.True(QueryRefinement.SharesRequirement(narrowed, Query(Ring())));
    }

    [Fact]
    public void WeaponClassesAreDistinctKindsForSharing()
    {
        // Mirrors the web rule: 'weapon' and 'melee_weapon' are different kinds.
        static ItemRequirement Of(ItemKind kind) => new() { Kind = kind };
        Assert.True(QueryRefinement.SharesRequirement(Query(Of(ItemKind.MeleeWeapon)), Query(Of(ItemKind.MeleeWeapon))));
        Assert.False(QueryRefinement.SharesRequirement(Query(Of(ItemKind.Weapon)), Query(Of(ItemKind.MeleeWeapon))));
    }
}
