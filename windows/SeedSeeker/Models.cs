using System.Collections.ObjectModel;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace SeedSeeker;

// This file must stay free of Windows App SDK types: SeedSeeker.Tests links it
// to run on any host. Members that need XAML types live in the partial halves
// in Models.Presentation.cs.

// MeleeWeapon and ThrownWeapon narrow a weapon requirement to one weapon
// class; the enum value doubles as the SSF7 wire kind ID (0..=5), so they
// must stay appended after the original four families.
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
}
public enum UpgradeMatch { Any, Exactly, AtLeast }
public enum TierMatch { Any, Exactly, AtLeast, AtMost }
public enum SearchState { Running, Completed, Cancelled, Failed }

public sealed record CatalogItem(string Id, string Name, ItemKind Kind, int SpriteIndex, int? Tier, WeaponClass? Class = null);

public enum ScoutItemSource
{
    Heap, Chest, LockedChest, CrystalChest, Tomb, Skeleton, SacrificialFire, Mimic,
    GoldenMimic, CrystalMimic, Statue, ArmoredStatue, Shop, GhostReward,
    WandmakerReward, BlacksmithReward, ImpReward
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
        _ => string.Concat(value.ToString().Select((c, i) => i > 0 && char.IsUpper(c) ? " " + char.ToLowerInvariant(c) : char.ToLowerInvariant(c).ToString()))
    };
}

public sealed partial class ItemRequirement
{
    public long Key { get; set; } = Random.Shared.NextInt64(1, long.MaxValue);
    public CatalogItem? Item { get; set; }
    public int Upgrade { get; set; }
    public string? Modifier { get; set; }
    public ItemKind Kind { get; set; }
    public int Tier { get; set; }
    public TierMatch TierMatch { get; set; }
    public UpgradeMatch UpgradeMatch { get; set; }
    public ScoutItemSource? Source { get; set; }
    public int? IdentityGroup { get; set; }
    public int? MaximumDepth { get; set; }
    public bool RequireUncursed { get; set; }
    [JsonIgnore] public string Glyph => KindStyle.Glyph(Kind);
    /// <summary>Row-major index into the upstream item atlas, or -1 for a wildcard.</summary>
    [JsonIgnore] public int SpriteIndex => Item?.SpriteIndex ?? -1;
    [JsonIgnore] public string Title => Item?.Name ?? (TierMatch switch { TierMatch.Exactly => $"Any Tier {Tier} {Labels.Singular(Kind)}", TierMatch.AtLeast => $"Any Tier {Tier}+ {Labels.Singular(Kind)}", TierMatch.AtMost => $"Any Tier {Tier} or lower {Labels.Singular(Kind)}", _ => $"Any {Labels.Singular(Kind)}" });
    [JsonIgnore] public string Description
    {
        get
        {
            var parts = new List<string> { UpgradeMatch switch { UpgradeMatch.Exactly => $"+{Upgrade} exactly", UpgradeMatch.AtLeast => $"+{Upgrade} or higher", _ => "Any upgrade" } };
            if (Modifier is not null) parts.Add(Modifier); if (RequireUncursed) parts.Add("uncursed"); if (Source is not null) parts.Add(Labels.Source(Source.Value));
            if (IdentityGroup is int g) parts.Add($"same item group {(char)(64 + g)}"); if (MaximumDepth is int d) parts.Add($"by floor {d}");
            return string.Join(" • ", parts);
        }
    }
    public ItemRequirement Clone() => (ItemRequirement)MemberwiseClone();
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

    /// <summary>Floors offered by floor-limit selectors: 1..24 minus the empty boss floors.</summary>
    public static readonly int[] Options = Enumerable.Range(1, 24).Where(f => !EmptyBossFloors.Contains(f)).ToArray();

    /// <summary>Snaps an empty boss-floor limit to the equivalent floor below it (5→4, 10→9, 15→14).</summary>
    public static int Normalize(int depth) => EmptyBossFloors.Contains(depth) ? depth - 1 : depth;

