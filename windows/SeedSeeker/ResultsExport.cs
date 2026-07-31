using System.Collections.ObjectModel;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace SeedSeeker;

/// <summary>User-facing failure while reading a results file.</summary>
public sealed class ResultsExportException(string message) : Exception(message);

/// <summary>
/// The cross-platform results-export document: search results plus the query
/// that found them.
///
/// The canonical implementation and compatibility rules live in the Rust core
/// (crates/seedfinder-core/src/results_export.rs, with the embedded query
/// document defined by json_query.rs); the schema is documented in
/// docs/results-export-format.md. Keep this codec schema-compatible with it:
/// unknown envelope and per-result fields are ignored, files declaring a newer
/// format_version are rejected with an "update the app" message, and unknown
/// or wrong-typed query content fails the import instead of silently changing
/// its meaning.
/// </summary>
public static partial class ResultsExport
{
    public const string FileFormat = "seed-seeker-results";
    public const int FormatVersion = 1;
    public const string SuggestedFileName = "seed-seeker-results";
    /// <summary>Mirrors the Rust core's SHPD_VERSION, the source of truth.</summary>
    public const string ShpdVersion = "3.3.8";
    /// <summary>Import size cap; a maximal legal results file is far below this.</summary>
    public const int MaxFileBytes = 2 * 1024 * 1024;

    public sealed record Imported(QuerySettings Query, IReadOnlyList<string> Seeds, string? FileShpdVersion);

    /// <summary>Stable document names, indexed by the matching enum value.</summary>
    // Indexed by the matching enum value; the narrowed weapon kinds are
    // additive within format version 1.
    private static readonly string[] KindNames = ["weapon", "armor", "wand", "ring", "melee_weapon", "thrown_weapon"];
    private static readonly string[] SourceNames = [
        "heap", "chest", "locked_chest", "crystal_chest", "tomb", "skeleton",
        "sacrificial_fire", "mimic", "golden_mimic", "crystal_mimic", "statue",
        "armored_statue", "shop", "ghost_reward", "wandmaker_reward",
        "blacksmith_reward", "imp_reward",
    ];
    private static readonly (string Name, int Bit)[] ChallengeNames = [
        ("on_diet", 1), ("faith_is_my_armor", 2), ("pharmacophobia", 4),
        ("barren_land", 8), ("swarm_intelligence", 16), ("into_darkness", 32),
        ("forbidden_runes", 64), ("hostile_champions", 128), ("badder_bosses", 256),
    ];
    private static readonly HashSet<string> QueryKeys = [
        "requirements", "max_depth", "require_blacksmith",
        "exclude_blacksmith_rewards", "fast_mode", "challenges",
    ];
    private static readonly HashSet<string> RequirementKeys = [
        "kind", "item", "tier", "upgrade", "effect", "uncursed", "source",
        "identity_group", "max_depth", "upgrade_sum",
    ];

    [GeneratedRegex("^[A-Z]{3}-[A-Z]{3}-[A-Z]{3}$")]
    private static partial Regex SeedCodePattern();

    public static string Encode(QuerySettings query, IEnumerable<string> seeds, string appVersion)
    {
        var document = new JsonObject
        {
            ["format"] = FileFormat,
            ["format_version"] = FormatVersion,
            ["app_version"] = appVersion,
            ["shpd_version"] = ShpdVersion,
            ["query"] = EncodeQuery(query),
            ["results"] = new JsonArray([.. seeds.Select(seed => (JsonNode)new JsonObject { ["seed"] = seed })]),
        };
        return document.ToJsonString(new JsonSerializerOptions { WriteIndented = true });
    }

