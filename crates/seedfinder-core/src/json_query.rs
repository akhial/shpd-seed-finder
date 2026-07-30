//! JSON search-query document decoding shared by the CLI and native frontends.

use crate::catalog::{Effect, ItemKind, item_by_stable_id};
use crate::challenges::Challenges;
use crate::model::ItemSource;
use crate::query::{
    EffectRequirement, EffectSet, Requirement, SearchQuery, TierRequirement, UpgradeRequirement,
    UpgradeSum,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryDocument {
    requirements: Vec<FileRequirementEntry>,
    #[serde(default = "default_max_depth")]
    max_depth: u8,
    #[serde(default)]
    require_blacksmith: bool,
    #[serde(default)]
    exclude_blacksmith_rewards: bool,
    #[serde(default)]
    fast_mode: bool,
    #[serde(default)]
    challenges: Vec<FileChallenge>,
}

/// One entry of the `requirements` array: a plain requirement, or an
/// `{"any_of": [...]}` group satisfied by any single member.
#[derive(Deserialize)]
#[serde(untagged)]
enum FileRequirementEntry {
    AnyOf(FileAnyOf),
    Single(FileRequirement),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileAnyOf {
    any_of: Vec<FileRequirement>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileChallenge {
    OnDiet,
    FaithIsMyArmor,
    Pharmacophobia,
    BarrenLand,
    SwarmIntelligence,
    IntoDarkness,
    ForbiddenRunes,
    HostileChampions,
    BadderBosses,
}

impl From<FileChallenge> for Challenges {
    fn from(value: FileChallenge) -> Self {
        match value {
            FileChallenge::OnDiet => Self::NO_FOOD,
            FileChallenge::FaithIsMyArmor => Self::NO_ARMOR,
            FileChallenge::Pharmacophobia => Self::NO_HEALING,
            FileChallenge::BarrenLand => Self::NO_HERBALISM,
            FileChallenge::SwarmIntelligence => Self::SWARM_INTELLIGENCE,
            FileChallenge::IntoDarkness => Self::DARKNESS,
            FileChallenge::ForbiddenRunes => Self::NO_SCROLLS,
            FileChallenge::HostileChampions => Self::CHAMPION_ENEMIES,
            FileChallenge::BadderBosses => Self::STRONGER_BOSSES,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRequirement {
    #[serde(default)]
    kind: Option<FileItemKind>,
    #[serde(default)]
    item: Option<String>,
    #[serde(default)]
    tier: FileTier,
    #[serde(default)]
    upgrade: FileUpgrade,
    #[serde(default)]
    effect: Option<FileEffect>,
    #[serde(default)]
    uncursed: bool,
    #[serde(default)]
    source: Option<FileItemSource>,
    #[serde(default)]
    identity_group: Option<u8>,
    #[serde(default)]
    max_depth: Option<u8>,
    #[serde(default)]
    upgrade_sum: Option<FileUpgradeSum>,
}

/// One effect name, or a list of acceptable effect names. The name
/// `any_enchantment` stands for every non-curse effect of the item's family.
#[derive(Deserialize)]
#[serde(untagged)]
enum FileEffect {
    Name(String),
    OneOf(Vec<String>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileUpgradeSum {
    group: u8,
    at_least: u8,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FileTier {
    Name(String),
    ExactObject(ExactTier),
    AtLeastObject(AtLeastTier),
    AtMostObject(AtMostTier),
}

impl Default for FileTier {
    fn default() -> Self {
        Self::Name("any".to_owned())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactTier {
    exact: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AtLeastTier {
    at_least: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AtMostTier {
    at_most: u8,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileItemKind {
    Weapon,
    Armor,
    Wand,
    Ring,
}

impl From<FileItemKind> for ItemKind {
    fn from(value: FileItemKind) -> Self {
        match value {
            FileItemKind::Weapon => Self::Weapon,
            FileItemKind::Armor => Self::Armor,
            FileItemKind::Wand => Self::Wand,
            FileItemKind::Ring => Self::Ring,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FileUpgrade {
    Exact(u8),
    Name(String),
    ExactObject(ExactUpgrade),
    AtLeastObject(AtLeastUpgrade),
}

impl Default for FileUpgrade {
    fn default() -> Self {
        Self::Name("any".to_owned())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactUpgrade {
    exact: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AtLeastUpgrade {
    at_least: u8,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileItemSource {
    Heap,
    Chest,
    LockedChest,
    CrystalChest,
    Tomb,
    Skeleton,
    SacrificialFire,
    Mimic,
    GoldenMimic,
    CrystalMimic,
    Statue,
    ArmoredStatue,
    Shop,
    GhostReward,
    WandmakerReward,
    BlacksmithReward,
    ImpReward,
}

impl From<FileItemSource> for ItemSource {
    fn from(value: FileItemSource) -> Self {
        match value {
            FileItemSource::Heap => Self::Heap,
            FileItemSource::Chest => Self::Chest,
            FileItemSource::LockedChest => Self::LockedChest,
            FileItemSource::CrystalChest => Self::CrystalChest,
            FileItemSource::Tomb => Self::Tomb,
            FileItemSource::Skeleton => Self::Skeleton,
            FileItemSource::SacrificialFire => Self::SacrificialFire,
            FileItemSource::Mimic => Self::Mimic,
            FileItemSource::GoldenMimic => Self::GoldenMimic,
            FileItemSource::CrystalMimic => Self::CrystalMimic,
            FileItemSource::Statue => Self::Statue,
            FileItemSource::ArmoredStatue => Self::ArmoredStatue,
            FileItemSource::Shop => Self::Shop,
            FileItemSource::GhostReward => Self::GhostReward,
            FileItemSource::WandmakerReward => Self::WandmakerReward,
            FileItemSource::BlacksmithReward => Self::BlacksmithReward,
            FileItemSource::ImpReward => Self::ImpReward,
        }
    }
}

const fn default_max_depth() -> u8 {
    24
}

/// Decodes and validates a JSON query document into a [`SearchQuery`].
///
/// # Errors
///
/// Returns a human-readable message for malformed JSON, unknown items,
/// effects, upgrade modes, or challenge names, and for invalid queries.
pub fn decode(contents: &str) -> Result<SearchQuery, String> {
    let document: QueryDocument =
        serde_json::from_str(contents).map_err(|error| format!("invalid JSON: {error}"))?;
    let mut requirements = Vec::new();
    let mut next_alternative_group: u8 = 0;
    for (index, entry) in document.requirements.into_iter().enumerate() {
        let position = index + 1;
        match entry {
            FileRequirementEntry::Single(requirement) => {
                requirements.push(
                    convert_requirement(requirement, None)
                        .map_err(|error| format!("requirement {position}: {error}"))?,
                );
            }
            FileRequirementEntry::AnyOf(group) => {
                if group.any_of.is_empty() {
                    return Err(format!(
                        "requirement {position}: any_of needs at least one alternative"
                    ));
                }
                next_alternative_group = next_alternative_group
                    .checked_add(1)
                    .ok_or_else(|| "too many any_of groups".to_owned())?;
                for requirement in group.any_of {
                    requirements.push(
                        convert_requirement(requirement, Some(next_alternative_group))
                            .map_err(|error| format!("requirement {position}: {error}"))?,
                    );
                }
            }
        }
    }
    let query = SearchQuery {
        requirements,
        max_depth: document.max_depth,
        challenges: document
            .challenges
            .into_iter()
            .fold(Challenges::NONE, |mask, challenge| mask | challenge.into()),
        require_blacksmith: document.require_blacksmith,
        exclude_blacksmith_rewards: document.exclude_blacksmith_rewards,
        fast_mode: document.fast_mode,
    };
    query
        .validate()
        .map_err(|error| format!("invalid query: {error}"))?;
    Ok(query)
}

fn convert_effect(kind: ItemKind, effect: FileEffect) -> Result<EffectRequirement, String> {
    let lookup = |name: &str| -> Result<Effect, String> {
        Effect::from_wire_name(kind, name).ok_or_else(|| format!("unknown effect '{name}'"))
    };
    match effect {
        FileEffect::Name(name) if name.eq_ignore_ascii_case("any_enchantment") => {
            EffectSet::enchantments(kind)
                .map(EffectRequirement::OneOf)
                .ok_or_else(|| "any_enchantment requires a weapon or armor".to_owned())
        }
        FileEffect::Name(name) => Ok(EffectRequirement::OneOf(EffectSet::single(lookup(&name)?))),
        FileEffect::OneOf(names) => {
            if names.is_empty() {
                return Err("effect list needs at least one entry".to_owned());
            }
            let effects = names
                .iter()
                .map(|name| lookup(name))
                .collect::<Result<Vec<_>, _>>()?;
            EffectSet::from_effects(effects)
                .map(EffectRequirement::OneOf)
                .ok_or_else(|| "effect list mixes item families".to_owned())
        }
    }
}

fn convert_requirement(
    requirement: FileRequirement,
    alternative_group: Option<u8>,
) -> Result<Requirement, String> {
    let definition = requirement
        .item
        .as_deref()
        .map(|stable_id| {
            item_by_stable_id(stable_id).ok_or_else(|| format!("unknown item '{stable_id}'"))
        })
        .transpose()?;
    let kind = requirement
        .kind
        .map(ItemKind::from)
        .or_else(|| definition.map(|value| value.kind))
        .ok_or_else(|| "kind is required when item is omitted".to_owned())?;
    let effect = requirement
        .effect
        .map(|effect| convert_effect(kind, effect))
        .transpose()?
        .unwrap_or(EffectRequirement::Any);
    let upgrade = match requirement.upgrade {
        FileUpgrade::Exact(value) | FileUpgrade::ExactObject(ExactUpgrade { exact: value }) => {
            UpgradeRequirement::Exact(value)
        }
        FileUpgrade::AtLeastObject(AtLeastUpgrade { at_least }) => {
            UpgradeRequirement::AtLeast(at_least)
        }
        FileUpgrade::Name(name) if name.eq_ignore_ascii_case("any") => UpgradeRequirement::Any,
        FileUpgrade::Name(name) => return Err(format!("unknown upgrade mode '{name}'")),
    };
    let tier = match requirement.tier {
        FileTier::ExactObject(ExactTier { exact }) => TierRequirement::Exact(exact),
        FileTier::AtLeastObject(AtLeastTier { at_least }) => TierRequirement::AtLeast(at_least),
        FileTier::AtMostObject(AtMostTier { at_most }) => TierRequirement::AtMost(at_most),
        FileTier::Name(name) if name.eq_ignore_ascii_case("any") => TierRequirement::Any,
        FileTier::Name(name) => return Err(format!("unknown tier mode '{name}'")),
    };
    Ok(Requirement {
        kind,
        item: definition.map(|value| value.id),
        tier,
        upgrade,
        effect,
        require_uncursed: requirement.uncursed,
        source: requirement.source.map(ItemSource::from),
        identity_group: requirement.identity_group,
        max_depth: requirement.max_depth,
        alternative_group,
        upgrade_sum: requirement.upgrade_sum.map(|sum| UpgradeSum {
            group: sum.group,
            minimum_total: sum.at_least,
        }),
    })
}

#[cfg(test)]
mod tests {
    use crate::catalog::{ArmorEffect, Effect, ItemId, ItemKind, WeaponEffect};
    use crate::challenges::Challenges;
    use crate::model::ItemSource;
    use crate::query::{EffectRequirement, EffectSet, TierRequirement, UpgradeRequirement, UpgradeSum};

    use super::decode;

    #[test]
    fn decodes_concrete_and_wildcard_requirements() {
        let query = decode(
            r#"{
                "max_depth": 12,
                "require_blacksmith": true,
                "exclude_blacksmith_rewards": true,
                "requirements": [
                    {"item": "ring_tenacity", "upgrade": 4, "source": "imp_reward"},
                    {"kind": "wand", "upgrade": {"at_least": 2}, "identity_group": 1, "uncursed": true,
                     "max_depth": 9}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(query.max_depth, 12);
        assert!(query.require_blacksmith);
        assert!(query.exclude_blacksmith_rewards);
        assert_eq!(query.requirements[0].item, Some(ItemId::RingTenacity));
        assert_eq!(query.requirements[0].upgrade, UpgradeRequirement::Exact(4));
        assert_eq!(query.requirements[0].source, Some(ItemSource::ImpReward));
        assert!(!query.requirements[0].require_uncursed);
        assert_eq!(query.requirements[1].kind, ItemKind::Wand);
        assert!(query.requirements[1].require_uncursed);
        assert_eq!(query.requirements[1].max_depth, Some(9));
        assert_eq!(
            query.requirements[1].upgrade,
            UpgradeRequirement::AtLeast(2)
        );
    }

    #[test]
    fn challenge_names_map_to_the_upstream_mask() {
        let query = decode(
            r#"{"challenges":["barren_land","into_darkness","forbidden_runes"],
                "requirements":[{"item":"sword"}]}"#,
        )
        .unwrap();
        assert_eq!(query.challenges, Challenges::new(104).unwrap());
        assert!(
            decode(r#"{"challenges":["not_a_challenge"],"requirements":[{"item":"sword"}]}"#)
                .is_err()
        );
    }

    #[test]
    fn defaults_scope_and_upgrade() {
        let query = decode(r#"{"requirements":[{"item":"sword"}]}"#).unwrap();
        assert_eq!(query.max_depth, 24);
        assert_eq!(query.challenges, Challenges::NONE);
        assert!(!query.require_blacksmith);
        assert!(!query.exclude_blacksmith_rewards);
        assert_eq!(query.requirements[0].upgrade, UpgradeRequirement::Any);
        assert_eq!(query.requirements[0].tier, TierRequirement::Any);
    }

    #[test]
    fn decodes_all_tier_forms() {
        let query = decode(
            r#"{"requirements":[
                {"kind":"weapon","tier":"any"},
                {"kind":"weapon","tier":{"exact":2}},
                {"kind":"armor","tier":{"at_least":3}},
                {"kind":"armor","tier":{"at_most":4}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(query.requirements[0].tier, TierRequirement::Any);
        assert_eq!(query.requirements[1].tier, TierRequirement::Exact(2));
        assert_eq!(query.requirements[2].tier, TierRequirement::AtLeast(3));
        assert_eq!(query.requirements[3].tier, TierRequirement::AtMost(4));
    }

    #[test]
    fn rejects_tier_filters_outside_typed_validation_rules() {
        for contents in [
            r#"{"requirements":[{"item":"sword","tier":{"exact":3}}]}"#,
            r#"{"requirements":[{"kind":"wand","tier":{"exact":3}}]}"#,
            r#"{"requirements":[{"kind":"ring","tier":{"exact":3}}]}"#,
            r#"{"requirements":[{"kind":"weapon","tier":{"exact":1}}]}"#,
            r#"{"requirements":[{"kind":"armor","tier":{"exact":6}}]}"#,
            r#"{"requirements":[{"kind":"weapon","tier":{"at_least":2}}]}"#,
            r#"{"requirements":[{"kind":"armor","tier":{"at_most":5}}]}"#,
        ] {
            let error = decode(contents).unwrap_err();
            assert!(error.contains("invalid query"), "{error}");
            assert!(error.contains("tier"), "{error}");
        }
    }

    #[test]
    fn rejects_unknown_fields_items_and_inconsistent_kinds() {
        assert!(decode(r#"{"requirements":[],"maximum_depth":4}"#).is_err());
        assert!(decode(r#"{"requirements":[{"item":"not_an_item"}]}"#).is_err());
        assert!(decode(r#"{"requirements":[{"kind":"wand","item":"sword"}]}"#).is_err());
    }

    #[test]
    fn decodes_effect_lists_and_the_any_enchantment_shorthand() {
        let query = decode(
            r#"{"requirements":[
                {"item":"greatshield","upgrade":2,
                 "effect":["blocking","projecting","vampiric"]},
                {"kind":"weapon","effect":"any_enchantment"},
                {"kind":"armor","effect":"thorns"}
            ]}"#,
        )
        .unwrap();
        let EffectRequirement::OneOf(set) = query.requirements[0].effect else {
            panic!("expected a one-of set");
        };
        assert_eq!(set.len(), 3);
        assert!(set.contains(Effect::Weapon(WeaponEffect::Vampiric)));
        assert_eq!(
            query.requirements[1].effect,
            EffectRequirement::OneOf(EffectSet::enchantments(ItemKind::Weapon).unwrap())
        );
        assert_eq!(
            query.requirements[2].effect,
            EffectRequirement::OneOf(EffectSet::single(Effect::Armor(ArmorEffect::Thorns)))
        );

        for invalid in [
            r#"{"requirements":[{"kind":"weapon","effect":[]}]}"#,
            r#"{"requirements":[{"kind":"weapon","effect":["thorns"]}]}"#,
            r#"{"requirements":[{"kind":"ring","effect":"any_enchantment"}]}"#,
            r#"{"requirements":[{"kind":"weapon","effect":["blocking","thorns"]}]}"#,
        ] {
            assert!(decode(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn any_of_groups_become_alternative_requirements() {
        let query = decode(
            r#"{"requirements":[
                {"any_of":[
                    {"item":"spear","upgrade":3},
                    {"item":"shuriken","upgrade":2},
                    {"item":"sword","upgrade":1}
                ]},
                {"kind":"wand"},
                {"any_of":[{"item":"sword"},{"item":"mace"}]}
            ]}"#,
        )
        .unwrap();
        assert_eq!(query.requirements.len(), 6);
        let groups: Vec<Option<u8>> = query
            .requirements
            .iter()
            .map(|requirement| requirement.alternative_group)
            .collect();
        assert_eq!(
            groups,
            vec![Some(1), Some(1), Some(1), None, Some(2), Some(2)]
        );
        assert_eq!(query.requirements[0].item, Some(ItemId::Spear));
        assert_eq!(
            query.requirements[1].upgrade,
            UpgradeRequirement::Exact(2)
        );

        assert!(decode(r#"{"requirements":[{"any_of":[]}]}"#).is_err());
        // Nested groups are not representable.
        assert!(
            decode(r#"{"requirements":[{"any_of":[{"any_of":[{"item":"sword"}]}]}]}"#).is_err()
        );
    }

    #[test]
    fn upgrade_sums_link_requirements_through_shared_groups() {
        let query = decode(
            r#"{"requirements":[
                {"item":"ring_might","identity_group":1,
                 "upgrade_sum":{"group":1,"at_least":2}},
                {"item":"ring_might","identity_group":1,
                 "upgrade_sum":{"group":1,"at_least":2}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            query.requirements[0].upgrade_sum,
            Some(UpgradeSum {
                group: 1,
                minimum_total: 2,
            })
        );
        assert_eq!(query.requirements[0].upgrade_sum, query.requirements[1].upgrade_sum);

        // Disagreeing totals and unattainable sums are query errors.
        assert!(
            decode(
                r#"{"requirements":[
                    {"item":"ring_might","upgrade_sum":{"group":1,"at_least":2}},
                    {"item":"ring_might","upgrade_sum":{"group":1,"at_least":3}}
                ]}"#,
            )
            .is_err()
        );
        assert!(
            decode(
                r#"{"requirements":[
                    {"item":"ring_might","upgrade_sum":{"group":1,"at_least":9}},
                    {"item":"ring_might","upgrade_sum":{"group":1,"at_least":9}}
                ]}"#,
            )
            .is_err()
        );
        // A sum inside an any_of group is rejected.
        assert!(
            decode(
                r#"{"requirements":[{"any_of":[
                    {"item":"ring_might","upgrade_sum":{"group":1,"at_least":2}},
                    {"item":"ring_haste"}
                ]}]}"#,
            )
            .is_err()
        );
    }
}
