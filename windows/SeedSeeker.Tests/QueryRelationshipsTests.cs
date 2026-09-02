using System.Collections.ObjectModel;
using System.Text.Json;
using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The structure between requirements — "any of these" slots, same-item
/// stacks, combined-level groups, effect sets — as the editor manipulates it, the summary text that
/// describes it, the local validation that runs before the engine is asked,
/// and the persisted schema's backward compatibility.
/// </summary>
public sealed class QueryRelationshipsTests
{
    private static ObservableCollection<ItemRequirement> List(params ItemRequirement[] requirements) => new(requirements);

    private static ItemRequirement Wand(int? group = null) => new() { Kind = ItemKind.Wand, AlternativeGroup = group };

    private static ItemRequirement Ring(int exact) => new() { Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.Exactly, Upgrade = exact };

    [Fact]
    public void SlotsCollapseAlternativesAtTheirFirstMembersPosition()
    {
        var a = Wand(1); var b = Wand(); var c = Wand(1); var d = Wand(2); var e = Wand(2);
        var slots = QueryRelationships.Slots([a, b, c, d, e]);
        Assert.Equal([[a, c], [b], [d, e]], slots.Select(slot => slot.ToArray()));
        Assert.Equal(3, QueryRelationships.SlotCount([a, b, c, d, e]));
        Assert.Equal(0, QueryRelationships.SlotCount([]));
    }

    [Fact]
    public void SumCapacityCountsLevelsByTheEnginesMaximumUpgradeRule()
    {
        var exact = Ring(1); exact.LevelSum = new(2, 1);
        var any = new ItemRequirement { Kind = ItemKind.Ring, LevelSum = new(2, 1) };
        var atLeast = new ItemRequirement { Kind = ItemKind.Wand, UpgradeMatch = UpgradeMatch.AtLeast, Upgrade = 1 };
        // A weapon reaches the vault's +5, one past every other family's ceiling.
        var weapon = new ItemRequirement { Kind = ItemKind.ThrownWeapon };
        var elsewhere = Ring(4); elsewhere.LevelSum = new(1, 4);
        Assert.Equal(1, exact.MaximumUpgrade); Assert.Equal(4, any.MaximumUpgrade); Assert.Equal(4, atLeast.MaximumUpgrade); Assert.Equal(5, weapon.MaximumUpgrade);
        // Every item counts its upgrade plus one.
        Assert.Equal(2, exact.MaximumLevel); Assert.Equal(5, any.MaximumLevel); Assert.Equal(5, atLeast.MaximumLevel); Assert.Equal(6, weapon.MaximumLevel);
        Assert.Equal(2 + 5, QueryRelationships.SumCapacity([exact, any, atLeast, weapon, elsewhere], 2));
        Assert.Equal(new LevelSum(1, 4), elsewhere.LevelSum);
        // The members' own ceilings are bounded by generation: a world levels
        // only one ring — the Imp vault's prize — past +2, so three
        // any-upgrade rings reach eleven levels together, not fifteen.
        var trio = Enumerable.Range(0, 3).Select(_ => new ItemRequirement { Kind = ItemKind.Ring, LevelSum = new(3, 1) }).ToList();
        Assert.Equal(11, QueryRelationships.SumCapacity(trio, 3));
    }

    [Fact]
    public void ValidationNamesTheGroupThatCannotReachItsTotal()
    {
        // Two rings of any upgrade reach eight levels together, not ten: the
        // vault's one +4 prize plus a standard +2 ring, upgrade plus one each.
        var a = new ItemRequirement { Kind = ItemKind.Ring, LevelSum = new(1, 9) };
        var b = new ItemRequirement { Kind = ItemKind.Ring, LevelSum = new(1, 9) };
        var query = new QuerySettings { Requirements = List(a, b) };
        Assert.Equal("Combined level group A needs 9 levels but its items can reach at most 8.", QueryRelationships.Validate(query));
        a.LevelSum = new(1, 8); b.LevelSum = new(1, 8);
        Assert.Null(QueryRelationships.Validate(query));
        b.LevelSum = new(1, 5);
        Assert.Equal("Combined level group A has members that disagree on the total.", QueryRelationships.Validate(query));
    }

