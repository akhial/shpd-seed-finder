using System.Collections.ObjectModel;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace SeedSeeker;

// This file must stay free of Windows App SDK types: SeedSeeker.Tests links it
// to run on any host. Members that need XAML types live in the partial halves
// in Models.Presentation.cs.

// MeleeWeapon and ThrownWeapon narrow a weapon requirement to one weapon
// class; the enum value indexes the document kind-name table in
// ResultsExport, so they must stay appended after the original four families.
public enum ItemKind { Weapon, Armor, Wand, Ring, MeleeWeapon, ThrownWeapon }

/// <summary>Melee/thrown classification of weapon catalog entries.</summary>
public enum WeaponClass { Melee, Thrown }

public static class ItemKindExtensions
{
    /// <summary>The broad item family; catalog items always carry the family.</summary>
    public static ItemKind Family(this ItemKind kind) =>
        kind is ItemKind.MeleeWeapon or ItemKind.ThrownWeapon ? ItemKind.Weapon : kind;

    /// <summary>The weapon class this kind restricts to, or null when unrestricted.</summary>
    public static WeaponClass? WeaponClass(this ItemKind kind) => kind switch
    {
        ItemKind.MeleeWeapon => SeedSeeker.WeaponClass.Melee,
        ItemKind.ThrownWeapon => SeedSeeker.WeaponClass.Thrown,
        _ => null,
    };

    /// <summary>Whether a catalog item can satisfy a requirement of this kind.</summary>
    public static bool Accepts(this ItemKind kind, CatalogItem item) =>
        item.Kind == kind.Family() && (kind.WeaponClass() is not { } weaponClass || item.Class == weaponClass);

    /// <summary>The highest upgrade a search may name for this family.</summary>
    public static int MaximumSearchUpgrade(this ItemKind kind) => kind.Family() switch
    {
        ItemKind.Weapon => SearchLimits.MaxUpgradeWeapon,
        ItemKind.Ring => SearchLimits.MaxUpgradeRing,
        _ => SearchLimits.MaxUpgradeDefault,
    };
}

/// <summary>
/// Local copies of the engine's query bounds and session limits
/// (<c>crates/seedfinder-core/src/engine_info.rs</c>). They stay constants so
/// the editor needs nothing from the engine to open; EngineConstantsTests
/// asserts each of them against the engine's <c>engine_info</c> document.
/// </summary>
public static class SearchLimits
{
    /// <summary>Deepest floor a search may cover.</summary>
    public const int MaxDepth = 24;
    /// <summary>Tiers an "exactly tier N" requirement may name (tier 1 is starting gear).</summary>
    public const int ExactTierMin = 2;
    public const int ExactTierMax = 5;
    /// <summary>Tiers an "at least / at most tier N" requirement may name.</summary>
    public const int BoundedTierMin = 3;
    public const int BoundedTierMax = 4;
    /// <summary>Highest same-item group number (groups run 1..this, shown as A..D).</summary>
    public const int IdentityGroupMax = 4;
    /// <summary>How many items of one kind a single board chip may ask for.</summary>
    public const int StackMax = 3;
    /// <summary>Highest combined-level group number (groups run 1..this, shown as A..D).</summary>
    public const int LevelSumGroupMax = 4;
    /// <summary>
    /// Highest upgrade a search may name, for everything but weapons. v4.0.0's
    /// Imp vault sets the ceilings: its final-room options reach +4 on plate
    /// armor, wands and rings.
    /// </summary>
    public const int MaxUpgradeDefault = 4;
    /// <summary>Highest upgrade a ring requirement may name.</summary>
    public const int MaxUpgradeRing = 4;
    /// <summary>Highest upgrade a weapon requirement may name; the vault reaches +5 on a tier-4 weapon.</summary>
    public const int MaxUpgradeWeapon = 5;
    /// <summary>How many results one run lists, and one import restores.</summary>
    public const int ResultCap = 1024;
}

/// <summary>
/// The nine challenges in engine mask order, with the stable document name the
/// results codec writes and whether the level generator consults the
/// challenge (so enabling it changes which seeds match). A local copy of the
/// engine's list, checked against <c>engine_info</c> by EngineConstantsTests.
/// </summary>
public static class Challenges
{
    public sealed record Entry(string Name, int Mask, string Label, bool ChangesLevelGeneration);

    public static readonly Entry[] All =
    [
        new("on_diet", 1, "On diet", false),
        new("faith_is_my_armor", 2, "Faith is my armor", false),
        new("pharmacophobia", 4, "Pharmacophobia", false),
        new("barren_land", 8, "Barren land", true),
        new("swarm_intelligence", 16, "Swarm intelligence", false),
        new("into_darkness", 32, "Into darkness", true),
        new("forbidden_runes", 64, "Forbidden runes", true),
        new("hostile_champions", 128, "Hostile champions", false),
        new("badder_bosses", 256, "Badder bosses", false),
    ];

    /// <summary>Every challenge bit together: the largest legal challenge mask.</summary>
    public static int AllMask { get; } = All.Aggregate(0, (mask, entry) => mask | entry.Mask);
}
public enum UpgradeMatch { Any, Exactly, AtLeast }
public enum TierMatch { Any, Exactly, AtLeast, AtMost }
public enum SearchState { Running, Completed, Cancelled, Failed }

public sealed record CatalogItem(string Id, string Name, ItemKind Kind, int SpriteIndex, int? Tier, WeaponClass? Class = null);

public enum ScoutItemSource
{
    Heap, Chest, LockedChest, CrystalChest, Tomb, Skeleton, SacrificialFire, Mimic,
    GoldenMimic, CrystalMimic, Statue, ArmoredStatue, Shop, GhostReward,
    WandmakerReward, BlacksmithReward, ImpReward,
    // v4.0.0's Imp vault. The value indexes the wire ids and the document
    // source-name table in ResultsExport, so it stays appended after the
    // v3.3.8 block.
    VaultTreasure
}

/// <summary>
/// The generic Fluent glyph and tint, kept only for wildcard requirements that pin
/// no concrete item and so have no sprite to draw.
/// </summary>
public static partial class KindStyle
{
    public static string Glyph(ItemKind kind) => kind.Family() switch { ItemKind.Weapon => "", ItemKind.Armor => "", ItemKind.Wand => "", _ => "" };
}

public static class Labels
{
    public static string Kind(ItemKind value) => value switch { ItemKind.Weapon => "Weapons", ItemKind.Armor => "Armor", ItemKind.Wand => "Wands", ItemKind.MeleeWeapon => "Melee weapons", ItemKind.ThrownWeapon => "Thrown weapons", _ => "Rings" };
    public static string Singular(ItemKind value) => Kind(value).TrimEnd('s').ToLowerInvariant();
    public static string Source(ScoutItemSource value) => value switch
    {
        ScoutItemSource.LockedChest => "Locked chest", ScoutItemSource.CrystalChest => "Crystal chest",
        ScoutItemSource.SacrificialFire => "Sacrificial fire", ScoutItemSource.GoldenMimic => "Golden mimic",
        ScoutItemSource.CrystalMimic => "Crystal mimic", ScoutItemSource.ArmoredStatue => "Armored statue",
        ScoutItemSource.GhostReward => "Ghost reward", ScoutItemSource.WandmakerReward => "Wandmaker reward",
        ScoutItemSource.BlacksmithReward => "Blacksmith reward", ScoutItemSource.ImpReward => "Imp reward",
        ScoutItemSource.VaultTreasure => "Vault treasure",
        _ => string.Concat(value.ToString().Select((c, i) => i > 0 && char.IsUpper(c) ? " " + char.ToLowerInvariant(c) : char.ToLowerInvariant(c).ToString()))
    };
}

/// <summary>
/// Which effects (enchantments, glyphs, curses) a requirement accepts: any
/// effect or none at all, any non-curse effect of the item's family, or one
/// of a chosen set. A plain class so System.Text.Json persists it as-is;
/// <see cref="ItemRequirement.Modifier"/> keeps the pre-set single-name view.
/// </summary>
public sealed class EffectFilter
{
    /// <summary>Every non-curse effect of the family ("any enchantment").</summary>
    public bool AnyEnchantment { get; set; }
    /// <summary>The accepted effect names, in the catalog's order; empty means no restriction.</summary>
    public List<string> Effects { get; set; } = [];

