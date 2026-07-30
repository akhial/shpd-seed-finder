// SPDX-License-Identifier: GPL-3.0-or-later

//! Query-state persistence in the user configuration directory.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use shpd_seedfinder_core::catalog::{Effect, ItemKind, WeaponCategory, item, item_by_stable_id};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::{EffectSet, TierRequirement, UpgradeRequirement, UpgradeSum};

use crate::config::APP_ID;
use crate::state::{ALL_SOURCES, AppState, UiEffect, UiRequirement};

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
struct SavedPreset {
    name: String,
    query: SavedState,
}

/// One named query saved by the user.
#[derive(Clone, Debug)]
pub struct UserPreset {
    pub name: String,
    pub state: AppState,
}

#[derive(Deserialize, Serialize)]
struct SavedRequirement {
    kind: String,
    item: Option<String>,
    tier: Option<SavedPredicate>,
    upgrade: Option<SavedPredicate>,
    /// Legacy single-effect field, still written for one-element sets so
    /// older builds keep reading new saves.
    effect: Option<String>,
    /// `"any"`, `"any_enchantment"`, or `"one_of"`; absent falls back to the
    /// legacy `effect` field.
    #[serde(default)]
    effect_mode: Option<String>,
    /// Effect wire names for the `"one_of"` mode.
    #[serde(default)]
    effects: Option<Vec<String>>,
    #[serde(default)]
    require_uncursed: bool,
    source: Option<String>,
    identity_group: Option<u8>,
    max_depth: Option<u8>,
    #[serde(default)]
    alternative_group: Option<u8>,
    #[serde(default)]
    upgrade_sum_group: Option<u8>,
    #[serde(default)]
    upgrade_sum_total: Option<u8>,
}

#[derive(Deserialize, Serialize)]
struct SavedPredicate {
    mode: String,
    value: u8,
}

fn state_path() -> PathBuf {
    gtk::glib::user_config_dir().join(APP_ID).join("state.json")
}

fn presets_path() -> PathBuf {
    gtk::glib::user_config_dir()
        .join(APP_ID)
        .join("presets.json")
}

/// Loads the previous session's query, falling back to defaults on any error.
pub fn load() -> AppState {
    let Ok(contents) = fs::read_to_string(state_path()) else {
        return AppState::default();
    };
    let Ok(saved) = serde_json::from_str::<SavedState>(&contents) else {
        return AppState::default();
    };
    restore_state(saved)
}

/// Saves the current query, quietly giving up on filesystem errors.
pub fn save(state: &AppState) {
    write_json(state_path(), &save_state(state));
}

/// Loads user-created presets, dropping malformed entries.
#[must_use]
pub fn load_presets() -> Vec<UserPreset> {
    let Ok(contents) = fs::read_to_string(presets_path()) else {
        return Vec::new();
    };
    decode_presets(&contents)
}

fn decode_presets(contents: &str) -> Vec<UserPreset> {
    let Ok(saved) = serde_json::from_str::<Vec<serde_json::Value>>(contents) else {
        return Vec::new();
    };
    saved
        .into_iter()
        .filter_map(|value| serde_json::from_value::<SavedPreset>(value).ok())
        .filter_map(|preset| {
            let name = preset.name.trim();
            (!name.is_empty()).then(|| UserPreset {
                name: name.to_owned(),
                state: restore_state(preset.query),
            })
        })
        .collect()
}

/// Saves every user-created preset, quietly giving up on filesystem errors.
pub fn save_presets(presets: &[UserPreset]) {
    let saved: Vec<_> = presets
        .iter()
        .map(|preset| SavedPreset {
            name: preset.name.clone(),
            query: save_state(&preset.state),
        })
        .collect();
    write_json(presets_path(), &saved);
}

fn save_state(state: &AppState) -> SavedState {
    SavedState {
        requirements: state.requirements.iter().map(save_requirement).collect(),
        max_depth: Some(state.max_depth),
        require_blacksmith: state.require_blacksmith,
        exclude_blacksmith_rewards: state.exclude_blacksmith_rewards,
        fast_mode: state.fast_mode,
        challenges: state.challenges.bits(),
    }
}

fn restore_state(saved: SavedState) -> AppState {
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
    // Dropped rows may leave one-member alternative groups behind.
    state.dissolve_lone_alternatives();
    state
}

fn write_json(path: PathBuf, value: &impl Serialize) {
    let Ok(contents) = serde_json::to_string_pretty(value) else {
        return;
    };
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let _ = fs::write(path, contents);
}