    [Fact]
    public void ValidationTreatsSameItemGroupsAsStacksWithOneAnchor()
    {
        static ItemRequirement Named(int group = 1, int? alternative = null) =>
            new() { Kind = ItemKind.Ring, Item = ItemCatalog.Find("ring_might"), UpgradeMatch = UpgradeMatch.Exactly, Upgrade = 2, IdentityGroup = group, AlternativeGroup = alternative };
        static ItemRequirement Plain(ItemKind kind = ItemKind.Ring, int group = 1, int? maximumDepth = null) =>
            new() { Kind = kind, UpgradeMatch = UpgradeMatch.Any, IdentityGroup = group, MaximumDepth = maximumDepth };
        static string? Check(params ItemRequirement[] requirements) =>
            QueryRelationships.Validate(new QuerySettings { Requirements = List(requirements) });
        // One anchor with plain copies is the intended shape; a floor limit on a copy is fine.
        Assert.Null(Check(Named(), Plain(), Plain(maximumDepth: 6)));
        Assert.Null(Check(Plain(), Plain()));
        // Two constrained members would force two described items to be one.
        Assert.Equal("Same-item group A can describe one item (or one set of alternatives); its other members must be plain.", Check(Named(), Named()));
        Assert.Contains("must be plain", Check(Named(), new ItemRequirement { Kind = ItemKind.Ring, UpgradeMatch = UpgradeMatch.Any, IdentityGroup = 1, RequireUncursed = true }));
        // The members of one alternative group form a single anchor unit.
        Assert.Null(Check(Named(alternative: 1), Named(alternative: 1), Plain()));
        Assert.Contains("must be plain", Check(Named(alternative: 1), Named(alternative: 1), Named()));
        // Members of different categories never describe one item; a narrowed kind is a constraint, the broad one is plain.
        Assert.Equal("Same-item group A mixes different categories.", Check(Named(), Plain(ItemKind.Wand)));
        var thrown = Plain(ItemKind.ThrownWeapon, group: 2); var weapon = Plain(ItemKind.Weapon, group: 2);
        Assert.False(thrown.IsBare); Assert.True(weapon.IsBare);
        Assert.Null(Check(thrown, weapon));
        // Separate groups are separate stacks.
        Assert.Null(Check(Named(), Named(group: 2)));
    }

    [Fact]
    public void ValidationRejectsWhatTheEngineRejects()
    {
        static string? Check(ItemRequirement requirement) =>
            QueryRelationships.Validate(new QuerySettings { Requirements = List(requirement) });
        Assert.Contains("alternative", Check(new() { Kind = ItemKind.Ring, AlternativeGroup = 1, LevelSum = new(1, 1) }));
        // Levels only combine across rings; no other family adds up that way.
        Assert.Contains("only rings", Check(new() { Kind = ItemKind.Wand, LevelSum = new(1, 1) }));
        Assert.Contains("carry none", Check(new() { Kind = ItemKind.Ring, Effect = EffectFilter.Enchantment() }));
        Assert.Contains("only lists curses", Check(new() { Kind = ItemKind.Weapon, RequireUncursed = true, Effect = EffectFilter.OneOf(["Annoying", "Wayward"]) }));
        // A mixed set with uncursed is fine: only the good members can match.
        Assert.Null(Check(new() { Kind = ItemKind.Weapon, RequireUncursed = true, Effect = EffectFilter.OneOf(["Annoying", "Blazing"]) }));
        Assert.Null(Check(new() { Kind = ItemKind.Armor, RequireUncursed = true, Effect = EffectFilter.Enchantment() }));
        Assert.Null(Check(new() { Kind = ItemKind.Wand }));
    }

    [Fact]
    public void TheDescriptionCoversEffectSetsAndSumGroups()
    {
        var sword = new ItemRequirement { Kind = ItemKind.MeleeWeapon, Item = ItemCatalog.Find("sword"), UpgradeMatch = UpgradeMatch.Exactly, Upgrade = 2, Effect = EffectFilter.OneOf(["Blocking", "Projecting"]) };
        Assert.Equal("+2 exactly • effect: Blocking/Projecting", sword.Description);
        var armor = new ItemRequirement { Kind = ItemKind.Armor, Effect = EffectFilter.Enchantment(), RequireUncursed = true };
        Assert.Equal("Any upgrade • any enchantment • uncursed", armor.Description);
        var ring = new ItemRequirement { Kind = ItemKind.Ring, Item = ItemCatalog.Find("ring_might"), IdentityGroup = 1, LevelSum = new(1, 4), MaximumDepth = 4 };
        Assert.Equal("Any upgrade • same-kind stack • levels ≥ 4 together • by floor 4", ring.Description);
        // One effect reads exactly as it did before effect sets existed.
        var single = new ItemRequirement { Kind = ItemKind.Weapon, Modifier = "Blazing" };
        Assert.Equal("Any upgrade • Blazing", single.Description);
    }

