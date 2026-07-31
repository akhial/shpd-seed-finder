//! JSON search-query document encoding and decoding shared by the CLI and
//! native frontends.

use crate::catalog::{Effect, ItemKind, WeaponCategory, item, item_by_stable_id};
use crate::challenges::Challenges;
use crate::model::ItemSource;
use crate::query::{
    EffectRequirement, EffectSet, Requirement, SearchQuery, TierRequirement, UpgradeRequirement,
    UpgradeSum,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

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
    /// A weapon narrowed to wielded weapons. Plain "weapon" continues to
    /// match both melee and thrown weapons, so pre-existing documents keep
    /// their meaning.
    MeleeWeapon,
    /// A weapon narrowed to missile weapons and tipped darts.
    ThrownWeapon,
    Armor,
    Wand,
    Ring,
}

impl FileItemKind {
    const fn decompose(self) -> (ItemKind, Option<WeaponCategory>) {
        match self {
            Self::Weapon => (ItemKind::Weapon, None),
            Self::MeleeWeapon => (ItemKind::Weapon, Some(WeaponCategory::Melee)),
            Self::ThrownWeapon => (ItemKind::Weapon, Some(WeaponCategory::Thrown)),
            Self::Armor => (ItemKind::Armor, None),
            Self::Wand => (ItemKind::Wand, None),
            Self::Ring => (ItemKind::Ring, None),
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
    let (kind, weapon_category) = requirement
        .kind
        .map(FileItemKind::decompose)
        .or_else(|| definition.map(|value| (value.kind, None)))
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
        weapon_category,
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

/// Every challenge's stable document name paired with its upstream mask bit,
/// in mask order.
pub const CHALLENGE_NAMES: &[(&str, Challenges)] = &[
    ("on_diet", Challenges::NO_FOOD),
    ("faith_is_my_armor", Challenges::NO_ARMOR),
    ("pharmacophobia", Challenges::NO_HEALING),
    ("barren_land", Challenges::NO_HERBALISM),
    ("swarm_intelligence", Challenges::SWARM_INTELLIGENCE),
    ("into_darkness", Challenges::DARKNESS),
    ("forbidden_runes", Challenges::NO_SCROLLS),
    ("hostile_champions", Challenges::CHAMPION_ENEMIES),
    ("badder_bosses", Challenges::STRONGER_BOSSES),
];

/// Stable document name for one item family.
#[must_use]
pub const fn kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Weapon => "weapon",
        ItemKind::Armor => "armor",
        ItemKind::Wand => "wand",
        ItemKind::Ring => "ring",
    }
}

/// Stable document name for one item source.
#[must_use]
pub const fn source_name(source: ItemSource) -> &'static str {
    match source {
        ItemSource::Heap => "heap",
        ItemSource::Chest => "chest",
        ItemSource::LockedChest => "locked_chest",
        ItemSource::CrystalChest => "crystal_chest",
        ItemSource::Tomb => "tomb",
        ItemSource::Skeleton => "skeleton",
        ItemSource::SacrificialFire => "sacrificial_fire",
        ItemSource::Mimic => "mimic",
        ItemSource::GoldenMimic => "golden_mimic",
        ItemSource::CrystalMimic => "crystal_mimic",
        ItemSource::Statue => "statue",
        ItemSource::ArmoredStatue => "armored_statue",
        ItemSource::Shop => "shop",
        ItemSource::GhostReward => "ghost_reward",
        ItemSource::WandmakerReward => "wandmaker_reward",
        ItemSource::BlacksmithReward => "blacksmith_reward",
        ItemSource::ImpReward => "imp_reward",
    }
}

/// Encodes a query as the canonical JSON document accepted by [`decode`].
///
/// Defaults are omitted, mirroring the web frontend's serializer, so the
/// document stays minimal and identical across platforms.
#[must_use]
pub fn encode(query: &SearchQuery) -> Value {
    let mut document = Map::new();
    // Alternative groups serialize as one any_of entry at the first member's
    // position, holding every member in requirement order; decode assigns the
    // groups fresh sequential ids, preserving the structure.
    let mut entries = Vec::new();
    let mut emitted_groups = Vec::new();
    for requirement in &query.requirements {
        let Some(group) = requirement.alternative_group else {
            entries.push(encode_requirement(requirement));
            continue;
        };
        if emitted_groups.contains(&group) {
            continue;
        }
        emitted_groups.push(group);
        let members = query
            .requirements
            .iter()
            .filter(|candidate| candidate.alternative_group == Some(group))
            .map(encode_requirement)
            .collect::<Vec<_>>();
        if members.len() == 1 {
            entries.extend(members);
        } else {
            entries.push(json!({ "any_of": members }));
        }
    }
    document.insert("requirements".to_owned(), Value::Array(entries));
    if query.max_depth != default_max_depth() {
        document.insert("max_depth".to_owned(), json!(query.max_depth));
    }
    if query.require_blacksmith {
        document.insert("require_blacksmith".to_owned(), json!(true));
    }
    if query.exclude_blacksmith_rewards {
        document.insert("exclude_blacksmith_rewards".to_owned(), json!(true));
    }
    if query.fast_mode {
        document.insert("fast_mode".to_owned(), json!(true));
    }
    let challenges = CHALLENGE_NAMES
        .iter()
        .filter(|(_, challenge)| query.challenges.contains(*challenge))
        .map(|(name, _)| json!(name))
        .collect::<Vec<_>>();
    if !challenges.is_empty() {
        document.insert("challenges".to_owned(), Value::Array(challenges));
    }
    Value::Object(document)
}

