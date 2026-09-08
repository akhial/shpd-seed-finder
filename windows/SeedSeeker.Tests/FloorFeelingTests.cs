using System.Text;
using Xunit;

namespace SeedSeeker.Tests;

public sealed class FloorFeelingTests
{
    private static byte[] Packet(string version, params byte[] feelings)
    {
        var w = new Writer();
        w.Bytes(Encoding.ASCII.GetBytes(version));
        w.U8(11); w.Bytes(Encoding.ASCII.GetBytes("AAA-AAA-AAA"));
        w.Bytes(Enumerable.Range(0, 12).Select(x => (byte)x));
        w.U8(0); w.U16(0);
        if (version != "SSC3")
        {
            var deck = ItemCatalog.For(ItemKind.Trinket).ToList();
            w.U8(deck.Count);
            foreach (var item in deck) w.Text(item.Id);
        }
        w.Bytes(feelings);
        return w.Finish();
    }

    [Theory]
    [InlineData("SSC3")]
    [InlineData("SSC4")]
    public void LegacyPacketsDefaultToEmptyFeelings(string version) =>
        Assert.Empty(NativeEngine.DecodeScout(Packet(version)).FloorFeelings!);

    [Fact]
    public void ReadsEveryFeelingAndRetainsDeck()
    {
        var world = NativeEngine.DecodeScout(Packet("SSC5", 8, 1, 0, 2, 1, 3, 2, 4, 3, 6, 4, 7, 5, 8, 6, 24, 7));
        Assert.Equal(Enum.GetValues<FloorFeeling>(), world.FloorFeelings!.Select(x => x.Feeling));
        Assert.Equal(24, world.FloorFeelings![^1].Depth);
        Assert.Equal(17, world.TrinketOrder!.Count);
        Assert.Empty(NativeEngine.DecodeScout(Packet("SSC5", 0)).FloorFeelings!);
    }

    [Theory]
    [InlineData(new byte[] { })]
    [InlineData(new byte[] { 21 })]
    [InlineData(new byte[] { 1, 1 })]
    [InlineData(new byte[] { 1, 0, 1 })]
    [InlineData(new byte[] { 1, 5, 1 })]
    [InlineData(new byte[] { 1, 25, 1 })]
    [InlineData(new byte[] { 1, 1, 8 })]
    [InlineData(new byte[] { 2, 1, 1, 1, 2 })]
    [InlineData(new byte[] { 2, 2, 1, 1, 2 })]
    [InlineData(new byte[] { 0, 1 })]
    public void RejectsMalformedFeelings(byte[] data) =>
        Assert.Throws<InvalidDataException>(() => NativeEngine.DecodeScout(Packet("SSC5", data)));
}
