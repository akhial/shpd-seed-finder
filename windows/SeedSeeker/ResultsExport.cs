using System.Collections.ObjectModel;
using System.Text.Json.Nodes;

namespace SeedSeeker;

/// <summary>User-facing failure while reading a results file.</summary>
public sealed class ResultsExportException(string message) : Exception(message);

/// <summary>
/// The cross-platform results-export document: search results plus the query
/// that found them.
///
/// The codec itself is the engine's — <c>seedfinder_results_encode</c> and
/// <c>seedfinder_results_decode</c> over the Rust core
/// (crates/seedfinder-core/src/results_export.rs; the schema is documented in
/// docs/results-export-format.md). It owns the envelope, the compatibility
/// rules, the 2 MiB import cap, the dedupe-and-cap step and every query
/// validation, so this file is only the mapping between the canonical query
/// document and <see cref="QuerySettings"/>. A document that arrives from the
/// engine has already been validated; the mapping is therefore lenient and
/// fails only on content it genuinely cannot represent.
/// </summary>
public static class ResultsExport
{
    public const string SuggestedFileName = "seed-seeker-results";

    /// <param name="Dropped">Exported entries the engine's dedupe-and-cap step removed.</param>
    public sealed record Imported(QuerySettings Query, IReadOnlyList<string> Seeds, int Dropped, string? FileShpdVersion);

    /// <summary>Stable document names, indexed by the matching enum value.</summary>
    private static readonly string[] KindNames = ["weapon", "armor", "wand", "ring", "melee_weapon", "thrown_weapon"];
    private static readonly string[] SourceNames = [
        "heap", "chest", "locked_chest", "crystal_chest", "tomb", "skeleton",
        "sacrificial_fire", "mimic", "golden_mimic", "crystal_mimic", "statue",
        "armored_statue", "shop", "ghost_reward", "wandmaker_reward",
        "blacksmith_reward", "imp_reward",
    ];
    private static readonly (string Name, int Bit)[] ChallengeNames =
        [.. Challenges.All.Select(entry => (entry.Name, entry.Mask))];

    /// <exception cref="ResultsExportException">With a user-facing message.</exception>
    public static string Encode(QuerySettings query, IEnumerable<string> seeds, string appVersion)
    {
        var request = new JsonObject
        {
            ["query"] = EncodeQuery(query),
            ["seeds"] = new JsonArray([.. seeds.Select(seed => (JsonNode)seed)]),
            ["app_version"] = appVersion,
        };
        return NativeEngine.TryEncodeResultsFile(request.ToJsonString())
            ?? throw new ResultsExportException("These results could not be written to a results file.");
    }

    /// <exception cref="ResultsExportException">With a user-facing message.</exception>
    public static Imported Decode(string text)
    {
        var decoded = NativeEngine.TryDecodeResultsFile(text)
            ?? throw new ResultsExportException(
                "This is not a Seed Seeker results file, or its query is not one this version can run.");
        if (JsonNode.Parse(decoded) is not JsonObject document || document["query"] is not JsonObject queryValue)
            throw new ResultsExportException("This results file could not be read.");
        var seeds = new List<string>();
        foreach (var entry in document["seeds"] as JsonArray ?? [])
            if (entry is JsonValue seedValue && seedValue.TryGetValue(out string? seed)) seeds.Add(seed);
        return new Imported(DecodeQuery(queryValue), seeds, IntField(document, "dropped") ?? 0,
            TolerantString(document, "shpd_version"));
    }

    /// <summary>Reads informational envelope strings; wrong types are ignored, not errors.</summary>
    private static string? TolerantString(JsonObject document, string key) =>
        document[key] is JsonValue value && value.TryGetValue(out string? text) ? text : null;

    /// <summary>The bare canonical JSON query document, the share-link codec's input.</summary>
    public static string EncodeQueryDocument(QuerySettings query) => EncodeQuery(query).ToJsonString();

