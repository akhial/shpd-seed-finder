using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The engine constants the app used to mirror. These pin the values the
/// editor clamps, the floor selectors and the challenge dialog now read, so a
/// change on the engine side shows up here rather than as an editor that
/// offers a query the search refuses.
/// </summary>
public sealed class EngineInfoTests
{
    [Fact]
    public void TheLimitsAreThePublishedOnes()
    {
        Assert.Equal("3.3.8", EngineInfo.ShpdVersion);
        Assert.Equal("7b8b845a76fe76c6b7c031ae9e570852411f56db", EngineInfo.ShpdCommit);
        Assert.Equal(24, EngineInfo.MaxDepth);
        Assert.Equal(2, EngineInfo.ExactTierMin);
        Assert.Equal(5, EngineInfo.ExactTierMax);
        Assert.Equal(3, EngineInfo.BoundedTierMin);
        Assert.Equal(4, EngineInfo.BoundedTierMax);
        Assert.Equal(4, EngineInfo.IdentityGroupMax);
        Assert.Equal(3, EngineInfo.MaxUpgradeDefault);
        Assert.Equal(4, EngineInfo.MaxUpgradeRing);
        Assert.Equal(1024, EngineInfo.MaxResults);
        Assert.Equal(2 * 1024 * 1024, EngineInfo.ResultsFileMaxBytes);
        Assert.Equal([5, 10, 15], EngineInfo.EmptyBossFloors);
    }

    [Fact]
    public void TheChallengesArriveInMaskOrderWithTheirGenerationEffect()
    {
        var challenges = EngineInfo.Challenges;
        Assert.Equal(9, challenges.Count);
        for (var index = 0; index < challenges.Count; index++) Assert.Equal(1 << index, challenges[index].Mask);
        Assert.Equal("on_diet", challenges[0].Name);
        Assert.Equal(
            ["barren_land", "into_darkness", "forbidden_runes"],
            challenges.Where(c => c.ChangesLevelGeneration).Select(c => c.Name));
    }

    [Fact]
    public void ChallengeLabelsReadAsSentences()
    {
        Assert.Equal(
            ["On diet", "Faith is my armor", "Pharmacophobia", "Barren land", "Swarm intelligence",
             "Into darkness", "Forbidden runes", "Hostile champions", "Badder bosses"],
            EngineInfo.Challenges.Select(challenge => Labels.Challenge(challenge.Name)));
        Assert.Equal("", Labels.Challenge(""));
    }

    [Fact]
    public void UpgradeMaximaComeFromTheEngineAndSingleOutRings()
    {
        Assert.Equal(EngineInfo.MaxUpgradeRing, ItemKind.Ring.MaximumSearchUpgrade());
        foreach (var kind in Enum.GetValues<ItemKind>().Where(kind => kind != ItemKind.Ring))
            Assert.Equal(EngineInfo.MaxUpgradeDefault, kind.MaximumSearchUpgrade());
    }

    [Fact]
    public void TheFloorSelectorOffersTheSearchableRangeWithoutEmptyBossFloors()
    {
        Assert.Equal(
            [.. Enumerable.Range(1, EngineInfo.MaxDepth).Where(floor => !EngineInfo.EmptyBossFloors.Contains(floor))],
            FloorLimits.Options);
        Assert.Equal(EngineInfo.EmptyBossFloors, FloorLimits.EmptyBossFloors);
        Assert.DoesNotContain(FloorLimits.Options, EngineInfo.EmptyBossFloors.Contains);
        Assert.Equal(EngineInfo.MaxDepth, FloorLimits.Options[^1]);
        foreach (var floor in EngineInfo.EmptyBossFloors) Assert.Equal(floor - 1, FloorLimits.Normalize(floor));
    }
}
