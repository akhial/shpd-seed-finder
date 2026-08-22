using Xunit;

namespace SeedSeeker.Tests;

public sealed class ScoutQuestsTests
{
    /// <summary>Parses a complete quest block, asserting it is consumed exactly.</summary>
    private static IReadOnlyList<ScoutQuest> Parse(params byte[] block)
    {
        var offset = 0;
        var quests = ScoutQuests.Parse(block, ref offset);
        Assert.Equal(block.Length, offset);
        return quests;
    }

    private static void AssertParseRejects(params byte[] block)
    {
        Assert.Throws<InvalidDataException>(() => { var offset = 0; ScoutQuests.Parse(block, ref offset); });
    }

    [Fact]
    public void ParsesTheGoldenQuestBlock()
    {
        // The quest block of the SSC2 golden packet: four quests, one per giver.
        var quests = Parse(0x04, 0x01, 0x03, 0x04, 0x02, 0x03, 0x08, 0x03, 0x01, 0x0D, 0x04, 0x02, 0x12);
        Assert.Equal(new ScoutQuest[]
        {
            new(QuestGiver.Ghost, QuestVariant.GreatCrab, 4),
            new(QuestGiver.Wandmaker, QuestVariant.Rotberry, 8),
            new(QuestGiver.Blacksmith, QuestVariant.Crystal, 13),
            new(QuestGiver.Imp, QuestVariant.Golem, 18),
        }, quests);
    }

    [Fact]
    public void ParsesAnEmptyQuestBlock()
    {
        Assert.Empty(Parse(0x00));
    }

    [Fact]
    public void ParseAdvancesTheCallersOffsetPastTheBlockOnly()
    {
        // The block sits mid-packet in practice; only its bytes may be consumed.
        byte[] data = [0xFF, 0xFF, 0x01, 0x01, 0x02, 0x02, 0xAA];
        var offset = 2;
        var quests = ScoutQuests.Parse(data, ref offset);
        Assert.Equal(6, offset);
        var quest = Assert.Single(quests);
        Assert.Equal(new ScoutQuest(QuestGiver.Ghost, QuestVariant.GnollTrickster, 2), quest);
    }

    [Theory]
    [InlineData(1, 1, 2, QuestVariant.FetidRat)]
    [InlineData(1, 2, 3, QuestVariant.GnollTrickster)]
    [InlineData(1, 3, 4, QuestVariant.GreatCrab)]
    [InlineData(2, 1, 7, QuestVariant.CorpseDust)]
    [InlineData(2, 2, 8, QuestVariant.ElementalEmbers)]
    [InlineData(2, 3, 9, QuestVariant.Rotberry)]
    [InlineData(3, 1, 12, QuestVariant.Crystal)]
    [InlineData(3, 2, 14, QuestVariant.Gnoll)]
    [InlineData(4, 1, 17, QuestVariant.Monk)]
    [InlineData(4, 2, 19, QuestVariant.Golem)]
    public void EntryMapsEveryQuestVariant(byte quest, byte variant, byte depth, QuestVariant expected)
    {
        var entry = ScoutQuests.Entry(quest, variant, depth, 0);
        Assert.Equal((QuestGiver)quest, entry.Giver);
        Assert.Equal(expected, entry.Variant);
        Assert.Equal(depth, entry.Depth);
    }

    [Theory]
    [InlineData(0)]
    [InlineData(5)]
    [InlineData(255)]
    public void EntryRejectsUnknownQuestIds(byte quest)
    {
        Assert.Throws<InvalidDataException>(() => ScoutQuests.Entry(quest, 1, 2, 0));
    }

    [Theory]
    [InlineData(1, 0)]
    [InlineData(1, 4)] // The Ghost has three variants.
    [InlineData(2, 0)]
    [InlineData(2, 4)] // So does the Wandmaker.
    [InlineData(3, 3)] // The Blacksmith has two.
    [InlineData(4, 3)] // So does the Imp.
    public void EntryRejectsUnknownVariants(byte quest, byte variant)
    {
        var depth = quest switch { 1 => (byte)2, 2 => (byte)7, 3 => (byte)12, _ => (byte)17 };
        Assert.Throws<InvalidDataException>(() => ScoutQuests.Entry(quest, variant, depth, 0));
    }

    [Fact]
    public void EntryTakesTheDepthAsGiven()
    {
        // The giver's floor window belongs to the engine's feasibility model,
        // and the packet came from the engine, so decoding does not re-check it.
        Assert.Equal(1, ScoutQuests.Entry(1, 1, 1, 0).Depth);
        Assert.Equal(24, ScoutQuests.Entry(2, 1, 24, 0).Depth);
    }

    [Fact]
    public void EntryRejectsDuplicateAndDescendingQuestIds()
    {
        Assert.Throws<InvalidDataException>(() => ScoutQuests.Entry(2, 1, 7, 2));
        Assert.Throws<InvalidDataException>(() => ScoutQuests.Entry(1, 1, 2, 3));
    }

    [Fact]
    public void ParseRejectsMoreThanFourQuests()
    {
        AssertParseRejects(0x05, 1, 1, 2, 2, 1, 7, 3, 1, 12, 4, 1, 17, 4, 2, 18);
    }

    [Fact]
    public void ParseRejectsATruncatedBlock()
    {
        AssertParseRejects(); // No count byte at all.
        AssertParseRejects(0x01); // One quest promised, none present.
        AssertParseRejects(0x01, 0x01, 0x01); // Record cut short of its depth.
    }

    [Fact]
    public void ParseRejectsAnOutOfOrderBlock()
    {
        // Wandmaker before Ghost: valid records, invalid order.
        AssertParseRejects(0x02, 2, 1, 7, 1, 1, 2);
        // The same giver twice.
        AssertParseRejects(0x02, 1, 1, 2, 1, 2, 3);
    }

    [Fact]
    public void GiverLabelsMatchTheGameNames()
    {
        Assert.Equal("Sad ghost", ScoutQuests.GiverLabel(QuestGiver.Ghost));
        Assert.Equal("Wandmaker", ScoutQuests.GiverLabel(QuestGiver.Wandmaker));
        Assert.Equal("Blacksmith", ScoutQuests.GiverLabel(QuestGiver.Blacksmith));
        Assert.Equal("Imp", ScoutQuests.GiverLabel(QuestGiver.Imp));
    }

    [Fact]
    public void VariantLabelsMatchTheGameNames()
    {
        Assert.Equal("Fetid rat", ScoutQuests.VariantLabel(QuestVariant.FetidRat));
        Assert.Equal("Gnoll trickster", ScoutQuests.VariantLabel(QuestVariant.GnollTrickster));
        Assert.Equal("Great crab", ScoutQuests.VariantLabel(QuestVariant.GreatCrab));
        Assert.Equal("Corpse dust", ScoutQuests.VariantLabel(QuestVariant.CorpseDust));
        Assert.Equal("Elemental embers", ScoutQuests.VariantLabel(QuestVariant.ElementalEmbers));
        Assert.Equal("Rotberry", ScoutQuests.VariantLabel(QuestVariant.Rotberry));
        Assert.Equal("Crystal spire", ScoutQuests.VariantLabel(QuestVariant.Crystal));
        Assert.Equal("Gnoll geomancer", ScoutQuests.VariantLabel(QuestVariant.Gnoll));
        Assert.Equal("Monks", ScoutQuests.VariantLabel(QuestVariant.Monk));
        Assert.Equal("Golems", ScoutQuests.VariantLabel(QuestVariant.Golem));
    }
}