fn encode_requirement(requirement: &Requirement) -> Value {
    let mut output = Map::new();
    // A weapon-category narrowing is part of the kind in this format;
    // dropping it here would silently widen the requirement on re-import.
    let kind = match (requirement.kind, requirement.weapon_category) {
        (ItemKind::Weapon, Some(WeaponCategory::Melee)) => "melee_weapon",
        (ItemKind::Weapon, Some(WeaponCategory::Thrown)) => "thrown_weapon",
        (kind, _) => kind_name(kind),
    };
    output.insert("kind".to_owned(), json!(kind));
    if let Some(item_id) = requirement.item {
        output.insert("item".to_owned(), json!(item(item_id).stable_id));
    }
    match requirement.tier {
        TierRequirement::Any => {}
        TierRequirement::Exact(tier) => {
            output.insert("tier".to_owned(), json!({ "exact": tier }));
        }
        TierRequirement::AtLeast(tier) => {
            output.insert("tier".to_owned(), json!({ "at_least": tier }));
        }
        TierRequirement::AtMost(tier) => {
            output.insert("tier".to_owned(), json!({ "at_most": tier }));
        }
    }
    match requirement.upgrade {
        UpgradeRequirement::Any => {}
        UpgradeRequirement::Exact(upgrade) => {
            output.insert("upgrade".to_owned(), json!(upgrade));
        }
        UpgradeRequirement::AtLeast(upgrade) => {
            output.insert("upgrade".to_owned(), json!({ "at_least": upgrade }));
        }
    }
    if let EffectRequirement::OneOf(set) = requirement.effect {
        // The full non-curse family set uses the shorthand; one effect stays
        // a bare name; anything else lists its members.
        let effect = if EffectSet::enchantments(set.family()) == Some(set) {
            json!("any_enchantment")
        } else {
            let names = set
                .effects()
                .map(|effect| json!(effect.wire_name()))
                .collect::<Vec<_>>();
            match <[Value; 1]>::try_from(names) {
                Ok([single]) => single,
                Err(names) => Value::Array(names),
            }
        };
        output.insert("effect".to_owned(), effect);
    }
    if requirement.require_uncursed {
        output.insert("uncursed".to_owned(), json!(true));
    }
    if let Some(source) = requirement.source {
        output.insert("source".to_owned(), json!(source_name(source)));
    }
    if let Some(group) = requirement.identity_group {
        output.insert("identity_group".to_owned(), json!(group));
    }
    if let Some(depth) = requirement.max_depth {
        output.insert("max_depth".to_owned(), json!(depth));
    }
    if let Some(sum) = requirement.upgrade_sum {
        output.insert(
            "upgrade_sum".to_owned(),
            json!({ "group": sum.group, "at_least": sum.minimum_total }),
        );
    }
    Value::Object(output)
}

#[cfg(test)]
mod tests {
    use crate::catalog::{ArmorEffect, Effect, ItemId, ItemKind, WeaponEffect};
    use crate::challenges::Challenges;
    use crate::model::ItemSource;
    use crate::query::{
        EffectRequirement, EffectSet, Requirement, SearchQuery, TierRequirement,
        UpgradeRequirement, UpgradeSum,
    };