    /// <summary>The slider index for a floor limit; off-list values snap to the floor below.</summary>
    public static int IndexOf(int depth) => Math.Max(0, Array.IndexOf(Options, Normalize(depth)));
}

public sealed class QuerySettings
{
    public ObservableCollection<ItemRequirement> Requirements { get; set; } = [];
    public int MaximumDepth { get; set; } = 24;
    public bool RequireBlacksmith { get; set; }
    public bool ExcludeBlacksmithRewards { get; set; }
    public bool FastMode { get; set; }
    public int Challenges { get; set; }

    public QuerySettings Clone() => new()
    {
        Requirements = new ObservableCollection<ItemRequirement>(Requirements.Select(x => x.Clone())),
        MaximumDepth = MaximumDepth,
        RequireBlacksmith = RequireBlacksmith,
        ExcludeBlacksmithRewards = ExcludeBlacksmithRewards,
        FastMode = FastMode,
        Challenges = Challenges,
    };
}

/// <summary>
/// Decides whether a query can continue a finished run instead of rescanning it:
/// identical scope, and every baseline requirement still present (counting
/// duplicates). Extra requirements are allowed but not required — an unchanged
/// query qualifies too, and continuing it is exactly right: its filter trivially
/// keeps every seed the run delivered and the scan resumes where it stopped. A
/// search session therefore survives until the user explicitly clears it.
/// The continuation rule itself belongs to the engine and is asked of it, since
/// soundness of the resumed scan depends on the two agreeing exactly.
/// <see cref="SharesRequirement"/> stays local by contrast: it gates nothing but
/// a re-verifying filter, so it is a UI heuristic rather than a soundness rule.
/// </summary>
public static class QueryRefinement
{
    /// <summary>
    /// True when every requirement of <paramref name="baseline"/> is covered by
    /// a distinct requirement of <paramref name="candidate"/> at least as strict
    /// (equal or strengthened) under an identical scope.
    /// Deliberately not strict: an equal query is a continuation, not a rescan.
    /// The engine decides — this encodes both queries and asks
    /// <c>seedfinder_query_continues</c>, so refine eligibility here is the very
    /// predicate the resumed scan relies on and cannot drift from it.
    /// </summary>
    public static bool CanRefine(QuerySettings candidate, QuerySettings baseline) =>
        NativeEngine.QueryContinues(candidate, baseline);

    /// <summary>
    /// Whether two queries name a common item: some requirement of each has the
    /// same kind, and either both name the same item or at least one names none
    /// (a kind-level requirement subsumes every item of its kind). Scope and
    /// challenge differences are irrelevant — a filter re-verifies seeds from
    /// scratch — so this deliberately checks nothing else: it only estimates
    /// whether the Target Set is enriched for the candidate query's matches.
    /// </summary>
    public static bool SharesRequirement(QuerySettings candidate, QuerySettings baseline) =>
        candidate.Requirements.Any(left => baseline.Requirements.Any(right =>
            left.Kind == right.Kind
            && (left.Item is null || right.Item is null || left.Item.Id == right.Item.Id)));
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

/// <summary>
/// The single gate for what Start Search does (docs/search-semantics.md). The
/// Target Set is the anchor: a continuation of the Target Query refines it, a
/// query sharing an item filters it (always from the full set, so loosening a
/// requirement brings seeds back), and anything else scans the full range
/// without touching it — continuing the previous detached scan when that is
/// sound. An empty Target Set holds nothing worth preserving, so a
/// non-continuing query re-anchors on this search instead of filtering nothing.
/// </summary>
public static class SearchPlan
{
    /// <param name="query">The query about to run.</param>
    /// <param name="target">The session's Target, if one has been established.</param>
    /// <param name="lastDetachedQuery">The query of the previous run when that
    /// run was a detached scan that concluded (completed or cancelled), null
    /// otherwise. Only such a run may be continued by a query unrelated to the
    /// Target; a failed run is never a continuation base.</param>
    public static StartMode DecideStart(QuerySettings query, TargetRun? target, QuerySettings? lastDetachedQuery = null)
    {
        if (target is null) return StartMode.Anchor;
        var continuesTarget = QueryRefinement.CanRefine(query, target.Query);
        if (target.Seeds.Count == 0)
            return continuesTarget && target.Remaining > 0 ? StartMode.TargetRefine : StartMode.Anchor;
        if (continuesTarget) return StartMode.TargetRefine;
        if (QueryRefinement.SharesRequirement(query, target.Query)) return StartMode.TargetFilter;
        if (lastDetachedQuery is not null && QueryRefinement.CanRefine(query, lastDetachedQuery)) return StartMode.ContinueDetached;
        return StartMode.Detached;
    }
}

public sealed class QueryPreset
{
    public string Id { get; set; } = Guid.NewGuid().ToString();
    public string Name { get; set; } = "";
    public QuerySettings Query { get; set; } = new();
    [JsonIgnore] public bool IsBuiltIn { get; set; }
}

public static class BuiltInPresets
{
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
    ];
}