fn save_requirement(requirement: &UiRequirement) -> SavedRequirement {
    SavedRequirement {
        kind: kind_key(requirement.kind, requirement.weapon_category).to_owned(),
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
            .single()
            .map(|effect| effect.wire_name().to_owned()),
        effect_mode: Some(
            match requirement.effect {
                UiEffect::Any => "any",
                UiEffect::AnyEnchantment => "any_enchantment",
                UiEffect::OneOf(_) => "one_of",
            }
            .to_owned(),
        ),
        effects: match requirement.effect {
            UiEffect::Any | UiEffect::AnyEnchantment => None,
            UiEffect::OneOf(set) => Some(
                set.effects()
                    .map(|effect| effect.wire_name().to_owned())
                    .collect(),
            ),
        },
        require_uncursed: requirement.require_uncursed,
        source: requirement
            .source
            .map(|source| source_key(source).to_owned()),
        identity_group: requirement.identity_group,
        max_depth: requirement.max_depth,
        alternative_group: requirement.alternative_group,
        upgrade_sum_group: requirement.upgrade_sum.map(|sum| sum.group),
        upgrade_sum_total: requirement.upgrade_sum.map(|sum| sum.minimum_total),
    }
}

fn save_predicate(predicate: Option<(&str, u8)>) -> Option<SavedPredicate> {
    predicate.map(|(mode, value)| SavedPredicate {
        mode: mode.to_owned(),
        value,
    })
}

fn restore_requirement(saved: &SavedRequirement, key: u64) -> Option<UiRequirement> {
    let (kind, weapon_category) = kind_from_key(&saved.kind)?;
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
    let effect = restore_effect(saved, kind)?;
    let source = match &saved.source {
        None => None,
        Some(name) => Some(source_from_key(name)?),
    };
    let upgrade_sum = match (saved.upgrade_sum_group, saved.upgrade_sum_total) {
        (None, None) => None,
        (Some(group), Some(minimum_total)) => Some(UpgradeSum {
            group,
            minimum_total,
        }),
        (None, Some(_)) | (Some(_), None) => return None,
    };
    Some(UiRequirement {
        key,
        kind,
        weapon_category,
        item,
        tier,
        upgrade,
        effect,
        require_uncursed: saved.require_uncursed,
        source,
        identity_group: saved.identity_group,
        max_depth: saved.max_depth,
        alternative_group: saved.alternative_group,
        upgrade_sum,
    })
}

/// Decodes the effect predicate, preferring the mode field and falling back
/// to the legacy single-name field; an unknown mode or name drops the row.
fn restore_effect(saved: &SavedRequirement, kind: ItemKind) -> Option<UiEffect> {
    match saved.effect_mode.as_deref() {
        None => match &saved.effect {
            None => Some(UiEffect::Any),
            Some(name) => Some(UiEffect::OneOf(EffectSet::single(Effect::from_wire_name(
                kind, name,
            )?))),
        },
        Some("any") => Some(UiEffect::Any),
        Some("any_enchantment") => Some(UiEffect::AnyEnchantment),
        Some("one_of") => {
            let names = saved.effects.as_ref()?;
            let mut effects = Vec::with_capacity(names.len());
            for name in names {
                effects.push(Effect::from_wire_name(kind, name)?);
            }
            Some(UiEffect::OneOf(EffectSet::from_effects(effects)?))
        }
        Some(_) => None,
    }
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

/// Stable snake-case kind names, matching the CLI's JSON query format. Plain
/// "weapon" keeps meaning "melee or thrown", so older saved states restore
/// with their original semantics.
const fn kind_key(kind: ItemKind, weapon_category: Option<WeaponCategory>) -> &'static str {
    match (kind, weapon_category) {
        (ItemKind::Weapon, None) => "weapon",
        (ItemKind::Weapon, Some(WeaponCategory::Melee)) => "melee_weapon",
        (ItemKind::Weapon, Some(WeaponCategory::Thrown)) => "thrown_weapon",
        (ItemKind::Armor, _) => "armor",
        (ItemKind::Wand, _) => "wand",
        (ItemKind::Ring, _) => "ring",
    }
}