    [Fact]
    public void TheSingleEffectViewStaysInStepWithTheFilter()
    {
        var requirement = new ItemRequirement { Kind = ItemKind.Weapon, Modifier = "Blazing" };
        Assert.Equal(["Blazing"], requirement.Effect.Effects);
        Assert.Equal("Blazing", requirement.Modifier);
        requirement.Effect = EffectFilter.OneOf(["Blazing", "Chilling"]);
        Assert.Null(requirement.Modifier);
        // Setting null never erases a wider filter.
        requirement.Modifier = null;
        Assert.Equal(2, requirement.Effect.Effects.Count);
        requirement.Effect = EffectFilter.Enchantment();
        Assert.Null(requirement.Modifier);
        Assert.True(EffectFilter.OneOf(ItemCatalog.Enchantments).IsEveryEnchantmentOf(ItemKind.ThrownWeapon));
        Assert.False(EffectFilter.OneOf(ItemCatalog.Enchantments).IsEveryEnchantmentOf(ItemKind.Armor));
        Assert.Equal(["Blazing"], EffectFilter.OneOf(["Blazing", "Annoying"]).WithoutCurses(ItemKind.Weapon).Effects);
    }

    [Fact]
    public void CloningCopiesTheEffectFilterRatherThanSharingIt()
    {
        var original = new ItemRequirement { Kind = ItemKind.Weapon, Effect = EffectFilter.OneOf(["Blazing"]), LevelSum = new(1, 2), AlternativeGroup = null };
        var copy = original.Clone();
        copy.Effect.Effects.Add("Chilling");
        Assert.Equal(["Blazing"], original.Effect.Effects);
        Assert.Equal(original.LevelSum, copy.LevelSum);
    }

    [Fact]
    public void SavedQueriesFromBeforeEffectSetsStillLoad()
    {
        // The shape MainWindow persisted before this change: a bare Modifier,
        // no Effect, AlternativeGroup or LevelSum. "FastMode" is deliberately
        // still here — settings and presets saved before that mode was retired
        // must keep loading, the flag ignored.
        const string legacy = """
            { "Requirements": [ { "Key": 7, "Item": null, "Upgrade": 2, "Modifier": "Blazing", "Kind": 0, "Tier": 0,
              "TierMatch": 0, "UpgradeMatch": 1, "Source": null, "IdentityGroup": 1, "MaximumDepth": 9, "RequireUncursed": true } ],
              "MaximumDepth": 12, "RequireBlacksmith": false, "ExcludeBlacksmithRewards": false, "WandmakerQuest": 0, "FastMode": true, "Challenges": 0 }
            """;
        var query = JsonSerializer.Deserialize<QuerySettings>(legacy)!;
        Assert.Equal(12, query.MaximumDepth);
        var requirement = Assert.Single(query.Requirements);
        Assert.Equal("Blazing", requirement.Modifier);
        Assert.Equal(["Blazing"], requirement.Effect.Effects);
        Assert.Null(requirement.AlternativeGroup); Assert.Null(requirement.LevelSum);
        Assert.Equal(1, requirement.IdentityGroup); Assert.True(requirement.RequireUncursed);
    }

    [Fact]
    public void TheNewFieldsPersistThroughTheSavedSchema()
    {
        var query = new QuerySettings
        {
            Requirements = List(
                new() { Kind = ItemKind.Weapon, Effect = EffectFilter.OneOf(["Blocking", "Projecting"]), AlternativeGroup = 1 },
                new() { Kind = ItemKind.Armor, Effect = EffectFilter.Enchantment(), AlternativeGroup = 1 },
                new() { Kind = ItemKind.Ring, LevelSum = new(2, 4) }),
        };
        var again = JsonSerializer.Deserialize<QuerySettings>(JsonSerializer.Serialize(query))!;
        Assert.Equal(["Blocking", "Projecting"], again.Requirements[0].Effect.Effects);
        Assert.Equal(1, again.Requirements[0].AlternativeGroup);
        Assert.True(again.Requirements[1].Effect.AnyEnchantment);
        Assert.Equal(new LevelSum(2, 4), again.Requirements[2].LevelSum);
        Assert.Equal(ResultsExport.EncodeQueryDocument(query), ResultsExport.EncodeQueryDocument(again));
    }

    // ---- the requirement board -------------------------------------------
    // Ported from the web design's relations.test.ts, case for case, so the
    // two implementations of the same model stay honest about each other.

    private static ItemRequirement WeaponItem(string id) => new() { Kind = ItemKind.Weapon, Item = ItemCatalog.Find(id) };

    private static ItemRequirement RingOf(string id, UpgradeMatch match = UpgradeMatch.Any, int upgrade = 0) =>
        new() { Kind = ItemKind.Ring, Item = ItemCatalog.Find(id), UpgradeMatch = match, Upgrade = upgrade };

    /// <summary>The board entry holding requirement <paramref name="index"/>.</summary>
    private static BoardItem Entry(IEnumerable<ItemRequirement> requirements, int index) =>
        QueryRelationships.ItemOf(requirements, index)!;