    public static EffectFilter Any() => new();
    public static EffectFilter Enchantment() => new() { AnyEnchantment = true };
    public static EffectFilter OneOf(IEnumerable<string> effects) => new() { Effects = [.. effects] };

    /// <summary>True when the requirement places no condition on the effect.</summary>
    [JsonIgnore] public bool IsAny => !AnyEnchantment && Effects.Count == 0;
    /// <summary>The one chosen effect, or null when the filter is anything else.</summary>
    [JsonIgnore] public string? Single => !AnyEnchantment && Effects.Count == 1 ? Effects[0] : null;

    /// <summary>
    /// Whether this filter lists exactly the family's non-curse effects,
    /// which the document writes as the "any_enchantment" shorthand.
    /// </summary>
    public bool IsEveryEnchantmentOf(ItemKind kind) =>
        !AnyEnchantment && Effects.Count > 0 && Effects.ToHashSet().SetEquals(ItemCatalog.EnchantmentsOf(kind));

    /// <summary>The filter with the curse-type effects removed.</summary>
    public EffectFilter WithoutCurses(ItemKind kind) =>
        AnyEnchantment ? Enchantment() : OneOf(Effects.Where(effect => !ItemCatalog.IsCurse(kind, effect)));

    /// <summary>Whether every listed effect is a curse (never true for "any" or "any enchantment").</summary>
    public bool IsCursesOnly(ItemKind kind) =>
        !AnyEnchantment && Effects.Count > 0 && Effects.All(effect => ItemCatalog.IsCurse(kind, effect));

    public EffectFilter Clone() => new() { AnyEnchantment = AnyEnchantment, Effects = [.. Effects] };

    /// <summary>Summary text for a requirement row, or null when there is nothing to say.</summary>
    public string? Describe() => AnyEnchantment ? "any enchantment"
        : Effects.Count == 0 ? null
        : Effects.Count == 1 ? Effects[0]
        : $"effect: {string.Join("/", Effects)}";
}

/// <summary>
/// Membership in a combined-level group: the <em>levels</em> of the members of
/// <paramref name="Group"/> (1..LevelSumGroupMax, shown as A..D) must add up to
/// at least <paramref name="AtLeast"/>, where a matched item counts its upgrade
/// plus one. Members are optional, so the group reads "up to N items reaching
/// <paramref name="AtLeast"/> levels" — one +2 ring satisfies a total of 3 on
/// its own, and so does a +0 with a +1. Every member carries the same total.
/// </summary>
public sealed record LevelSum(int Group, int AtLeast);

/// <summary>A tiny qualifier beside a chip's name; the upgrade is tinted apart from the rest.</summary>
public sealed record ChipTag(string Text, bool Upgrade = false);

public sealed partial class ItemRequirement
{
    public long Key { get; set; } = Random.Shared.NextInt64(1, long.MaxValue);
    public CatalogItem? Item { get; set; }
    public int Upgrade { get; set; }
    /// <summary>
    /// The single pinned effect, or null — the pre-effect-set view of
    /// <see cref="Effect"/>, kept so saved queries and presets written before
    /// effect sets existed still load (the setter adopts a name; null leaves
    /// the filter alone, so a newer file's <c>Effect</c> is never erased).
    /// </summary>
    public string? Modifier
    {
        get => Effect.Single;
        set { if (value is not null) Effect = EffectFilter.OneOf([value]); }
    }
    /// <summary>Which effects the item may carry.</summary>
    public EffectFilter Effect { get; set; } = EffectFilter.Any();
    public ItemKind Kind { get; set; }
    public int Tier { get; set; }
    public TierMatch TierMatch { get; set; }
    public UpgradeMatch UpgradeMatch { get; set; }
    public ScoutItemSource? Source { get; set; }
    public int? IdentityGroup { get; set; }
    public int? MaximumDepth { get; set; }
    public bool RequireUncursed { get; set; }
    /// <summary>
    /// Requirements sharing a number form one "any of these" slot, satisfied
    /// by any single member. Null for a requirement that stands alone.
    /// </summary>
    public int? AlternativeGroup { get; set; }
    /// <summary>Combined-level group membership; never set on an alternative.</summary>
    public LevelSum? LevelSum { get; set; }
    [JsonIgnore] public string Glyph => KindStyle.Glyph(Kind);
    /// <summary>Row-major index into the upstream item atlas, or -1 for a wildcard.</summary>
    [JsonIgnore] public int SpriteIndex => Item?.SpriteIndex ?? -1;
    [JsonIgnore] public string Title => Item?.Name ?? (TierMatch switch { TierMatch.Exactly => $"Any Tier {Tier} {Labels.Singular(Kind)}", TierMatch.AtLeast => $"Any Tier {Tier}+ {Labels.Singular(Kind)}", TierMatch.AtMost => $"Any Tier {Tier} or lower {Labels.Singular(Kind)}", _ => $"Any {Labels.Singular(Kind)}" });
    [JsonIgnore] public string Description
    {
        get
        {
            var parts = new List<string> { UpgradeMatch switch { UpgradeMatch.Exactly => $"+{Upgrade} exactly", UpgradeMatch.AtLeast => $"+{Upgrade} or higher", _ => "Any upgrade" } };
            if (Effect.Describe() is string effect) parts.Add(effect); if (RequireUncursed) parts.Add("uncursed"); if (Source is not null) parts.Add(Labels.Source(Source.Value));
            if (IdentityGroup is not null) parts.Add("same-kind stack");
            if (LevelSum is { } sum) parts.Add($"levels \u2265 {sum.AtLeast} together");
            if (MaximumDepth is int d) parts.Add($"by floor {d}");
            return string.Join(" \u2022 ", parts);
        }
    }
    /// <summary>The short name a chip shows: the item, or its wildcard family.</summary>
    [JsonIgnore] public string ShortTitle => Item?.Name ?? (Kind switch
    {
        ItemKind.MeleeWeapon => "Any melee", ItemKind.ThrownWeapon => "Any thrown", _ => $"Any {Labels.Singular(Kind)}",
    });
    /// <summary>The tiny qualifiers beside a chip's name: tier (wildcards only), upgrade, floor.</summary>
    [JsonIgnore] public IReadOnlyList<ChipTag> Tags
    {
        get
        {
            var tags = new List<ChipTag>();
            if (Item is null && TierMatch == TierMatch.Exactly) tags.Add(new($"T{Tier}"));
            if (Item is null && TierMatch == TierMatch.AtLeast) tags.Add(new($"T{Tier}+"));
            if (Item is null && TierMatch == TierMatch.AtMost) tags.Add(new($"T\u2264{Tier}"));
            if (UpgradeMatch == UpgradeMatch.Exactly) tags.Add(new($"+{Upgrade}", true));
            if (UpgradeMatch == UpgradeMatch.AtLeast) tags.Add(new($"+{Upgrade}\u2191", true));
            if (MaximumDepth is int depth) tags.Add(new($"F\u2264{depth}"));
            return tags;
        }
    }
    /// <summary>
    /// The highest upgrade an item satisfying this requirement can carry. The
    /// engine's own rule: an exact upgrade counts as itself, anything else as
    /// the family cap.
    /// </summary>
    [JsonIgnore] public int MaximumUpgrade => UpgradeMatch == UpgradeMatch.Exactly ? Upgrade : Kind.MaximumSearchUpgrade();
    /// <summary>
    /// The most <em>levels</em> this requirement can contribute to a combined
    /// total: its highest upgrade plus one, since every matched item counts itself.
    /// </summary>
    [JsonIgnore] public int MaximumLevel => MaximumUpgrade + 1;
    /// <summary>
    /// Whether this constrains nothing beyond its category — the shape a
    /// same-item group's extra copies take. A narrowed weapon kind is a
    /// constraint; a per-item floor limit is a placement bound, not an item
    /// property, and does not count.
    /// </summary>
    [JsonIgnore] public bool IsBare =>
        Item is null && Kind == Kind.Family() && TierMatch == TierMatch.Any && UpgradeMatch == UpgradeMatch.Any
        && Effect.IsAny && !RequireUncursed && Source is null;
    public ItemRequirement Clone()
    {
        var copy = (ItemRequirement)MemberwiseClone();
        copy.Effect = Effect.Clone();
        return copy;
    }
}