    use super::{decode, encode};

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
    fn melee_and_thrown_kinds_narrow_weapons_and_stay_compatible() {
        use crate::catalog::WeaponCategory;

        let query = decode(
            r#"{"requirements":[
                {"kind":"weapon"},
                {"kind":"melee_weapon", "tier":{"exact":5}},
                {"kind":"thrown_weapon"},
                {"kind":"thrown_weapon", "item":"shuriken"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(query.requirements[0].kind, ItemKind::Weapon);
        assert_eq!(query.requirements[0].weapon_category, None);
        assert_eq!(
            query.requirements[1].weapon_category,
            Some(WeaponCategory::Melee)
        );
        assert_eq!(query.requirements[1].tier, TierRequirement::Exact(5));
        assert_eq!(
            query.requirements[2].weapon_category,
            Some(WeaponCategory::Thrown)
        );
        assert_eq!(query.requirements[3].item, Some(ItemId::Shuriken));
        assert_eq!(
            query.requirements[3].weapon_category,
            Some(WeaponCategory::Thrown)
        );

        // A melee kind cannot pin a thrown item and vice versa.
        assert!(decode(r#"{"requirements":[{"kind":"melee_weapon","item":"shuriken"}]}"#).is_err());
        assert!(decode(r#"{"requirements":[{"kind":"thrown_weapon","item":"sword"}]}"#).is_err());
        // Enchantments remain valid for both weapon sub-kinds.
        let enchanted =
            decode(r#"{"requirements":[{"kind":"thrown_weapon","effect":"Projecting"}]}"#).unwrap();
        assert_eq!(
            enchanted.requirements[0].effect,
            EffectRequirement::OneOf(EffectSet::single(Effect::Weapon(WeaponEffect::Projecting)))
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
        assert_eq!(query.requirements[1].upgrade, UpgradeRequirement::Exact(2));

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
        assert_eq!(
            query.requirements[0].upgrade_sum,
            query.requirements[1].upgrade_sum
        );

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

    #[test]
    fn encoding_omits_defaults_and_round_trips_a_loaded_query() {
        let query = SearchQuery {
            requirements: vec![
                Requirement {
                    kind: ItemKind::Weapon,
                    weapon_category: None,
                    item: None,
                    tier: TierRequirement::AtLeast(4),
                    upgrade: UpgradeRequirement::Exact(2),
                    effect: EffectRequirement::OneOf(EffectSet::single(Effect::Weapon(
                        WeaponEffect::Blazing,
                    ))),
                    require_uncursed: true,
                    source: Some(ItemSource::LockedChest),
                    identity_group: Some(2),
                    max_depth: Some(9),
                    alternative_group: None,
                    upgrade_sum: None,
                },
                Requirement {
                    kind: ItemKind::Ring,
                    weapon_category: None,
                    item: Some(ItemId::RingWealth),
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::AtLeast(3),
                    effect: EffectRequirement::Any,
                    require_uncursed: false,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                    alternative_group: None,
                    upgrade_sum: None,
                },
            ],
            max_depth: 19,
            challenges: Challenges::NO_HERBALISM | Challenges::NO_SCROLLS,
            require_blacksmith: true,
            exclude_blacksmith_rewards: true,
            fast_mode: true,
        };
        let document = encode(&query);
        assert_eq!(
            document,
            serde_json::json!({
                "requirements": [
                    {
                        "kind": "weapon",
                        "tier": {"at_least": 4},
                        "upgrade": 2,
                        "effect": "Blazing",
                        "uncursed": true,
                        "source": "locked_chest",
                        "identity_group": 2,
                        "max_depth": 9,
                    },
                    {"kind": "ring", "item": "ring_wealth", "upgrade": {"at_least": 3}},
                ],
                "max_depth": 19,
                "require_blacksmith": true,
                "exclude_blacksmith_rewards": true,
                "fast_mode": true,
                "challenges": ["barren_land", "forbidden_runes"],
            })
        );
        assert_eq!(decode(&document.to_string()).unwrap(), query);
    }

    #[test]
    fn encoding_a_minimal_query_emits_requirements_only() {
        let query = SearchQuery {
            requirements: vec![Requirement {
                kind: ItemKind::Wand,
                weapon_category: None,
                item: None,
                tier: TierRequirement::Any,
                upgrade: UpgradeRequirement::Any,
                effect: EffectRequirement::Any,
                require_uncursed: false,
                source: None,
                identity_group: None,
                max_depth: None,
                alternative_group: None,
                upgrade_sum: None,
            }],
            max_depth: 24,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        };
        assert_eq!(
            encode(&query).to_string(),
            r#"{"requirements":[{"kind":"wand"}]}"#
        );
        assert_eq!(decode(&encode(&query).to_string()).unwrap(), query);
    }

    #[test]
    fn encoding_round_trips_groups_sums_and_effect_sets() {
        let query = decode(
            r#"{"requirements":[
                {"any_of":[
                    {"item":"spear","upgrade":3},
                    {"item":"shuriken","upgrade":2}
                ]},
                {"kind":"melee_weapon","effect":"any_enchantment"},
                {"item":"greatshield","effect":["blocking","projecting","vampiric"]},
                {"item":"ring_might","identity_group":1,
                 "upgrade_sum":{"group":1,"at_least":2}},
                {"item":"ring_might","identity_group":1,
                 "upgrade_sum":{"group":1,"at_least":2}}
            ]}"#,
        )
        .unwrap();
        let document = encode(&query);
        assert_eq!(decode(&document.to_string()).unwrap(), query);
        let entries = document["requirements"].as_array().unwrap();
        assert!(entries[0].get("any_of").is_some());
        assert_eq!(entries[1]["effect"], serde_json::json!("any_enchantment"));
        assert_eq!(entries[2]["effect"].as_array().unwrap().len(), 3);
        assert_eq!(entries[3]["upgrade_sum"]["at_least"], 2);
    }
}
