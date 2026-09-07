// SPDX-License-Identifier: GPL-3.0-or-later

//! Query-state persistence in the user configuration directory.
//!
//! The saved query and every user preset are written as the engine's
//! canonical query document — the format share links, results files and the
//! CLI already speak — and read back with
//! [`json_query::decode_unvalidated`], which accepts the half-finished
//! queries an editor holds.
//!
//! Preferences that describe this machine rather than the search — the worker
//! count so far — are kept in a separate file. A canonical query document
//! admits no keys of its own, so a device-local setting cannot ride along in
//! `state.json` without making it unreadable to every other tool, and it has
//! no business travelling with a preset, a share link or an exported results
//! file either.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use shpd_seedfinder_core::json_query;
use shpd_seedfinder_core::main_world::normalize_floor_limit;
use shpd_seedfinder_core::query::{MAX_SEARCH_DEPTH, SearchQuery};
use shpd_seedfinder_session::available_workers;

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

/// The device-local preferences file. Unknown keys are accepted and a missing
/// one falls back to its default, so a file written by any other build of the
/// app still loads.
#[derive(Default, Deserialize, Serialize)]
struct SavedPreferences {
    /// Search threads to spawn; unset means every core this machine has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workers: Option<usize>,
}

fn state_path() -> PathBuf {
    gtk::glib::user_config_dir().join(APP_ID).join("state.json")
}

