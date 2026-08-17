//! The engine constants every frontend must agree on, as one JSON document.
//!
//! Frontends used to hardcode their own copies of the query bounds, the empty
//! boss floors, the quest windows, and the challenge list. They are all facts
//! about this engine, so it publishes them instead: the same document is
//! served by `seedfinder_engine_info` (C), `engineInfo` (Android) and
//! `engine_info` (wasm), and every value below is read from the constant that
//! the engine itself uses.

use serde_json::{Value, json};

use crate::catalog::ItemKind;
use crate::feasibility::Quest;
use crate::json_query::CHALLENGE_NAMES;
use crate::main_world::EMPTY_BOSS_FLOORS;
use crate::query::{
    BOUNDED_TIER_MAX, BOUNDED_TIER_MIN, EXACT_TIER_MAX, EXACT_TIER_MIN, MAX_IDENTITY_GROUP,
    MAX_SEARCH_DEPTH,
};
use crate::results_export::MAX_FILE_BYTES;
use crate::search::PRODUCTION_SEARCH_START_STRIDE;
use crate::seed::TOTAL_SEEDS;
use crate::{SHPD_COMMIT, SHPD_VERSION};

/// Builds the engine-info document. `max_results` is the caller's own
/// result cap — the browser session and the native sessions cap alike, but
/// each owns its constant — and appears both as the pre-existing
/// `maxResults` key and inside `limits`.
#[must_use]
pub fn document(max_results: usize) -> Value {
    json!({
        "shpdVersion": SHPD_VERSION,
        "shpdCommit": SHPD_COMMIT,
        "totalSeeds": TOTAL_SEEDS,
        "maxResults": max_results,
        "limits": {
            "max_depth": MAX_SEARCH_DEPTH,
            "exact_tier_min": EXACT_TIER_MIN,
            "exact_tier_max": EXACT_TIER_MAX,
            "bounded_tier_min": BOUNDED_TIER_MIN,
            "bounded_tier_max": BOUNDED_TIER_MAX,
            "identity_group_max": MAX_IDENTITY_GROUP,
            "max_upgrade_default": ItemKind::Weapon.maximum_search_upgrade(),
            "max_upgrade_ring": ItemKind::Ring.maximum_search_upgrade(),
            "max_results": max_results,
            "results_file_max_bytes": MAX_FILE_BYTES,
        },
        "empty_boss_floors": EMPTY_BOSS_FLOORS,
        "quest_windows": quest_windows(),
        "challenges": challenges(),
        "search_start_stride": PRODUCTION_SEARCH_START_STRIDE,
    })
}

fn quest_windows() -> Value {
    let window = |quest: Quest| {
        let (start, end) = quest.window();
        json!([start, end])
    };
    json!({
        "ghost": window(Quest::Ghost),
        "wandmaker": window(Quest::Wandmaker),
        "blacksmith": window(Quest::Blacksmith),
        "imp": window(Quest::Imp),
    })
}

fn challenges() -> Value {
    Value::Array(
        CHALLENGE_NAMES
            .iter()
            .map(|(name, challenge)| {
                json!({
                    "name": name,
                    "mask": challenge.bits(),
                    "changes_level_generation": challenge.changes_level_generation(),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use crate::feasibility::QUESTS;

    use super::{Quest, document};

    #[test]
    fn the_document_publishes_the_engine_constants() {
        let info = document(1_024);
        assert_eq!(info["shpdVersion"], crate::SHPD_VERSION);
        assert_eq!(info["totalSeeds"], crate::seed::TOTAL_SEEDS);
        assert_eq!(info["maxResults"], 1_024);
        assert_eq!(info["limits"]["max_depth"], 24);
        assert_eq!(info["limits"]["exact_tier_min"], 2);
        assert_eq!(info["limits"]["exact_tier_max"], 5);
        assert_eq!(info["limits"]["bounded_tier_min"], 3);
        assert_eq!(info["limits"]["bounded_tier_max"], 4);
        assert_eq!(info["limits"]["identity_group_max"], 4);
        assert_eq!(info["limits"]["max_upgrade_default"], 3);
        assert_eq!(info["limits"]["max_upgrade_ring"], 4);
        assert_eq!(info["limits"]["max_results"], 1_024);
        assert_eq!(info["limits"]["results_file_max_bytes"], 2 * 1_024 * 1_024);
        assert_eq!(info["empty_boss_floors"], serde_json::json!([5, 10, 15]));
        assert_eq!(info["search_start_stride"], 3_355_211_884_971_u64);

        // Every quest window is the feasibility model's own.
        for (name, quest) in ["ghost", "wandmaker", "blacksmith", "imp"]
            .into_iter()
            .zip(QUESTS)
        {
            let (start, end) = Quest::window(quest);
            assert_eq!(info["quest_windows"][name], serde_json::json!([start, end]));
        }
        assert_eq!(
            info["quest_windows"]["wandmaker"],
            serde_json::json!([7, 9])
        );

        // The challenges are listed in mask order with their generation
        // relevance; only the three the generator consults are marked.
        let challenges = info["challenges"].as_array().unwrap();
        assert_eq!(challenges.len(), 9);
        for (index, challenge) in challenges.iter().enumerate() {
            assert_eq!(challenge["mask"], 1_u16 << index);
        }
        let generating: Vec<&str> = challenges
            .iter()
            .filter(|challenge| challenge["changes_level_generation"] == true)
            .map(|challenge| challenge["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            generating,
            ["barren_land", "into_darkness", "forbidden_runes"]
        );
        assert_eq!(challenges[0]["name"], "on_diet");
    }
}