/// <summary>One board entry: a chip, or an either/or cluster of chips.</summary>
public sealed class BoardItem
{
    /// <summary>Stable identity of the entry, for the badges' flyouts.</summary>
    public string Key { get; init; } = "";
    /// <summary>Visible requirement indices: one for a chip, all members for a cluster.</summary>
    public IReadOnlyList<int> Members { get; init; } = [];
    /// <summary>The cluster's alternative group, when this is a cluster.</summary>
    public int? Cluster { get; init; }
    /// <summary>Hidden copy indices behind the stack badge, in requirement order.</summary>
    public IReadOnlyList<int> Extras { get; init; } = [];
    /// <summary>The stack's combined level, when one is set.</summary>
    public int? Total { get; init; }
    /// <summary>How many items this asks for: its anchor plus the hidden copies.</summary>
    public int StackCount => 1 + Extras.Count;
    /// <summary>The requirement the badges and the editor act on.</summary>
    public int Anchor => Members[0];
}

/// <summary>
/// What the editor needs to know — and hands back — about a chip's stack: how
/// many items it asks for, the combined level across them when one is set, and
/// the floor limit its extra copies share. A cluster member's stack belongs to
/// the cluster, so its editor shows none of this.
/// </summary>
public sealed record StackShape(int Count, int? Total, int? CopyDepth, bool InCluster)
{
    /// <summary>The stack of a chip that asks for a single item.</summary>
    public static StackShape Lone { get; } = new(1, null, null, false);

    /// <summary>The stack of the board entry <paramref name="item"/>.</summary>
    public static StackShape Of(IEnumerable<ItemRequirement> requirements, BoardItem item) =>
        new(item.StackCount, item.Total, QueryRelationships.CopyDepthOf(requirements, item), item.Cluster is not null);
}

/// <summary>
/// The structure between requirements — either/or clusters, same-item stacks
/// and combined-level groups — and the edits that keep it consistent. Ported
/// from the web design's <c>relations.ts</c> so every platform writes the same
/// documents; pure list manipulation, so SeedSeeker.Tests covers it and the
/// window only wires it to chips.
///
/// Two ideas cover all three relationship kinds of the model:
///
/// <list type="bullet">
/// <item>an <em>either/or cluster</em> is several requirements sharing an
/// <see cref="ItemRequirement.AlternativeGroup"/>: one slot, any member fills it;</item>
/// <item>a <em>stack</em> is a chip (or a whole cluster) asking for more than one
/// item of the same kind — the blacksmith's reforge fodder. Its extra copies
/// never carry their own constraints. A stack of a concrete item encodes as
/// plain repeated requirements; a wildcard or cluster stack encodes as bare
/// copies tied to the anchor with an <see cref="ItemRequirement.IdentityGroup"/>;
/// a stack with a <em>combined level</em> encodes as identical members sharing a
/// <see cref="SeedSeeker.LevelSum"/> (each matched item counts upgrade+1 towards
/// the total, and members are optional, so "up to N items reaching T levels").</item>
/// </list>
///
/// Every edit returns a new list; the requirements themselves are copied rather
/// than mutated, and a copy keeps its <see cref="ItemRequirement.Key"/>, so the
/// board can follow an entry across an edit. The slot rule mirrors the engine's
/// <c>SearchQuery::slots</c>: a slot sits at its first member's position and
/// holds every member in requirement order.
/// </summary>
public static class QueryRelationships
{
    /// <summary>"A".."D" for a 1-based group number.</summary>
    public static string GroupLabel(int group) => ((char)('A' + group - 1)).ToString();

    /// <summary>The requirements grouped into slots, in slot order.</summary>
    public static List<List<ItemRequirement>> Slots(IEnumerable<ItemRequirement> requirements)
    {
        var slots = new List<List<ItemRequirement>>(); var slotOfGroup = new Dictionary<int, List<ItemRequirement>>();
        foreach (var requirement in requirements)
        {
            if (requirement.AlternativeGroup is int group)
            {
                if (!slotOfGroup.TryGetValue(group, out var slot)) { slot = []; slotOfGroup[group] = slot; slots.Add(slot); }
                slot.Add(requirement);
            }
            else slots.Add([requirement]);
        }
        return slots;
    }

    /// <summary>How many slots the query has — what the engine counts as one requirement each.</summary>
    public static int SlotCount(IEnumerable<ItemRequirement> requirements) => Slots(requirements).Count;

    /// <summary>The members of combined-level group <paramref name="group"/>.</summary>
    public static IEnumerable<ItemRequirement> SumMembers(IEnumerable<ItemRequirement> requirements, int group) =>
        requirements.Where(requirement => requirement.LevelSum?.Group == group);

    /// <summary>The highest level total the members of <paramref name="group"/> can reach together.</summary>
    public static int SumCapacity(IEnumerable<ItemRequirement> requirements, int group) =>
        SumMembers(requirements, group).Sum(member => member.MaximumLevel);

    // ---- the board's collapsed view -----------------------------------------

    /// <summary>One entry under construction, before its members are all known.</summary>
    private sealed class Building
    {
        public string Key = "";
        public List<int> Members = [];
        public int? Cluster;
        public List<int> Extras = [];
        public int? Total;
    }

    /// <summary>A copy of <paramref name="requirement"/> with <paramref name="change"/> applied; the key stays.</summary>
    private static ItemRequirement With(ItemRequirement requirement, Action<ItemRequirement> change)
    {
        var copy = requirement.Clone(); change(copy); return copy;
    }

    /// <summary>
    /// Whether <paramref name="copy"/> is the plain repeat of the named
    /// <paramref name="item"/>. A floor limit is a placement bound, not an item
    /// property, so a repeat that carries only one still folds into its stack.
    /// </summary>
    private static bool IsPlainItemCopy(ItemRequirement copy, CatalogItem item) =>
        copy.Item?.Id == item.Id && copy.TierMatch == TierMatch.Any && copy.UpgradeMatch == UpgradeMatch.Any
        && copy.Effect.IsAny && !copy.RequireUncursed && copy.Source is null
        && copy.IdentityGroup is null && copy.AlternativeGroup is null && copy.LevelSum is null;

    /// <summary>
    /// The bare copy a stack of <paramref name="anchor"/>'s kind grows by; it may
    /// carry its own floor limit, the one bound that is a placement, not an item
    /// property. The broad family, never a narrowed weapon class: a copy that
    /// named one would count as a second constrained member of the stack.
    /// </summary>
    private static ItemRequirement BareCopy(ItemRequirement anchor, int identityGroup, int? maximumDepth, long? key = null)
    {
        var copy = new ItemRequirement { Kind = anchor.Kind.Family(), IdentityGroup = identityGroup, MaximumDepth = maximumDepth };
        if (key is long value) copy.Key = value;
        return copy;
    }

    /// <summary>The plain repeat a concrete stack of <paramref name="anchor"/>'s item grows by.</summary>
    private static ItemRequirement PlainCopy(ItemRequirement anchor, int? maximumDepth, long? key = null)
    {
        var copy = new ItemRequirement { Kind = anchor.Kind, Item = anchor.Item, MaximumDepth = maximumDepth };
        if (key is long value) copy.Key = value;
        return copy;
    }

    /// <summary>The indices of every group, keyed by group number, in first-appearance order.</summary>
    private static Dictionary<int, List<int>> GroupIndices(IReadOnlyList<ItemRequirement> requirements, Func<ItemRequirement, int?> group)
    {
        var groups = new Dictionary<int, List<int>>();
        for (var index = 0; index < requirements.Count; index++)
            if (group(requirements[index]) is int number)
            {
                if (!groups.TryGetValue(number, out var members)) groups[number] = members = [];
                members.Add(index);
            }
        return groups;
    }

    /// <summary>How many requirements each group holds.</summary>
    private static Dictionary<int, int> Counts(IEnumerable<ItemRequirement> requirements, Func<ItemRequirement, int?> group)
    {
        var counts = new Dictionary<int, int>();
        foreach (var requirement in requirements)
            if (group(requirement) is int number) counts[number] = counts.GetValueOrDefault(number) + 1;
        return counts;
    }