    private static QuerySettings Query(IEnumerable<ItemRequirement> requirements) => new() { Requirements = new(requirements) };

    private static string? Problem(IEnumerable<ItemRequirement> requirements) => QueryRelationships.Validate(Query(requirements));

    [Fact]
    public void DroppingAChipOnAnotherMakesOneSlotPlacedAfterTheTarget()
    {
        List<ItemRequirement> requirements = [WeaponItem("spear"), new() { Kind = ItemKind.Armor }, WeaponItem("shuriken")];
        var next = QueryRelationships.JoinAlternatives(requirements, 2, 0);
        Assert.Equal(["spear", "shuriken", null], next.Select(requirement => requirement.Item?.Id));
        Assert.NotNull(next[0].AlternativeGroup);
        Assert.Equal(next[0].AlternativeGroup, next[1].AlternativeGroup);
        Assert.Equal([[0, 1], [2]], QueryRelationships.BoardItems(next).Select(entry => entry.Members.ToArray()));
        Assert.Contains("any_of", ResultsExport.EncodeQueryDocument(Query(next)));
    }

    [Fact]
    public void JoiningAClusterDropsACombinedLevelAndLeavingAPairDissolvesIt()
    {
        var first = RingOf("ring_might"); first.LevelSum = new(1, 3);
        var second = RingOf("ring_might"); second.LevelSum = new(1, 3);
        List<ItemRequirement> requirements = [first, second, WeaponItem("shuriken")];
        var next = QueryRelationships.JoinAlternatives(requirements, 0, 2);
        Assert.All(next, requirement => Assert.Null(requirement.LevelSum));
        var out_ = QueryRelationships.Detach(next, next.FindIndex(requirement => requirement.Item?.Id == "shuriken"));
        Assert.All(out_, requirement => Assert.Null(requirement.AlternativeGroup));
    }

    [Fact]
    public void AConcreteStackEncodesAsPlainRepeatsWithNoIdentityGroup()
    {
        List<ItemRequirement> requirements = [RingOf("ring_might", UpgradeMatch.Exactly, 2), new() { Kind = ItemKind.Wand }];
        var next = QueryRelationships.SetStackCount(requirements, Entry(requirements, 0), 3);
        Assert.Equal(4, next.Count);
        Assert.Equal(3, next.Count(requirement => requirement.Item?.Id == "ring_might"));
        Assert.All(next, requirement => Assert.Null(requirement.IdentityGroup));
        // The board folds the repeats back into one ×3 chip.
        var board = QueryRelationships.BoardItems(next);
        Assert.Equal(2, board.Count);
        Assert.Equal(3, board[0].StackCount);
        Assert.Null(board[0].Total);
        Assert.Null(Problem(next));
        // The round trip through the document keeps the stack.
        var reloaded = ResultsExport.DecodeQueryDocument(ResultsExport.EncodeQueryDocument(Query(next)));
        Assert.Equal(3, QueryRelationships.BoardItems(reloaded.Requirements)[0].StackCount);
    }

    [Fact]
    public void AWildcardStackEncodesAsBareCopiesSharingAnIdentityGroup()
    {
        List<ItemRequirement> requirements = [new() { Kind = ItemKind.Wand, UpgradeMatch = UpgradeMatch.AtLeast, Upgrade = 1 }];
        var next = QueryRelationships.SetStackCount(requirements, Entry(requirements, 0), 3);
        Assert.Equal(3, next.Count);
        Assert.Single(next.Select(requirement => requirement.IdentityGroup).Distinct());
        Assert.Equal(1, next[0].IdentityGroup);
        Assert.All(next.Skip(1), copy =>
        {
            Assert.Equal(ItemKind.Wand, copy.Kind); Assert.Null(copy.Item); Assert.Equal(UpgradeMatch.Any, copy.UpgradeMatch);
        });
        Assert.Null(Problem(next));
        Assert.Equal(3, QueryRelationships.BoardItems(next)[0].StackCount);
        // Shrinking to one dissolves the group entirely.
        var shrunk = QueryRelationships.SetStackCount(next, Entry(next, 0), 1);
        Assert.Single(shrunk);
        Assert.Null(shrunk[0].IdentityGroup);
    }

    [Fact]
    public void AnEitherOrClusterAnchorsAStackAndEveryMemberCarriesTheLabel()
    {
        var basis = QueryRelationships.JoinAlternatives([WeaponItem("runic_blade"), WeaponItem("war_hammer")], 1, 0);
        var next = QueryRelationships.SetStackCount(basis, Entry(basis, 0), 3);
        Assert.Equal(4, next.Count);
        Assert.Equal(4, next.Count(requirement => requirement.IdentityGroup == 1));
        Assert.Equal(2, next.Count(requirement => requirement.AlternativeGroup is not null));
        Assert.Null(Problem(next));
        var board = QueryRelationships.BoardItems(next);
        Assert.Single(board);
        Assert.NotNull(board[0].Cluster);
        Assert.Equal(3, board[0].StackCount);
        // Removing one cluster member keeps the stack on the survivor.
        var dissolved = QueryRelationships.RemoveMember(next, 1);
        Assert.Single(QueryRelationships.BoardItems(dissolved));
        Assert.Equal(3, QueryRelationships.BoardItems(dissolved)[0].StackCount);
        Assert.Null(Problem(dissolved));
    }

