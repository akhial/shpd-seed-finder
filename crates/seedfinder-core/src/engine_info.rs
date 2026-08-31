//! The engine constants every frontend must agree on, as one JSON document.
//!
//! Frontends used to hardcode their own copies of the query bounds, the empty
//! boss floors, the quest windows, and the challenge list. They are all facts
//! about this engine, so it publishes them instead: the same document is
//! served by `seedfinder_engine_info` (C), `engineInfo` (Android) and
//! `engine_info` (wasm), and every value below is read from the constant that
//! the engine itself uses.

use serde_json::{Value, json};

use crate::catalog::{EXTRA_UPGRADE_TIER, ItemKind, MAX_GENERATED_UPGRADE};
use crate::feasibility::Quest;
use crate::json_query::CHALLENGE_NAMES;
use crate::main_world::EMPTY_BOSS_FLOORS;
use crate::query::{
    BOUNDED_TIER_MAX, BOUNDED_TIER_MIN, EXACT_TIER_MAX, EXACT_TIER_MIN, MAX_IDENTITY_GROUP,
    MAX_LEVEL_SUM_GROUP, MAX_SEARCH_DEPTH,
};
use crate::results_export::{MAX_FILE_BYTES, MAX_RESULTS};
use crate::search::PRODUCTION_SEARCH_START_STRIDE;
use crate::seed::TOTAL_SEEDS;
use crate::{SHPD_COMMIT, SHPD_VERSION};