    /// <summary>
    /// Decodes a bare canonical JSON query document, the share-link codec's
    /// output — which the engine has already validated.
    /// </summary>
    /// <exception cref="ResultsExportException">With a user-facing message.</exception>
    public static QuerySettings DecodeQueryDocument(string text)
    {
        if (JsonNode.Parse(text) is not JsonObject document)
            throw new ResultsExportException("The shared query could not be read.");
        return DecodeQuery(document);
    }

    private static JsonObject EncodeQuery(QuerySettings query)
    {
        var output = new JsonObject
        {
            ["requirements"] = new JsonArray([.. query.Requirements.Select(r => (JsonNode)EncodeRequirement(r))]),
        };
        if (query.MaximumDepth != 24) output["max_depth"] = query.MaximumDepth;
        if (query.RequireBlacksmith) output["require_blacksmith"] = true;
        if (query.ExcludeBlacksmithRewards) output["exclude_blacksmith_rewards"] = true;
        if (WandmakerQuests.DocumentName(query.WandmakerQuest) is string quest) output["wandmaker_quest"] = quest;
        if (query.FastMode) output["fast_mode"] = true;
        var challenges = ChallengeNames.Where(c => (query.Challenges & c.Bit) != 0).Select(c => c.Name).ToArray();
        if (challenges.Length != 0)
            output["challenges"] = new JsonArray([.. challenges.Select(name => (JsonNode)name)]);
        return output;
    }

    private static JsonObject EncodeRequirement(ItemRequirement requirement)
    {
        var output = new JsonObject { ["kind"] = KindNames[(int)requirement.Kind] };
        if (requirement.Item is not null) output["item"] = requirement.Item.Id;
        output["tier"] = requirement.TierMatch switch
        {
            TierMatch.Exactly => new JsonObject { ["exact"] = requirement.Tier },
            TierMatch.AtLeast => new JsonObject { ["at_least"] = requirement.Tier },
            TierMatch.AtMost => new JsonObject { ["at_most"] = requirement.Tier },
            _ => null,
        };
        if (output["tier"] is null) output.Remove("tier");
        output["upgrade"] = requirement.UpgradeMatch switch
        {
            UpgradeMatch.Exactly => JsonValue.Create(requirement.Upgrade),
            UpgradeMatch.AtLeast => new JsonObject { ["at_least"] = requirement.Upgrade },
            _ => null,
        };
        if (output["upgrade"] is null) output.Remove("upgrade");
        if (requirement.Modifier is not null) output["effect"] = requirement.Modifier;
        if (requirement.RequireUncursed) output["uncursed"] = true;
        if (requirement.Source is ScoutItemSource source) output["source"] = SourceNames[(int)source];
        if (requirement.IdentityGroup is int group) output["identity_group"] = group;
        if (requirement.MaximumDepth is int depth) output["max_depth"] = depth;
        return output;
    }

    private static QuerySettings DecodeQuery(JsonObject value)
    {
        var requirements = new ObservableCollection<ItemRequirement>();
        foreach (var entry in value["requirements"] as JsonArray ?? [])
            if (entry is JsonObject requirement) requirements.Add(DecodeRequirement(requirement));
        var challenges = 0;
        foreach (var nameValue in value["challenges"] as JsonArray ?? [])
        {
            string? name = null;
            if (nameValue is JsonValue nameJson) nameJson.TryGetValue(out name);
            var match = ChallengeNames.FirstOrDefault(c => c.Name == name);
            if (match.Name is not null) challenges |= match.Bit;
        }
        return new QuerySettings
        {
            Requirements = requirements,
            MaximumDepth = IntField(value, "max_depth") ?? 24,
            RequireBlacksmith = BoolField(value, "require_blacksmith"),
            ExcludeBlacksmithRewards = BoolField(value, "exclude_blacksmith_rewards"),
            WandmakerQuest = TolerantString(value, "wandmaker_quest") is string questName
                ? WandmakerQuests.Named(questName) ?? WandmakerQuest.Any
                : WandmakerQuest.Any,
            FastMode = BoolField(value, "fast_mode"),
            Challenges = challenges,
        };
    }

