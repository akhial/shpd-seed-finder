using System.Collections.ObjectModel;
using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The Start Search dispatch of docs/search-semantics.md: how a query's
/// relationship to the session's Target (and, failing that, to the previous
/// detached scan) picks between refining, filtering, continuing, and scanning.
/// The engine decides it (<c>seedfinder_decide_start</c>), so these are the
/// behaviour this app gets rather than a second implementation of the rules.
/// </summary>
public sealed class SearchPlanTests
{
    private static QuerySettings Query(params ItemRequirement[] requirements) =>
        new() { Requirements = new ObservableCollection<ItemRequirement>(requirements) };

    private static ItemRequirement Ring() => new() { Kind = ItemKind.Ring };
    private static ItemRequirement Wand() => new() { Kind = ItemKind.Wand };
    private static ItemRequirement Armor() => new() { Kind = ItemKind.Armor };

    private static TargetRun Target(QuerySettings query, int seeds = 3, long remaining = 100) =>
        new(query, [.. Enumerable.Range(0, seeds).Select(i => $"AAA-AAA-AA{(char)('A' + i)}")], 50, remaining);

    [Fact]
    public void WithNoTargetEverySearchAnchors()
    {
        Assert.Equal(StartMode.Anchor, NativeEngine.DecideStart(Query(Ring()), null));
        Assert.Equal(StartMode.Anchor, NativeEngine.DecideStart(Query(Ring()), null, Query(Ring())));
    }

    [Fact]
    public void AContinuationRefinesTheTarget()
    {
        var target = Target(Query(Ring()));
        Assert.Equal(StartMode.TargetRefine, NativeEngine.DecideStart(Query(Ring()), target));
        // A superset continues too — even though it also shares an item, the
        // continuation test wins so the target's coverage keeps advancing.
        Assert.Equal(StartMode.TargetRefine, NativeEngine.DecideStart(Query(Ring(), Wand()), target));
    }

    [Fact]
    public void ASharedItemFiltersTheFullTargetSet()
    {
        var target = Target(Query(Ring(), Wand()));
        // A dropped requirement is no continuation, but it still shares an
        // item: the full Target Set is filtered, bringing seeds back.
        Assert.Equal(StartMode.TargetFilter, NativeEngine.DecideStart(Query(Ring()), target));
        // Scope differences break continuation but never sharing.
        var narrowed = Query(Ring()); narrowed.MaximumDepth = 5;
        Assert.Equal(StartMode.TargetFilter, NativeEngine.DecideStart(narrowed, target));
    }

    [Fact]
    public void AnUnrelatedQueryDetachesWithoutTouchingTheTarget()
    {
        var target = Target(Query(Ring()));
        Assert.Equal(StartMode.Detached, NativeEngine.DecideStart(Query(Wand()), target));
        // An unrelated query only continues the previous run when that run was
        // itself a concluded detached scan the query continues.
        Assert.Equal(StartMode.ContinueDetached, NativeEngine.DecideStart(Query(Wand()), target, Query(Wand())));
        Assert.Equal(StartMode.ContinueDetached, NativeEngine.DecideStart(Query(Wand(), Armor()), target, Query(Wand())));
        Assert.Equal(StartMode.Detached, NativeEngine.DecideStart(Query(Wand()), target, Query(Armor())));
    }

    [Fact]
    public void ATargetRelatedQueryNeverContinuesADetachedScan()
    {
        // Even when the query would continue the last (detached) run, a
        // relationship to the Target wins: the Target Set is the base.
        var target = Target(Query(Ring()));
        Assert.Equal(StartMode.TargetRefine, NativeEngine.DecideStart(Query(Ring()), target, Query(Ring())));
        var narrowed = Query(Ring()); narrowed.MaximumDepth = 5;
        Assert.Equal(StartMode.TargetFilter, NativeEngine.DecideStart(narrowed, target, narrowed));
    }

    [Fact]
    public void AnEmptyTargetSetStillResumesAContinuingQuery()
    {
        var target = Target(Query(Ring()), seeds: 0, remaining: 100);
        Assert.Equal(StartMode.TargetRefine, NativeEngine.DecideStart(Query(Ring()), target));
        Assert.Equal(StartMode.TargetRefine, NativeEngine.DecideStart(Query(Ring(), Wand()), target));
    }

