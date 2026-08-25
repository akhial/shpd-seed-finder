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
    public void AnAlternativeJoinsTheOriginalsSlotAndClearsItsSum()
    {
        var original = Ring(2); original.LevelSum = new(1, 3);
        var other = Ring(1); other.LevelSum = new(1, 3);
        var list = List(original, other);
        var alternative = QueryRelationships.PrepareAlternative(list, original);
        // Nothing moves until the edit is accepted.
        Assert.Equal(2, list.Count); Assert.Null(original.AlternativeGroup); Assert.NotNull(original.LevelSum);
        Assert.NotEqual(original.Key, alternative.Key);
        Assert.Equal(1, alternative.AlternativeGroup);
        Assert.Null(alternative.LevelSum);
        QueryRelationships.CommitAlternative(list, original, alternative);
        Assert.Equal([original, alternative, other], list);
        Assert.Equal(1, original.AlternativeGroup);
        // The bug the original PR had: a row pulled into a slot kept its sum.
        Assert.Null(original.LevelSum);
        Assert.NotNull(other.LevelSum);
    }

    [Fact]
    public void ExtendingASlotAppendsAfterItsLastMemberWithAFreshGroupNumberOnlyWhenNeeded()
    {
        var first = Wand(3); var second = Wand(3); var outsider = Wand();
        var list = List(first, outsider, second);
        var third = QueryRelationships.PrepareAlternative(list, first);
        Assert.Equal(3, third.AlternativeGroup);
        QueryRelationships.CommitAlternative(list, first, third);
        Assert.Equal([first, outsider, second, third], list);
        // A lone row gets a number no other slot uses.
        var forked = QueryRelationships.PrepareAlternative(list, outsider);
        Assert.Equal(4, forked.AlternativeGroup);
    }

    [Fact]
    public void RemovingDownToOneMemberCollapsesTheSlot()
    {
        var a = Wand(1); var b = Wand(1); var c = Wand(1);
        var list = List(a, b, c);
        QueryRelationships.Remove(list, b);
        Assert.Equal([a, c], list); Assert.Equal(1, a.AlternativeGroup);
        QueryRelationships.Remove(list, c);
        Assert.Equal([a], list); Assert.Null(a.AlternativeGroup);
        QueryRelationships.Remove(list, a);
        Assert.Empty(list);
    }

    [Fact]
    public void SumCapacityCountsLevelsByTheEnginesMaximumUpgradeRule()
    {
        var exact = Ring(1); exact.LevelSum = new(2, 1);
        var any = new ItemRequirement { Kind = ItemKind.Ring, LevelSum = new(2, 1) };
        var atLeast = new ItemRequirement { Kind = ItemKind.Wand, UpgradeMatch = UpgradeMatch.AtLeast, Upgrade = 1, LevelSum = new(2, 1) };
        var elsewhere = Ring(4); elsewhere.LevelSum = new(1, 4);
        Assert.Equal(1, exact.MaximumUpgrade); Assert.Equal(4, any.MaximumUpgrade); Assert.Equal(3, atLeast.MaximumUpgrade);
        // Every item counts its upgrade plus one.
        Assert.Equal(2, exact.MaximumLevel); Assert.Equal(5, any.MaximumLevel); Assert.Equal(4, atLeast.MaximumLevel);
        Assert.Equal(2 + 5 + 4, QueryRelationships.SumCapacity([exact, any, atLeast, elsewhere], 2));
        QueryRelationships.PropagateSum([exact, any, atLeast, elsewhere], 2, 5);
        Assert.All(new[] { exact, any, atLeast }, member => Assert.Equal(new LevelSum(2, 5), member.LevelSum));
        Assert.Equal(new LevelSum(1, 4), elsewhere.LevelSum);
    }

    [Fact]
    public void ValidationNamesTheGroupThatCannotReachItsTotal()
    {
        // Two wands of any upgrade reach eight levels: +3 plus one each.
        var a = Wand(); var b = Wand(); a.LevelSum = new(1, 9); b.LevelSum = new(1, 9);
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
        Assert.Contains("alternative", Check(new() { Kind = ItemKind.Wand, AlternativeGroup = 1, LevelSum = new(1, 1) }));
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
        Assert.Equal("Any upgrade • same item group A • level group A ≥ 4 • by floor 4", ring.Description);
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
        // no Effect, AlternativeGroup or LevelSum.
        const string legacy = """
            { "Requirements": [ { "Key": 7, "Item": null, "Upgrade": 2, "Modifier": "Blazing", "Kind": 0, "Tier": 0,
              "TierMatch": 0, "UpgradeMatch": 1, "Source": null, "IdentityGroup": 1, "MaximumDepth": 9, "RequireUncursed": true } ],
              "MaximumDepth": 12, "RequireBlacksmith": false, "ExcludeBlacksmithRewards": false, "WandmakerQuest": 0, "FastMode": false, "Challenges": 0 }
            """;
        var query = JsonSerializer.Deserialize<QuerySettings>(legacy)!;
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
}
