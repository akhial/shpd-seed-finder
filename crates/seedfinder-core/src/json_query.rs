//! JSON search-query document encoding and decoding shared by the CLI and
//! native frontends.

use crate::catalog::{Effect, ItemKind, WeaponCategory, item, item_by_stable_id};
use crate::challenges::Challenges;
use crate::model::ItemSource;
use crate::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};
use crate::quests::WandmakerQuestType;
use serde::Deserialize;
use serde_json::{Map, Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryDocument {
    requirements: Vec<FileRequirement>,
    #[serde(default = "default_max_depth")]
    max_depth: u8,
    #[serde(default)]
    require_blacksmith: bool,
    #[serde(default)]
    exclude_blacksmith_rewards: bool,
    #[serde(default)]
    wandmaker_quest: Option<String>,
    #[serde(default)]
    fast_mode: bool,
    #[serde(default)]
    challenges: Vec<FileChallenge>,
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
    effect: Option<String>,
    #[serde(default)]
    uncursed: bool,
    #[serde(default)]
    source: Option<FileItemSource>,
    #[serde(default)]
    identity_group: Option<u8>,
    #[serde(default)]
    max_depth: Option<u8>,
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
    let query = decode_unvalidated(contents)?;
    query
        .validate()
        .map_err(|error| format!("invalid query: {error}"))?;
    Ok(query)
}

/// Decodes a JSON query document into a [`SearchQuery`] without checking that
/// the result is a runnable search.
///
/// This is the entry point for persistence and editor state: a frontend that
/// saves whatever the user has typed so far needs to reload documents that
/// [`decode`] rejects, such as one with no requirements yet but a floor limit,
/// challenges, or flags already chosen. The document is parsed exactly as
/// strictly as [`decode`] parses it — unknown fields, unknown items, effects,
/// upgrade modes, tier modes, Wandmaker quests, and challenge names are all
/// still errors — only [`SearchQuery::validate`] is skipped, so the returned
/// query may well fail it. [`encode`] is the intended round-trip pair; it
/// accepts an empty requirement list and emits it verbatim.
///
/// # Errors
///
/// Returns a human-readable message for malformed JSON, unknown items,
/// effects, upgrade modes, or challenge names.
pub fn decode_unvalidated(contents: &str) -> Result<SearchQuery, String> {
    let document: QueryDocument =
        serde_json::from_str(contents).map_err(|error| format!("invalid JSON: {error}"))?;
    let requirements = document
        .requirements
        .into_iter()
        .enumerate()
        .map(|(index, requirement)| {
            convert_requirement(requirement)
                .map_err(|error| format!("requirement {}: {error}", index + 1))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let wandmaker_quest = document
        .wandmaker_quest
        .as_deref()
        .map(|name| {
            WandmakerQuestType::from_document_name(name)
                .ok_or_else(|| format!("unknown Wandmaker quest '{name}'"))
        })
        .transpose()?;
    Ok(SearchQuery {
        requirements,
        max_depth: document.max_depth,
        challenges: document
            .challenges
            .into_iter()
            .fold(Challenges::NONE, |mask, challenge| mask | challenge.into()),
        require_blacksmith: document.require_blacksmith,
        exclude_blacksmith_rewards: document.exclude_blacksmith_rewards,
        wandmaker_quest,
        fast_mode: document.fast_mode,
    })
}

fn convert_requirement(requirement: FileRequirement) -> Result<Requirement, String> {
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
        .as_deref()
        .map(|name| {
            Effect::from_wire_name(kind, name).ok_or_else(|| format!("unknown effect '{name}'"))
        })
        .transpose()?;
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
    document.insert(
        "requirements".to_owned(),
        Value::Array(
            query
                .requirements
                .iter()
                .map(encode_requirement)
                .collect::<Vec<_>>(),
        ),
    );
    if query.max_depth != default_max_depth() {
        document.insert("max_depth".to_owned(), json!(query.max_depth));
    }
    if query.require_blacksmith {
        document.insert("require_blacksmith".to_owned(), json!(true));
    }
    if query.exclude_blacksmith_rewards {
        document.insert("exclude_blacksmith_rewards".to_owned(), json!(true));
    }
    if let Some(variant) = query.wandmaker_quest {
        document.insert("wandmaker_quest".to_owned(), json!(variant.document_name()));
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
    if let Some(effect) = requirement.effect {
        output.insert("effect".to_owned(), json!(effect.wire_name()));
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
    Value::Object(output)
}

#[cfg(test)]
mod tests {
    use crate::catalog::{Effect, ItemId, ItemKind, WeaponEffect};
    use crate::challenges::Challenges;
    use crate::model::ItemSource;
    use crate::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};
    use crate::quests::WandmakerQuestType;

    use super::{decode, decode_unvalidated, encode};

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
        assert!(enchanted.requirements[0].effect.is_some());
    }

    #[test]
    fn wandmaker_quest_names_round_trip_and_default_to_any() {
        use crate::quests::WandmakerQuestType;

        assert_eq!(
            decode(r#"{"requirements":[{"item":"sword"}]}"#)
                .unwrap()
                .wandmaker_quest,
            None
        );
        for variant in WandmakerQuestType::ALL {
            let contents = format!(
                r#"{{"requirements":[{{"item":"sword"}}],"wandmaker_quest":"{}"}}"#,
                variant.document_name()
            );
            let query = decode(&contents).unwrap();
            assert_eq!(query.wandmaker_quest, Some(variant));
            assert_eq!(decode(&encode(&query).to_string()).unwrap(), query);
        }

        let error =
            decode(r#"{"requirements":[{"item":"sword"}],"wandmaker_quest":"corpse dust"}"#)
                .unwrap_err();
        assert!(error.contains("corpse dust"), "{error}");
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
    fn parse_only_decoding_keeps_editor_state_without_requirements() {
        let contents = r#"{
            "requirements": [],
            "max_depth": 11,
            "require_blacksmith": true,
            "exclude_blacksmith_rewards": true,
            "fast_mode": true,
            "wandmaker_quest": "corpse_dust",
            "challenges": ["barren_land", "forbidden_runes"]
        }"#;

        let query = decode_unvalidated(contents).unwrap();
        assert!(query.requirements.is_empty());
        assert_eq!(query.max_depth, 11);
        assert!(query.require_blacksmith);
        assert!(query.exclude_blacksmith_rewards);
        assert!(query.fast_mode);
        assert_eq!(
            query.challenges,
            Challenges::NO_HERBALISM | Challenges::NO_SCROLLS
        );
        assert_eq!(query.wandmaker_quest, Some(WandmakerQuestType::CorpseDust));

        // The editor state survives a save/load round trip through `encode`.
        let reloaded = decode_unvalidated(&encode(&query).to_string()).unwrap();
        assert_eq!(reloaded, query);
        assert_eq!(encode(&reloaded), encode(&query));

        // The very same document is not a runnable search.
        let error = decode(contents).unwrap_err();
        assert!(error.contains("at least one item requirement"), "{error}");

        // Parse-only decoding is still strict about the document shape.
        assert!(decode_unvalidated(r#"{"requirements":[],"maximum_depth":4}"#).is_err());
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
                    effect: Some(Effect::Weapon(WeaponEffect::Blazing)),
                    require_uncursed: true,
                    source: Some(ItemSource::LockedChest),
                    identity_group: Some(2),
                    max_depth: Some(9),
                },
                Requirement {
                    kind: ItemKind::Ring,
                    weapon_category: None,
                    item: Some(ItemId::RingWealth),
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::AtLeast(3),
                    effect: None,
                    require_uncursed: false,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                },
            ],
            max_depth: 19,
            challenges: Challenges::NO_HERBALISM | Challenges::NO_SCROLLS,
            require_blacksmith: true,
            exclude_blacksmith_rewards: true,
            wandmaker_quest: None,
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
                effect: None,
                require_uncursed: false,
                source: None,
                identity_group: None,
                max_depth: None,
            }],
            max_depth: 24,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        assert_eq!(
            encode(&query).to_string(),
            r#"{"requirements":[{"kind":"wand"}]}"#
        );
        assert_eq!(decode(&encode(&query).to_string()).unwrap(), query);
    }
}