    [Fact]
    public void APlainRepeatStackTradesItsCopiesForLabelsWhenItJoinsACluster()
    {
        List<ItemRequirement> start = [WeaponItem("spear"), WeaponItem("mace")];
        var basis = QueryRelationships.SetStackCount(start, Entry(start, 0), 2);
        var next = QueryRelationships.JoinAlternatives(basis, basis.FindIndex(requirement => requirement.Item?.Id == "mace"), 0);
        // The copy is now a bare weapon tied to the whole cluster.
        var bare = next.Where(requirement => requirement.Item is null).ToList();
        Assert.Single(bare);
        Assert.NotNull(bare[0].IdentityGroup);
        Assert.All(next.Where(requirement => requirement.AlternativeGroup is not null),
            member => Assert.Equal(bare[0].IdentityGroup, member.IdentityGroup));
        Assert.Null(Problem(next));
    }

    [Fact]
    public void DeletingTheAnchorDeletesItsCopiesAndLeavesNoStaleGroups()
    {
        List<ItemRequirement> wildcards = [new() { Kind = ItemKind.Wand }, new() { Kind = ItemKind.Armor }];
        var wildcard = QueryRelationships.SetStackCount(wildcards, Entry(wildcards, 0), 3);
        var afterWildcard = QueryRelationships.RemoveItem(wildcard, Entry(wildcard, 0));
        Assert.Single(afterWildcard);
        Assert.Equal(ItemKind.Armor, afterWildcard[0].Kind);
        Assert.All(afterWildcard, requirement => Assert.Null(requirement.IdentityGroup));

        List<ItemRequirement> rings = [RingOf("ring_might")];
        var stacked = QueryRelationships.SetStackCount(rings, Entry(rings, 0), 2);
        var total = QueryRelationships.SetStackTotal(stacked, Entry(stacked, 0), 3);
        Assert.Empty(QueryRelationships.RemoveItem(total, Entry(total, 0)));
    }

    [Fact]
    public void EjectingAMemberFromAStackedClusterStripsItsLabel()
    {
        var basis = QueryRelationships.JoinAlternatives([WeaponItem("spear"), WeaponItem("mace")], 1, 0);
        basis = QueryRelationships.SetStackCount(basis, Entry(basis, 0), 2);
        var ejected = QueryRelationships.Detach(basis, 0);
        var spear = ejected.First(requirement => requirement.Item?.Id == "spear");
        Assert.Null(spear.AlternativeGroup);
        Assert.Null(spear.IdentityGroup);
        Assert.Null(Problem(ejected));
    }

    [Fact]
    public void ATotalTurnsTheStackIntoIdenticalOptionalMembers()
    {
        List<ItemRequirement> rings = [RingOf("ring_might", UpgradeMatch.Exactly, 2)];
        var basis = QueryRelationships.SetStackCount(rings, Entry(rings, 0), 2);
        var next = QueryRelationships.SetStackTotal(basis, Entry(basis, 0), 3);
        Assert.Equal(2, next.Count);
        Assert.All(next, member => Assert.Equal(new LevelSum(1, 3), member.LevelSum));
        // The total speaks for the stack: per-member upgrades reset to any.
        Assert.All(next, member => Assert.Equal(UpgradeMatch.Any, member.UpgradeMatch));
        var board = QueryRelationships.BoardItems(next);
        Assert.Single(board);
        Assert.Equal(3, board[0].Total);
        Assert.Equal(2, board[0].StackCount);
        Assert.Contains("\"level_sum\":{\"group\":1,\"at_least\":3}", ResultsExport.EncodeQueryDocument(Query(next)));
        Assert.Null(Problem(next));
        // Clearing the total returns to plain repeats.
        var cleared = QueryRelationships.SetStackTotal(next, board[0], null);
        Assert.All(cleared, member => Assert.Null(member.LevelSum));
        Assert.Equal(2, QueryRelationships.BoardItems(cleared)[0].StackCount);
    }

