using System.Text.Json;
using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The catalog asset the app ships, which is the Android app's file linked
/// rather than copied. The item entries and the four effect tables are read
/// from it, so a catalog bump carries both and neither can fall behind.
/// </summary>
public sealed class ItemCatalogTests
{
    /// <summary>The asset in the repository, read independently of ItemCatalog.</summary>
    private static JsonElement Asset()
    {
        var root = NativeEngineLibrary.WorkspaceRoot()
            ?? throw new InvalidOperationException("Could not locate the workspace root.");
        var path = Path.Combine(root, "android", "app", "src", "main", "assets", "third_party",
            "shattered-pixel-dungeon", "catalog-v4.0.0.json");
        return JsonDocument.Parse(File.ReadAllText(path)).RootElement;
    }

    private static string[] Table(string name) =>
        [.. Asset().GetProperty("modifiers").GetProperty(name).EnumerateArray().Select(value => value.GetString()!)];

    [Fact]
    public void TheEffectTablesAreTheAssetsOwn()
    {
        Assert.Equal(Table("weaponEnchantments"), ItemCatalog.Enchantments);
        Assert.Equal(Table("weaponCurses"), ItemCatalog.WeaponCurses);
        Assert.Equal(Table("armorGlyphs"), ItemCatalog.Glyphs);
        Assert.Equal(Table("armorCurses"), ItemCatalog.ArmorCurses);
        // Non-empty, so an asset the deserializer failed to read cannot pass.
        Assert.NotEmpty(ItemCatalog.Enchantments);
        Assert.NotEmpty(ItemCatalog.WeaponCurses);
        Assert.NotEmpty(ItemCatalog.Glyphs);
        Assert.NotEmpty(ItemCatalog.ArmorCurses);
    }

    [Fact]
    public void ModifiersOffersEnchantmentsThenCursesPerFamily()
    {
        Assert.Equal([.. ItemCatalog.Enchantments, .. ItemCatalog.WeaponCurses], ItemCatalog.Modifiers(ItemKind.Weapon));
        Assert.Equal(ItemCatalog.Modifiers(ItemKind.Weapon), ItemCatalog.Modifiers(ItemKind.ThrownWeapon));
        Assert.Equal([.. ItemCatalog.Glyphs, .. ItemCatalog.ArmorCurses], ItemCatalog.Modifiers(ItemKind.Armor));
        Assert.Empty(ItemCatalog.Modifiers(ItemKind.Wand));
        Assert.Empty(ItemCatalog.Modifiers(ItemKind.Ring));
    }

    [Fact]
    public void IsCurseAsksTheCurseTableOfTheItemsFamily()
    {
        Assert.All(ItemCatalog.WeaponCurses, curse => Assert.True(ItemCatalog.IsCurse(ItemKind.MeleeWeapon, curse)));
        Assert.All(ItemCatalog.Enchantments, effect => Assert.False(ItemCatalog.IsCurse(ItemKind.Weapon, effect)));
        Assert.All(ItemCatalog.ArmorCurses, curse => Assert.True(ItemCatalog.IsCurse(ItemKind.Armor, curse)));
        Assert.All(ItemCatalog.Glyphs, glyph => Assert.False(ItemCatalog.IsCurse(ItemKind.Armor, glyph)));
    }

    [Fact]
    public void TheFreshPickListHidesTierOneItems()
    {
        Assert.Contains(ItemCatalog.All, item => item.Tier == 1);
        Assert.All(ItemCatalog.For(ItemKind.Weapon), item => Assert.NotEqual(1, item.Tier));
        Assert.All(ItemCatalog.For(ItemKind.Armor), item => Assert.NotEqual(1, item.Tier));
    }

    [Fact]
    public void TheFreshPickListHidesTippedDarts()
    {
        // Every shop stocks tipped darts and any dart can be tipped by hand,
        // so nobody searches for one; scouted worlds still show them.
        Assert.Contains(ItemCatalog.All, item => item.Id.EndsWith("_dart", StringComparison.Ordinal));
        Assert.All(ItemCatalog.For(ItemKind.ThrownWeapon),
            item => Assert.False(item.Id.EndsWith("_dart", StringComparison.Ordinal)));
        // A requirement importing one still round-trips through the editor.
        var dart = ItemCatalog.Find("poison_dart")!;
        Assert.Contains(dart, ItemCatalog.EditorItems(ItemKind.ThrownWeapon, dart));
    }

