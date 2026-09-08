using System.Text.Json.Nodes;
using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The app keeps local copies of the engine's scalar constants so the editor
/// needs nothing from the engine to open. This is the one place they meet the
/// engine: every local is asserted against the <c>engine_info</c> document
/// the linked engine publishes, so a change on either side fails here rather
/// than as an editor offering a query the search refuses.
/// </summary>
public sealed class EngineConstantsTests
{
    private static readonly JsonObject Info = (JsonObject)JsonNode.Parse(NativeEngine.EngineInfoJson())!;
    private static readonly JsonObject Limits = (JsonObject)Info["limits"]!;

    private static int Limit(string key) => (int)Limits[key]!;

    [Fact]
    public void QueryBoundsMatchTheEngine()
    {
        Assert.Equal(SearchLimits.MaxDepth, Limit("maxDepth"));
        Assert.Equal(SearchLimits.ExactTierMin, Limit("exactTierMin"));
        Assert.Equal(SearchLimits.ExactTierMax, Limit("exactTierMax"));
        Assert.Equal(SearchLimits.BoundedTierMin, Limit("boundedTierMin"));
        Assert.Equal(SearchLimits.BoundedTierMax, Limit("boundedTierMax"));
        Assert.Equal(SearchLimits.IdentityGroupMax, Limit("identityGroupMax"));
        Assert.Equal(SearchLimits.LevelSumGroupMax, Limit("levelSumGroupMax"));
        Assert.Equal(SearchLimits.MaxUpgradeDefault, Limit("maxUpgradeDefault"));
        Assert.Equal(SearchLimits.MaxUpgradeRing, Limit("maxUpgradeRing"));
        Assert.Equal(SearchLimits.MaxUpgradeRingStandard, Limit("maxUpgradeRingStandard"));
        Assert.Equal(SearchLimits.MaxUpgradeWeapon, Limit("maxUpgradeWeapon"));
        // The families route to the right maximum, narrowed weapon kinds included.
        Assert.Equal(Limit("maxUpgradeRing"), ItemKind.Ring.MaximumSearchUpgrade());
        foreach (var kind in new[] { ItemKind.Weapon, ItemKind.MeleeWeapon, ItemKind.ThrownWeapon })
            Assert.Equal(Limit("maxUpgradeWeapon"), kind.MaximumSearchUpgrade());
        foreach (var kind in new[] { ItemKind.Armor, ItemKind.Wand })
            Assert.Equal(Limit("maxUpgradeDefault"), kind.MaximumSearchUpgrade());
        // Every family's ceiling is the engine's own, keyed by the document's name for it.
        foreach (var (name, kind) in new[] {
            ("weapon", ItemKind.Weapon), ("armor", ItemKind.Armor), ("wand", ItemKind.Wand), ("ring", ItemKind.Ring), ("artifact", ItemKind.Artifact) })
            Assert.Equal((int)((JsonObject)Limits["maxUpgradeByKind"]!)[name]!, kind.MaximumSearchUpgrade());
        Assert.Equal(SearchLimits.MaxUpgradeAnyTier, Limit("maxUpgradeAnyTier"));
        Assert.Equal(SearchLimits.ExtraUpgradeTier, Limit("extraUpgradeTier"));
        Assert.Equal(SearchLimits.MaxDepth, new QuerySettings().MaximumDepth);
    }

    /// <summary>
    /// Only a tier-4 weapon is levelled past the shared ceiling, so a
    /// requirement that rules that tier out loses the top of its range.
    /// </summary>
    [Fact]
    public void TopWeaponUpgradeNeedsTheTierThatReachesIt()
    {
        var ceiling = Limit("maxUpgradeWeapon");
        var capped = Limit("maxUpgradeAnyTier");
        var extraTier = Limit("extraUpgradeTier");
        Assert.Equal(ceiling, ItemKind.Weapon.MaximumSearchUpgrade(null, TierMatch.Any, 0));
        Assert.Equal(ceiling, ItemKind.Weapon.MaximumSearchUpgrade(null, TierMatch.Exactly, extraTier));
        Assert.Equal(capped, ItemKind.Weapon.MaximumSearchUpgrade(null, TierMatch.Exactly, 5));
        Assert.Equal(capped, ItemKind.Weapon.MaximumSearchUpgrade(null, TierMatch.AtMost, 3));
        Assert.Equal(ceiling, ItemKind.Weapon.MaximumSearchUpgrade(ItemCatalog.Find("battle_axe"), TierMatch.Any, 0));
        Assert.Equal(ceiling, ItemKind.ThrownWeapon.MaximumSearchUpgrade(ItemCatalog.Find("javelin"), TierMatch.Any, 0));
        Assert.Equal(capped, ItemKind.Weapon.MaximumSearchUpgrade(ItemCatalog.Find("sword"), TierMatch.Any, 0));
        Assert.Equal(capped, ItemKind.Armor.MaximumSearchUpgrade(null, TierMatch.Exactly, extraTier));
    }

    [Fact]
    public void SessionAndFileLimitsMatchTheEngine()
    {
        Assert.Equal(SearchLimits.ResultCap, (int)Info["maxResults"]!);
        // The import byte cap has no local copy: the app reads it from the
        // engine at runtime (EngineInfo.ResultsFileMaxBytes) and the codec
        // applies it itself. Pin that the runtime reader agrees with the
        // document this test reads.
        Assert.Equal(Limit("resultsFileMaxBytes"), EngineInfo.ResultsFileMaxBytes);
        Assert.Equal((string?)Info["shpdVersion"], EngineInfo.ShpdVersion);
        Assert.Equal((string?)Info["shpdCommit"], EngineInfo.ShpdCommit);
    }

    [Fact]
    public void EmptyBossFloorsMatchTheEngine()
    {
        var floors = ((JsonArray)Info["emptyBossFloors"]!).Select(floor => (int)floor!).ToArray();
        Assert.Equal(floors, FloorLimits.EmptyBossFloors);
        Assert.Equal(Enumerable.Range(1, SearchLimits.MaxDepth).Except(floors), FloorLimits.Options);
    }

    [Theory]
    [InlineData("ghost", QuestGiver.Ghost)]
    [InlineData("wandmaker", QuestGiver.Wandmaker)]
    [InlineData("blacksmith", QuestGiver.Blacksmith)]
    [InlineData("imp", QuestGiver.Imp)]
    public void QuestWindowsMatchTheEngine(string name, QuestGiver giver)
    {
        var window = ((JsonArray)Info["questWindows"]![name]!).Select(floor => (int)floor!).ToArray();
        Assert.Equal(2, window.Length);
        Assert.Equal((window[0], window[1]), ScoutQuests.Window(giver));
    }

    [Fact]
    public void ChallengesMatchTheEngineInMaskOrder()
    {
        var engine = ((JsonArray)Info["challenges"]!)
            .Select(entry => ((string)entry!["name"]!, (int)entry["mask"]!, (bool)entry["changesLevelGeneration"]!))
            .ToArray();
        var local = Challenges.All.Select(entry => (entry.Name, entry.Mask, entry.ChangesLevelGeneration)).ToArray();
        Assert.Equal(engine, local);
        for (var index = 0; index < local.Length; index++) Assert.Equal(1 << index, local[index].Mask);
        Assert.Equal(engine.Aggregate(0, (mask, entry) => mask | entry.Item2), Challenges.AllMask);
    }
}