public sealed record SeedResult(string Seed, int Number);
public sealed record ScoutItem(CatalogItem Item, int Depth, int Upgrade, string? Effect, bool Cursed,
    ScoutItemSource Source, byte AccessibilityTag, int AccessibilityGroup, ulong AccessibilityValue,
    bool Secret = false);
public sealed record ScoutWorld(string Seed, IReadOnlyList<ScoutQuest> Quests, IReadOnlyList<ScoutItem> Items);
public sealed record SearchStatus(SearchState State, long Scanned, long Total, long ErrorCode, double Probability);

public static class ScoutMatcher
{
    public static HashSet<int> SelectMatches(IReadOnlyList<ScoutItem> items,
        IEnumerable<ItemRequirement> requirements, int maximumDepth = 24,
        bool excludeBlacksmithRewards = false)
    {
        bool Matches(ScoutItem item, ItemRequirement requirement)
        {
            var tierMatches = requirement.TierMatch switch
            {
                TierMatch.Any => true,
                TierMatch.Exactly => item.Item.Tier == requirement.Tier,
                TierMatch.AtLeast => item.Item.Tier >= requirement.Tier,
                TierMatch.AtMost => item.Item.Tier <= requirement.Tier,
                _ => false,
            };
            var upgradeMatches = requirement.UpgradeMatch switch
            {
                UpgradeMatch.Any => true,
                UpgradeMatch.Exactly => item.Upgrade == requirement.Upgrade,
                UpgradeMatch.AtLeast => item.Upgrade >= requirement.Upgrade,
                _ => false,
            };
            return item.Depth <= maximumDepth
                && item.Depth <= (requirement.MaximumDepth ?? maximumDepth)
                && (!excludeBlacksmithRewards || item.Source != ScoutItemSource.BlacksmithReward)
                && requirement.Kind.Accepts(item.Item)
                && (requirement.Item is null || requirement.Item.Id == item.Item.Id)
                && tierMatches && upgradeMatches
                && (requirement.Modifier is null || requirement.Modifier == item.Effect)
                && (!requirement.RequireUncursed || !item.Cursed)
                && (requirement.Source is null || requirement.Source == item.Source);
        }

        var candidates = requirements
            .Select(requirement => (Requirement: requirement, Items: Enumerable.Range(0, items.Count)
                .Where(index => Matches(items[index], requirement)).ToArray()))
            .OrderBy(candidate => candidate.Items.Length).ToArray();
        var used = new HashSet<int>();
        var selected = new HashSet<int>();
        var best = new HashSet<int>();
        var scenarios = new Dictionary<int, ulong>();
        var identities = new Dictionary<int, string>();

        void Visit(int position)
        {
            if (position == candidates.Length)
            {
                if (selected.Count > best.Count) best = [.. selected];
                return;
            }
            if (selected.Count + candidates.Length - position <= best.Count) return;
            var (requirement, itemCandidates) = candidates[position];
            foreach (var index in itemCandidates)
            {
                if (used.Contains(index)) continue;
                var item = items[index];
                string? previousIdentity = null;
                if (requirement.IdentityGroup is int identityGroup)
                {
                    identities.TryGetValue(identityGroup, out previousIdentity);
                    if (previousIdentity is not null && previousIdentity != item.Item.Id) continue;
                    identities[identityGroup] = item.Item.Id;
                }
                (int Group, ulong Mask)? constraint = item.AccessibilityTag switch
                {
                    1 => (item.AccessibilityGroup, 1UL << (int)item.AccessibilityValue),
                    2 => (item.AccessibilityGroup, item.AccessibilityValue),
                    _ => null,
                };
                ulong? previousScenarios = null;
                if (constraint is { } value)
                {
                    if (scenarios.TryGetValue(value.Group, out var previous)) previousScenarios = previous;
                    var compatible = (previousScenarios ?? ulong.MaxValue) & value.Mask;
                    if (compatible == 0)
                    {
                        RestoreIdentity(requirement, previousIdentity);
                        continue;
                    }
                    scenarios[value.Group] = compatible;
                }
                used.Add(index); selected.Add(index);
                Visit(position + 1);
                used.Remove(index); selected.Remove(index);
                if (constraint is { } oldConstraint)
                {
                    if (previousScenarios is ulong previous) scenarios[oldConstraint.Group] = previous;
                    else scenarios.Remove(oldConstraint.Group);
                }
                RestoreIdentity(requirement, previousIdentity);
            }
            Visit(position + 1);
        }

        void RestoreIdentity(ItemRequirement requirement, string? previous)
        {
            if (requirement.IdentityGroup is not int group) return;
            if (previous is null) identities.Remove(group); else identities[group] = previous;
        }

        Visit(0);
        return best;
    }
}