    [Fact]
    public void OnlyARingStackCanCountLevelsTogether()
    {
        // The editor and the menu never offer a total off a ring, and the model
        // refuses one from anywhere else — the badge of a hand-written document.
        List<ItemRequirement> swords = [WeaponItem("longsword")];
        var stacked = QueryRelationships.SetStackCount(swords, Entry(swords, 0), 2);
        var refused = QueryRelationships.SetStackTotal(stacked, Entry(stacked, 0), 3);
        Assert.All(refused, requirement => Assert.Null(requirement.LevelSum));
        Assert.Equal(2, QueryRelationships.BoardItems(refused)[0].StackCount);
        // A stale non-ring sum can still be dissolved, back to plain repeats.
        var sword = WeaponItem("longsword"); sword.LevelSum = new(1, 3);
        var again = WeaponItem("longsword"); again.LevelSum = new(1, 3);
        List<ItemRequirement> stale = [sword, again];
        Assert.Contains("only rings", Problem(stale));
        var cleared = QueryRelationships.SetStackTotal(stale, Entry(stale, 0), null);
        Assert.All(cleared, requirement => Assert.Null(requirement.LevelSum));
        Assert.Null(Problem(cleared));
        // The capacity a total is held to: one ring at the vault's +4, every
        // other at the standard +2, each counting its upgrade plus one.
        Assert.Equal(5, QueryRelationships.RingStackCapacity(1));
        Assert.Equal(8, QueryRelationships.RingStackCapacity(2));
        Assert.Equal(11, QueryRelationships.RingStackCapacity(3));
    }

    [Fact]
    public void ALoadedLevelSumDocumentCollapsesBackIntoOneChip()
    {
        const string document = """
            {"requirements":[
              {"kind":"ring","item":"ring_might","level_sum":{"group":2,"at_least":4}},
              {"kind":"ring","item":"ring_might","level_sum":{"group":2,"at_least":4}},
              {"kind":"wand"}]}
            """;
        var board = QueryRelationships.BoardItems(ResultsExport.DecodeQueryDocument(document).Requirements);
        Assert.Equal(2, board.Count);
        Assert.Equal(4, board[0].Total);
        Assert.Equal(2, board[0].StackCount);
    }

    [Fact]
    public void TheEditorAppliesCountAndTotalAndRebuildsTheStack()
    {
        var requirements = QueryRelationships.ApplyEdit([], null, RingOf("ring_might"), 2, 3);
        Assert.Equal(2, requirements.Count);
        Assert.All(requirements, member => Assert.Equal(3, member.LevelSum?.AtLeast));
        // Raising the count keeps the total; clearing it returns plain repeats.
        requirements = QueryRelationships.ApplyEdit(requirements, 0, requirements[0], 3, 5);
        Assert.Equal(3, requirements.Count);
        Assert.All(requirements, member => Assert.Equal(5, member.LevelSum?.AtLeast));
        requirements = QueryRelationships.ApplyEdit(requirements, 0, requirements[0], 2, null);
        Assert.Equal(2, requirements.Count);
        Assert.All(requirements, member => Assert.Null(member.LevelSum));
        Assert.Equal(2, requirements.Count(member => member.Item?.Id == "ring_might"));
        Assert.Null(Problem(requirements));
    }

    [Fact]
    public void TheEditorRebuildsTheCopiesWhenItChangesTheAnchorsCategory()
    {
        var requirements = QueryRelationships.ApplyEdit([], null, new() { Kind = ItemKind.Wand }, 3, null);
        Assert.All(requirements, copy => Assert.Equal(ItemKind.Wand, copy.Kind));
        // The old copies named wands; the edited chip asks for rings, so the
        // stack comes down and is rebuilt rather than keeping stale wands.
        requirements = QueryRelationships.ApplyEdit(requirements, 0, new() { Kind = ItemKind.Ring }, 3, null);
        Assert.Equal(3, requirements.Count);
        Assert.All(requirements, copy => Assert.Equal(ItemKind.Ring, copy.Kind));
        Assert.Null(Problem(requirements));
    }

    [Fact]
    public void ShrinkingALevelSumStackFromTheEditorDropsItsOrphanedMembers()
    {
        var requirements = QueryRelationships.ApplyEdit([], null, RingOf("ring_might"), 3, 4);
        Assert.Equal(3, requirements.Count);
        requirements = QueryRelationships.ApplyEdit(requirements, 0, RingOf("ring_might"), 1, null);
        Assert.Single(requirements);
        Assert.Null(requirements[0].LevelSum);
    }

    [Fact]
    public void AStackDoesNotFollowItsChipIntoAClusterOfAnotherCategory()
    {
        // A copy has to name the kind it copies, and "ring or wand" names none,
        // so the second ring stays the standalone chip it already encodes as.
        var requirements = QueryRelationships.ApplyEdit([], null, RingOf("ring_might"), 2, null);
        requirements = [.. requirements, new ItemRequirement { Kind = ItemKind.Wand }];
        var joined = QueryRelationships.JoinAlternatives(requirements, 0, 2);
        Assert.DoesNotContain(joined, requirement => requirement.IdentityGroup is not null);
        Assert.Null(Problem(joined));
        Assert.Equal(2, QueryRelationships.BoardItems(joined).Count);
    }