    /// <summary>
    /// The board's collapsed view of the flat requirement list: clusters group
    /// alternatives, and a stack's copies fold into their anchor's badge.
    /// </summary>
    public static List<BoardItem> BoardItems(IEnumerable<ItemRequirement> source)
    {
        var requirements = source.ToList();
        var hidden = new HashSet<int>();

        // Combined-level groups: the first member anchors, the rest fold away.
        var sumAnchors = new Dictionary<int, (int Anchor, List<int> Extras, int Total)>();
        for (var index = 0; index < requirements.Count; index++)
        {
            if (requirements[index].LevelSum is not { } sum) continue;
            if (sumAnchors.TryGetValue(sum.Group, out var existing)) existing.Extras.Add(index);
            else sumAnchors[sum.Group] = (index, [], sum.AtLeast);
        }
        foreach (var group in sumAnchors.Values) hidden.UnionWith(group.Extras);

        // Identity stacks: bare copies fold into the constrained unit (or the
        // first member when every member is bare). Groups with two constrained
        // units cannot collapse; Validate reports them.
        var identityExtras = new Dictionary<int, List<int>>();
        foreach (var members in GroupIndices(requirements, requirement => requirement.IdentityGroup).Values)
        {
            var constrained = members.Where(index => !requirements[index].IsBare).ToList();
            var units = constrained
                .Select(index => requirements[index].AlternativeGroup is int alternative ? $"alt:{alternative}" : $"req:{index}")
                .Distinct().Count();
            if (units > 1) continue;
            var anchor = constrained.Count > 0 ? constrained[0] : members[0];
            // A cluster anchor labels every member; fold only the lone bare copies.
            var extras = members.Where(index => index != anchor && requirements[index].AlternativeGroup is null && requirements[index].IsBare).ToList();
            if (extras.Count == 0) continue;
            identityExtras[anchor] = extras; hidden.UnionWith(extras);
        }

        // Walk the list building chips and clusters, folding plain item repeats
        // into the nearest earlier chip naming the same item.
        var items = new List<Building>(); var clusters = new Dictionary<int, Building>(); var chipByItem = new Dictionary<string, Building>();
        void Attach(Building entry, int anchorIndex)
        {
            if (requirements[anchorIndex].LevelSum is { } sum && sumAnchors.TryGetValue(sum.Group, out var group) && group.Anchor == anchorIndex)
            {
                entry.Extras.AddRange(group.Extras); entry.Total = group.Total;
            }
            if (identityExtras.TryGetValue(anchorIndex, out var extras)) entry.Extras.AddRange(extras);
        }
        for (var index = 0; index < requirements.Count; index++)
        {
            if (hidden.Contains(index)) continue;
            var requirement = requirements[index];
            if (requirement.AlternativeGroup is int cluster)
            {
                if (clusters.TryGetValue(cluster, out var existing)) { existing.Members.Add(index); Attach(existing, index); continue; }
                var made = new Building { Key = $"alt:{cluster}", Members = [index], Cluster = cluster };
                clusters[cluster] = made; Attach(made, index); items.Add(made); continue;
            }
            // A plain repeat of an earlier chip's item folds into that chip.
            if (requirement.Item is { } repeated && IsPlainItemCopy(requirement, repeated)
                && chipByItem.TryGetValue(repeated.Id, out var earlier) && earlier.Total is null && earlier.Extras.Count + 1 < SearchLimits.StackMax)
            {
                earlier.Extras.Add(index); continue;
            }
            var chip = new Building { Key = $"req:{index}", Members = [index] };
            Attach(chip, index);
            if (requirement.Item is { } named && requirement.LevelSum is null) chipByItem[named.Id] = chip;
            items.Add(chip);
        }
        // Single-member clusters render as chips.
        return items.Select(entry => new BoardItem
        {
            Key = entry.Key, Members = entry.Members, Extras = entry.Extras, Total = entry.Total,
            Cluster = entry.Members.Count > 1 ? entry.Cluster : null,
        }).ToList();
    }

    /// <summary>The number of visible board entries, for the pane's header count.</summary>
    public static int BoardCount(IEnumerable<ItemRequirement> requirements) => BoardItems(requirements).Count;

    /// <summary>The board entry holding requirement <paramref name="index"/>, or null.</summary>
    public static BoardItem? ItemOf(IEnumerable<ItemRequirement> requirements, int index) =>
        BoardItems(requirements).FirstOrDefault(entry => entry.Members.Contains(index));

    // ---- edits ---------------------------------------------------------------

    private static int? FreeGroup(IEnumerable<int?> used, int max)
    {
        var taken = used.ToHashSet();
        for (var group = 1; group <= max; group++) if (!taken.Contains(group)) return group;
        return null;
    }

    private static int NextAlternativeGroup(IEnumerable<ItemRequirement> requirements) =>
        requirements.Aggregate(0, (highest, requirement) => Math.Max(highest, requirement.AlternativeGroup ?? 0)) + 1;

    /// <summary>
    /// Rewrites the list into its canonical stack encoding and drops every group
    /// that no longer says anything:
    ///
    /// <list type="bullet">
    /// <item>a lone alternative, a lone identity label and a lone level-sum member dissolve;</item>
    /// <item>a labelled cluster labels every one of its members;</item>
    /// <item>a stack anchored on a lone concrete chip carries plain repeats, not identity labels.</item>
    /// </list>
    ///
    /// Every operation funnels through this, so a deleted anchor can never leave
    /// stale groups behind.
    /// </summary>
    public static List<ItemRequirement> Normalize(IEnumerable<ItemRequirement> source)
    {
        var next = source.ToList();
        // A cluster that holds an identity label spreads it to all its members.
        var clusterLabel = new Dictionary<int, int>();
        foreach (var requirement in next)
            if (requirement.AlternativeGroup is int cluster && requirement.IdentityGroup is int label) clusterLabel[cluster] = label;
        for (var index = 0; index < next.Count; index++)
            if (next[index].AlternativeGroup is int cluster && clusterLabel.TryGetValue(cluster, out var label) && next[index].IdentityGroup != label)
                next[index] = With(next[index], copy => copy.IdentityGroup = label);
        // A stack anchored on a lone concrete chip encodes as plain repeats.
        foreach (var members in GroupIndices(next, requirement => requirement.IdentityGroup).Values)
        {
            var constrained = members.Where(index => !next[index].IsBare).ToList();
            if (constrained.Count != 1) continue;
            var anchorIndex = constrained[0]; var anchor = next[anchorIndex];
            if (anchor.Item is null || anchor.AlternativeGroup is not null) continue;
            foreach (var index in members)
                next[index] = index == anchorIndex
                    ? With(anchor, copy => copy.IdentityGroup = null)
                    : PlainCopy(anchor, next[index].MaximumDepth, next[index].Key);
        }
        // Groups of one say nothing.
        var alternatives = Counts(next, requirement => requirement.AlternativeGroup);
        var identities = Counts(next, requirement => requirement.IdentityGroup);
        var sums = Counts(next, requirement => requirement.LevelSum?.Group);
        for (var index = 0; index < next.Count; index++)
        {
            var requirement = next[index];
            if (requirement.AlternativeGroup is int alternative && alternatives.GetValueOrDefault(alternative) < 2) requirement = With(requirement, copy => copy.AlternativeGroup = null);
            if (requirement.IdentityGroup is int identity && identities.GetValueOrDefault(identity) < 2) requirement = With(requirement, copy => copy.IdentityGroup = null);
            if (requirement.LevelSum is { } sum && sums.GetValueOrDefault(sum.Group) < 2) requirement = With(requirement, copy => copy.LevelSum = null);
            next[index] = requirement;
        }
        return next;
    }

    /// <summary>Moves the requirement at <paramref name="from"/> after the last one matching <paramref name="after"/>.</summary>
    private static List<ItemRequirement> MoveAfter(List<ItemRequirement> requirements, int from, Func<ItemRequirement, bool> after)
    {
        var moving = requirements[from];
        var rest = requirements.Where((_, index) => index != from).ToList();
        rest.Insert(rest.FindLastIndex(requirement => after(requirement)) + 1, moving);
        return rest;
    }

