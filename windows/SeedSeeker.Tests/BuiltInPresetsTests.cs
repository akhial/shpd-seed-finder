using Xunit;

namespace SeedSeeker.Tests;

/// <summary>
/// The queries the app ships as read-only presets: every one must pass the same
/// local validation the editor runs before the engine is asked, and the two
/// vault presets must ask for the levels only the Imp's vault reaches.
/// </summary>
public sealed class BuiltInPresetsTests
{
    [Fact]
    public void EveryPresetIsARunnableQuery()
    {
        foreach (var preset in BuiltInPresets.All)
            Assert.Null(QueryRelationships.Validate(preset.Query));
    }

    [Fact]
    public void Staff22AsksForTheVaultWand()
    {
        var preset = Assert.Single(BuiltInPresets.All, entry => entry.Name == "+22 Staff");
        var requirements = preset.Query.Requirements;
        Assert.Equal(BuiltInPresets.VaultFloorLimit, preset.Query.MaximumDepth);
        Assert.All(requirements, requirement => Assert.Equal(ItemKind.Wand, requirement.Kind));
        Assert.Equal([UpgradeMatch.Exactly, UpgradeMatch.Any, UpgradeMatch.Any, UpgradeMatch.AtLeast],
            requirements.Select(requirement => requirement.UpgradeMatch));
        Assert.Equal([4, 0, 0, 1], requirements.Select(requirement => requirement.Upgrade));
        Assert.Equal(new int?[] { 1, 1, 1, null }, requirements.Select(requirement => requirement.IdentityGroup));
    }

    [Fact]
    public void Tier4WeaponStacksTwoCopiesOnAPlusFive()
    {
        var preset = Assert.Single(BuiltInPresets.All, entry => entry.Name == "+26 Tier 4 Weapon");
        var requirements = preset.Query.Requirements;
        Assert.Equal(BuiltInPresets.VaultFloorLimit, preset.Query.MaximumDepth);
        Assert.Equal(3, requirements.Count);
        Assert.All(requirements, requirement =>
        {
            Assert.Equal(ItemKind.Weapon, requirement.Kind);
            Assert.Equal(1, requirement.IdentityGroup);
        });
        Assert.Equal(TierMatch.Exactly, requirements[0].TierMatch);
        Assert.Equal(4, requirements[0].Tier);
        Assert.Equal(UpgradeMatch.Exactly, requirements[0].UpgradeMatch);
        Assert.Equal(5, requirements[0].Upgrade);
        // Only the anchor may constrain the item a stack binds to.
        Assert.All(requirements.Skip(1), requirement => Assert.True(requirement.IsBare));
    }
}
