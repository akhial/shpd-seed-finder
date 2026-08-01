using System.Collections.ObjectModel;
using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The Start Search dispatch of docs/search-semantics.md: how a query's
/// relationship to the session's Target (and, failing that, to the previous
/// detached scan) picks between refining, filtering, continuing, and scanning.
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
        Assert.Equal(StartMode.Anchor, SearchPlan.DecideStart(Query(Ring()), null));
        Assert.Equal(StartMode.Anchor, SearchPlan.DecideStart(Query(Ring()), null, Query(Ring())));
    }

    [Fact]
    public void AContinuationRefinesTheTarget()
    {
        var target = Target(Query(Ring()));
        Assert.Equal(StartMode.TargetRefine, SearchPlan.DecideStart(Query(Ring()), target));
        // A superset continues too — even though it also shares an item, the
        // continuation test wins so the target's coverage keeps advancing.
        Assert.Equal(StartMode.TargetRefine, SearchPlan.DecideStart(Query(Ring(), Wand()), target));
    }

    [Fact]
    public void ASharedItemFiltersTheFullTargetSet()
    {
        var target = Target(Query(Ring(), Wand()));
        // A dropped requirement is no continuation, but it still shares an
        // item: the full Target Set is filtered, bringing seeds back.
        Assert.Equal(StartMode.TargetFilter, SearchPlan.DecideStart(Query(Ring()), target));
        // Scope differences break continuation but never sharing.
        var narrowed = Query(Ring()); narrowed.MaximumDepth = 5;
        Assert.Equal(StartMode.TargetFilter, SearchPlan.DecideStart(narrowed, target));
    }

    [Fact]
    public void AnUnrelatedQueryDetachesWithoutTouchingTheTarget()
    {
        var target = Target(Query(Ring()));
        Assert.Equal(StartMode.Detached, SearchPlan.DecideStart(Query(Wand()), target));
        // An unrelated query only continues the previous run when that run was
        // itself a concluded detached scan the query continues.
        Assert.Equal(StartMode.ContinueDetached, SearchPlan.DecideStart(Query(Wand()), target, Query(Wand())));
        Assert.Equal(StartMode.ContinueDetached, SearchPlan.DecideStart(Query(Wand(), Armor()), target, Query(Wand())));
        Assert.Equal(StartMode.Detached, SearchPlan.DecideStart(Query(Wand()), target, Query(Armor())));
    }

    [Fact]
    public void ATargetRelatedQueryNeverContinuesADetachedScan()
    {
        // Even when the query would continue the last (detached) run, a
        // relationship to the Target wins: the Target Set is the base.
        var target = Target(Query(Ring()));
        Assert.Equal(StartMode.TargetRefine, SearchPlan.DecideStart(Query(Ring()), target, Query(Ring())));
        var narrowed = Query(Ring()); narrowed.MaximumDepth = 5;
        Assert.Equal(StartMode.TargetFilter, SearchPlan.DecideStart(narrowed, target, narrowed));
    }

    [Fact]
    public void AnEmptyTargetSetStillResumesAContinuingQuery()
    {
        var target = Target(Query(Ring()), seeds: 0, remaining: 100);
        Assert.Equal(StartMode.TargetRefine, SearchPlan.DecideStart(Query(Ring()), target));
        Assert.Equal(StartMode.TargetRefine, SearchPlan.DecideStart(Query(Ring(), Wand()), target));
    }

    [Fact]
    public void AnEmptyTargetSetReAnchorsOnAnyOtherQuery()
    {
        var empty = Target(Query(Ring()), seeds: 0, remaining: 100);
        // Sharing a kind is not enough: an empty set holds nothing to filter.
        var narrowed = Query(Ring()); narrowed.MaximumDepth = 5;
        Assert.Equal(StartMode.Anchor, SearchPlan.DecideStart(narrowed, empty));
        Assert.Equal(StartMode.Anchor, SearchPlan.DecideStart(Query(Wand()), empty));
        // The empty target is replaced, never continued around via the
        // detached thread.
        Assert.Equal(StartMode.Anchor, SearchPlan.DecideStart(Query(Wand()), empty, Query(Wand())));
        // With no coverage left either (an import that found nothing, or an
        // exhausted scan), even a continuation re-anchors.
        var exhausted = Target(Query(Ring()), seeds: 0, remaining: 0);
        Assert.Equal(StartMode.Anchor, SearchPlan.DecideStart(Query(Ring()), exhausted));
    }

    [Fact]
    public void AFullDisplayNeverDowngradesARefine()
    {
        // The 1,024-row display cap truncates the listing only. A Target Set
        // at or beyond the cap still refines-and-resumes on a continuation —
        // the resumed session accepts up to another cap's worth of new finds,
        // which is what lets an identical query grow the Target Set by
        // roughly a cap per run (docs/search-semantics.md, Start decision 1).
        var grown = Target(Query(Ring()), seeds: 1500, remaining: 100);
        Assert.Equal(StartMode.TargetRefine, SearchPlan.DecideStart(Query(Ring()), grown));
        Assert.Equal(StartMode.TargetRefine, SearchPlan.DecideStart(Query(Ring(), Wand()), grown));
        // A shared-item query still filters the full, uncapped set.
        var narrowed = Query(Ring()); narrowed.MaximumDepth = 5;
        Assert.Equal(StartMode.TargetFilter, SearchPlan.DecideStart(narrowed, grown));
        // Only exhausted coverage makes a continuation filter-only in
        // practice; the mode stays TargetRefine and its scan phase is empty.
        var exhausted = Target(Query(Ring()), seeds: 1500, remaining: 0);
        Assert.Equal(StartMode.TargetRefine, SearchPlan.DecideStart(Query(Ring()), exhausted));
    }

    [Fact]
    public void AnImportedTargetIsFilterOnly()
    {
        // Imports carry no coverage (Remaining = 0): a continuation still
        // refines — its scan phase is simply empty — and sharing filters.
        var imported = new TargetRun(Query(Ring()), ["AAA-AAA-AAA"], 0, 0);
        Assert.Equal(StartMode.TargetRefine, SearchPlan.DecideStart(Query(Ring(), Wand()), imported));
        var narrowed = Query(Ring()); narrowed.MaximumDepth = 5;
        Assert.Equal(StartMode.TargetFilter, SearchPlan.DecideStart(narrowed, imported));
    }
}
