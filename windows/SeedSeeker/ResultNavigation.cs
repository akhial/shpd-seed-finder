namespace SeedSeeker;

/// <summary>
/// Navigation through the ordered list of search-result seeds while scouting.
///
/// The seed detail pane can be reached either from a search result or by
/// typing a seed by hand; navigation is only meaningful in the first case, so
/// every helper returns null when the current seed is not a search result.
/// Pure so the logic stays testable off-Windows (see SeedSeeker.Tests).
/// </summary>
public static class ResultNavigation
{
    /// <summary>0-based index of <paramref name="seed"/> within <paramref name="seeds"/>, or null when it is not one of them.</summary>
    public static int? IndexOf(IReadOnlyList<string> seeds, string? seed)
    {
        if (string.IsNullOrEmpty(seed)) return null;
        for (var index = 0; index < seeds.Count; index++)
            if (seeds[index] == seed) return index;
        return null;
    }

    /// <summary>
    /// Index of the result <paramref name="delta"/> steps away from
    /// <paramref name="seed"/>, clamped to the list ends. Null when the seed
    /// is not a search result or the step would not move (already at the
    /// first or last result).
    /// </summary>
    public static int? Step(IReadOnlyList<string> seeds, string? seed, int delta)
    {
        if (IndexOf(seeds, seed) is not int index) return null;
        var target = Math.Clamp(index + delta, 0, seeds.Count - 1);
        return target == index ? null : target;
    }
}