    [Fact]
    public void AWildcardStackLetsItsCopiesGoWhenItsChipJoinsAnotherCategory()
    {
        // The copies were bare wands tied to the anchor by a label; a "wand or
        // spear" cluster is nothing they can be copies of, so they are dropped
        // rather than left behind as a stack the engine would refuse.
        var requirements = QueryRelationships.ApplyEdit([], null, new() { Kind = ItemKind.Wand }, 3, null);
        requirements = [.. requirements, WeaponItem("spear")];
        var joined = QueryRelationships.JoinAlternatives(requirements, 0, 3);
        Assert.Equal(["spear", null], joined.Select(requirement => requirement.Item?.Id));
        Assert.DoesNotContain(joined, requirement => requirement.IdentityGroup is not null);
        Assert.NotNull(joined[0].AlternativeGroup);
        Assert.Equal(joined[0].AlternativeGroup, joined[1].AlternativeGroup);
        Assert.Null(Problem(joined));
        Assert.Equal(1, QueryRelationships.BoardItems(joined)[0].StackCount);
    }

    [Fact]
    public void AClusterSpanningTwoCategoriesCannotGrowAStack()
    {
        var requirements = QueryRelationships.JoinAlternatives([new() { Kind = ItemKind.Wand }, WeaponItem("spear")], 0, 1);
        var cluster = Entry(requirements, 0);
        Assert.Equal(2, cluster.Members.Count);
        Assert.False(QueryRelationships.CanStack(requirements, cluster));
        Assert.Equal(requirements, QueryRelationships.SetStackCount(requirements, cluster, 2));
        // A cluster of one category is still free to stack.
        var weapons = QueryRelationships.JoinAlternatives([WeaponItem("mace"), WeaponItem("spear")], 0, 1);
        Assert.True(QueryRelationships.CanStack(weapons, Entry(weapons, 0)));
    }

    [Fact]
    public void TheAnchorAndItsCopiesCarryIndependentFloorLimits()
    {
        var armor = new ItemRequirement { Kind = ItemKind.Armor, Item = ItemCatalog.Find("plate_armor"), UpgradeMatch = UpgradeMatch.Exactly, Upgrade = 3, MaximumDepth = 4 };
        var requirements = QueryRelationships.ApplyEdit([], null, armor, 2, null, 9);
        Assert.Equal(2, requirements.Count);
        Assert.Equal(4, requirements[0].MaximumDepth);
        Assert.Equal(9, requirements[1].MaximumDepth);
        // Still one chip: a repeat with only a floor limit folds into its stack.
        var board = QueryRelationships.BoardItems(requirements);
        Assert.Single(board);
        Assert.Equal(2, board[0].StackCount);
        Assert.Equal(9, QueryRelationships.CopyDepthOf(requirements, board[0]));
        Assert.Null(Problem(requirements));
        // The round trip through the document keeps both limits.
        var reloaded = ResultsExport.DecodeQueryDocument(ResultsExport.EncodeQueryDocument(Query(requirements))).Requirements;
        Assert.Equal([4, 9], reloaded.Select(requirement => requirement.MaximumDepth ?? 0));
        Assert.Single(QueryRelationships.BoardItems(reloaded));
    }

    [Fact]
    public void UnlimitedCopiesStayUnlimitedWhileTheAnchorIsFloorBound()
    {
        var anchor = new ItemRequirement { Kind = ItemKind.Armor, UpgradeMatch = UpgradeMatch.Exactly, Upgrade = 3, MaximumDepth = 4 };
        var requirements = QueryRelationships.ApplyEdit([], null, anchor, 2, null, null);
        Assert.Equal(4, requirements[0].MaximumDepth);
        Assert.Null(requirements[1].MaximumDepth);
        Assert.NotNull(requirements[0].IdentityGroup);
        Assert.Equal(requirements[0].IdentityGroup, requirements[1].IdentityGroup);
        Assert.Null(Problem(requirements));
    }

    [Fact]
    public void AWildcardStackLimitsItsBareCopiesWithoutConstrainingThemOtherwise()
    {
        var anchor = new ItemRequirement { Kind = ItemKind.Wand, UpgradeMatch = UpgradeMatch.AtLeast, Upgrade = 2 };
        var requirements = QueryRelationships.ApplyEdit([], null, anchor, 2, null, 9);
        Assert.All(requirements.Skip(1), copy =>
        {
            Assert.Equal(9, copy.MaximumDepth); Assert.Equal(UpgradeMatch.Any, copy.UpgradeMatch);
        });
        Assert.Null(Problem(requirements));
        // Growing the stack from the chip badge keeps the copies' floor.
        requirements = QueryRelationships.SetStackCount(requirements, Entry(requirements, 0), 3);
        Assert.Equal(3, requirements.Count);
        Assert.All(requirements.Skip(1), copy => Assert.Equal(9, copy.MaximumDepth));
    }

