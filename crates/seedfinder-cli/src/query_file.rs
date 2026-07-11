use serde::Deserialize;
use shpd_seedfinder_core::catalog::{Effect, ItemKind, item_by_stable_id};
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::{Requirement, SearchQuery, UpgradeRequirement};

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
    fast_mode: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRequirement {
    #[serde(default)]
    kind: Option<FileItemKind>,
    #[serde(default)]
    item: Option<String>,
    #[serde(default)]
    upgrade: FileUpgrade,
    #[serde(default)]
    effect: Option<String>,
    #[serde(default)]
    source: Option<FileItemSource>,
    #[serde(default)]
    identity_group: Option<u8>,
    #[serde(default)]
    max_depth: Option<u8>,
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

pub fn decode(contents: &str) -> Result<SearchQuery, String> {
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
    let query = SearchQuery {
        requirements,
        max_depth: document.max_depth,
        require_blacksmith: document.require_blacksmith,
        exclude_blacksmith_rewards: document.exclude_blacksmith_rewards,
        fast_mode: document.fast_mode,
    };
    query
        .validate()
        .map_err(|error| format!("invalid query: {error}"))?;
    Ok(query)
}

fn convert_requirement(requirement: FileRequirement) -> Result<Requirement, String> {
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
    Ok(Requirement {
        kind,
        item: definition.map(|value| value.id),
        upgrade,
        effect,
        source: requirement.source.map(ItemSource::from),
        identity_group: requirement.identity_group,
        max_depth: requirement.max_depth,
    })
}

#[cfg(test)]
mod tests {
    use shpd_seedfinder_core::catalog::{ItemId, ItemKind};
    use shpd_seedfinder_core::model::ItemSource;
    use shpd_seedfinder_core::query::UpgradeRequirement;

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
                    {"kind": "wand", "upgrade": {"at_least": 2}, "identity_group": 1,
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
        assert_eq!(query.requirements[1].kind, ItemKind::Wand);
        assert_eq!(query.requirements[1].max_depth, Some(9));
        assert_eq!(
            query.requirements[1].upgrade,
            UpgradeRequirement::AtLeast(2)
        );
    }

    #[test]
    fn defaults_scope_and_upgrade() {
        let query = decode(r#"{"requirements":[{"item":"sword"}]}"#).unwrap();
        assert_eq!(query.max_depth, 24);
        assert!(!query.require_blacksmith);
        assert!(!query.exclude_blacksmith_rewards);
        assert_eq!(query.requirements[0].upgrade, UpgradeRequirement::Any);
    }

    #[test]
    fn rejects_unknown_fields_items_and_inconsistent_kinds() {
        assert!(decode(r#"{"requirements":[],"maximum_depth":4}"#).is_err());
        assert!(decode(r#"{"requirements":[{"item":"not_an_item"}]}"#).is_err());
        assert!(decode(r#"{"requirements":[{"kind":"wand","item":"sword"}]}"#).is_err());
    }
}