    /// <exception cref="ResultsExportException">With a user-facing message.</exception>
    public static Imported Decode(string text)
    {
        JsonObject document;
        try
        {
            document = JsonNode.Parse(text) as JsonObject
                ?? throw new ResultsExportException("This is not a Seed Seeker results file.");
        }
        catch (JsonException)
        {
            throw new ResultsExportException("This is not a Seed Seeker results file (not valid JSON).");
        }
        // TryGetValue-based reads throughout: GetValue<string>() would throw a
        // raw .NET exception on number or boolean nodes.
        if (TolerantString(document, "format") != FileFormat)
            throw new ResultsExportException("This is not a Seed Seeker results file.");
        var versionNode = document["format_version"];
        if (versionNode is null)
            throw new ResultsExportException("This results file is missing its format version.");
        if (versionNode is not JsonValue versionValue
            || !versionValue.TryGetValue(out int version) || version < 1)
            throw new ResultsExportException(
                "This results file does not declare a valid format version (a positive whole number).");
        if (version > FormatVersion)
            throw new ResultsExportException(
                $"This results file uses format version {version}, but this app understands " +
                $"up to version {FormatVersion}. Update Seed Seeker to import it.");
        if (document["query"] is not JsonObject queryValue)
            throw new ResultsExportException("This results file is missing its query.");
        var query = DecodeQuery(queryValue);
        if (document["results"] is not JsonArray resultsValue)
            throw new ResultsExportException("This results file is missing its results list.");
        var seeds = new List<string>();
        foreach (var (entry, index) in resultsValue.Select((entry, index) => (entry, index)))
        {
            string? seed = null;
            if ((entry as JsonObject)?["seed"] is JsonValue seedValue) seedValue.TryGetValue(out seed);
            if (seed is null || !SeedCodePattern().IsMatch(seed))
                throw new ResultsExportException(
                    $"Result {index + 1} does not have a valid seed code (canonical XXX-XXX-XXX form).");
            seeds.Add(seed);
        }
        return new Imported(query, seeds, TolerantString(document, "shpd_version"));
    }

    /// <summary>Reads informational envelope strings; wrong types are ignored, not errors.</summary>
    private static string? TolerantString(JsonObject document, string key) =>
        document[key] is JsonValue value && value.TryGetValue(out string? text) ? text : null;

    private static JsonObject EncodeQuery(QuerySettings query)
    {
        // Alternative groups serialize as one any_of entry at the first
        // member's position, holding every member in requirement order; decode
        // assigns the groups fresh sequential ids, preserving the structure.
        var entries = new JsonArray();
        var emittedGroups = new HashSet<int>();
        foreach (var requirement in query.Requirements)
        {
            if (requirement.AlternativeGroup is not int group)
            {
                entries.Add((JsonNode)EncodeRequirement(requirement));
                continue;
            }
            if (!emittedGroups.Add(group)) continue;
            var members = query.Requirements.Where(r => r.AlternativeGroup == group)
                .Select(r => (JsonNode)EncodeRequirement(r)).ToList();
            // A group that shrank to one member is no longer an alternative.
            entries.Add(members.Count == 1 ? members[0] : new JsonObject { ["any_of"] = new JsonArray([.. members]) });
        }
        var output = new JsonObject { ["requirements"] = entries };
        if (query.MaximumDepth != 24) output["max_depth"] = query.MaximumDepth;
        if (query.RequireBlacksmith) output["require_blacksmith"] = true;
        if (query.ExcludeBlacksmithRewards) output["exclude_blacksmith_rewards"] = true;
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
        if (EncodeEffect(requirement) is JsonNode effect) output["effect"] = effect;
        if (requirement.RequireUncursed) output["uncursed"] = true;
        if (requirement.Source is ScoutItemSource source) output["source"] = SourceNames[(int)source];
        if (requirement.IdentityGroup is int group) output["identity_group"] = group;
        if (requirement.MaximumDepth is int depth) output["max_depth"] = depth;
        if (requirement is { UpgradeSumGroup: int sumGroup, UpgradeSumTotal: int sumTotal })
            output["upgrade_sum"] = new JsonObject { ["group"] = sumGroup, ["at_least"] = sumTotal };
        return output;
    }

    /// <summary>
    /// The effect predicate: absent for the wildcard, "any_enchantment" for the
    /// full non-curse family set, a bare name for one effect, and a name list
    /// otherwise, mirroring the core encoder.
    /// </summary>
    private static JsonNode? EncodeEffect(ItemRequirement requirement)
    {
        if (requirement.EffectMode == EffectMode.AnyEnchantment) return "any_enchantment";
        if (requirement.EffectMode != EffectMode.Specific || requirement.Effects.Count == 0) return null;
        var enchantments = ItemCatalog.NonCurse(requirement.Kind);
        if (requirement.Effects.Count == enchantments.Length && enchantments.All(requirement.Effects.Contains))
            return "any_enchantment";
        if (requirement.Effects.Count == 1) return requirement.Effects[0];
        return new JsonArray([.. requirement.Effects.Select(name => (JsonNode)name)]);
    }

