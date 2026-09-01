using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The gems a run gives its ring classes, which decide what colour each ring is
/// drawn. Shattered Pixel Dungeon shuffles <c>Ring.gems</c> once per run before
/// the first floor exists, so this follows from the seed alone; the engine
/// reproduces the shuffle and carries the table in the <c>SSC3</c> scout packet,
/// so the world a scout describes arrives with its own gems.
///
/// Pinned on seed YKH-LGJ-WDQ, whose ring of haste the game draws as a diamond —
/// the seed the engine checked against the Java oracle.
/// </summary>
public sealed class RingGemsTests
{
    private const string Seed = "YKH-LGJ-WDQ";

    /// <summary>The run's table, gem ordinal per ring class in catalog order.</summary>
    private static readonly byte[] Table = [7, 8, 3, 5, 4, 6, 2, 11, 10, 1, 0, 9];

    private static CatalogItem Item(string id) =>
        ItemCatalog.Find(id) ?? throw new InvalidOperationException($"The catalog has no \"{id}\".");

    [Fact]
    public void TheScoutedWorldCarriesTheRunsTable()
    {
        Assert.Equal(Table, new NativeEngine().Scout(Seed, 0).Gems.Ordinals);
    }

    [Fact]
    public void TheChallengeMaskDoesNotMoveTheGems()
    {
        // The gems are drawn before any challenge is read, so every scout of
        // this seed describes a world holding the same twelve gems.
        Assert.Equal(Table, new NativeEngine().Scout(Seed, Challenges.AllMask).Gems.Ordinals);
    }

    [Fact]
    public void AScoutedRingIsDrawnInTheRunsGemCell()
    {
        var haste = Item("ring_haste");
        // The class's own identity is untouched: its catalog cell, and the glyph
        // that tells it from the other eleven rings, are the same in every run.
        Assert.Equal(7, haste.TypeIconIndex);
        Assert.Equal(RingGems.RingSpriteBase + 7, haste.SpriteIndex);
        // The run draws it in its gem's cell instead — the diamond, not the
        // sapphire the catalog cell would have shown.
        Assert.Equal(RingGems.RingSpriteBase + 11, new RingGems(Table).SpriteIndex(haste));
    }

    [Fact]
    public void EveryRingMovesWithTheRunAndNothingElseDoes()
    {
        var gems = new RingGems(Table);
        foreach (var item in ItemCatalog.All)
        {
            var expected = item.TypeIconIndex is int type
                ? RingGems.RingSpriteBase + Table[type]
                : item.SpriteIndex;
            Assert.Equal(expected, gems.SpriteIndex(item));
        }
    }

    [Fact]
    public void TheUnshuffledTableDrawsTheCatalogsOwnCells()
    {
        // What a surface with no seed shows: every class holding its own gem,
        // which is exactly the block of cells the catalog spells out.
        foreach (var item in ItemCatalog.All)
            Assert.Equal(item.SpriteIndex, RingGems.Unshuffled.SpriteIndex(item));
    }

    [Fact]
    public void ACorruptTableIsRejectedRatherThanDrawn()
    {
        // A shuffle deals every class a distinct gem, so anything that is not a
        // permutation of the twelve is a damaged packet — and is refused the way
        // every other malformed scout field is, not drawn as nonsense colours.
        Assert.Throws<InvalidDataException>(() => new RingGems([.. Table[..11]]));
        Assert.Throws<InvalidDataException>(() => new RingGems([.. Table, (byte)0]));
        Assert.Throws<InvalidDataException>(() => new RingGems([0, 0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]));
        Assert.Throws<InvalidDataException>(() => new RingGems([12, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]));
    }
}
