using System.Collections.ObjectModel;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace SeedSeeker;

// This file must stay free of Windows App SDK types: SeedSeeker.Tests links it
// to run on any host. Members that need XAML types live in the partial halves
// in Models.Presentation.cs.

// MeleeWeapon and ThrownWeapon narrow a weapon requirement to one weapon
// class; the enum value doubles as the SSF8 wire kind ID (0..=5), so they
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

    /// <summary>
    /// The highest upgrade a requirement of this kind may demand. Rings reach
    /// one higher than everything else; both bounds are the engine's own, so
    /// the editor can never offer an upgrade a search would reject.
    /// </summary>
    public static int MaximumSearchUpgrade(this ItemKind kind) =>
        kind == ItemKind.Ring ? EngineInfo.MaxUpgradeRing : EngineInfo.MaxUpgradeDefault;
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
    /// <summary>
    /// A challenge's display name from the engine's document name: the words
    /// it already carries, with only the first capitalised ("faith_is_my_armor"
    /// → "Faith is my armor").
    /// </summary>
    public static string Challenge(string documentName) =>
        documentName.Length == 0 ? documentName
        : char.ToUpperInvariant(documentName[0]) + documentName.Replace('_', ' ')[1..];

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
    public static IReadOnlyList<int> EmptyBossFloors => EngineInfo.EmptyBossFloors;

    /// <summary>Floors offered by floor-limit selectors: the searchable range minus the empty boss floors.</summary>
    public static readonly int[] Options =
        [.. Enumerable.Range(1, EngineInfo.MaxDepth).Where(f => !EngineInfo.EmptyBossFloors.Contains(f))];

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
/// handed in. The value doubles as the SSF8 wire id, with 0 meaning "any".
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
    public int MaximumDepth { get; set; } = 24;
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
        JsonSerializer.Deserialize<Root>(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Assets", "catalog-v3.3.8.json")), new JsonSerializerOptions { PropertyNameCaseInsensitive = true })!;
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
    public static bool IsCurse(ItemKind kind, string effect) => (kind.Family() == ItemKind.Weapon ? WeaponCurses : ArmorCurses).Contains(effect);
}