    private static QuerySettings DecodeQuery(JsonObject value)
    {
        foreach (var pair in value)
        {
            if (!QueryKeys.Contains(pair.Key))
                throw new ResultsExportException(
                    $"The query in this results file uses an unknown field \"{pair.Key}\". " +
                    "Update Seed Seeker to import it.");
        }
        if (value["requirements"] is not JsonArray requirementsValue || requirementsValue.Count == 0)
            throw new ResultsExportException("The query in this results file has no requirements.");
        var requirements = new ObservableCollection<ItemRequirement>();
        var alternativeGroupCount = 0;
        foreach (var (entry, index) in requirementsValue.Select((entry, index) => (entry, index)))
        {
            if (entry is not JsonObject requirement)
                throw new ResultsExportException($"Requirement {index + 1} is not a JSON object.");
            try
            {
                // An entry is a plain requirement or an {"any_of": [...]}
                // group satisfied by any single member; the groups get fresh
                // sequential ids in entry order.
                if (requirement.ContainsKey("any_of"))
                    DecodeAlternatives(requirement, requirements, ref alternativeGroupCount);
                else
                    requirements.Add(DecodeRequirement(requirement, alternativeGroup: null));
            }
            catch (ResultsExportException failure)
            {
                throw new ResultsExportException($"Requirement {index + 1}: {failure.Message}");
            }
        }
        // Members of one combined-upgrade group must agree on the total,
        // mirroring the core validator (the editor keeps them in lockstep).
        foreach (var sumGroup in requirements.Where(r => r.UpgradeSumGroup is not null).GroupBy(r => r.UpgradeSumGroup))
        {
            if (sumGroup.Select(r => r.UpgradeSumTotal).Distinct().Count() > 1)
                throw new ResultsExportException(
                    $"The query in this results file uses different totals for combined upgrade group {sumGroup.Key}.");
        }
        var maximumDepth = IntField(value, "max_depth") ?? 24;
        if (maximumDepth is < 1 or > 24)
            throw new ResultsExportException("Maximum floor must be 1..24.");
        var challenges = 0;
        var challengesNode = value["challenges"];
        if (challengesNode is not null)
        {
            if (challengesNode is not JsonArray names)
                throw new ResultsExportException("\"challenges\" must be a list of challenge names");
            foreach (var nameValue in names)
            {
                string? name = null;
                if (nameValue is JsonValue nameJson) nameJson.TryGetValue(out name);
                // Challenge names match the core decoder exactly.
                var match = ChallengeNames.FirstOrDefault(c => c.Name == name);
                if (name is null || match.Name is null)
                    throw new ResultsExportException(
                        $"The query in this results file uses an unknown challenge \"{nameValue}\".");
                challenges |= match.Bit;
            }
        }
        return new QuerySettings
        {
            Requirements = requirements,
            MaximumDepth = maximumDepth,
            RequireBlacksmith = BoolField(value, "require_blacksmith"),
            ExcludeBlacksmithRewards = BoolField(value, "exclude_blacksmith_rewards"),
            FastMode = BoolField(value, "fast_mode"),
            Challenges = challenges,
        };
    }

    /// <summary>One any_of entry: an alternative group satisfied by any single member.</summary>
    private static void DecodeAlternatives(JsonObject entry,
        ObservableCollection<ItemRequirement> requirements, ref int alternativeGroupCount)
    {
        foreach (var pair in entry)
        {
            if (pair.Key != "any_of")
                throw new ResultsExportException(
                    $"unknown field \"{pair.Key}\" — update Seed Seeker to import it");
        }
        if (entry["any_of"] is not JsonArray members)
            throw new ResultsExportException("\"any_of\" must be a list of requirements");
        if (members.Count == 0)
            throw new ResultsExportException("any_of needs at least one alternative");
        if (alternativeGroupCount == 255)
            throw new ResultsExportException("too many any_of groups");
        alternativeGroupCount++;
        foreach (var member in members)
        {
            if (member is not JsonObject alternative)
                throw new ResultsExportException("each any_of alternative must be a JSON object");
            requirements.Add(DecodeRequirement(alternative, alternativeGroupCount));
        }
    }