    [Fact]
    public void TheEditorListIsTheFreshPickListUntilTheRequirementNamesAHiddenItem()
    {
        Assert.Equal(ItemCatalog.For(ItemKind.Weapon), ItemCatalog.EditorItems(ItemKind.Weapon, null));
        // An item the list already offers adds nothing.
        var sword = ItemCatalog.Find("sword")!;
        Assert.Equal(ItemCatalog.For(ItemKind.MeleeWeapon), ItemCatalog.EditorItems(ItemKind.MeleeWeapon, sword));
        // Rings and wands carry no tier, so nothing is ever hidden from them.
        Assert.Equal(ItemCatalog.For(ItemKind.Ring), ItemCatalog.EditorItems(ItemKind.Ring, null));
    }

    [Fact]
    public void AnImportedTierOneItemIsListedSoItRoundTrips()
    {
        // Imports and share links resolve through the whole catalog, so the
        // editor has to be able to show — and re-save — a tier-1 item.
        var worn = ItemCatalog.Find("worn_shortsword")!;
        Assert.Equal(1, worn.Tier);
        foreach (var kind in new[] { ItemKind.Weapon, ItemKind.MeleeWeapon })
        {
            var listed = ItemCatalog.EditorItems(kind, worn);
            Assert.Contains(worn, listed);
            // Exactly one extra entry, and the catalog order is preserved.
            Assert.Equal(ItemCatalog.For(kind).Count() + 1, listed.Count);
            Assert.Equal([.. ItemCatalog.All.Where(listed.Contains)], listed);
        }
        // Only the named item is unhidden, not every tier-1 item.
        Assert.DoesNotContain(ItemCatalog.Find("cloth_armor"), ItemCatalog.EditorItems(ItemKind.Armor, worn));
    }

    [Fact]
    public void EveryCatalogItemCanBeRepresentedByItsOwnKind()
    {
        foreach (var item in ItemCatalog.All)
        {
            var kind = item.Kind == ItemKind.Weapon && item.Class == WeaponClass.Thrown ? ItemKind.ThrownWeapon
                : item.Kind == ItemKind.Weapon ? ItemKind.MeleeWeapon : item.Kind;
            Assert.Contains(item, ItemCatalog.EditorItems(kind, item));
            Assert.Contains(item, ItemCatalog.EditorItems(item.Kind, item));
        }
    }

    [Fact]
    public void AnItemOfAnotherKindIsNeverListed()
    {
        var worn = ItemCatalog.Find("worn_shortsword")!;
        Assert.DoesNotContain(worn, ItemCatalog.EditorItems(ItemKind.Ring, worn));
        Assert.DoesNotContain(worn, ItemCatalog.EditorItems(ItemKind.ThrownWeapon, worn));
    }

    [Fact]
    public void TheItemsAreTheAssetsOwn()
    {
        var entries = Asset().GetProperty("entries").EnumerateArray()
            .Select(entry => entry.GetProperty("id").GetString()).ToArray();
        Assert.Equal(entries, ItemCatalog.All.Select(item => item.Id));
    }

    /// <summary>
    /// The ring glyphs are the asset's <c>typeIcon</c>, carried through rather
    /// than derived from the sprite index: once a scouted ring is drawn in the
    /// cell its run's gem picks, that derivation names the wrong ring.
    /// </summary>
    [Fact]
    public void TheRingGlyphsAreTheAssetsOwn()
    {
        var glyphs = Asset().GetProperty("entries").EnumerateArray()
            .Select(entry => entry.TryGetProperty("typeIcon", out var icon) ? icon.GetInt32() : (int?)null)
            .ToArray();
        Assert.Equal(glyphs, ItemCatalog.All.Select(item => item.TypeIconIndex));
        // Every ring carries one and nothing else does, and each is its class's
        // own offset into the catalog's block of ring cells.
        foreach (var item in ItemCatalog.All)
        {
            Assert.Equal(item.Kind == ItemKind.Ring, item.TypeIconIndex is not null);
            if (item.TypeIconIndex is int glyph) Assert.Equal(RingGems.RingSpriteBase + glyph, item.SpriteIndex);
        }
    }
}