    [Fact]
    public void AnEmptyTargetSetReAnchorsOnAnyOtherQuery()
    {
        var empty = Target(Query(Ring()), seeds: 0, remaining: 100);
        // Sharing a kind is not enough: an empty set holds nothing to filter.
        var narrowed = Query(Ring()); narrowed.MaximumDepth = 5;
        Assert.Equal(StartMode.Anchor, NativeEngine.DecideStart(narrowed, empty));
        Assert.Equal(StartMode.Anchor, NativeEngine.DecideStart(Query(Wand()), empty));
        // The empty target is replaced, never continued around via the
        // detached thread.
        Assert.Equal(StartMode.Anchor, NativeEngine.DecideStart(Query(Wand()), empty, Query(Wand())));
        // With no coverage left either (an import that found nothing, or an
        // exhausted scan), even a continuation re-anchors.
        var exhausted = Target(Query(Ring()), seeds: 0, remaining: 0);
        Assert.Equal(StartMode.Anchor, NativeEngine.DecideStart(Query(Ring()), exhausted));
    }

    [Fact]
    public void AnImportedTargetIsFilterOnly()
    {
        // Imports carry no coverage (Remaining = 0): a continuation still
        // refines — its scan phase is simply empty — and sharing filters.
        var imported = new TargetRun(Query(Ring()), ["AAA-AAA-AAA"], 0, 0);
        Assert.Equal(StartMode.TargetRefine, NativeEngine.DecideStart(Query(Ring(), Wand()), imported));
        var narrowed = Query(Ring()); narrowed.MaximumDepth = 5;
        Assert.Equal(StartMode.TargetFilter, NativeEngine.DecideStart(narrowed, imported));
    }

    // Sharing an item is what separates a filter from a detached scan; it is
    // only observable through the decision now, so it is spelled out here.

    [Fact]
    public void SharingNeedsOnlyOneCommonKind()
    {
        // Dropping a requirement breaks continuation but not sharing.
        Assert.Equal(StartMode.TargetFilter, Decide(Query(Wand()), Query(Wand(), Ring())));
        Assert.Equal(StartMode.TargetFilter, Decide(Query(Ring()), Query(Wand(), Ring())));
        Assert.Equal(StartMode.Detached, Decide(Query(Wand()), Query(Ring())));
    }

    [Fact]
    public void AKindLevelRequirementSubsumesEveryItemOfItsKind()
    {
        Assert.Equal(StartMode.TargetFilter, Decide(Query(Pinned("ring_wealth")), Query(Ring(), Wand())));
        Assert.Equal(StartMode.TargetFilter, Decide(Query(Ring()), Query(Pinned("ring_wealth"), Wand())));
        // Two different pinned items name nothing in common.
        Assert.Equal(StartMode.Detached, Decide(Query(Pinned("ring_force")), Query(Pinned("ring_wealth"), Wand())));
    }

    [Fact]
    public void SharingIgnoresScopeChallengesAndOtherPredicates()
    {
        // A filter re-verifies seeds from scratch under the candidate query,
        // so only the kind/item overlap matters — never scope or predicates.
        var narrowed = Query(new ItemRequirement
        {
            Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.AtLeast, Upgrade = 4,
        });
        narrowed.MaximumDepth = 5; narrowed.Challenges = 4; narrowed.FastMode = true;
        Assert.Equal(StartMode.TargetFilter, Decide(narrowed, Query(Ring())));
    }

    [Fact]
    public void AWeaponCategoryNarrowingStillSharesTheWeaponFamily()
    {
        // The engine's sharing test is on the item family; the melee/thrown
        // narrowing is a separate predicate, so a plain weapon requirement and
        // a melee one can name the same item and the Target Set is worth
        // filtering. The hand-written C# rule folded the narrowing into the
        // kind and detached instead, discarding a set it could have filtered.
        static ItemRequirement Of(ItemKind kind) => new() { Kind = kind };
        Assert.Equal(StartMode.TargetFilter,
            Decide(Query(Of(ItemKind.MeleeWeapon)), Query(Of(ItemKind.MeleeWeapon), Of(ItemKind.Wand))));
        Assert.Equal(StartMode.TargetFilter,
            Decide(Query(Of(ItemKind.Weapon)), Query(Of(ItemKind.MeleeWeapon))));
        // Different families still detach.
        Assert.Equal(StartMode.Detached,
            Decide(Query(Of(ItemKind.ThrownWeapon)), Query(Of(ItemKind.Ring))));
    }

    /// <summary>The decision for a candidate against a populated, uncovered Target.</summary>
    private static StartMode Decide(QuerySettings candidate, QuerySettings targetQuery) =>
        NativeEngine.DecideStart(candidate, Target(targetQuery));

    private static ItemRequirement Pinned(string id) => new()
    {
        Kind = ItemKind.Ring, Item = new CatalogItem(id, id, ItemKind.Ring, 0, 3),
    };
}
