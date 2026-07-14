using System.Collections.ObjectModel;
using System.Text.Json;
using System.Text.Json.Serialization;
using Microsoft.UI;
using Microsoft.UI.Xaml.Media;

namespace SeedSeeker;

public enum ItemKind { Weapon, Armor, Wand, Ring }
public enum UpgradeMatch { Any, Exactly, AtLeast }
public enum TierMatch { Any, Exactly, AtLeast, AtMost }
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
    public bool RequireUncursed { get; set; }
    [JsonIgnore] public string Glyph => KindStyle.Glyph(Kind);
    [JsonIgnore] public Brush Tint => KindStyle.Tint(Kind);
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
                && requirement.Kind == item.Item.Kind
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
