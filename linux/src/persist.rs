// SPDX-License-Identifier: GPL-3.0-or-later

//! Query-state persistence in the user configuration directory.
//!
//! The saved query and every user preset are written as the engine's
//! canonical query document — the format share links, results files and the
//! CLI already speak — and read back with
//! [`json_query::decode_unvalidated`], which accepts the half-finished
//! queries an editor holds. Files written before that format are still read,
//! once, by [`legacy`].

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

/// Restores one saved query: the canonical document first, then the legacy
/// format. A document the engine cannot read — one from a newer build, say —
/// is refused whole rather than in part, and the caller falls back to
/// defaults.
fn decode_state(contents: &str) -> Option<AppState> {
    json_query::decode_unvalidated(contents)
        .ok()
        .or_else(|| legacy::decode(contents))
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

/// The saved-state format this app wrote before it wrote query documents.
///
/// Read-only: nothing produces it any more and it is consulted only when the
/// canonical decoder refuses a file. It rewrites the old shape into a query
/// document and lets the engine decode that, so the kind, item, effect and
/// source name tables the old format shared with the document format do not
/// survive here — only the field shapes it spelled differently.
mod legacy {
    use serde::Deserialize;
    use serde_json::{Map, Value, json};
    use shpd_seedfinder_core::challenges::Challenges;
    use shpd_seedfinder_core::json_query::{self, CHALLENGE_NAMES};
    use shpd_seedfinder_core::query::SearchQuery;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SavedState {
        requirements: Vec<SavedRequirement>,
        max_depth: Option<u8>,
        #[serde(default)]
        require_blacksmith: bool,
        #[serde(default)]
        exclude_blacksmith_rewards: bool,
        #[serde(default)]
        wandmaker_quest: Option<String>,
        #[serde(default)]
        fast_mode: bool,
        #[serde(default)]
        challenges: u16,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SavedRequirement {
        kind: String,
        item: Option<String>,
        tier: Option<SavedPredicate>,
        upgrade: Option<SavedPredicate>,
        effect: Option<String>,
        #[serde(default)]
        require_uncursed: bool,
        source: Option<String>,
        identity_group: Option<u8>,
        max_depth: Option<u8>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SavedPredicate {
        mode: String,
        value: u8,
    }

    /// Decodes a pre-document saved query, or `None` when the file is not one.
    pub fn decode(contents: &str) -> Option<SearchQuery> {
        let saved: SavedState = serde_json::from_str(contents).ok()?;
        json_query::decode_unvalidated(&document(saved).to_string()).ok()
    }

    /// Rewrites the old shape into a query document: `{"mode","value"}`
    /// predicates become the document's single-key filters and the challenge
    /// mask becomes its names. Every name is passed through untouched, so a
    /// name the engine does not know stays unknown and its decoder says so.
    fn document(saved: SavedState) -> Value {
        let mut document = Map::new();
        document.insert(
            "requirements".to_owned(),
            Value::Array(saved.requirements.into_iter().map(requirement).collect()),
        );
        if let Some(max_depth) = saved.max_depth {
            document.insert("max_depth".to_owned(), json!(max_depth));
        }
        if saved.require_blacksmith {
            document.insert("require_blacksmith".to_owned(), json!(true));
        }
        if saved.exclude_blacksmith_rewards {
            document.insert("exclude_blacksmith_rewards".to_owned(), json!(true));
        }
        if let Some(quest) = saved.wandmaker_quest {
            document.insert("wandmaker_quest".to_owned(), json!(quest));
        }
        if saved.fast_mode {
            document.insert("fast_mode".to_owned(), json!(true));
        }
        // An out-of-range mask named no challenge the app could set, so it
        // restores as none at all, exactly as it used to.
        let mask = Challenges::new(saved.challenges).unwrap_or(Challenges::NONE);
        let challenges: Vec<Value> = CHALLENGE_NAMES
            .iter()
            .filter(|(_, challenge)| mask.contains(*challenge))
            .map(|(name, _)| json!(name))
            .collect();
        if !challenges.is_empty() {
            document.insert("challenges".to_owned(), Value::Array(challenges));
        }
        Value::Object(document)
    }

    fn requirement(saved: SavedRequirement) -> Value {
        let mut output = Map::new();
        output.insert("kind".to_owned(), json!(saved.kind));
        if let Some(item) = saved.item {
            output.insert("item".to_owned(), json!(item));
        }
        if let Some(tier) = saved.tier {
            output.insert("tier".to_owned(), predicate(tier));
        }
        if let Some(upgrade) = saved.upgrade {
            output.insert("upgrade".to_owned(), predicate(upgrade));
        }
        if let Some(effect) = saved.effect {
            output.insert("effect".to_owned(), json!(effect));
        }
        if saved.require_uncursed {
            output.insert("uncursed".to_owned(), json!(true));
        }
        if let Some(source) = saved.source {
            output.insert("source".to_owned(), json!(source));
        }
        if let Some(group) = saved.identity_group {
            output.insert("identity_group".to_owned(), json!(group));
        }
        if let Some(max_depth) = saved.max_depth {
            output.insert("max_depth".to_owned(), json!(max_depth));
        }
        Value::Object(output)
    }

    fn predicate(saved: SavedPredicate) -> Value {
        let mut filter = Map::new();
        filter.insert(saved.mode, json!(saved.value));
        Value::Object(filter)
    }
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

    use super::{SavedPreset, decode_presets, decode_state, legacy, save_document};
    use crate::state::{AppState, UiRequirement};

    /// One saved state written to disk and read back.
    fn round_trip(state: &AppState) -> AppState {
        decode_state(&save_document(state).to_string()).expect("the canonical document must load")
    }

    /// The engine predicates of every row, which carry no session row keys.
    fn predicates(state: &AppState) -> Vec<Requirement> {
        state.requirements.iter().map(|r| r.to_core()).collect()
    }

    /// A saved file in the format the app wrote before query documents.
    const LEGACY_STATE: &str = r#"{
      "requirements": [
        {
          "kind": "melee_weapon",
          "item": null,
          "tier": { "mode": "at_least", "value": 4 },
          "upgrade": { "mode": "exact", "value": 2 },
          "effect": null,
          "require_uncursed": true,
          "source": "sacrificial_fire",
          "identity_group": 2,
          "max_depth": 10
        },
        {
          "kind": "ring",
          "item": "ring_tenacity",
          "tier": null,
          "upgrade": { "mode": "at_least", "value": 2 },
          "effect": null,
          "require_uncursed": false,
          "source": null,
          "identity_group": null,
          "max_depth": null
        }
      ],
      "max_depth": 15,
      "require_blacksmith": true,
      "exclude_blacksmith_rewards": true,
      "wandmaker_quest": "elemental_embers",
      "fast_mode": true,
      "challenges": 72
    }"#;

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
    fn legacy_saved_states_load_and_are_rewritten_as_documents() {
        let restored = decode_state(LEGACY_STATE).expect("legacy state must still load");
        assert_eq!(restored.max_depth, 14);
        assert!(restored.require_blacksmith);
        assert!(restored.exclude_blacksmith_rewards);
        assert_eq!(
            restored.wandmaker_quest,
            Some(WandmakerQuestType::ElementalEmbers)
        );
        assert!(restored.fast_mode);
        assert_eq!(
            restored.challenges,
            Challenges::NO_HERBALISM | Challenges::NO_SCROLLS
        );
        assert_eq!(
            predicates(&restored),
            vec![
                Requirement {
                    kind: ItemKind::Weapon,
                    weapon_category: Some(WeaponCategory::Melee),
                    item: None,
                    tier: TierRequirement::AtLeast(4),
                    upgrade: UpgradeRequirement::Exact(2),
                    effect: None,
                    require_uncursed: true,
                    source: Some(ItemSource::SacrificialFire),
                    identity_group: Some(2),
                    max_depth: Some(9),
                },
                Requirement {
                    kind: ItemKind::Ring,
                    weapon_category: None,
                    item: Some(ItemId::RingTenacity),
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::AtLeast(2),
                    effect: None,
                    require_uncursed: false,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                },
            ]
        );

        // Re-saving migrates the file: what goes back to disk is a canonical
        // document, the legacy reader no longer recognizes it, and loading it
        // again restores the identical state.
        let migrated = save_document(&restored).to_string();
        assert!(json_query::decode_unvalidated(&migrated).is_ok());
        assert!(legacy::decode(&migrated).is_none());
        let reloaded = decode_state(&migrated).unwrap();
        assert_eq!(predicates(&reloaded), predicates(&restored));
        assert_eq!(reloaded.max_depth, restored.max_depth);
        assert_eq!(reloaded.challenges, restored.challenges);
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
        // An unknown name is refused whole rather than in part: neither
        // decoder accepts it, and the caller starts from defaults.
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
            json!({ "name": "Legacy preset", "query": serde_json::from_str::<serde_json::Value>(LEGACY_STATE).unwrap() }),
            json!({ "bad": true }),
            json!({ "name": "  ", "query": { "requirements": [] } }),
        ]);

        let presets = decode_presets(&saved.to_string());
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[0].name, "My preset");
        assert_eq!(predicates(&presets[0].state), predicates(&state));
        assert!(presets[0].state.fast_mode);
        // Presets saved in the old format keep loading too.
        assert_eq!(presets[1].name, "Legacy preset");
        assert_eq!(presets[1].state.max_depth, 14);
        assert_eq!(presets[1].state.requirements.len(), 2);
    }
}
