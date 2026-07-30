using System.Collections.ObjectModel;
using System.Text.Json;
using System.Text.Json.Serialization;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;

namespace SeedSeeker;

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
public enum EffectMode { Any, AnyEnchantment, Specific }
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
public static class KindStyle
{
    public static string Glyph(ItemKind kind) => kind.Family() switch { ItemKind.Weapon => "", ItemKind.Armor => "", ItemKind.Wand => "", _ => "" };
    public static Brush Tint(ItemKind kind) => new SolidColorBrush(kind.Family() switch { ItemKind.Weapon => Colors.DarkOrange, ItemKind.Armor => Colors.DodgerBlue, ItemKind.Wand => Colors.MediumPurple, _ => Colors.Goldenrod });
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

public sealed class ItemRequirement
{
    public long Key { get; set; } = Random.Shared.NextInt64(1, long.MaxValue);
    public CatalogItem? Item { get; set; }
    public int Upgrade { get; set; }
    /// <summary>
    /// Legacy pre-0.7 single-effect field. Never written any more (the getter is
    /// always null and nulls are skipped); reading an old save maps it onto a
    /// one-element specific effect set.
    /// </summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Modifier { get => null; set { if (value is not null) { EffectMode = EffectMode.Specific; Effects = [value]; } } }
    public EffectMode EffectMode { get; set; }
    public List<string> Effects { get; set; } = [];
    public ItemKind Kind { get; set; }
    public int Tier { get; set; }
    public TierMatch TierMatch { get; set; }
    public UpgradeMatch UpgradeMatch { get; set; }
    public ScoutItemSource? Source { get; set; }
    public int? IdentityGroup { get; set; }
    public int? MaximumDepth { get; set; }
    public bool RequireUncursed { get; set; }
    /// <summary>Requirements sharing a non-zero group are alternatives: one match satisfies them all.</summary>
    public int? AlternativeGroup { get; set; }
    /// <summary>Combined-upgrade group label shared with other requirements, or null.</summary>
    public int? UpgradeSumGroup { get; set; }
    /// <summary>Minimum combined upgrade total agreed by every member of the sum group.</summary>
    public int? UpgradeSumTotal { get; set; }
    [JsonIgnore] public string Glyph => KindStyle.Glyph(Kind);
    [JsonIgnore] public Brush Tint => KindStyle.Tint(Kind);
    /// <summary>Row-major index into the upstream item atlas, or -1 for a wildcard.</summary>
    [JsonIgnore] public int SpriteIndex => Item?.SpriteIndex ?? -1;
    [JsonIgnore] public Visibility SpriteVisibility => Item is null ? Visibility.Collapsed : Visibility.Visible;
    /// <summary>The generic glyph shows only where there is genuinely no concrete item.</summary>
    [JsonIgnore] public Visibility FallbackVisibility => Item is null ? Visibility.Visible : Visibility.Collapsed;
    /// <summary>
    /// Glow for the pinned enchantment or curse, with the bare-effect-name semantics
    /// of the web's <c>effectGlow</c>: an unrecognised effect is a curse and glows
    /// black. Only a single specific effect is unambiguous enough to glow as, and
    /// there is nothing to tint without a sprite, so wildcards never glow.
    /// </summary>
    private string? GlowEffect => EffectMode == EffectMode.Specific && Effects.Count == 1 ? Effects[0] : null;
    [JsonIgnore] public Windows.UI.Color GlowColor => ItemGlow.ForEffect(GlowEffect)?.Color ?? default;
    [JsonIgnore] public double GlowPeriod => Item is null ? 0 : ItemGlow.ForEffect(GlowEffect)?.Period ?? 0;
    [JsonIgnore] public string Title => Item?.Name ?? (TierMatch switch { TierMatch.Exactly => $"Any Tier {Tier} {Labels.Singular(Kind)}", TierMatch.AtLeast => $"Any Tier {Tier}+ {Labels.Singular(Kind)}", TierMatch.AtMost => $"Any Tier {Tier} or lower {Labels.Singular(Kind)}", _ => $"Any {Labels.Singular(Kind)}" });
    [JsonIgnore] public string Description
    {
        get
        {
            var parts = new List<string> { UpgradeMatch switch { UpgradeMatch.Exactly => $"+{Upgrade} exactly", UpgradeMatch.AtLeast => $"+{Upgrade} or higher", _ => "Any upgrade" } };
            if (EffectSummary is string effect) parts.Add(effect); if (RequireUncursed) parts.Add("uncursed"); if (Source is not null) parts.Add(Labels.Source(Source.Value));
            if (IdentityGroup is int g) parts.Add($"same item group {(char)(64 + g)}");
            if (UpgradeSumGroup is int sumGroup && UpgradeSumTotal is int sumTotal) parts.Add($"combined +{sumTotal} total (group {(char)(64 + sumGroup)})");
            if (MaximumDepth is int d) parts.Add($"by floor {d}");
            return string.Join(" • ", parts);
        }
    }
    /// <summary>The effect predicate as row-subtitle text, or null for the wildcard.</summary>
    [JsonIgnore] public string? EffectSummary
    {
        get
        {
            if (Kind.Family() is not (ItemKind.Weapon or ItemKind.Armor)) return null;
            var family = Kind == ItemKind.Armor ? "glyph" : "enchantment";
            if (EffectMode == EffectMode.AnyEnchantment) return $"any {family}";
            if (EffectMode != EffectMode.Specific || Effects.Count == 0) return null;
            var enchantments = ItemCatalog.NonCurse(Kind);
            if (Effects.Count == enchantments.Length && enchantments.All(Effects.Contains)) return $"any {family}";
            if (Effects.Count == 1) return Effects[0];
            if (Effects.Count <= 4) return $"{string.Join(", ", Effects.Take(Effects.Count - 1))} or {Effects[^1]}";
            return $"any of {Effects.Count} {family}s";
        }
    }
    /// <summary>Whether a scouted item's effect (or lack of one) satisfies this requirement.</summary>
    public bool EffectMatches(string? effect) => EffectMode switch
    {
        EffectMode.AnyEnchantment => effect is not null && !ItemCatalog.IsCurse(Kind, effect),
        EffectMode.Specific when Effects.Count > 0 => effect is not null && Effects.Contains(effect),
        _ => true,
    };
    /// <summary>Drops unknown or inconsistent values restored from a saved file.</summary>
    public void Normalize()
    {
        Effects = Effects.Where(effect => ItemCatalog.Modifiers(Kind).Contains(effect)).Distinct().ToList();
        if (Kind is ItemKind.Wand or ItemKind.Ring || (EffectMode == EffectMode.Specific && Effects.Count == 0)) EffectMode = EffectMode.Any;
        if (EffectMode != EffectMode.Specific) Effects = [];
        if (AlternativeGroup is < 1 or > 255) AlternativeGroup = null;
        if (UpgradeSumGroup is < 1 or > 255) UpgradeSumGroup = null;
        if (UpgradeSumTotal is < 1 or > 8) UpgradeSumTotal = null;
        if (UpgradeSumGroup is null || UpgradeSumTotal is null || AlternativeGroup is not null) { UpgradeSumGroup = null; UpgradeSumTotal = null; }
    }
    public ItemRequirement Clone() { var copy = (ItemRequirement)MemberwiseClone(); copy.Effects = [.. Effects]; return copy; }
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
    ScoutItemSource Source, byte AccessibilityTag, int AccessibilityGroup, ulong AccessibilityValue);
public sealed record ScoutWorld(string Seed, IReadOnlyList<ScoutItem> Items);
public sealed record SearchStatus(SearchState State, long Scanned, long Total, long ErrorCode, double Probability);

public static class ScoutMatcher
{
    /// <summary>Highest upgrade any generated item can carry (+4 rings from the Imp).</summary>
    private const int MaxItemUpgrade = 4;

