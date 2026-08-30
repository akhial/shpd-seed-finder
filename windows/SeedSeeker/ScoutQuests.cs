namespace SeedSeeker;

/// <summary>The four quest-giving NPCs, numbered as in the SSC2 wire format.</summary>
public enum QuestGiver { Ghost = 1, Wandmaker = 2, Blacksmith = 3, Imp = 4 }

/// <summary>The concrete quest rolled for a giver in a scouted world.</summary>
public enum QuestVariant
{
    FetidRat, GnollTrickster, GreatCrab,       // Ghost
    CorpseDust, ElementalEmbers, Rotberry,     // Wandmaker
    Crystal, Gnoll,                            // Blacksmith
    Vault,                                     // Imp
}

/// <summary>One quest of a scouted world: who gives it, which variant, and its floor.</summary>
public sealed record ScoutQuest(QuestGiver Giver, QuestVariant Variant, int Depth);

/// <summary>
/// Decoding and labelling of the SSC2 scout packet's quest block. Pure so the
/// wire logic stays testable off-Windows (see SeedSeeker.Tests); the WinUI
/// layer only renders the returned records.
/// </summary>
public static class ScoutQuests
{
    /// <summary>A world rolls at most one quest per giver.</summary>
    public const int MaximumQuests = 4;

    private static readonly QuestVariant[] GhostVariants = [QuestVariant.FetidRat, QuestVariant.GnollTrickster, QuestVariant.GreatCrab];
    private static readonly QuestVariant[] WandmakerVariants = [QuestVariant.CorpseDust, QuestVariant.ElementalEmbers, QuestVariant.Rotberry];
    private static readonly QuestVariant[] BlacksmithVariants = [QuestVariant.Crystal, QuestVariant.Gnoll];
    // v4.0.0 replaced the Monk/Golem token hunts with a single vault expedition.
    private static readonly QuestVariant[] ImpVariants = [QuestVariant.Vault];

    /// <summary>
    /// Decodes the quest block — <c>count:u8</c> then <c>count</c> ×
    /// <c>{quest:u8, variant:u8, depth:u8}</c> — starting at
    /// <paramref name="offset"/>, which is left just past the block.
    /// Throws <see cref="InvalidDataException"/> on any malformed block,
    /// mirroring the strictness of the rest of the packet decoder.
    /// </summary>
    public static IReadOnlyList<ScoutQuest> Parse(ReadOnlySpan<byte> data, ref int offset)
    {
        var count = Next(data, ref offset);
        if (count > MaximumQuests) throw new InvalidDataException("Too many quests in scout packet");
        var quests = new List<ScoutQuest>(count);
        byte previous = 0;
        for (var i = 0; i < count; i++)
        {
            var quest = Next(data, ref offset);
            var variant = Next(data, ref offset);
            var depth = Next(data, ref offset);
            quests.Add(Entry(quest, variant, depth, previous));
            previous = quest;
        }
        return quests;
    }

    /// <summary>
    /// Validates one wire quest record and maps it to a typed
    /// <see cref="ScoutQuest"/>. <paramref name="previousQuest"/> is the wire
    /// id of the record before it (0 for the first): ids must be strictly
    /// ascending, so each giver appears at most once, in a canonical order.
    /// </summary>
    public static ScoutQuest Entry(byte quest, byte variant, byte depth, byte previousQuest)
    {
        if (quest is < 1 or > 4) throw new InvalidDataException("Unknown quest in scout packet");
        if (quest <= previousQuest) throw new InvalidDataException("Out-of-order quest in scout packet");
        var giver = (QuestGiver)quest;
        var variants = giver switch
        {
            QuestGiver.Ghost => GhostVariants,
            QuestGiver.Wandmaker => WandmakerVariants,
            QuestGiver.Blacksmith => BlacksmithVariants,
            _ => ImpVariants,
        };
        if (variant < 1 || variant > variants.Length) throw new InvalidDataException("Unknown quest variant in scout packet");
        var (first, last) = Window(giver);
        if (depth < first || depth > last) throw new InvalidDataException("Quest depth out of range in scout packet");
        return new(giver, variants[variant - 1], depth);
    }

    /// <summary>
    /// The inclusive floor window a giver's quest can sit on. A local copy of
    /// the engine's feasibility model, checked against <c>engine_info</c> by
    /// EngineConstantsTests.
    /// </summary>
    public static (int First, int Last) Window(QuestGiver giver) => giver switch
    {
        QuestGiver.Ghost => (2, 4),
        QuestGiver.Wandmaker => (7, 9),
        QuestGiver.Blacksmith => (12, 14),
        _ => (17, 19),
    };

    public static string GiverLabel(QuestGiver value) => value switch
    {
        QuestGiver.Ghost => "Sad ghost",
        QuestGiver.Wandmaker => "Wandmaker",
        QuestGiver.Blacksmith => "Blacksmith",
        _ => "Imp",
    };

    public static string VariantLabel(QuestVariant value) => value switch
    {
        QuestVariant.FetidRat => "Fetid rat",
        QuestVariant.GnollTrickster => "Gnoll trickster",
        QuestVariant.GreatCrab => "Great crab",
        QuestVariant.CorpseDust => "Corpse dust",
        QuestVariant.ElementalEmbers => "Elemental embers",
        QuestVariant.Rotberry => "Rotberry",
        QuestVariant.Crystal => "Crystal spire",
        QuestVariant.Gnoll => "Gnoll geomancer",
        _ => "Vault",
    };

    private static byte Next(ReadOnlySpan<byte> data, ref int offset)
    {
        if (offset >= data.Length) throw new InvalidDataException("Truncated native packet");
        return data[offset++];
    }
}
