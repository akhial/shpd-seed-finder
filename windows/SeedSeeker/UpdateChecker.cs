// SPDX-License-Identifier: GPL-3.0-or-later
using System.Net.Http.Headers;
using System.Text.Json;

namespace SeedSeeker;

/// <summary>A newer published release, as reported by the GitHub Releases API.</summary>
public sealed record UpdateInfo(string Version, string Url);

public static class UpdateChecker
{
    public const string ReleasesPage = "https://github.com/akhial/shpd-seed-seeker/releases/latest";
    private const string ApiUrl = "https://api.github.com/repos/akhial/shpd-seed-seeker/releases/latest";

    /// <summary>
    /// The SEED_SEEKER_FAKE_LATEST environment variable stands in for the
    /// latest release tag, bypassing the network and the daily throttle.
    /// </summary>
    public static string? FakeLatest => Environment.GetEnvironmentVariable("SEED_SEEKER_FAKE_LATEST");

    /// <summary>
    /// Returns the latest release when it is strictly newer than <paramref name="current"/>;
    /// null when up to date or on any network or parsing failure.
    /// </summary>
    public static async Task<UpdateInfo?> CheckAsync(string current)
    {
        if (FakeLatest is { Length: > 0 } fake) return Newer(fake, current, ReleasesPage);
        try
        {
            using var client = new HttpClient { Timeout = TimeSpan.FromSeconds(10) };
            // The GitHub API rejects requests without a User-Agent.
            client.DefaultRequestHeaders.UserAgent.Add(new ProductInfoHeaderValue("SeedSeeker", current));
            client.DefaultRequestHeaders.Accept.Add(new MediaTypeWithQualityHeaderValue("application/vnd.github+json"));
            using var document = JsonDocument.Parse(await client.GetStringAsync(ApiUrl));
            var tag = document.RootElement.GetProperty("tag_name").GetString() ?? "";
            var url = document.RootElement.TryGetProperty("html_url", out var html) ? html.GetString() : null;
            return Newer(tag, current, string.IsNullOrEmpty(url) ? ReleasesPage : url);
        }
        catch
        {
            return null;
        }
    }

    internal static UpdateInfo? Newer(string latest, string current, string url)
    {
        if (Parse(latest) is not { } latestParts || Parse(current) is not { } currentParts) return null;
        for (var index = 0; index < Math.Max(latestParts.Length, currentParts.Length); index++)
        {
            var left = index < latestParts.Length ? latestParts[index] : 0;
            var right = index < currentParts.Length ? currentParts[index] : 0;
            if (left != right) return left > right ? new UpdateInfo(DisplayVersion(latest), url) : null;
        }
        return null;
    }

    /// <summary>Strips the tag prefix and any pre-release suffix: "v1.2.3-beta" → "1.2.3".</summary>
    internal static string DisplayVersion(string tag)
    {
        var bare = tag.Trim();
        if (bare.StartsWith('v') || bare.StartsWith('V')) bare = bare[1..];
        var dash = bare.IndexOf('-');
        return dash >= 0 ? bare[..dash] : bare;
    }

    private static int[]? Parse(string version)
    {
        var parts = DisplayVersion(version).Split('.');
        var numbers = new int[parts.Length];
        for (var index = 0; index < parts.Length; index++)
        {
            if (!int.TryParse(parts[index], out numbers[index])) return null;
        }
        return numbers.Length > 0 ? numbers : null;
    }
}
