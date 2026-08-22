// SPDX-License-Identifier: GPL-3.0-or-later

//! Query-state persistence in the user configuration directory.
//!
//! The saved query and every user preset are written as the engine's
//! canonical query document — the format share links, results files and the
//! CLI already speak — and read back with
//! [`json_query::decode_unvalidated`], which accepts the half-finished
//! queries an editor holds.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use shpd_seedfinder_core::json_query;
use shpd_seedfinder_core::main_world::normalize_floor_limit;
use shpd_seedfinder_core::query::{MAX_SEARCH_DEPTH, SearchQuery};

use crate::config::APP_ID;
use crate::state::AppState;

/// One named query saved by the user.
#[derive(Clone, Debug)]
pub struct UserPreset {
    pub name: String,
    pub state: AppState,
}

/// One preset on disk: a name and the query document it stands for.
#[derive(Deserialize, Serialize)]
struct SavedPreset {
    name: String,
    query: Value,
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
    decode_state(&contents).unwrap_or_default()
}

/// Saves the current query, quietly giving up on filesystem errors.
pub fn save(state: &AppState) {
    write_json(state_path(), &save_document(state));
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
    let Ok(saved) = serde_json::from_str::<Vec<Value>>(contents) else {
        return Vec::new();
    };
    saved
        .into_iter()
        .filter_map(|value| serde_json::from_value::<SavedPreset>(value).ok())
        .filter_map(|preset| {
            let name = preset.name.trim();
            if name.is_empty() {
                return None;
            }
            Some(UserPreset {
                name: name.to_owned(),
                state: decode_state(&preset.query.to_string())?,
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
            query: save_document(&preset.state),
        })
        .collect();
    write_json(presets_path(), &saved);
}

/// The editor state as a canonical query document.
///
/// The document keeps `require_blacksmith` exactly as the user left it:
/// [`AppState::to_query`] drops that filter once the floor limit makes the
/// quest certain, which is a rule about the search rather than about the
/// saved preference.
fn save_document(state: &AppState) -> Value {
    json_query::encode(&SearchQuery {
        requirements: state.requirements.iter().map(|r| r.to_core()).collect(),
        max_depth: state.max_depth,
        challenges: state.challenges,
        require_blacksmith: state.require_blacksmith,
        exclude_blacksmith_rewards: state.exclude_blacksmith_rewards,
        wandmaker_quest: state.wandmaker_quest,
        fast_mode: state.fast_mode,
    })
}

/// Restores one saved query. A document the engine cannot read — one from a
/// newer build, say — is refused whole rather than in part, and the caller
/// falls back to defaults.
fn decode_state(contents: &str) -> Option<AppState> {
    json_query::decode_unvalidated(contents)
        .ok()
        .map(|query| restore(&query))
}

/// Rebuilds editor state from a decoded document. Floor limits saved before
/// the empty boss floors were removed may hold 5/10/15 and snap to the
/// equivalent limit below; a requirement the engine would reject is dropped
/// rather than loaded into the editor.
fn restore(query: &SearchQuery) -> AppState {
    let mut state = AppState::from_query(query);
    state.max_depth = normalize_floor_limit(state.max_depth.clamp(1, MAX_SEARCH_DEPTH));
    state.requirements.retain_mut(|requirement| {
        requirement.max_depth = requirement.max_depth.map(normalize_floor_limit);
        requirement.to_core().validate().is_ok()
    });
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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use shpd_seedfinder_core::catalog::{Effect, ItemId, ItemKind, WeaponCategory, WeaponEffect};
    use shpd_seedfinder_core::challenges::Challenges;
    use shpd_seedfinder_core::json_query;
    use shpd_seedfinder_core::model::ItemSource;
    use shpd_seedfinder_core::query::{Requirement, TierRequirement, UpgradeRequirement};
    use shpd_seedfinder_core::quests::WandmakerQuestType;

    use super::{SavedPreset, decode_presets, decode_state, save_document};
    use crate::state::{AppState, UiRequirement};

    /// One saved state written to disk and read back.
    fn round_trip(state: &AppState) -> AppState {
        decode_state(&save_document(state).to_string()).expect("the canonical document must load")
    }

    /// The engine predicates of every row, which carry no session row keys.
    fn predicates(state: &AppState) -> Vec<Requirement> {
        state.requirements.iter().map(|r| r.to_core()).collect()
    }

    fn populated_state() -> AppState {
        let mut state = AppState::default();
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            item: Some(ItemId::Greatsword),
            upgrade: UpgradeRequirement::AtLeast(2),
            effect: Some(Effect::Weapon(WeaponEffect::Blazing)),
            require_uncursed: true,
            source: Some(ItemSource::SacrificialFire),
            identity_group: Some(3),
            max_depth: Some(21),
            ..UiRequirement::new(key)
        });
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            weapon_category: Some(WeaponCategory::Thrown),
            tier: TierRequirement::Exact(4),
            ..UiRequirement::new(key)
        });
        state.max_depth = 13;
        state.require_blacksmith = true;
        state.exclude_blacksmith_rewards = true;
        state.wandmaker_quest = Some(WandmakerQuestType::Rotberry);
        state.fast_mode = true;
        state.challenges = Challenges::DARKNESS;
        state
    }

    #[test]
    fn saved_queries_round_trip_as_canonical_documents() {
        let state = populated_state();
        let document = save_document(&state);
        // What the app writes is the format every other tool reads.
        assert!(json_query::decode_unvalidated(&document.to_string()).is_ok());

        let restored = round_trip(&state);
        assert_eq!(predicates(&restored), predicates(&state));
        assert_eq!(restored.max_depth, 13);
        assert!(restored.require_blacksmith);
        assert!(restored.exclude_blacksmith_rewards);
        assert_eq!(restored.wandmaker_quest, Some(WandmakerQuestType::Rotberry));
        assert!(restored.fast_mode);
        assert_eq!(restored.challenges, Challenges::DARKNESS);
        // Saving the restored state again produces the identical document.
        assert_eq!(save_document(&restored), document);
    }

    #[test]
    fn an_empty_editor_state_keeps_its_scope_and_flags() {
        let mut state = AppState::default();
        state.max_depth = 11;
        state.exclude_blacksmith_rewards = true;
        state.wandmaker_quest = Some(WandmakerQuestType::CorpseDust);
        state.fast_mode = true;
        state.challenges = Challenges::NO_HERBALISM | Challenges::NO_SCROLLS;

        // A query with no requirements yet is not a runnable search, but it is
        // the state the editor holds most of the time and it must survive a
        // restart intact.
        let restored = round_trip(&state);
        assert!(restored.requirements.is_empty());
        assert_eq!(restored.max_depth, 11);
        assert!(restored.exclude_blacksmith_rewards);
        assert_eq!(
            restored.wandmaker_quest,
            Some(WandmakerQuestType::CorpseDust)
        );
        assert!(restored.fast_mode);
        assert_eq!(
            restored.challenges,
            Challenges::NO_HERBALISM | Challenges::NO_SCROLLS
        );
    }

    #[test]
    fn the_blacksmith_switch_is_saved_as_the_user_left_it() {
        let mut state = AppState::default();
        let key = state.claim_key();
        state.requirements.push(UiRequirement::new(key));
        state.require_blacksmith = true;
        state.max_depth = 24;
        // The search drops a filter every seed satisfies; the saved
        // preference is the user's, so it comes back with the switch on.
        assert!(!state.to_query().unwrap().require_blacksmith);
        assert!(round_trip(&state).require_blacksmith);
    }

    #[test]
    fn empty_boss_floor_limits_snap_to_the_floor_below() {
        let mut state = AppState::default();
        state.max_depth = 15;
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            kind: ItemKind::Wand,
            max_depth: Some(10),
            ..UiRequirement::new(key)
        });
        let restored = round_trip(&state);
        assert_eq!(restored.max_depth, 14);
        assert_eq!(restored.requirements[0].max_depth, Some(9));
    }

    #[test]
    fn wandmaker_quests_round_trip_by_name() {
        let mut state = AppState::default();
        for variant in WandmakerQuestType::ALL {
            state.wandmaker_quest = Some(variant);
            assert_eq!(round_trip(&state).wandmaker_quest, Some(variant));
        }
    }

    #[test]
    fn documents_the_engine_cannot_read_fall_back_to_defaults() {
        // An unknown name is refused whole rather than in part, and the
        // caller starts from defaults.
        assert!(decode_state(r#"{"requirements":[{"kind":"trinket"}]}"#).is_none());
        assert!(decode_state(r#"{"requirements":[],"wandmaker_quest":"newt"}"#).is_none());
        assert!(decode_state("not json at all").is_none());
        // A row the engine would reject is dropped; the rest of the query
        // still loads.
        let restored = decode_state(
            r#"{"requirements":[{"kind":"wand","tier":{"exact":3}}],"fast_mode":true}"#,
        )
        .unwrap();
        assert!(restored.requirements.is_empty());
        assert!(restored.fast_mode);
    }

    #[test]
    fn user_presets_round_trip_and_drop_bad_entries() {
        let state = populated_state();
        let saved = json!([
            serde_json::to_value(SavedPreset {
                name: "My preset".to_owned(),
                query: save_document(&state),
            })
            .unwrap(),
            json!({ "bad": true }),
            json!({ "name": "  ", "query": { "requirements": [] } }),
        ]);

        let presets = decode_presets(&saved.to_string());
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "My preset");
        assert_eq!(predicates(&presets[0].state), predicates(&state));
        assert!(presets[0].state.fast_mode);
    }
}