    /// <summary>
    /// The chip at <paramref name="source"/> becomes an either/or alternative of
    /// the chip at <paramref name="target"/>. A combined level cannot travel into
    /// a cluster and is dropped; a plain-repeat stack keeps its copies by trading
    /// them for identity labels, which the cluster's members then share. Across
    /// categories the stacks let go instead: a cluster of two categories names no
    /// kind for a copy to name, so the labelled copies are dropped.
    /// </summary>
    public static List<ItemRequirement> JoinAlternatives(IEnumerable<ItemRequirement> requirements, int source, int target)
    {
        var next = requirements.ToList();
        if (source == target) return next;
        var group = next[target].AlternativeGroup ?? NextAlternativeGroup(next);
        if (next[source].AlternativeGroup == group) return next;
        // A copy has to name the kind it copies, so only a cluster that stays
        // within one category can anchor a stack. When the join would mix
        // categories the repeats simply stay the standalone chips they encode as.
        var clusterMembers = Enumerable.Range(0, next.Count)
            .Where(index => index == source || index == target || next[index].AlternativeGroup == group).ToList();
        var oneCategory = clusterMembers.Select(index => next[index].Kind.Family()).Distinct().Count() == 1;
        var sourceKey = next[source].Key; var targetKey = next[target].Key;
        if (oneCategory)
        {
            // Trade plain repeats for identity copies so the stack survives the move.
            foreach (var index in new[] { source, target })
            {
                var anchor = next[index];
                if (anchor.Item is not { } named || anchor.IdentityGroup is not null) continue;
                var copies = Enumerable.Range(0, next.Count).Where(other => other != index && IsPlainItemCopy(next[other], named)).ToList();
                if (copies.Count == 0) continue;
                if (FreeGroup(next.Select(requirement => requirement.IdentityGroup), SearchLimits.IdentityGroupMax) is not int label) continue;
                next[index] = With(anchor, copy => copy.IdentityGroup = label);
                foreach (var other in copies) next[other] = BareCopy(anchor, label, next[other].MaximumDepth, next[other].Key);
            }
        }
        else
        {
            // The stacks let go: a copy tied to a member by a label is dropped,
            // and a plain repeat stays the standalone chip it already encodes as.
            // The chip's badge falls back to one, the visible half of this.
            var labels = clusterMembers.Select(index => next[index].IdentityGroup).OfType<int>().ToHashSet();
            var keys = clusterMembers.Select(index => next[index].Key).ToHashSet();
            bool Labelled(ItemRequirement requirement) => requirement.IdentityGroup is int label && labels.Contains(label);
            next = next.Where(requirement => keys.Contains(requirement.Key) || !Labelled(requirement)).ToList();
            for (var index = 0; index < next.Count; index++)
                if (Labelled(next[index])) next[index] = With(next[index], copy => copy.IdentityGroup = null);
        }
        // The dropped copies moved the pair, so both are found again by key.
        var movedSource = next.FindIndex(requirement => requirement.Key == sourceKey);
        var movedTarget = next.FindIndex(requirement => requirement.Key == targetKey);
        for (var index = 0; index < next.Count; index++)
            if (index == movedSource || index == movedTarget)
                next[index] = With(next[index], copy => { copy.AlternativeGroup = group; copy.LevelSum = null; });
        return Normalize(MoveAfter(next, movedSource, requirement => requirement.AlternativeGroup == group));
    }

    /// <summary>
    /// Whether the board entry <paramref name="item"/> can carry a stack. A copy
    /// has to name the kind it copies, and a cluster spanning two categories —
    /// "spear or wand" — names none, so such a cluster is offered no stack and
    /// cannot grow one.
    /// </summary>
    public static bool CanStack(IEnumerable<ItemRequirement> requirements, BoardItem item)
    {
        var next = requirements.ToList();
        var family = next[item.Anchor].Kind.Family();
        return item.Members.All(index => next[index].Kind.Family() == family);
    }

    /// <summary>Pulls the chip at <paramref name="index"/> out of its cluster; it leaves its stack behind.</summary>
    public static List<ItemRequirement> Detach(IEnumerable<ItemRequirement> requirements, int index)
    {
        var next = requirements.ToList();
        next[index] = With(next[index], copy => { copy.AlternativeGroup = null; copy.IdentityGroup = null; });
        return Normalize(next);
    }

    /// <summary>Deletes a whole board entry: its members and its hidden copies.</summary>
    public static List<ItemRequirement> RemoveItem(IEnumerable<ItemRequirement> requirements, BoardItem item)
    {
        var doomed = item.Members.Concat(item.Extras).ToHashSet();
        return Normalize(requirements.Where((_, index) => !doomed.Contains(index)));
    }

    /// <summary>Deletes one cluster member; the cluster and its stack live on without it.</summary>
    public static List<ItemRequirement> RemoveMember(IEnumerable<ItemRequirement> requirements, int index) =>
        Normalize(requirements.Where((_, other) => other != index));

    /// <summary>
    /// Sets how many items the board entry <paramref name="item"/> asks for. An
    /// entry that <see cref="CanStack"/> refuses can still shrink, never grow.
    /// </summary>
    public static List<ItemRequirement> SetStackCount(IEnumerable<ItemRequirement> requirements, BoardItem item, int count)
    {
        var next = requirements.ToList();
        var wanted = Math.Clamp(count, 1, SearchLimits.StackMax) - 1;
        if (wanted == item.Extras.Count) return next;
        if (wanted < item.Extras.Count)
        {
            var doomed = item.Extras.Skip(wanted).ToHashSet();
            return Normalize(next.Where((_, index) => !doomed.Contains(index)));
        }
        if (!CanStack(next, item)) return next;
        var anchor = next[item.Anchor];
        var added = wanted - item.Extras.Count;
        // New copies keep to the floor limit the existing copies already carry.
        var inherited = item.Extras.Count > 0 ? next[item.Extras[0]].MaximumDepth : null;
        Func<ItemRequirement> copy;
        if (item.Total is not null && anchor.LevelSum is not null)
            copy = () => { var member = anchor.Clone(); member.Key = Random.Shared.NextInt64(1, long.MaxValue); return member; };
        else if (item.Cluster is null && anchor.Item is not null) copy = () => PlainCopy(anchor, inherited);
        else
        {
            if ((anchor.IdentityGroup ?? FreeGroup(next.Select(requirement => requirement.IdentityGroup), SearchLimits.IdentityGroupMax)) is not int label) return next;
            foreach (var index in item.Members) next[index] = With(next[index], member => member.IdentityGroup = label);
            copy = () => BareCopy(anchor, label, inherited);
        }
        var insertAt = item.Members.Concat(item.Extras).Max() + 1;
        next.InsertRange(insertAt, Enumerable.Range(0, added).Select(_ => copy()));
        return Normalize(next);
    }

    /// <summary>
    /// The floor limit the stack's extra copies share (the first copy's, when a
    /// hand-written document gave them different ones).
    /// </summary>
    public static int? CopyDepthOf(IEnumerable<ItemRequirement> requirements, BoardItem item) =>
        item.Extras.Count > 0 ? requirements.ElementAt(item.Extras[0]).MaximumDepth : null;

    /// <summary>
    /// Sets or clears the floor limit of the stack's extra copies. The anchor
    /// keeps its own limit: "the +3 one before floor 4, the rest wherever" and
    /// "…the rest before floor 10" are both sayable. A combined-level stack has
    /// identical members and no lone copies to bound.
    /// </summary>
    public static List<ItemRequirement> SetCopyDepth(IEnumerable<ItemRequirement> requirements, BoardItem item, int? maximumDepth)
    {
        var next = requirements.ToList();
        if (item.Total is not null) return next;
        foreach (var index in item.Extras) next[index] = With(next[index], copy => copy.MaximumDepth = maximumDepth);
        return Normalize(next);
    }

