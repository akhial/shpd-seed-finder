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
    [Fact]
    public void SelectedTrinketSurvivesDocumentsCloningAndNativeScouting()
    {
        var query = new QuerySettings { Requirements = [
            new() { Kind = ItemKind.Trinket, Item = ItemCatalog.Find("mimic_tooth"), SelectTrinket = true },
        ] };
        var decoded = ResultsExport.DecodeQueryDocument(ResultsExport.EncodeQueryDocument(query));
        Assert.True(decoded.Requirements[0].SelectTrinket);
        Assert.True(decoded.Clone().Requirements[0].SelectTrinket);
        Assert.Equal("choose at +3", decoded.Requirements[0].Description);
        var engine = new NativeEngine();
        var auto = engine.Scout("AAA-AAA-AAA", 0, decoded);
        Assert.Equal("mimic_tooth", auto.SelectedTrinket);
        var none = engine.Scout(auto.Seed, 0, decoded, "none");
        Assert.Null(none.SelectedTrinket);
        var other = none.TrinketOrder!.Take(4).First(item => item.Id != "mimic_tooth").Id;
        Assert.Equal(other, engine.Scout(auto.Seed, 0, decoded, other).SelectedTrinket);
        Assert.Equal("mimic_tooth", engine.Scout(auto.Seed, 0, decoded, "mimic_tooth").SelectedTrinket);
        var matches = NativeEngine.ScoutMatches(auto.Seed, 0, decoded);
        Assert.All(matches.Matched, index => Assert.Equal("mimic_tooth", auto.Items[index].Item.Id));
        Assert.Contains("select_trinket", ResultsExport.EncodeQueryDocument(decoded));
        var shared = NativeEngine.TryEncodeShareLink(ResultsExport.EncodeQueryDocument(decoded));
        Assert.NotNull(shared);
        var imported = ResultsExport.DecodeQueryDocument(NativeEngine.TryDecodeShareText(shared)!);
        Assert.True(imported.Requirements[0].SelectTrinket);
        decoded.Requirements[0].AlternativeGroup = 1;
        decoded.Requirements.Add(new() { Kind = ItemKind.Trinket, Item = ItemCatalog.Find(other), AlternativeGroup = 1 });
        Assert.Null(engine.Scout(auto.Seed, 0, decoded).SelectedTrinket);
    }

    [Fact]
    public void ScoutRequestCarriesQueryAndExplicitDeselection()
    {
        var query = new QuerySettings { Requirements = [
            new() { Kind = ItemKind.Trinket, Item = ItemCatalog.Find("mimic_tooth"), SelectTrinket = true },
        ] };
        var bytes = NativeEngine.EncodeScoutRequest("AAA-AAA-AAA", 257, query, "none");
        Assert.Equal("SSQ3", System.Text.Encoding.UTF8.GetString(bytes, 0, 4));
        Assert.Equal(new byte[] { 1, 1, 11, 0 }, bytes[4..8]);
        Assert.Equal(new byte[] { 4, 0 }, bytes[19..21]);
        Assert.Equal("none", System.Text.Encoding.UTF8.GetString(bytes, 21, 4));
        Assert.True(ResultsExport.DecodeQueryDocument(System.Text.Encoding.UTF8.GetString(bytes, 25, bytes.Length - 25)).Requirements[0].SelectTrinket);
    }

}
