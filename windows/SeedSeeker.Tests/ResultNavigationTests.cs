using Xunit;

namespace SeedSeeker.Tests;

public sealed class ResultNavigationTests
{
    private static readonly string[] Seeds = ["AAA-AAA-AAA", "BBB-BBB-BBB", "CCC-CCC-CCC"];

    [Fact]
    public void IndexOfLocatesAScoutedSeedInsideTheResults()
    {
        Assert.Equal(0, ResultNavigation.IndexOf(Seeds, "AAA-AAA-AAA"));
        Assert.Equal(2, ResultNavigation.IndexOf(Seeds, "CCC-CCC-CCC"));
    }

    [Fact]
    public void IndexOfIsNullOutsideTheResults()
    {
        Assert.Null(ResultNavigation.IndexOf(Seeds, "ZZZ-ZZZ-ZZZ"));
        Assert.Null(ResultNavigation.IndexOf(Seeds, null));
        Assert.Null(ResultNavigation.IndexOf(Seeds, ""));
    }

    [Fact]
    public void PositionIsDroppedWhenANewSearchClearsTheResults()
    {
        // A scouted seed keeps its manifest, but an emptied results list must
        // invalidate its position.
        Assert.Null(ResultNavigation.IndexOf([], "AAA-AAA-AAA"));
        Assert.Null(ResultNavigation.Step([], "AAA-AAA-AAA", 1));
    }

    [Fact]
    public void StepMovesForwardAndBackward()
    {
        Assert.Equal(1, ResultNavigation.Step(Seeds, "AAA-AAA-AAA", 1));
        Assert.Equal(2, ResultNavigation.Step(Seeds, "BBB-BBB-BBB", 1));
        Assert.Equal(1, ResultNavigation.Step(Seeds, "CCC-CCC-CCC", -1));
    }

    [Fact]
    public void StepDoesNotWrapPastTheEnds()
    {
        Assert.Null(ResultNavigation.Step(Seeds, "AAA-AAA-AAA", -1));
        Assert.Null(ResultNavigation.Step(Seeds, "CCC-CCC-CCC", 1));
    }

    [Fact]
    public void StepClampsLargerJumpsToTheListEnds()
    {
        Assert.Equal(2, ResultNavigation.Step(Seeds, "BBB-BBB-BBB", 5));
        Assert.Equal(0, ResultNavigation.Step(Seeds, "BBB-BBB-BBB", -5));
    }

    [Fact]
    public void StepIsInertWithoutAnAnchorInTheResults()
    {
        Assert.Null(ResultNavigation.Step(Seeds, "ZZZ-ZZZ-ZZZ", 1));
        Assert.Null(ResultNavigation.Step(Seeds, null, 1));
        Assert.Null(ResultNavigation.Step(["AAA-AAA-AAA"], "AAA-AAA-AAA", 1));
    }
}