    /// <summary>
    /// The client-side mirror of seedfinder-core's <c>best_match_indices</c>:
    /// alternative groups collapse to one slot served by any single member,
    /// every slot needs a distinct item, and combined-upgrade groups count (and
    /// highlight) only when fully assigned at or above their total.
    /// </summary>
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
                && requirement.EffectMatches(item.Effect)
                && (!requirement.RequireUncursed || !item.Cursed)
                && (requirement.Source is null || requirement.Source == item.Source);
        }

        // Slots: an alternative group is one slot whose candidates are the
        // union of every member's matches; each candidate remembers which
        // member matched for identity/sum bookkeeping.
        var slots = new List<List<(int Index, ItemRequirement Requirement)>>();
        var slotOfGroup = new Dictionary<int, int>();
        var sumGroups = new Dictionary<int, (int Members, int MinimumTotal)>();
        foreach (var requirement in requirements)
        {
            int slot;
            if (requirement.AlternativeGroup is int alternative)
            {
                if (!slotOfGroup.TryGetValue(alternative, out slot)) { slot = slots.Count; slots.Add([]); slotOfGroup[alternative] = slot; }
            }
            else { slot = slots.Count; slots.Add([]); }
            if (requirement.UpgradeSumGroup is int label)
            {
                var group = sumGroups.TryGetValue(label, out var existing) ? existing : (0, requirement.UpgradeSumTotal ?? 0);
                sumGroups[label] = (group.Item1 + 1, group.Item2);
            }
            for (var index = 0; index < items.Count; index++)
                if (Matches(items[index], requirement)) slots[slot].Add((index, requirement));
        }
        // Fail early by assigning the most constrained slot first.
        slots.Sort((left, right) => left.Count.CompareTo(right.Count));

