using System.Collections.ObjectModel;
using System.Text.Json;
using System.Text.Json.Serialization;
using Microsoft.UI;
using Microsoft.UI.Xaml.Media;

namespace SeedSeeker;

public enum ItemKind { Weapon, Armor, Wand, Ring }
public enum UpgradeMatch { Any, Exactly, AtLeast }
public enum TierMatch { Any, Exactly, AtLeast }
public enum SearchState { Running, Completed, Cancelled, Failed }

public sealed record CatalogItem(string Id, string Name, ItemKind Kind, int SpriteIndex, int? Tier);

public enum ScoutItemSource
{
    Heap, Chest, LockedChest, CrystalChest, Tomb, Skeleton, SacrificialFire, Mimic,
    GoldenMimic, CrystalMimic, Statue, ArmoredStatue, Shop, GhostReward,
    WandmakerReward, BlacksmithReward, ImpReward
}

public static class KindStyle
{
    public static string Glyph(ItemKind kind) => kind switch { ItemKind.Weapon => "", ItemKind.Armor => "", ItemKind.Wand => "", _ => "" };
    public static Brush Tint(ItemKind kind) => new SolidColorBrush(kind switch { ItemKind.Weapon => Colors.DarkOrange, ItemKind.Armor => Colors.DodgerBlue, ItemKind.Wand => Colors.MediumPurple, _ => Colors.Goldenrod });
}

public static class Labels
{
    public static string Kind(ItemKind value) => value switch { ItemKind.Weapon => "Weapons", ItemKind.Armor => "Armor", ItemKind.Wand => "Wands", _ => "Rings" };
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

public sealed class ItemRequirement
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
    [JsonIgnore] public string Glyph => KindStyle.Glyph(Kind);
    [JsonIgnore] public Brush Tint => KindStyle.Tint(Kind);
    [JsonIgnore] public string Title => Item?.Name ?? (TierMatch switch { TierMatch.Exactly => $"Any Tier {Tier} {Labels.Singular(Kind)}", TierMatch.AtLeast => $"Any Tier {Tier}+ {Labels.Singular(Kind)}", _ => $"Any {Labels.Singular(Kind)}" });
    [JsonIgnore] public string Description
    {
        get
        {
            var parts = new List<string> { UpgradeMatch switch { UpgradeMatch.Exactly => $"+{Upgrade} exactly", UpgradeMatch.AtLeast => $"+{Upgrade} or higher", _ => "Any upgrade" } };
            if (Modifier is not null) parts.Add(Modifier); if (Source is not null) parts.Add(Labels.Source(Source.Value));
            if (IdentityGroup is int g) parts.Add($"same item group {(char)(64 + g)}"); if (MaximumDepth is int d) parts.Add($"by floor {d}");
            return string.Join(" • ", parts);
        }
    }
    public ItemRequirement Clone() => (ItemRequirement)MemberwiseClone();
}

public sealed class QuerySettings
{
    public ObservableCollection<ItemRequirement> Requirements { get; set; } = [];
    public int MaximumDepth { get; set; } = 24;
    public bool RequireBlacksmith { get; set; }
    public bool ExcludeBlacksmithRewards { get; set; }
    public bool FastMode { get; set; }
    public int Challenges { get; set; }
}

public sealed record SeedResult(string Seed, int Number);
public sealed record ScoutItem(CatalogItem Item, int Depth, int Upgrade, string? Effect, bool Cursed,
    ScoutItemSource Source, byte AccessibilityTag, int AccessibilityGroup, ulong AccessibilityValue);
public sealed record ScoutWorld(string Seed, IReadOnlyList<ScoutItem> Items);
public sealed record SearchStatus(SearchState State, long Scanned, long Total, long ErrorCode, double Probability);

public static class ItemCatalog
{
    private sealed class Root { public Entry[] Entries { get; set; } = []; }
    private sealed class Entry { public string Id { get; set; } = ""; public string Name { get; set; } = ""; public string Type { get; set; } = ""; public int? Tier { get; set; } public int Sprite { get; set; } }
    public static IReadOnlyList<CatalogItem> All { get; } = Load();
    public static readonly string[] Enchantments = ["Blazing", "Blocking", "Blooming", "Chilling", "Corrupting", "Elastic", "Grim", "Kinetic", "Lucky", "Projecting", "Shocking", "Unstable", "Vampiric"];
    public static readonly string[] WeaponCurses = ["Annoying", "Dazzling", "Displacing", "Explosive", "Friendly", "Polarized", "Sacrificial", "Wayward"];
    public static readonly string[] Glyphs = ["Affection", "Anti-Magic", "Brimstone", "Camouflage", "Entanglement", "Flow", "Obfuscation", "Potential", "Repulsion", "Stone", "Swiftness", "Thorns", "Viscosity"];
    public static readonly string[] ArmorCurses = ["Anti-Entropy", "Bulk", "Corrosion", "Displacement", "Metabolism", "Multiplicity", "Overgrowth", "Stench"];
    private static IReadOnlyList<CatalogItem> Load()
    {
        var root = JsonSerializer.Deserialize<Root>(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Assets", "catalog-v3.3.8.json")), new JsonSerializerOptions { PropertyNameCaseInsensitive = true })!;
        return root.Entries.Select(e => new CatalogItem(e.Id, e.Name, Enum.Parse<ItemKind>(e.Type, true), e.Sprite, e.Tier)).ToArray();
    }
    public static IEnumerable<CatalogItem> For(ItemKind kind) => All.Where(x => x.Kind == kind && x.Tier != 1);
    public static CatalogItem? Find(string id) => All.FirstOrDefault(x => x.Id == id);
    public static IEnumerable<string> Modifiers(ItemKind kind) => kind switch { ItemKind.Weapon => Enchantments.Concat(WeaponCurses), ItemKind.Armor => Glyphs.Concat(ArmorCurses), _ => [] };
    public static bool IsCurse(ItemKind kind, string effect) => (kind == ItemKind.Weapon ? WeaponCurses : ArmorCurses).Contains(effect);
}