    private static ItemRequirement DecodeRequirement(JsonObject entry, int? alternativeGroup)
    {
        foreach (var pair in entry)
        {
            if (!RequirementKeys.Contains(pair.Key))
                throw new ResultsExportException(
                    $"unknown field \"{pair.Key}\" — update Seed Seeker to import it");
        }
        CatalogItem? item = null;
        if (StringField(entry, "item") is string id)
            item = ItemCatalog.Find(id) ?? throw new ResultsExportException($"unknown item \"{id}\"");
        // Enum names match the core decoder exactly (lowercase snake_case);
        // only effect names and the "any" keyword match case-insensitively.
        ItemKind kind;
        if (StringField(entry, "kind") is string kindName)
        {
            var index = Array.IndexOf(KindNames, kindName);
            if (index < 0) throw new ResultsExportException($"unknown category \"{kindName}\"");
            kind = (ItemKind)index;
        }
        else if (item is not null)
        {
            kind = item.Kind;
        }
        else
        {
            throw new ResultsExportException("a category is required when no item is set");
        }
        if (item is not null && !kind.Accepts(item))
            throw new ResultsExportException("the item does not belong to this category");
        var (tier, tierMatch) = DecodeTier(entry["tier"]);
        var (upgrade, upgradeMatch) = DecodeUpgrade(entry["upgrade"]);
        var (effectMode, effects) = DecodeEffect(entry["effect"], kind);
        ScoutItemSource? source = null;
        if (StringField(entry, "source") is string sourceName)
        {
            var index = Array.IndexOf(SourceNames, sourceName);
            if (index < 0) throw new ResultsExportException($"unknown source \"{sourceName}\"");
            source = (ScoutItemSource)index;
        }
        var identityGroup = IntField(entry, "identity_group");
        if (identityGroup is < 1 or > 4)
            throw new ResultsExportException("same-item group must be A..D");
        var maximumDepth = IntField(entry, "max_depth");
        if (maximumDepth is < 1 or > 24)
            throw new ResultsExportException("item floor limit must be 1..24");
        var (upgradeSumGroup, upgradeSumTotal) = DecodeUpgradeSum(entry["upgrade_sum"], alternativeGroup);
        return new ItemRequirement
        {
            Item = item,
            Upgrade = upgrade,
            Kind = kind,
            Tier = tier,
            TierMatch = tierMatch,
            UpgradeMatch = upgradeMatch,
            EffectMode = effectMode,
            Effects = effects,
            Source = source,
            IdentityGroup = identityGroup,
            MaximumDepth = maximumDepth,
            RequireUncursed = BoolField(entry, "uncursed"),
            AlternativeGroup = alternativeGroup,
            UpgradeSumGroup = upgradeSumGroup,
            UpgradeSumTotal = upgradeSumTotal,
        };
    }

    /// <summary>
    /// The effect predicate: absent (any), one name, a list of same-family
    /// names, or "any_enchantment" for the full non-curse family set. Names
    /// match the family's effect table case-insensitively; a list spelling out
    /// the full set canonicalizes onto the shorthand's mode.
    /// </summary>
    private static (EffectMode Mode, List<string> Effects) DecodeEffect(JsonNode? value, ItemKind kind)
    {
        switch (value)
        {
            case null:
                return (EffectMode.Any, []);
            case JsonValue name when name.TryGetValue(out string? text):
                if (string.Equals(text, "any_enchantment", StringComparison.OrdinalIgnoreCase))
                {
                    if (kind.Family() is not (ItemKind.Weapon or ItemKind.Armor))
                        throw new ResultsExportException("\"any_enchantment\" requires a weapon or armor");
                    return (EffectMode.AnyEnchantment, []);
                }
                return (EffectMode.Specific, [ResolveEffect(kind, text)]);
            case JsonArray names:
            {
                if (names.Count == 0)
                    throw new ResultsExportException("the effect list needs at least one entry");
                var effects = new List<string>();
                foreach (var nameNode in names)
                {
                    string? text = null;
                    if (nameNode is JsonValue nameValue) nameValue.TryGetValue(out text);
                    if (text is null)
                        throw new ResultsExportException("the effect list must contain effect names");
                    var effect = ResolveEffect(kind, text);
                    if (!effects.Contains(effect)) effects.Add(effect);
                }
                var enchantments = ItemCatalog.NonCurse(kind);
                if (effects.Count == enchantments.Length && enchantments.All(effects.Contains))
                    return (EffectMode.AnyEnchantment, []);
                return (EffectMode.Specific, effects);
            }
            default:
                throw new ResultsExportException("unrecognized effect filter");
        }
    }

