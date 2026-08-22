using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// Seed-code text handling, which the engine owns
/// (<c>seedfinder_seed_format</c> / <c>seedfinder_seed_parse</c>): the mask the
/// seed field applies as the user types, and the parse the scout button and the
/// result filter depend on.
/// </summary>
public sealed class SeedCodeTests
{
    [Theory]
    [InlineData("", "")]
    [InlineData("a", "A")]
    [InlineData("abc", "ABC")]
    [InlineData("abcd", "ABC-D")]
    [InlineData("abcdefghi", "ABC-DEF-GHI")]
    // Only the first nine letters survive; everything else is dropped.
    [InlineData("a1b2c3d4e5f6g7h8i9j", "ABC-DEF-GHI")]
    [InlineData("aaa-aaa-aaa", "AAA-AAA-AAA")]
    [InlineData("  a b c  ", "ABC")]
    public void FormatMasksInputIntoGroupsOfThree(string input, string expected) =>
        Assert.Equal(expected, SeedCode.Format(input));

    [Fact]
    public void FormatKeepsOnlyAsciiLetters()
    {
        // Turkish dotless "ı" is not an ASCII letter, so the mask drops it —
        // where a locale-independent C# ToUpperInvariant folded it to "I" and
        // silently produced a different seed. Same for accents and kana.
        Assert.Equal("STA-NBU-L", SeedCode.Format("ıstanbul"));
        Assert.Equal("CAF", SeedCode.Format("café"));
        Assert.Equal("", SeedCode.Format("日本語"));
        Assert.Equal("ABC", SeedCode.Format("aβbγc"));
    }

    [Fact]
    public void ParseReturnsTheCanonicalCodeAndItsNumericValue()
    {
        Assert.Equal(("AAA-AAA-AAA", 0UL), SeedCode.TryParse("AAA-AAA-AAA"));
        Assert.Equal(("AAA-AAA-AAB", 1UL), SeedCode.TryParse("AAA-AAA-AAB"));
        Assert.Equal(("AAA-AAA-ABA", 26UL), SeedCode.TryParse("AAA-AAA-ABA"));
        Assert.Equal(("ZZZ-ZZZ-ZZZ", 5_429_503_678_975UL), SeedCode.TryParse("ZZZ-ZZZ-ZZZ"));
        Assert.Equal(1UL, SeedCode.Value("AAA-AAA-AAB"));
    }

    [Fact]
    public void ParseFollowsTheGamesOwnLenience()
    {
        // A properly dashed code is case-insensitive; an undashed one is not,
        // matching the upstream implementation.
        Assert.Equal(("AAA-AAA-AAB", 1UL), SeedCode.TryParse("aaa-aaa-aab"));
        Assert.Equal(("AAA-AAA-AAB", 1UL), SeedCode.TryParse("AAAAAAAAB"));
        Assert.Null(SeedCode.TryParse("aaaaaaaab"));
        // The uppercasing of a properly dashed code is the game's, quirks and
        // all: Turkish dotless "ı" folds to "I" here, where the old C# parser
        // rejected it outright.
        Assert.Equal(("AAA-AAA-AAI", 8UL), SeedCode.TryParse("AAA-AAA-Aaı"));
    }

    [Theory]
    [InlineData("")]
    [InlineData("AAA-AAA-AA")]
    [InlineData("AAA-AAA-AAAA")]
    [InlineData("AAA-AAA-AA0")]
    [InlineData("日本語テキスト")]
    // Nine code units, but not nine seed digits.
    [InlineData("日本語テキスト日本")]
    public void TextThatIsNotASeedCodeIsRefused(string input)
    {
        Assert.Null(SeedCode.TryParse(input));
        Assert.False(SeedCode.IsCanonical(input));
        Assert.Throws<ArgumentException>(() => SeedCode.Value(input));
    }

    [Fact]
    public void MaskedInputIsCanonicalOnceItIsComplete()
    {
        Assert.False(SeedCode.IsCanonical(SeedCode.Format("abcdefgh")));
        Assert.True(SeedCode.IsCanonical(SeedCode.Format("abcdefghi")));
        Assert.True(SeedCode.IsCanonical(SeedCode.Format("a1b2c3d4e5f6g7h8i9j")));
    }
}