public static class ItemCatalog
{
    private sealed class Root { public Entry[] Entries { get; set; } = []; }
    private sealed class Entry { public string Id { get; set; } = ""; public string Name { get; set; } = ""; public string Type { get; set; } = ""; public string? Class { get; set; } public int? Tier { get; set; } public int Sprite { get; set; } }
    public static IReadOnlyList<CatalogItem> All { get; } = Load();
    public static readonly string[] Enchantments = ["Blazing", "Blocking", "Blooming", "Chilling", "Corrupting", "Elastic", "Grim", "Kinetic", "Lucky", "Projecting", "Shocking", "Unstable", "Vampiric"];
    public static readonly string[] WeaponCurses = ["Annoying", "Dazzling", "Displacing", "Explosive", "Friendly", "Polarized", "Sacrificial", "Wayward"];
    public static readonly string[] Glyphs = ["Affection", "Anti-Magic", "Brimstone", "Camouflage", "Entanglement", "Flow", "Obfuscation", "Potential", "Repulsion", "Stone", "Swiftness", "Thorns", "Viscosity"];
    public static readonly string[] ArmorCurses = ["Anti-Entropy", "Bulk", "Corrosion", "Displacement", "Metabolism", "Multiplicity", "Overgrowth", "Stench"];
    private static IReadOnlyList<CatalogItem> Load()
    {
        var root = JsonSerializer.Deserialize<Root>(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Assets", "catalog-v3.3.8.json")), new JsonSerializerOptions { PropertyNameCaseInsensitive = true })!;
        return root.Entries.Select(e => new CatalogItem(e.Id, e.Name, Enum.Parse<ItemKind>(e.Type, true), e.Sprite, e.Tier,
            string.IsNullOrEmpty(e.Class) ? null : Enum.Parse<WeaponClass>(e.Class, true))).ToArray();
    }
    public static IEnumerable<CatalogItem> For(ItemKind kind) => All.Where(x => kind.Accepts(x) && x.Tier != 1);
    public static CatalogItem? Find(string id) => All.FirstOrDefault(x => x.Id == id);
    public static IEnumerable<string> Modifiers(ItemKind kind) => kind.Family() switch { ItemKind.Weapon => Enchantments.Concat(WeaponCurses), ItemKind.Armor => Glyphs.Concat(ArmorCurses), _ => [] };
    public static bool IsCurse(ItemKind kind, string effect) => (kind.Family() == ItemKind.Weapon ? WeaponCurses : ArmorCurses).Contains(effect);
}