fn kind_from_key(key: &str) -> Option<(ItemKind, Option<WeaponCategory>)> {
    match key {
        "weapon" => Some((ItemKind::Weapon, None)),
        "melee_weapon" => Some((ItemKind::Weapon, Some(WeaponCategory::Melee))),
        "thrown_weapon" => Some((ItemKind::Weapon, Some(WeaponCategory::Thrown))),
        "armor" => Some((ItemKind::Armor, None)),
        "wand" => Some((ItemKind::Wand, None)),
        "ring" => Some((ItemKind::Ring, None)),
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
    use shpd_seedfinder_core::query::{EffectSet, TierRequirement, UpgradeRequirement, UpgradeSum};

    use super::{SavedPreset, decode_presets, restore_requirement, save_requirement, save_state};
    use crate::state::{AppState, UiEffect, UiRequirement};

    #[test]
    fn requirements_round_trip() {
        let requirement = UiRequirement {
            key: 7,
            kind: ItemKind::Weapon,
            weapon_category: None,
            item: Some(ItemId::Greatsword),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::AtLeast(2),
            effect: UiEffect::OneOf(EffectSet::single(Effect::Weapon(WeaponEffect::Blazing))),
            require_uncursed: true,
            source: Some(ItemSource::SacrificialFire),
            identity_group: Some(3),
            max_depth: Some(21),
            alternative_group: Some(2),
            upgrade_sum: None,
        };
        let restored = restore_requirement(&save_requirement(&requirement), 7).unwrap();
        assert_eq!(restored, requirement);

        let mut bounded = UiRequirement::new(8);
        bounded.tier = TierRequirement::AtMost(3);
        let restored = restore_requirement(&save_requirement(&bounded), 8).unwrap();
        assert_eq!(restored, bounded);
    }

    #[test]
    fn effect_sets_and_upgrade_sums_round_trip() {
        let mut requirement = UiRequirement::new(4);
        requirement.effect = UiEffect::OneOf(
            EffectSet::from_effects([
                Effect::Weapon(WeaponEffect::Blocking),
                Effect::Weapon(WeaponEffect::Vampiric),
            ])
            .unwrap(),
        );
        let saved = save_requirement(&requirement);
        assert_eq!(saved.effect_mode.as_deref(), Some("one_of"));
        // Two-element sets have no legacy single-effect spelling.
        assert!(saved.effect.is_none());
        assert_eq!(restore_requirement(&saved, 4).unwrap(), requirement);

        requirement.effect = UiEffect::AnyEnchantment;
        requirement.upgrade_sum = Some(UpgradeSum {
            group: 1,
            minimum_total: 2,
        });
        let restored = restore_requirement(&save_requirement(&requirement), 4).unwrap();
        assert_eq!(restored, requirement);
    }

    #[test]
    fn legacy_single_effect_saves_still_load() {
        let mut saved = save_requirement(&UiRequirement::new(1));
        saved.effect = Some("Blazing".to_owned());
        saved.effect_mode = None;
        saved.effects = None;
        assert_eq!(
            restore_requirement(&saved, 1).unwrap().effect,
            UiEffect::OneOf(EffectSet::single(Effect::Weapon(WeaponEffect::Blazing)))
        );

        saved.effect = None;
        assert_eq!(
            restore_requirement(&saved, 1).unwrap().effect,
            UiEffect::Any
        );
    }

    #[test]
    fn weapon_category_round_trips_with_stable_keys() {
        use shpd_seedfinder_core::catalog::WeaponCategory;

        for (category, key) in [
            (None, "weapon"),
            (Some(WeaponCategory::Melee), "melee_weapon"),
            (Some(WeaponCategory::Thrown), "thrown_weapon"),
        ] {
            let mut requirement = UiRequirement::new(3);
            requirement.weapon_category = category;
            let saved = save_requirement(&requirement);
            assert_eq!(saved.kind, key);
            assert_eq!(restore_requirement(&saved, 3), Some(requirement));
        }

        // A narrowed kind with an item of the other class is dropped instead
        // of silently widening.
        let mut inconsistent = UiRequirement::new(4);
        inconsistent.weapon_category = Some(WeaponCategory::Thrown);
        inconsistent.item = Some(ItemId::Greatsword);
        let saved = save_requirement(&inconsistent);
        assert!(
            restore_requirement(&saved, 4)
                .is_none_or(|requirement| requirement.to_core().validate().is_err())
        );
    }

    #[test]
    fn unknown_names_are_dropped() {
        let mut saved = save_requirement(&UiRequirement::new(1));
        saved.kind = "trinket".to_owned();
        assert!(restore_requirement(&saved, 1).is_none());

        let mut saved = save_requirement(&UiRequirement::new(1));
        saved.effect_mode = Some("someday".to_owned());
        assert!(restore_requirement(&saved, 1).is_none());

        // A sum total without its group is a decode error, matching the wire.
        let mut saved = save_requirement(&UiRequirement::new(1));
        saved.upgrade_sum_total = Some(2);
        assert!(restore_requirement(&saved, 1).is_none());
    }

    #[test]
    fn user_presets_round_trip_and_drop_bad_entries() {
        let mut state = AppState::default();
        state.max_depth = 12;
        state.fast_mode = true;
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            key,
            kind: ItemKind::Weapon,
            item: Some(ItemId::Greatsword),
            upgrade: UpgradeRequirement::AtLeast(2),
            require_uncursed: true,
            ..UiRequirement::new(key)
        });
        let value = serde_json::to_value(SavedPreset {
            name: "My preset".to_owned(),
            query: save_state(&state),
        })
        .unwrap();
        let contents =
            serde_json::to_string(&vec![value, serde_json::json!({ "bad": true })]).unwrap();

        let presets = decode_presets(&contents);
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "My preset");
        assert_eq!(presets[0].state.max_depth, 12);
        assert!(presets[0].state.fast_mode);
        assert_eq!(presets[0].state.requirements.len(), 1);
        assert_eq!(
            presets[0].state.requirements[0].item,
            Some(ItemId::Greatsword)
        );
        assert!(presets[0].state.requirements[0].require_uncursed);
    }
}