fn preferences_path() -> PathBuf {
    gtk::glib::user_config_dir()
        .join(APP_ID)
        .join("preferences.json")
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

/// Loads the search worker count, falling back to every core when the
/// preference has never been set or cannot be read.
#[must_use]
pub fn load_workers() -> usize {
    let contents = fs::read_to_string(preferences_path()).unwrap_or_default();
    decode_workers(&contents, available_workers())
}

/// Saves the search worker count, quietly giving up on filesystem errors.
pub fn save_workers(workers: usize) {
    write_json(
        preferences_path(),
        &SavedPreferences {
            workers: Some(clamp_workers(workers, available_workers())),
        },
    );
}

/// Reads the worker count out of a preferences file. A file the app cannot
/// parse is treated as absent rather than refused: the setting is a
/// convenience, and losing it must never cost the user anything else.
fn decode_workers(contents: &str, ceiling: usize) -> usize {
    serde_json::from_str::<SavedPreferences>(contents)
        .ok()
        .and_then(|preferences| preferences.workers)
        .map_or_else(
            || clamp_workers(ceiling, ceiling),
            |workers| clamp_workers(workers, ceiling),
        )
}

/// Fits a worker count into what this machine offers. A count saved on a
/// bigger machine, or a nonsensical zero, comes back inside `[1, ceiling]`.
fn clamp_workers(workers: usize, ceiling: usize) -> usize {
    workers.clamp(1, ceiling.max(1))
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
    json_query::encode(&state.unvalidated_query())
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
    use shpd_seedfinder_core::query::{
        EffectRequirement, Requirement, TierRequirement, UpgradeRequirement,
    };
    use shpd_seedfinder_core::quests::WandmakerQuestType;

    use super::{
        SavedPreset, clamp_workers, decode_presets, decode_state, decode_workers, save_document,
    };
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
            effect: EffectRequirement::exactly(Effect::Weapon(WeaponEffect::Blazing)),
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
        assert_eq!(restored.challenges, Challenges::DARKNESS);
        // Saving the restored state again produces the identical document.
        assert_eq!(save_document(&restored), document);
    }

    #[test]
    fn alternatives_effect_sets_and_level_sums_round_trip() {
        use shpd_seedfinder_core::catalog::ArmorEffect;
        use shpd_seedfinder_core::query::{EffectRequirement, EffectSet, LevelSum};

        let mut state = AppState::default();
        // An "any of these" slot of two weapons.
        for (item, upgrade) in [(ItemId::Spear, 3), (ItemId::Shuriken, 2)] {
            let key = state.claim_key();
            state.requirements.push(UiRequirement {
                item: Some(item),
                upgrade: UpgradeRequirement::Exact(upgrade),
                alternative_group: Some(1),
                ..UiRequirement::new(key)
            });
        }
        // Armor with one of two glyphs, then any enchantment.
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            kind: ItemKind::Armor,
            effect: EffectRequirement::OneOf(
                EffectSet::from_effects([
                    Effect::Armor(ArmorEffect::Stone),
                    Effect::Armor(ArmorEffect::Brimstone),
                ])
                .unwrap(),
            ),
            ..UiRequirement::new(key)
        });
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            effect: EffectRequirement::OneOf(EffectSet::enchantments(ItemKind::Weapon).unwrap()),
            require_uncursed: true,
            ..UiRequirement::new(key)
        });
        // Two Rings of Might adding up to +4.
        for _ in 0..2 {
            let key = state.claim_key();
            state.requirements.push(UiRequirement {
                kind: ItemKind::Ring,
                item: Some(ItemId::RingMight),
                level_sum: Some(LevelSum {
                    group: 1,
                    minimum_total: 4,
                }),
                ..UiRequirement::new(key)
            });
        }
        assert!(state.to_query().is_ok());

        let document = save_document(&state);
        let entries = document["requirements"].as_array().unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0]["any_of"].as_array().unwrap().len(), 2);
        assert_eq!(entries[1]["effect"], json!(["Brimstone", "Stone"]));
        assert_eq!(entries[2]["effect"], json!("any_enchantment"));
        assert_eq!(
            entries[3]["level_sum"],
            json!({ "group": 1, "at_least": 4 })
        );

        let restored = round_trip(&state);
        assert_eq!(predicates(&restored), predicates(&state));
        assert_eq!(restored.unvalidated_query().slot_count(), 5);
        assert_eq!(save_document(&restored), document);
    }

    #[test]
    fn an_empty_editor_state_keeps_its_scope_and_flags() {
        let mut state = AppState::default();
        state.max_depth = 11;
        state.exclude_blacksmith_rewards = true;
        state.wandmaker_quest = Some(WandmakerQuestType::CorpseDust);
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
        assert!(decode_state(r#"{"requirements":[{"kind":"unknown_kind"}]}"#).is_none());
        assert!(decode_state(r#"{"requirements":[],"wandmaker_quest":"newt"}"#).is_none());
        assert!(decode_state("not json at all").is_none());
        // A row the engine would reject is dropped; the rest of the query
        // still loads.
        let restored =
            decode_state(r#"{"requirements":[{"kind":"wand","tier":{"exact":3}}]}"#).unwrap();
        assert!(restored.requirements.is_empty());
    }

    #[test]
    fn named_trinkets_survive_persistence_while_wildcards_are_dropped() {
        let restored =
            decode_state(r#"{"requirements":[{"kind":"trinket"},{"item":"mimic_tooth"}]}"#)
                .expect("known trinkets are readable even when a row is invalid");
        assert_eq!(restored.requirements.len(), 1);
        assert_eq!(restored.requirements[0].kind, ItemKind::Trinket);
        assert_eq!(restored.requirements[0].item, Some(ItemId::MimicTooth));
        let saved_again = round_trip(&restored);
        assert_eq!(saved_again.requirements.len(), 1);
        assert_eq!(saved_again.requirements[0].item, Some(ItemId::MimicTooth));
    }

    #[test]
    fn states_saved_before_fast_mode_was_retired_still_load() {
        // `fast_mode` is a retired flag: a state file written by an older
        // build must still load, with the key accepted and ignored rather
        // than refusing the whole document.
        let restored =
            decode_state(r#"{"requirements":[{"kind":"wand"}],"max_depth":11,"fast_mode":true}"#)
                .expect("an old saved state must still load");
        assert_eq!(restored.requirements.len(), 1);
        assert_eq!(restored.max_depth, 11);
        // Saving again writes the current format, without the retired key.
        assert!(save_document(&restored).get("fast_mode").is_none());
    }

    #[test]
    fn preferences_saved_before_the_worker_count_existed_still_load() {
        // Mirror image of the retired-flag case: a preferences file written
        // before this setting existed — or none at all, or one from a build
        // that keeps settings this one has never heard of — must load, with
        // the missing count meaning "every core".
        for contents in [
            "",
            "{}",
            r#"{"theme":"dark"}"#,
            "not json at all",
            r#"{"workers":null}"#,
        ] {
            assert_eq!(decode_workers(contents, 8), 8, "{contents:?}");
        }
        // A saved count is honoured as written.
        assert_eq!(decode_workers(r#"{"workers":3}"#, 8), 3);
    }

    #[test]
    fn worker_counts_are_clamped_to_this_machine() {
        // A file carried over from a bigger machine cannot ask for more
        // threads than this one has, and no file can ask for none.
        assert_eq!(decode_workers(r#"{"workers":64}"#, 8), 8);
        assert_eq!(decode_workers(r#"{"workers":0}"#, 8), 1);
        assert_eq!(decode_workers(r#"{"workers":1}"#, 1), 1);
        assert_eq!(decode_workers(r#"{"workers":9}"#, 1), 1);
        // The ceiling is never below one, even if the host reports nothing.
        assert_eq!(decode_workers("{}", 0), 1);
        assert_eq!(clamp_workers(4, 4), 4);
    }

    #[test]
    fn the_worker_count_never_enters_a_saved_query() {
        // The setting describes the machine, not the search: it stays out of
        // the query document the app saves, shares and stores in presets,
        // which must keep decoding as the engine's canonical format.
        let document = save_document(&populated_state());
        assert!(document.get("workers").is_none());
        assert!(json_query::decode_unvalidated(&document.to_string()).is_ok());
        // A preset stores the same document, so it cannot carry the setting
        // either — and a preferences file is not a query the editor loads.
        assert!(decode_state(r#"{"requirements":[],"workers":3}"#).is_none());
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
    }
}