    /// <summary>
    /// Sets or clears the stack's combined level. Only a lone concrete chip can
    /// count levels; with a total the whole stack becomes identical optional
    /// members ("up to N items reaching T levels"), without one it returns to an
    /// anchor with plain repeats ("exactly N of the item").
    /// </summary>
    public static List<ItemRequirement> SetStackTotal(IEnumerable<ItemRequirement> requirements, BoardItem item, int? total)
    {
        var next = requirements.ToList();
        var anchor = next[item.Anchor];
        if (item.Cluster is not null || anchor.Item is null) return next;
        var indices = item.Extras.Prepend(item.Anchor).ToList();
        if (total is not int atLeast)
        {
            foreach (var index in indices)
                next[index] = index == item.Anchor
                    ? With(anchor, copy => copy.LevelSum = null)
                    : PlainCopy(anchor, null, next[index].Key);
            return Normalize(next);
        }
        if ((anchor.LevelSum?.Group ?? FreeGroup(next.Select(requirement => requirement.LevelSum?.Group), SearchLimits.LevelSumGroupMax)) is not int group) return next;
        foreach (var index in indices)
        {
            var key = next[index].Key;
            next[index] = With(anchor, copy =>
            {
                copy.Key = key; copy.Upgrade = 0; copy.UpgradeMatch = UpgradeMatch.Any;
                copy.IdentityGroup = null; copy.LevelSum = new(group, atLeast);
            });
        }
        return Normalize(next);
    }

    /// <summary>
    /// The most levels a stack of <paramref name="count"/> items of
    /// <paramref name="kind"/> can reach together: each counts its upgrade plus
    /// one, and the copies of a combined-level stack are unconstrained.
    /// </summary>
    public static int StackCapacity(ItemKind kind, int count) => count * (kind.MaximumSearchUpgrade() + 1);

    /// <summary>
    /// Applies the editor's result: the anchor's own fields plus the stack's
    /// shape. <paramref name="index"/> is the edited anchor, or null for a new
    /// chip. Editing a cluster member leaves the stack's count and total to the
    /// cluster.
    /// </summary>
    public static List<ItemRequirement> ApplyEdit(IEnumerable<ItemRequirement> requirements, int? index, ItemRequirement requirement, int count, int? total, int? copyDepth = null)
    {
        var current = requirements.ToList();
        List<ItemRequirement> next; long anchorKey;
        if (index is not int at)
        {
            var added = requirement.Clone(); anchorKey = added.Key;
            next = [.. current, added];
        }
        else
        {
            var edited = current[at]; anchorKey = edited.Key;
            // The copies belonged to the chip as it was, and the edit may have
            // changed the very kind they copy — so the stack comes down here and
            // is rebuilt below from the count and total the editor returned. A
            // cluster member leaves its stack to the cluster and keeps its copies.
            var doomed = new HashSet<int>();
            if (edited.AlternativeGroup is null && ItemOf(current, at) is { } owner) doomed.UnionWith(owner.Extras);
            var replacement = requirement.Clone();
            replacement.Key = anchorKey; replacement.AlternativeGroup = edited.AlternativeGroup;
            current[at] = replacement;
            next = current.Where((_, other) => !doomed.Contains(other)).ToList();
        }
        next = Normalize(next);
        var anchorIndex = next.FindIndex(entry => entry.Key == anchorKey);
        if (anchorIndex < 0 || next[anchorIndex].AlternativeGroup is not null) return next;
        if (ItemOf(next, anchorIndex) is not { } item) return next;
        if (item.Total is not null && total is null)
        {
            next = SetStackTotal(next, item, null);
            item = ItemOf(next, anchorIndex) ?? item;
        }
        next = SetStackCount(next, item, count);
        if (ItemOf(next, anchorIndex) is not { } refreshed) return next;
        return total is int atLeast ? SetStackTotal(next, refreshed, atLeast) : SetCopyDepth(next, refreshed, copyDepth);
    }

    /// <summary>
    /// The chip's hover detail: its title, what it asks of one item, the
    /// relationships it stands in, and the problem the engine would reject it
    /// for. One line each, as the web board's popover has them.
    /// </summary>
    public static string ChipDetail(IReadOnlyList<ItemRequirement> requirements, int index, BoardItem? item, string? problem)
    {
        var requirement = requirements[index];
        var lines = new List<string> { requirement.Title };
        var facts = new List<string>();
        if (requirement.UpgradeMatch == UpgradeMatch.Exactly) facts.Add($"exactly +{requirement.Upgrade}");
        else if (requirement.UpgradeMatch == UpgradeMatch.AtLeast) facts.Add($"+{requirement.Upgrade} or higher");
        // A combined level speaks for the stack's upgrades, so the chip's own says nothing.
        else if (item?.Total is null) facts.Add("any upgrade");
        if (requirement.Effect.Describe() is string effect) facts.Add(effect);
        if (requirement.RequireUncursed) facts.Add("uncursed");
        if (requirement.Source is ScoutItemSource source) facts.Add(Labels.Source(source));
        if (requirement.MaximumDepth is int depth) facts.Add($"floors 1\u2013{depth}");
        if (facts.Count > 0) lines.Add(string.Join(" \u00b7 ", facts));
        if (requirement.AlternativeGroup is int cluster)
            lines.Add($"or {string.Join(", ", requirements.Where((other, position) => position != index && other.AlternativeGroup == cluster).Select(other => other.ShortTitle))}");
        if (item is { Total: int total }) lines.Add($"\u03a3 up to {item.StackCount} \u2014 levels add to \u2265 {total}");
        else if (item is not null && item.StackCount > 1)
        {
            // The chip's own bounds (+3, floors 1–4) describe one copy, not the extras.
            var depths = item.Extras.Select(extra => requirements[extra].MaximumDepth).Distinct().ToList();
            var floors = depths.Count > 1 ? "own floor limits" : depths[0] is int only ? $"floors 1\u2013{only}" : "any floor";
            lines.Add($"\u00d7 {item.StackCount} of the same kind \u2014 the extra copies: any upgrade, {floors}");
        }
        if (problem is not null) lines.Add(problem);
        return string.Join("\n", lines);
    }

    /// <summary>
    /// The first problem the engine would reject the query for, as a message
    /// naming the offending group or item, or null when it is sound. The engine
    /// only reports a generic rejection over the FFI, so this runs first.
    /// </summary>
    public static string? Validate(QuerySettings query)
    {
        var requirements = query.Requirements;
        foreach (var requirement in requirements)
        {
            var family = requirement.Kind.Family();
            if (!requirement.Effect.IsAny && family is not (ItemKind.Weapon or ItemKind.Armor))
                return $"{requirement.Title} cannot require an effect: {Labels.Kind(requirement.Kind).ToLowerInvariant()} carry none.";
            if (requirement.RequireUncursed && requirement.Effect.IsCursesOnly(requirement.Kind))
                return $"{requirement.Title} requires uncursed but only lists curses.";
            if (requirement.LevelSum is { } sum)
            {
                if (requirement.AlternativeGroup is not null)
                    return $"{requirement.Title} is an alternative, so it cannot be in combined level group {GroupLabel(sum.Group)}.";
                if (sum.Group < 1 || sum.Group > SearchLimits.LevelSumGroupMax || sum.AtLeast < 1)
                    return $"{requirement.Title} has an invalid combined level group.";
            }
        }
        // A same-item group is a stack: one anchor unit — a lone requirement, or
        // the members of one alternative group — may constrain the item it binds
        // to; every other member is a plain copy of the same category.
        foreach (var group in requirements.Select(requirement => requirement.IdentityGroup).OfType<int>().Distinct().Order())
        {
            var members = requirements.Where(requirement => requirement.IdentityGroup == group).ToList();
            if (members.Select(member => member.Kind.Family()).Distinct().Count() > 1)
                return $"Same-item group {GroupLabel(group)} mixes different categories.";
            var units = members.Where(member => !member.IsBare)
                .Select(member => member.AlternativeGroup is int alternative ? $"alternative {alternative}" : $"requirement {member.Key}")
                .Distinct().Count();
            if (units > 1)
                return $"Same-item group {GroupLabel(group)} can describe one item (or one set of alternatives); its other members must be plain.";
        }
        // Combined-level groups: one shared, reachable total, counted in levels
        // (upgrade plus one per item).
        foreach (var group in requirements.Select(requirement => requirement.LevelSum?.Group).OfType<int>().Distinct().Order())
        {
            var totals = SumMembers(requirements, group).Select(member => member.LevelSum!.AtLeast).Distinct().ToList();
            if (totals.Count > 1) return $"Combined level group {GroupLabel(group)} has members that disagree on the total.";
            var capacity = SumCapacity(requirements, group);
            if (totals[0] > capacity)
                return $"Combined level group {GroupLabel(group)} needs {totals[0]} levels but its items can reach at most {capacity}.";
        }
        return null;
    }
}

