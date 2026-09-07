using Xunit;

namespace SeedSeeker.Tests;

public sealed class TrinketTests
{
    [Fact]
    public void CatalogContainsSeventeenNamedTrinkets()
    {
        var items = ItemCatalog.For(ItemKind.Trinket).ToList();
        Assert.Equal(17, items.Count);
        Assert.Equal(17, items.Select(x => x.Id).Distinct().Count());
        Assert.All(items, item => Assert.InRange(item.SpriteIndex, 272, 288));
        Assert.Equal("Mimic Tooth", ItemCatalog.Find("mimic_tooth")!.Name);
    }

    [Fact]
    public void ScoutCarriesFullOrderAndFourLocatedChoicesWithMatchingIndices()
    {
        var world = new NativeEngine().Scout("AAA-AAA-AAA", 0);
        Assert.NotNull(world.TrinketOrder);
        Assert.Equal(17, world.TrinketOrder.Count);
        var choices = world.Items.Where(x => x.Item.Kind == ItemKind.Trinket).ToList();
        Assert.Equal(4, choices.Count);
        Assert.Equal(world.TrinketOrder.Take(4).Select(x => x.Id).Order(), choices.Select(x => x.Item.Id).Order());
        Assert.All(choices, item => { Assert.Equal(3, item.Depth); Assert.Equal(ScoutItemSource.LockedChest, item.Source); });
        var query = new QuerySettings { Requirements = [
            new() { Kind = ItemKind.Trinket, Item = ItemCatalog.Find("mimic_tooth"), AlternativeGroup = 1 },
            new() { Kind = ItemKind.Trinket, Item = ItemCatalog.Find("rat_skull"), AlternativeGroup = 1 },
        ] };
        var matches = NativeEngine.ScoutMatches(world.Seed, 0, query);
        Assert.Equal(1, matches.TotalRequirements);
        Assert.Equal(1, matches.MatchedRequirements);
        Assert.Contains(matches.Matched, index => world.Items[index].Item.Id == "mimic_tooth");
        Assert.Equal("", query.Requirements[0].Description);
    }

    [Fact]
    public void TrinketsJoinAlternativesAndRoundTripDocuments()
    {
        var requirements = QueryRelationships.JoinAlternatives([
            new() { Kind = ItemKind.Trinket, Item = ItemCatalog.Find("mimic_tooth") },
            new() { Kind = ItemKind.Trinket, Item = ItemCatalog.Find("rat_skull") },
        ], 1, 0);
        var query = new QuerySettings { Requirements = new(requirements) };
        var json = ResultsExport.EncodeQueryDocument(query);
        var decoded = ResultsExport.DecodeQueryDocument(json);
        Assert.Equal(2, decoded.Requirements.Count);
        Assert.All(decoded.Requirements, item => Assert.Equal(ItemKind.Trinket, item.Kind));
        Assert.NotNull(decoded.Requirements[0].AlternativeGroup);
        Assert.Equal(decoded.Requirements[0].AlternativeGroup, decoded.Requirements[1].AlternativeGroup);
        Assert.Equal(0, ItemKind.Trinket.MaximumSearchUpgrade());
    }

    [Fact]
    public void ScoutRejectsDuplicateDeckIdentities()
    {
        var packet = new Writer();
        packet.Bytes(System.Text.Encoding.UTF8.GetBytes("SSC4"));
        packet.U8(11); packet.Bytes(System.Text.Encoding.UTF8.GetBytes("AAA-AAA-AAA"));
        packet.Bytes(Enumerable.Range(0, 12).Select(x => (byte)x));
        packet.U8(0); packet.U16(0); packet.U8(17);
        for (var i = 0; i < 17; i++) packet.Text("mimic_tooth");
        Assert.Throws<InvalidDataException>(() => NativeEngine.DecodeScout(packet.Finish()));
    }
}