    [Fact]
    public void EditingAwayTheLimitClearsItFromEveryCopy()
    {
        var requirements = QueryRelationships.ApplyEdit([], null, WeaponItem("longsword"), 3, null, 6);
        Assert.All(requirements.Skip(1), copy => Assert.Equal(6, copy.MaximumDepth));
        requirements = QueryRelationships.ApplyEdit(requirements, 0, WeaponItem("longsword"), 3, null, null);
        Assert.All(requirements, copy => Assert.Null(copy.MaximumDepth));
    }

    [Fact]
    public void TheCopiesKeepTheirFloorWhenTheStackFollowsItsChipIntoACluster()
    {
        var requirements = QueryRelationships.ApplyEdit([], null, RingOf("ring_might"), 2, null, 7);
        requirements = [.. requirements, RingOf("ring_haste")];
        var joined = QueryRelationships.JoinAlternatives(requirements, 0, 2);
        var copy = joined.First(requirement => requirement.Item is null);
        Assert.Equal(7, copy.MaximumDepth);
        Assert.Null(Problem(joined));
    }

    [Fact]
    public void TheBoardCountsWhatItShows()
    {
        List<ItemRequirement> requirements = [WeaponItem("spear"), WeaponItem("spear"), new() { Kind = ItemKind.Wand }];
        // Two chips: the plain repeat is the first chip's second copy.
        Assert.Equal(2, QueryRelationships.BoardCount(requirements));
        Assert.Equal(3, QueryRelationships.SlotCount(requirements));
    }

    [Fact]
    public void AChipNamesItselfAndCarriesItsQualifiersAsTags()
    {
        var wildcard = new ItemRequirement { Kind = ItemKind.ThrownWeapon, TierMatch = TierMatch.AtLeast, Tier = 3, UpgradeMatch = UpgradeMatch.AtLeast, Upgrade = 2, MaximumDepth = 4 };
        Assert.Equal("Any thrown", wildcard.ShortTitle);
        Assert.Equal(["T3+", "+2\u2191", "F\u22644"], wildcard.Tags.Select(tag => tag.Text));
        Assert.Equal([false, true, false], wildcard.Tags.Select(tag => tag.Upgrade));
        // A named item shows no tier: it is the tier it is.
        var named = RingOf("ring_might", UpgradeMatch.Exactly, 2);
        Assert.Equal("Ring of Might", named.ShortTitle);
        Assert.Equal(["+2"], named.Tags.Select(tag => tag.Text));
        Assert.Equal("Any wand", new ItemRequirement { Kind = ItemKind.Wand }.ShortTitle);
    }

    [Fact]
    public void TheChipDetailReadsTheStackAndTheRelationsAroundIt()
    {
        var requirements = QueryRelationships.ApplyEdit([], null, WeaponItem("longsword"), 3, null, 4);
        Assert.Equal(
            "Longsword\nany upgrade\n\u00d7 3 of the same kind \u2014 the extra copies: any upgrade, floors 1\u20134",
            QueryRelationships.ChipDetail(requirements, 0, Entry(requirements, 0), null));
        // A combined level speaks for the upgrades, so the chip's own says nothing.
        var rings = QueryRelationships.ApplyEdit([], null, RingOf("ring_might"), 3, null);
        var counted = QueryRelationships.SetStackTotal(rings, Entry(rings, 0), 5);
        Assert.Equal(
            "Ring of Might\n\u03a3 up to 3 \u2014 levels add to \u2265 5",
            QueryRelationships.ChipDetail(counted, 0, Entry(counted, 0), null));
        // A cluster member names its peers, and a problem has the last word.
        var joined = QueryRelationships.JoinAlternatives([WeaponItem("spear"), WeaponItem("shuriken")], 1, 0);
        Assert.Equal(
            "Spear\nany upgrade\nor Shuriken\nfrom the engine",
            QueryRelationships.ChipDetail(joined, 0, Entry(joined, 0), "from the engine"));
        // v4.0.0's vault treasure reads as its own source, like every other one.
        List<ItemRequirement> vault = [WeaponItem("longsword")];
        vault[0].Source = ScoutItemSource.VaultTreasure;
        Assert.Equal(
            "Longsword\nany upgrade · Vault treasure",
            QueryRelationships.ChipDetail(vault, 0, Entry(vault, 0), null));
    }
}