/// <summary>
/// Floor-limit helpers shared by every floor selector. Boss floors 5, 10 and 15
/// generate no searchable items: the engine treats a floor limit of 5/10/15
/// exactly like 4/9/14, so selectors skip them. Floor 20 stays selectable
/// because the Imp shop gives the City boss floor searchable stock.
/// </summary>
public static class FloorLimits
{
    public static readonly int[] EmptyBossFloors = [5, 10, 15];

    /// <summary>Floors offered by floor-limit selectors: 1..MaxDepth minus the empty boss floors.</summary>
    public static readonly int[] Options = Enumerable.Range(1, SearchLimits.MaxDepth).Where(f => !EmptyBossFloors.Contains(f)).ToArray();

    /// <summary>Snaps an empty boss-floor limit to the equivalent floor below it (5→4, 10→9, 15→14).</summary>
    public static int Normalize(int depth) => EmptyBossFloors.Contains(depth) ? depth - 1 : depth;

    /// <summary>The slider index for a floor limit; off-list values snap to the nearest option below (or the first option).</summary>
    public static int IndexOf(int depth)
    {
        var floor = Normalize(depth);
        var exact = Array.IndexOf(Options, floor);
        return exact >= 0 ? exact : Math.Max(0, Array.FindLastIndex(Options, option => option <= floor));
    }

    /// <summary>
    /// Where a floor-limit control lands when the user moves it onto an empty boss floor.
    /// A single upward step (spin button, arrow key) continues to the next real floor; every
    /// other move — single steps down and typed jumps in either direction — snaps to the
    /// equivalent floor below, matching <see cref="Normalize"/>. Typing "10" therefore means
    /// "first 10 floors" (≡ 9), never 11.
    /// </summary>
    public static int SkipTarget(int previous, int requested) =>
        !EmptyBossFloors.Contains(requested) ? requested
        : requested == previous + 1 ? requested + 1
        : requested - 1;
}

/// <summary>
/// The Wandmaker quest a search can demand. Only this giver's variant is worth
/// filtering on: its quest item can be used in the dungeon instead of being
/// handed in. The value orders the picker, with 0 meaning "any".
/// </summary>
public enum WandmakerQuest
{
    Any = 0,
    CorpseDust = 1,
    ElementalEmbers = 2,
    Rotberry = 3,
}

public static class WandmakerQuests
{
    /// <summary>The pickable quests in wire order, "Any" first.</summary>
    public static readonly WandmakerQuest[] All =
    [
        WandmakerQuest.Any,
        WandmakerQuest.CorpseDust,
        WandmakerQuest.ElementalEmbers,
        WandmakerQuest.Rotberry,
    ];

    public static string Label(WandmakerQuest quest) => quest switch
    {
        WandmakerQuest.CorpseDust => "Corpse dust",
        WandmakerQuest.ElementalEmbers => "Elemental embers",
        WandmakerQuest.Rotberry => "Rotberry",
        _ => "Any",
    };

    /// <summary>Stable snake_case name used by the shared query document.</summary>
    public static string? DocumentName(WandmakerQuest quest) => quest switch
    {
        WandmakerQuest.CorpseDust => "corpse_dust",
        WandmakerQuest.ElementalEmbers => "elemental_embers",
        WandmakerQuest.Rotberry => "rotberry",
        _ => null,
    };

    public static WandmakerQuest? Named(string name) => name switch
    {
        "corpse_dust" => WandmakerQuest.CorpseDust,
        "elemental_embers" => WandmakerQuest.ElementalEmbers,
        "rotberry" => WandmakerQuest.Rotberry,
        _ => null,
    };
}

public sealed class QuerySettings
{
    public ObservableCollection<ItemRequirement> Requirements { get; set; } = [];
    public int MaximumDepth { get; set; } = SearchLimits.MaxDepth;
    public bool RequireBlacksmith { get; set; }
    public bool ExcludeBlacksmithRewards { get; set; }
    public WandmakerQuest WandmakerQuest { get; set; } = WandmakerQuest.Any;
    public bool FastMode { get; set; }
    public int Challenges { get; set; }

    public QuerySettings Clone() => new()
    {
        Requirements = new ObservableCollection<ItemRequirement>(Requirements.Select(x => x.Clone())),
        MaximumDepth = MaximumDepth,
        RequireBlacksmith = RequireBlacksmith,
        ExcludeBlacksmithRewards = ExcludeBlacksmithRewards,
        WandmakerQuest = WandmakerQuest,
        FastMode = FastMode,
        Challenges = Challenges,
    };
}

/// <summary>
/// Decides whether a query can continue a finished run instead of rescanning it:
/// an identical floor limit, challenge set and fast mode, world conditions (the
/// blacksmith flags and the Wandmaker quest) at least as strict as the
/// baseline's, and every baseline requirement still present (counting
/// duplicates). Extra requirements are allowed but not required — an unchanged
/// query qualifies too, and continuing it is exactly right: its filter trivially
/// keeps every seed the run delivered and the scan resumes where it stopped. A
/// search session therefore survives until the user explicitly clears it.
/// The continuation rule itself belongs to the engine and is asked of it, since
/// soundness of the resumed scan depends on the two agreeing exactly.
/// </summary>
public static class QueryRefinement
{
    /// <summary>
    /// True when every requirement of <paramref name="baseline"/> is covered by
    /// a distinct requirement of <paramref name="candidate"/> at least as strict
    /// (equal or strengthened) under a scope the candidate never widens.
    /// Deliberately not strict: an equal query is a continuation, not a rescan.
    /// The engine decides — this encodes both queries and asks
    /// <c>seedfinder_query_continues</c>, so refine eligibility here is the very
    /// predicate the resumed scan relies on and cannot drift from it.
    /// </summary>
    public static bool CanRefine(QuerySettings candidate, QuerySettings baseline) =>
        NativeEngine.QueryContinues(candidate, baseline);

}

/// <summary>What pressing Start Search does with a query, per docs/search-semantics.md.</summary>
public enum StartMode
{
    /// <summary>Fresh full-range scan that establishes the Target on conclusion.</summary>
    Anchor,
    /// <summary>Filter the Target Set, then resume the target's uncovered remainder.</summary>
    TargetRefine,
    /// <summary>Filter the Target Set only; coverage and set stay untouched.</summary>
    TargetFilter,
    /// <summary>Continue the previous detached scan (filter its results, resume its remainder).</summary>
    ContinueDetached,
    /// <summary>Fresh full-range scan that leaves the Target untouched.</summary>
    Detached,
}

/// <summary>
/// The session's anchor: established by the first concluded search (or an
/// import) and reset only by Clear Results. <see cref="Seeds"/> is uncapped and
/// a superset of any related run's display, which is what lets a loosened query
/// bring seeds back. <see cref="Remaining"/> is zero for imports, whose refines
/// are filter-only.
/// </summary>
public sealed record TargetRun(QuerySettings Query, IReadOnlyList<string> Seeds, long ResumeFrom, long Remaining);

public sealed class QueryPreset
{
    public string Id { get; set; } = Guid.NewGuid().ToString();
    public string Name { get; set; } = "";
    public QuerySettings Query { get; set; } = new();
    [JsonIgnore] public bool IsBuiltIn { get; set; }
}

public static class BuiltInPresets
{
    /// <summary>
    /// The floor limit the vault presets carry: floor 19 is the last floor the
    /// Imp — and so the vault holding its levelled prizes — can appear on, so a
    /// deeper scan only costs time.
    /// </summary>
    public const int VaultFloorLimit = 19;