/// Builds the engine-info document. Every key is camelCase: the four keys the
/// browser already read set the convention, and the rest follow it.
#[must_use]
pub fn document() -> Value {
    json!({
        "shpdVersion": SHPD_VERSION,
        "shpdCommit": SHPD_COMMIT,
        "totalSeeds": TOTAL_SEEDS,
        "maxResults": MAX_RESULTS,
        "limits": {
            "maxDepth": MAX_SEARCH_DEPTH,
            "exactTierMin": EXACT_TIER_MIN,
            "exactTierMax": EXACT_TIER_MAX,
            "boundedTierMin": BOUNDED_TIER_MIN,
            "boundedTierMax": BOUNDED_TIER_MAX,
            "identityGroupMax": MAX_IDENTITY_GROUP,
            "levelSumGroupMax": MAX_LEVEL_SUM_GROUP,
            // `maxUpgradeDefault` predates per-kind ceilings; it is the
            // ceiling shared by armor, wands and rings, while weapons (melee
            // and thrown) reach `maxUpgradeWeapon`.
            "maxUpgradeDefault": ItemKind::Armor.maximum_search_upgrade(),
            "maxUpgradeRing": ItemKind::Ring.maximum_search_upgrade(),
            "maxUpgradeWeapon": ItemKind::Weapon.maximum_search_upgrade(),
            "maxUpgradeByKind": {
                "weapon": ItemKind::Weapon.maximum_search_upgrade(),
                "armor": ItemKind::Armor.maximum_search_upgrade(),
                "wand": ItemKind::Wand.maximum_search_upgrade(),
                "ring": ItemKind::Ring.maximum_search_upgrade(),
            },
            // The per-kind ceilings above are what a weapon reaches at its
            // best tier. Only tier `extraUpgradeTier` gets there: every other
            // tier, and every other family, stops at `maxUpgradeAnyTier`, so
            // an editor offering the top level has to know the tier the
            // requirement is asking for.
            "maxUpgradeAnyTier": MAX_GENERATED_UPGRADE,
            "extraUpgradeTier": EXTRA_UPGRADE_TIER,
            "resultsFileMaxBytes": MAX_FILE_BYTES,
        },
        "emptyBossFloors": EMPTY_BOSS_FLOORS,
        "questWindows": quest_windows(),
        "challenges": challenges(),
        "searchStartStride": PRODUCTION_SEARCH_START_STRIDE,
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
                    "changesLevelGeneration": challenge.changes_level_generation(),
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
        let info = document();
        assert_eq!(info["shpdVersion"], crate::SHPD_VERSION);
        assert_eq!(info["totalSeeds"], crate::seed::TOTAL_SEEDS);
        assert_eq!(info["maxResults"], 1_024);
        assert_eq!(info["shpdCommit"], crate::SHPD_COMMIT);
        assert_eq!(info["limits"]["maxDepth"], 24);
        assert_eq!(info["limits"]["exactTierMin"], 2);
        assert_eq!(info["limits"]["exactTierMax"], 5);
        assert_eq!(info["limits"]["boundedTierMin"], 3);
        assert_eq!(info["limits"]["boundedTierMax"], 4);
        assert_eq!(info["limits"]["identityGroupMax"], 4);
        assert_eq!(info["limits"]["levelSumGroupMax"], 4);
        assert_eq!(info["limits"]["maxUpgradeDefault"], 4);
        assert_eq!(info["limits"]["maxUpgradeRing"], 4);
        assert_eq!(info["limits"]["maxUpgradeWeapon"], 5);
        assert_eq!(
            info["limits"]["maxUpgradeByKind"],
            serde_json::json!({"weapon": 5, "armor": 4, "wand": 4, "ring": 4})
        );
        assert_eq!(info["limits"]["maxUpgradeAnyTier"], 4);
        assert_eq!(info["limits"]["extraUpgradeTier"], 4);
        assert_eq!(info["limits"]["resultsFileMaxBytes"], 2 * 1_024 * 1_024);
        assert_eq!(
            info["limits"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            [
                "boundedTierMax",
                "boundedTierMin",
                "exactTierMax",
                "exactTierMin",
                "extraUpgradeTier",
                "identityGroupMax",
                "levelSumGroupMax",
                "maxDepth",
                "maxUpgradeAnyTier",
                "maxUpgradeByKind",
                "maxUpgradeDefault",
                "maxUpgradeRing",
                "maxUpgradeWeapon",
                "resultsFileMaxBytes",
            ]
        );
        assert_eq!(
            info.as_object().unwrap().keys().collect::<Vec<_>>(),
            [
                "challenges",
                "emptyBossFloors",
                "limits",
                "maxResults",
                "questWindows",
                "searchStartStride",
                "shpdCommit",
                "shpdVersion",
                "totalSeeds",
            ]
        );
        assert_eq!(info["emptyBossFloors"], serde_json::json!([5, 10, 15]));
        assert_eq!(info["searchStartStride"], 3_355_211_884_971_u64);

        // Every quest window is the feasibility model's own.
        for (name, quest) in ["ghost", "wandmaker", "blacksmith", "imp"]
            .into_iter()
            .zip(QUESTS)
        {
            let (start, end) = Quest::window(quest);
            assert_eq!(info["questWindows"][name], serde_json::json!([start, end]));
        }
        assert_eq!(info["questWindows"]["wandmaker"], serde_json::json!([7, 9]));

        // The challenges are listed in mask order with their generation
        // relevance; only the three the generator consults are marked.
        let challenges = info["challenges"].as_array().unwrap();
        assert_eq!(challenges.len(), 9);
        for (index, challenge) in challenges.iter().enumerate() {
            assert_eq!(challenge["mask"], 1_u16 << index);
        }
        assert_eq!(
            info["challenges"],
            serde_json::json!([
                {"name": "on_diet", "mask": 1, "changesLevelGeneration": false},
                {"name": "faith_is_my_armor", "mask": 2, "changesLevelGeneration": false},
                {"name": "pharmacophobia", "mask": 4, "changesLevelGeneration": false},
                {"name": "barren_land", "mask": 8, "changesLevelGeneration": true},
                {"name": "swarm_intelligence", "mask": 16, "changesLevelGeneration": false},
                {"name": "into_darkness", "mask": 32, "changesLevelGeneration": true},
                {"name": "forbidden_runes", "mask": 64, "changesLevelGeneration": true},
                {"name": "hostile_champions", "mask": 128, "changesLevelGeneration": false},
                {"name": "badder_bosses", "mask": 256, "changesLevelGeneration": false},
            ])
        );
    }
}