    /// <exception cref="ResultsExportException">
    /// When the requirement names catalog content this build does not know —
    /// the one failure the engine cannot rule out, since the item and effect
    /// tables are the app's own copies of the shared catalog asset.
    /// </exception>
    private static ItemRequirement DecodeRequirement(JsonObject entry)
    {
        CatalogItem? item = null;
        if (TolerantString(entry, "item") is string id)
            item = ItemCatalog.Find(id) ?? throw new ResultsExportException($"This query names an unknown item \"{id}\".");
        ItemKind kind;
        if (TolerantString(entry, "kind") is string kindName)
        {
            var index = Array.IndexOf(KindNames, kindName);
            if (index < 0) throw new ResultsExportException($"This query names an unknown category \"{kindName}\".");
            kind = (ItemKind)index;
        }
        else
        {
            kind = item?.Kind ?? ItemKind.Weapon;
        }
        string? modifier = null;
        if (TolerantString(entry, "effect") is string effectName)
        {
            modifier = ItemCatalog.Modifiers(kind)
                .FirstOrDefault(known => string.Equals(known, effectName, StringComparison.OrdinalIgnoreCase))
                ?? throw new ResultsExportException($"This query names an unknown effect \"{effectName}\".");
        }
        ScoutItemSource? source = null;
        if (TolerantString(entry, "source") is string sourceName)
        {
            var index = Array.IndexOf(SourceNames, sourceName);
            if (index < 0) throw new ResultsExportException($"This query names an unknown source \"{sourceName}\".");
            source = (ScoutItemSource)index;
        }
        var (tier, tierMatch) = DecodeTier(entry["tier"]);
        var (upgrade, upgradeMatch) = DecodeUpgrade(entry["upgrade"]);
        return new ItemRequirement
        {
            Item = item,
            Upgrade = upgrade,
            Modifier = modifier,
            Kind = kind,
            Tier = tier,
            TierMatch = tierMatch,
            UpgradeMatch = upgradeMatch,
            Source = source,
            IdentityGroup = IntField(entry, "identity_group"),
            MaximumDepth = IntField(entry, "max_depth"),
            RequireUncursed = BoolField(entry, "uncursed"),
        };
    }

    private static (int, TierMatch) DecodeTier(JsonNode? value) => value switch
    {
        JsonObject filter when IntField(filter, "exact") is int exact => (exact, TierMatch.Exactly),
        JsonObject filter when IntField(filter, "at_least") is int atLeast => (atLeast, TierMatch.AtLeast),
        JsonObject filter when IntField(filter, "at_most") is int atMost => (atMost, TierMatch.AtMost),
        _ => (0, TierMatch.Any),
    };

    private static (int, UpgradeMatch) DecodeUpgrade(JsonNode? value) => value switch
    {
        JsonValue number when number.TryGetValue(out int upgrade) => (upgrade, UpgradeMatch.Exactly),
        JsonObject filter when IntField(filter, "exact") is int exact => (exact, UpgradeMatch.Exactly),
        JsonObject filter when IntField(filter, "at_least") is int atLeast => (atLeast, UpgradeMatch.AtLeast),
        _ => (0, UpgradeMatch.Any),
    };

    /// <summary>Reads a whole number; absent or wrong-typed values read as absent.</summary>
    private static int? IntField(JsonObject entry, string key) =>
        entry[key] is JsonValue value && value.TryGetValue(out int number) ? number : null;

    /// <summary>Reads a flag; absent or wrong-typed values read as false.</summary>
    private static bool BoolField(JsonObject entry, string key) =>
        entry[key] is JsonValue value && value.TryGetValue(out bool flag) && flag;
}