    private static string ResolveEffect(ItemKind kind, string name) =>
        ItemCatalog.Modifiers(kind)
            .FirstOrDefault(known => string.Equals(known, name, StringComparison.OrdinalIgnoreCase))
        ?? throw new ResultsExportException($"unknown effect \"{name}\"");

    private static (int? Group, int? Total) DecodeUpgradeSum(JsonNode? value, int? alternativeGroup)
    {
        if (value is null) return (null, null);
        if (value is not JsonObject sum || sum.Count != 2
            || IntField(sum, "group") is not int group || IntField(sum, "at_least") is not int atLeast)
            throw new ResultsExportException("\"upgrade_sum\" must be an object with \"group\" and \"at_least\"");
        if (alternativeGroup is not null)
            throw new ResultsExportException("a combined upgrade total cannot be used inside an any_of group");
        if (group is < 1 or > 255)
            throw new ResultsExportException("combined upgrade group must be 1..255");
        if (atLeast is < 1 or > 8)
            throw new ResultsExportException("combined upgrade total must be 1..8");
        return (group, atLeast);
    }

    private static (int, TierMatch) DecodeTier(JsonNode? value) => value switch
    {
        null => (0, TierMatch.Any),
        JsonValue name when name.TryGetValue(out string? text) =>
            string.Equals(text, "any", StringComparison.OrdinalIgnoreCase)
                ? (0, TierMatch.Any)
                : throw new ResultsExportException($"unknown tier mode \"{text}\""),
        JsonObject filter when filter.Count == 1 && IntField(filter, "exact") is int exact => (exact, TierMatch.Exactly),
        JsonObject filter when filter.Count == 1 && IntField(filter, "at_least") is int atLeast => (atLeast, TierMatch.AtLeast),
        JsonObject filter when filter.Count == 1 && IntField(filter, "at_most") is int atMost => (atMost, TierMatch.AtMost),
        _ => throw new ResultsExportException("unrecognized tier filter"),
    };

    private static (int, UpgradeMatch) DecodeUpgrade(JsonNode? value) => value switch
    {
        null => (0, UpgradeMatch.Any),
        JsonValue number when number.TryGetValue(out int upgrade) => (upgrade, UpgradeMatch.Exactly),
        JsonValue name when name.TryGetValue(out string? text) =>
            string.Equals(text, "any", StringComparison.OrdinalIgnoreCase)
                ? (0, UpgradeMatch.Any)
                : throw new ResultsExportException($"unknown upgrade mode \"{text}\""),
        JsonObject filter when filter.Count == 1 && IntField(filter, "exact") is int exact => (exact, UpgradeMatch.Exactly),
        JsonObject filter when filter.Count == 1 && IntField(filter, "at_least") is int atLeast => (atLeast, UpgradeMatch.AtLeast),
        _ => throw new ResultsExportException("unrecognized upgrade filter"),
    };

    // Strict typed readers: a present-but-wrong-type value is an error, never
    // coerced or treated as absent. A JSON null is parsed as a null JsonNode,
    // so explicit nulls count as absent, matching the core decoder.
    private static string? StringField(JsonObject entry, string key)
    {
        var node = entry[key];
        if (node is null) return null;
        if (node is JsonValue value && value.TryGetValue(out string? text)) return text;
        throw new ResultsExportException($"\"{key}\" must be a string");
    }

    private static int? IntField(JsonObject entry, string key)
    {
        var node = entry[key];
        if (node is null) return null;
        if (node is JsonValue value && value.TryGetValue(out int number)) return number;
        throw new ResultsExportException($"\"{key}\" must be a whole number");
    }

    private static bool BoolField(JsonObject entry, string key)
    {
        var node = entry[key];
        if (node is null) return false;
        if (node is JsonValue value && value.TryGetValue(out bool flag)) return flag;
        throw new ResultsExportException($"\"{key}\" must be true or false");
    }
}
