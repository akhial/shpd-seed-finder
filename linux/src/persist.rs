// SPDX-License-Identifier: GPL-3.0-or-later

//! Query-state persistence in the user configuration directory.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use shpd_seedfinder_core::catalog::{Effect, ItemKind, item, item_by_stable_id};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::{TierRequirement, UpgradeRequirement};

use crate::config::APP_ID;
use crate::state::{ALL_SOURCES, AppState, UiRequirement};

#[derive(Default, Deserialize, Serialize)]
struct SavedState {
    requirements: Vec<SavedRequirement>,
    max_depth: Option<u8>,
    #[serde(default)]
    require_blacksmith: bool,
    #[serde(default)]
    exclude_blacksmith_rewards: bool,
    #[serde(default)]
    fast_mode: bool,
    #[serde(default)]
    challenges: u16,
}

#[derive(Deserialize, Serialize)]
struct SavedRequirement {
    kind: String,
    item: Option<String>,
    tier: Option<SavedPredicate>,
    upgrade: Option<SavedPredicate>,
    effect: Option<String>,
    source: Option<String>,
    identity_group: Option<u8>,
    max_depth: Option<u8>,
}

#[derive(Deserialize, Serialize)]
struct SavedPredicate {
    mode: String,
    value: u8,
}

fn state_path() -> PathBuf {
    gtk::glib::user_config_dir().join(APP_ID).join("state.json")
}

/// Loads the previous session's query, falling back to defaults on any error.
pub fn load() -> AppState {
    let Ok(contents) = fs::read_to_string(state_path()) else {
        return AppState::default();
    };
    let Ok(saved) = serde_json::from_str::<SavedState>(&contents) else {
        return AppState::default();
    };
    let mut state = AppState::default();
    state.max_depth = saved.max_depth.unwrap_or(24).clamp(1, 24);
    state.require_blacksmith = saved.require_blacksmith;
    state.exclude_blacksmith_rewards = saved.exclude_blacksmith_rewards;
    state.fast_mode = saved.fast_mode;
    state.challenges = Challenges::new(saved.challenges).unwrap_or(Challenges::NONE);
    for requirement in saved.requirements {
        let key = state.claim_key();
        if let Some(restored) = restore_requirement(&requirement, key)
            && restored.to_core().validate().is_ok()
        {
            state.requirements.push(restored);
        }
    }
    state
}

/// Saves the current query, quietly giving up on filesystem errors.
pub fn save(state: &AppState) {
    let saved = SavedState {
        requirements: state.requirements.iter().map(save_requirement).collect(),
        max_depth: Some(state.max_depth),
        require_blacksmith: state.require_blacksmith,
        exclude_blacksmith_rewards: state.exclude_blacksmith_rewards,
        fast_mode: state.fast_mode,
        challenges: state.challenges.bits(),
    };
    let Ok(contents) = serde_json::to_string_pretty(&saved) else {
        return;
    };
    let path = state_path();
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let _ = fs::write(path, contents);
}

fn save_requirement(requirement: &UiRequirement) -> SavedRequirement {
    SavedRequirement {
        kind: kind_key(requirement.kind).to_owned(),
        item: requirement
            .item
            .map(|item_id| item(item_id).stable_id.to_owned()),
        tier: save_predicate(match requirement.tier {
            TierRequirement::Any => None,
            TierRequirement::Exact(value) => Some(("exact", value)),
            TierRequirement::AtLeast(value) => Some(("at_least", value)),
            TierRequirement::AtMost(value) => Some(("at_most", value)),
        }),
        upgrade: save_predicate(match requirement.upgrade {
            UpgradeRequirement::Any => None,
            UpgradeRequirement::Exact(value) => Some(("exact", value)),
            UpgradeRequirement::AtLeast(value) => Some(("at_least", value)),
        }),
        effect: requirement
            .effect
            .map(|effect| effect.wire_name().to_owned()),
        source: requirement
            .source
            .map(|source| source_key(source).to_owned()),
        identity_group: requirement.identity_group,
        max_depth: requirement.max_depth,
    }
}

fn save_predicate(predicate: Option<(&str, u8)>) -> Option<SavedPredicate> {
    predicate.map(|(mode, value)| SavedPredicate {
        mode: mode.to_owned(),
        value,
    })
}