    public static IReadOnlyList<QueryPreset> All { get; } = [
        new()
        {
            Id = "staff-21", Name = "+21 Staff", IsBuiltIn = true,
            Query = new QuerySettings { Requirements = [
                new() { Kind = ItemKind.Wand, Upgrade = 3, UpgradeMatch = UpgradeMatch.Exactly, IdentityGroup = 1 },
                new() { Kind = ItemKind.Wand, UpgradeMatch = UpgradeMatch.Any, IdentityGroup = 1 },
                new() { Kind = ItemKind.Wand, UpgradeMatch = UpgradeMatch.Any, IdentityGroup = 1 },
                new() { Kind = ItemKind.Wand, Upgrade = 1, UpgradeMatch = UpgradeMatch.AtLeast },
            ] },
        },
        // The same stack anchored one level higher, on the +4 wand v4.0.0's Imp
        // vault lays out among its prizes.
        new()
        {
            Id = "staff-22", Name = "+22 Staff", IsBuiltIn = true,
            Query = new QuerySettings { MaximumDepth = VaultFloorLimit, Requirements = [
                new() { Kind = ItemKind.Wand, Upgrade = 4, UpgradeMatch = UpgradeMatch.Exactly, IdentityGroup = 1 },
                new() { Kind = ItemKind.Wand, UpgradeMatch = UpgradeMatch.Any, IdentityGroup = 1 },
                new() { Kind = ItemKind.Wand, UpgradeMatch = UpgradeMatch.Any, IdentityGroup = 1 },
                new() { Kind = ItemKind.Wand, Upgrade = 1, UpgradeMatch = UpgradeMatch.AtLeast },
            ] },
        },
        new()
        {
            Id = "wand-bonanza", Name = "Wand Bonanza", IsBuiltIn = true,
            Query = new QuerySettings { Requirements = [
                new() { Kind = ItemKind.Wand, Upgrade = 3, UpgradeMatch = UpgradeMatch.Exactly },
                new() { Kind = ItemKind.Wand, Upgrade = 2, UpgradeMatch = UpgradeMatch.Exactly, MaximumDepth = 4 },
                new() { Kind = ItemKind.Wand, Upgrade = 2, UpgradeMatch = UpgradeMatch.Exactly, MaximumDepth = 4 },
                new() { Kind = ItemKind.Wand, Upgrade = 2, UpgradeMatch = UpgradeMatch.Exactly },
            ] },
        },
        new()
        {
            Id = "ring-of-wealth-21", Name = "+21 Ring of Wealth", IsBuiltIn = true,
            Query = new QuerySettings { Requirements = [
                new() { Kind = ItemKind.Ring, Item = ItemCatalog.Find("ring_wealth"), Upgrade = 4, UpgradeMatch = UpgradeMatch.Exactly, Source = ScoutItemSource.ImpReward },
                new() { Kind = ItemKind.Ring, Item = ItemCatalog.Find("ring_wealth"), Upgrade = 2, UpgradeMatch = UpgradeMatch.Exactly },
                new() { Kind = ItemKind.Ring, Item = ItemCatalog.Find("ring_wealth"), UpgradeMatch = UpgradeMatch.Any },
            ] },
        },
        // A tier-4 weapon at the +5 only the vault reaches, with two more of the
        // same weapon to pour into it.
        new()
        {
            Id = "tier-4-weapon-26", Name = "+26 Tier 4 Weapon", IsBuiltIn = true,
            Query = new QuerySettings { MaximumDepth = VaultFloorLimit, Requirements = [
                new() { Kind = ItemKind.Weapon, Tier = 4, TierMatch = TierMatch.Exactly, Upgrade = 5, UpgradeMatch = UpgradeMatch.Exactly, IdentityGroup = 1 },
                new() { Kind = ItemKind.Weapon, UpgradeMatch = UpgradeMatch.Any, IdentityGroup = 1 },
                new() { Kind = ItemKind.Weapon, UpgradeMatch = UpgradeMatch.Any, IdentityGroup = 1 },
            ] },
        },
    ];
}

public sealed record SeedResult(string Seed, int Number);
public sealed record ScoutItem(CatalogItem Item, int Depth, int Upgrade, string? Effect, bool Cursed,
    ScoutItemSource Source, byte AccessibilityTag, int AccessibilityGroup, ulong AccessibilityValue,
    bool Secret = false);
public sealed record ScoutWorld(string Seed, IReadOnlyList<ScoutQuest> Quests, IReadOnlyList<ScoutItem> Items);
public sealed record SearchStatus(SearchState State, long Scanned, long Total, long ErrorCode, double Probability);

/// <summary>
/// The engine's marks for one scouted world: which items satisfy the query,
/// as indices into the scout manifest, and how many of the requirements that
/// selection explains. Produced by <see cref="NativeEngine.ScoutMatches"/>.
/// </summary>
public sealed record ScoutMatches(IReadOnlySet<int> Matched, int MatchedRequirements, int TotalRequirements);

public static class ItemCatalog
{
    private sealed class Root { public Entry[] Entries { get; set; } = []; public EffectTables Modifiers { get; set; } = new(); }
    private sealed class Entry { public string Id { get; set; } = ""; public string Name { get; set; } = ""; public string Type { get; set; } = ""; public string? Class { get; set; } public int? Tier { get; set; } public int Sprite { get; set; } }
    /// <summary>The upstream effect names, exactly as the shared catalog lists them.</summary>
    private sealed class EffectTables { public string[] WeaponEnchantments { get; set; } = []; public string[] WeaponCurses { get; set; } = []; public string[] ArmorGlyphs { get; set; } = []; public string[] ArmorCurses { get; set; } = []; }
    private static readonly Root Catalog = Load();
    public static IReadOnlyList<CatalogItem> All { get; } = Catalog.Entries.Select(e => new CatalogItem(e.Id, e.Name, Enum.Parse<ItemKind>(e.Type, true), e.Sprite, e.Tier,
        string.IsNullOrEmpty(e.Class) ? null : Enum.Parse<WeaponClass>(e.Class, true))).ToArray();
    // The four effect tables come from the same asset as the items, so a
    // catalog bump carries them and no hand-typed list can fall behind it.
    public static IReadOnlyList<string> Enchantments => Catalog.Modifiers.WeaponEnchantments;
    public static IReadOnlyList<string> WeaponCurses => Catalog.Modifiers.WeaponCurses;
    public static IReadOnlyList<string> Glyphs => Catalog.Modifiers.ArmorGlyphs;
    public static IReadOnlyList<string> ArmorCurses => Catalog.Modifiers.ArmorCurses;
    private static Root Load() =>
        JsonSerializer.Deserialize<Root>(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Assets", "catalog-v4.0.0.json")), new JsonSerializerOptions { PropertyNameCaseInsensitive = true })!;
    /// <summary>
    /// The items offered when picking one fresh. Tier-1 items are hidden: they
    /// are the starting gear, never worth searching for.
    /// </summary>
    public static IEnumerable<CatalogItem> For(ItemKind kind) => All.Where(x => kind.Accepts(x) && x.Tier != 1);

    /// <summary>
    /// The items a requirement editor lists for <paramref name="kind"/>: the
    /// fresh-pick list, plus <paramref name="current"/> when the requirement
    /// being edited already names an item that list hides. Imports and share
    /// links resolve items through the whole catalog, so a requirement can name
    /// a tier-1 item the picker would otherwise be unable to show — and saving
    /// it unchanged would silently swap it for whichever item took its slot.
    /// The order stays the catalog's.
    /// </summary>
    public static IReadOnlyList<CatalogItem> EditorItems(ItemKind kind, CatalogItem? current) =>
        [.. All.Where(x => kind.Accepts(x) && (x.Tier != 1 || x.Id == current?.Id))];
    public static CatalogItem? Find(string id) => All.FirstOrDefault(x => x.Id == id);
    public static IEnumerable<string> Modifiers(ItemKind kind) => kind.Family() switch { ItemKind.Weapon => Enchantments.Concat(WeaponCurses), ItemKind.Armor => Glyphs.Concat(ArmorCurses), _ => [] };
    /// <summary>The family's non-curse effects: what "any enchantment" stands for.</summary>
    public static IReadOnlyList<string> EnchantmentsOf(ItemKind kind) => kind.Family() switch { ItemKind.Weapon => Enchantments, ItemKind.Armor => Glyphs, _ => [] };
    /// <summary>The family's curse-type effects.</summary>
    public static IReadOnlyList<string> CursesOf(ItemKind kind) => kind.Family() switch { ItemKind.Weapon => WeaponCurses, ItemKind.Armor => ArmorCurses, _ => [] };
    public static bool IsCurse(ItemKind kind, string effect) => (kind.Family() == ItemKind.Weapon ? WeaponCurses : ArmorCurses).Contains(effect);
}