        var used = new HashSet<int>();
        var selected = new List<(int Index, int? SumGroup)>();
        var best = new HashSet<int>();
        var scenarios = new Dictionary<int, ulong>();
        var identities = new Dictionary<int, string>();
        var sums = new Dictionary<int, (int Assigned, int Total)>();

        void Visit(int position)
        {
            if (position == slots.Count)
            {
                // Items serving an incomplete or short combined-upgrade group
                // do not count and are not highlighted.
                var failed = sums.Where(pair =>
                {
                    var group = sumGroups.GetValueOrDefault(pair.Key);
                    return pair.Value.Assigned < group.Members || pair.Value.Total < group.MinimumTotal;
                }).Select(pair => pair.Key).ToHashSet();
                var counted = selected.Where(entry => entry.SumGroup is not int label || !failed.Contains(label))
                    .Select(entry => entry.Index).ToList();
                if (counted.Count > best.Count) best = [.. counted];
                return;
            }
            if (selected.Count + slots.Count - position <= best.Count) return;
            foreach (var (index, requirement) in slots[position])
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
                (int Assigned, int Total)? previousSum = null;
                if (requirement.UpgradeSumGroup is int sumLabel)
                {
                    var group = sumGroups.GetValueOrDefault(sumLabel);
                    if (sums.TryGetValue(sumLabel, out var current)) previousSum = current;
                    var assigned = (previousSum?.Assigned ?? 0) + 1;
                    var total = (previousSum?.Total ?? 0) + item.Upgrade;
                    var remaining = Math.Max(0, group.Members - assigned);
                    if (total + remaining * MaxItemUpgrade < group.MinimumTotal)
                    {
                        RestoreScenario(constraint, previousScenarios);
                        RestoreIdentity(requirement, previousIdentity);
                        continue;
                    }
                    sums[sumLabel] = (assigned, total);
                }
                used.Add(index); selected.Add((index, requirement.UpgradeSumGroup));
                Visit(position + 1);
                used.Remove(index); selected.RemoveAt(selected.Count - 1);
                if (requirement.UpgradeSumGroup is int oldLabel)
                {
                    if (previousSum is { } previous) sums[oldLabel] = previous;
                    else sums.Remove(oldLabel);
                }
                RestoreScenario(constraint, previousScenarios);
                RestoreIdentity(requirement, previousIdentity);
            }
            Visit(position + 1);
        }

        void RestoreScenario((int Group, ulong Mask)? constraint, ulong? previous)
        {
            if (constraint is not { } value) return;
            if (previous is ulong mask) scenarios[value.Group] = mask;
            else scenarios.Remove(value.Group);
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
    /// <summary>The family's 13 non-curse effect names ("Any enchantment" expands to these).</summary>
    public static string[] NonCurse(ItemKind kind) => kind.Family() == ItemKind.Armor ? Glyphs : Enchantments;
    public static bool IsCurse(ItemKind kind, string effect) => (kind.Family() == ItemKind.Weapon ? WeaponCurses : ArmorCurses).Contains(effect);
}