fn restore_requirement(saved: &SavedRequirement, key: u64) -> Option<UiRequirement> {
    let kind = kind_from_key(&saved.kind)?;
    let item = match &saved.item {
        None => None,
        Some(stable_id) => Some(item_by_stable_id(stable_id)?.id),
    };
    let tier = restore_tier_predicate(saved.tier.as_ref())?;
    let upgrade = restore_predicate(
        saved.upgrade.as_ref(),
        UpgradeRequirement::Any,
        UpgradeRequirement::Exact,
        UpgradeRequirement::AtLeast,
    )?;
    let effect = match &saved.effect {
        None => None,
        Some(name) => Some(Effect::from_wire_name(kind, name)?),
    };
    let source = match &saved.source {
        None => None,
        Some(name) => Some(source_from_key(name)?),
    };
    Some(UiRequirement {
        key,
        kind,
        item,
        tier,
        upgrade,
        effect,
        source,
        identity_group: saved.identity_group,
        max_depth: saved.max_depth,
    })
}

fn restore_tier_predicate(saved: Option<&SavedPredicate>) -> Option<TierRequirement> {
    match saved {
        None => Some(TierRequirement::Any),
        Some(predicate) if predicate.mode == "exact" => {
            Some(TierRequirement::Exact(predicate.value))
        }
        Some(predicate) if predicate.mode == "at_least" => {
            Some(TierRequirement::AtLeast(predicate.value))
        }
        Some(predicate) if predicate.mode == "at_most" => {
            Some(TierRequirement::AtMost(predicate.value))
        }
        Some(_) => None,
    }
}

/// Maps an optional saved predicate into a typed one; an unknown mode drops
/// the whole requirement by returning `None`.
fn restore_predicate<T>(
    saved: Option<&SavedPredicate>,
    any: T,
    exact: fn(u8) -> T,
    at_least: fn(u8) -> T,
) -> Option<T> {
    match saved {
        None => Some(any),
        Some(predicate) if predicate.mode == "exact" => Some(exact(predicate.value)),
        Some(predicate) if predicate.mode == "at_least" => Some(at_least(predicate.value)),
        Some(_) => None,
    }
}

const fn kind_key(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Weapon => "weapon",
        ItemKind::Armor => "armor",
        ItemKind::Wand => "wand",
        ItemKind::Ring => "ring",
    }
}

fn kind_from_key(key: &str) -> Option<ItemKind> {
    match key {
        "weapon" => Some(ItemKind::Weapon),
        "armor" => Some(ItemKind::Armor),
        "wand" => Some(ItemKind::Wand),
        "ring" => Some(ItemKind::Ring),
        _ => None,
    }
}

/// Stable snake-case source names, matching the CLI's JSON query format.
const fn source_key(source: ItemSource) -> &'static str {
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

fn source_from_key(key: &str) -> Option<ItemSource> {
    ALL_SOURCES
        .iter()
        .copied()
        .find(|source| source_key(*source) == key)
}

#[cfg(test)]
mod tests {
    use shpd_seedfinder_core::catalog::{Effect, ItemId, ItemKind, WeaponEffect};
    use shpd_seedfinder_core::model::ItemSource;
    use shpd_seedfinder_core::query::{TierRequirement, UpgradeRequirement};

    use super::{restore_requirement, save_requirement};
    use crate::state::UiRequirement;

    #[test]
    fn requirements_round_trip() {
        let requirement = UiRequirement {
            key: 7,
            kind: ItemKind::Weapon,
            item: Some(ItemId::Greatsword),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::AtLeast(2),
            effect: Some(Effect::Weapon(WeaponEffect::Blazing)),
            source: Some(ItemSource::SacrificialFire),
            identity_group: Some(3),
            max_depth: Some(21),
        };
        let restored = restore_requirement(&save_requirement(&requirement), 7).unwrap();
        assert_eq!(restored, requirement);

        let mut bounded = UiRequirement::new(8);
        bounded.tier = TierRequirement::AtMost(3);
        let restored = restore_requirement(&save_requirement(&bounded), 8).unwrap();
        assert_eq!(restored, bounded);
    }

    #[test]
    fn unknown_names_are_dropped() {
        let mut saved = save_requirement(&UiRequirement::new(1));
        saved.kind = "trinket".to_owned();
        assert!(restore_requirement(&saved, 1).is_none());
    }
}
