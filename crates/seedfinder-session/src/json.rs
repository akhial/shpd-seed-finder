//! The UTF-8 JSON envelopes the thin bridges (C, JNI, wasm) hand their
//! frontends, built in one place so every platform reads identical documents.
//!
//! Each bridge function reduces to marshalling its platform's bytes and one
//! call here; the shapes below are the contract the frontends parse.

use serde_json::{Value, json};
use shpd_seedfinder_core::seed::{DungeonSeed, SeedError};
use shpd_seedfinder_core::wire::WireError;
use shpd_seedfinder_core::{json_query, results_export};

use crate::{ScoutMatchError, StartDecision, decide_start_packets, production_scout_matches};

/// Encodes a results file from the request `{"query": <canonical query
/// document>, "seeds": ["AAA-AAA-AAA", ...], "app_version": "..."}` and
/// returns the results-file text. The codec is
/// `crates/seedfinder-core/src/results_export.rs`, specified in
/// `docs/results-export-format.md`.
///
/// # Errors
///
/// Returns the codec's own message for a malformed request, an invalid
/// query, or a seed code that is not in the canonical `XXX-XXX-XXX` form.
pub fn results_encode_document(request_json: &str) -> Result<String, String> {
    let request: Value = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid results request JSON: {error}"))?;
    let query_value = request
        .get("query")
        .filter(|value| value.is_object())
        .ok_or("the results request is missing its \"query\" object")?;
    let query = json_query::decode(&query_value.to_string())?;
    let seeds = request
        .get("seeds")
        .and_then(Value::as_array)
        .ok_or("the results request is missing its \"seeds\" list")?
        .iter()
        .enumerate()
        .map(|(index, entry)| results_request_seed(index, entry))
        .collect::<Result<Vec<_>, _>>()?;
    let app_version = request
        .get("app_version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(results_export::encode(&query, &seeds, app_version))
}

fn results_request_seed(index: usize, entry: &Value) -> Result<DungeonSeed, String> {
    let code = entry
        .as_str()
        .ok_or_else(|| format!("seed {}: expected a seed code string", index + 1))?;
    if !results_export::is_canonical_code(code) {
        return Err(format!(
            "seed {}: seed code must use the canonical XXX-XXX-XXX form",
            index + 1
        ));
    }
    DungeonSeed::from_code(code).map_err(|error| format!("seed {}: {error}", index + 1))
}

/// Decodes results-file text into `{"query": <canonical query document>,
/// "seeds": [...], "dropped": <number>, "app_version": ..., "shpd_version":
/// ...}`. The seeds are already deduplicated and capped at `max_results` —
/// the caller's own result cap, so every platform restores the identical
/// list — and `dropped` counts the exported entries that step removed.
///
/// # Errors
///
/// Returns the codec's own message for input above the 2 MiB import cap,
/// for files that are not results files, and for an invalid query or seed
/// code.
pub fn results_decode_document(contents: &str, max_results: usize) -> Result<String, String> {
    let file = results_export::decode(contents)?;
    let (seeds, dropped) = results_export::dedupe_and_cap(&file.seeds, max_results);
    Ok(json!({
        "query": json_query::encode(&file.query),
        "seeds": seeds.iter().copied().map(DungeonSeed::to_code).collect::<Vec<_>>(),
        "dropped": dropped,
        "app_version": file.app_version,
        "shpd_version": file.shpd_version,
    })
    .to_string())
}

/// Marks which items of the world named by an `SSQ2` (or legacy raw seed)
/// scout request satisfy the `SSF8` query, as `{"matched": [<item indices>],
/// "matched_requirements": <n>, "total_requirements": <n>}`. The indices
/// address the item list of the `SSC2` packet the same request scouts to.
///
/// # Errors
///
/// Returns [`production_scout_matches`]'s error.
pub fn scout_matches_document(request: &[u8], query: &[u8]) -> Result<String, ScoutMatchError> {
    let marks = production_scout_matches(request, query)?;
    Ok(json!({
        "matched": marks.matched_indices(),
        "matched_requirements": marks.matched_requirements,
        "total_requirements": marks.total_requirements,
    })
    .to_string())
}

/// Parses seed-code text with the game's own rules into `{"code":
/// "XXX-XXX-XXX", "value": <number>}`: the canonical code for display and
/// the numeric value the seed filters take.
///
/// # Errors
///
/// Returns the seed parser's error when the input is not a seed code.
pub fn seed_parse_document(input: &str) -> Result<String, SeedError> {
    let seed = DungeonSeed::from_code(input)?;
    Ok(json!({ "code": seed.to_code(), "value": seed.value() }).to_string())
}

/// The documented name of [`decide_start_packets`]'s decision: one of
/// `anchor`, `target-refine`, `target-filter`, `continue-detached` or
/// `detached`.
///
/// # Errors
///
/// Returns the decode error of the first undecodable packet.
pub fn decide_start_name(
    candidate: &[u8],
    target: Option<&[u8]>,
    target_set_empty: bool,
    target_has_uncovered_seeds: bool,
    detached_base: Option<&[u8]>,
) -> Result<&'static str, WireError> {
    decide_start_packets(
        candidate,
        target,
        target_set_empty,
        target_has_uncovered_seeds,
        detached_base,
    )
    .map(StartDecision::as_str)
}

#[cfg(test)]
mod tests {
    use shpd_seedfinder_core::catalog::item;
    use shpd_seedfinder_core::challenges::Challenges;
    use shpd_seedfinder_core::engine_info;
    use shpd_seedfinder_core::wire::{decode_scout_world, encode_query};

    use super::*;
    use crate::{MAX_ACCEPTED_RESULTS, production_scout_packet, production_scout_world};

    #[test]
    fn seed_documents_carry_the_canonical_code_and_value() {
        use shpd_seedfinder_core::seed::format_input;

        // The masker filters ASCII letters before uppercasing, so a
        // locale-dependent uppercase-then-filter port cannot pass this.
        assert_eq!(format_input("abcD"), "ABC-D");
        assert_eq!(format_input(" 1a!b@c#d$e%f^g&h*i extra"), "ABC-DEF-GHI");
        assert_eq!(format_input("\u{131}ab"), "AB");

        let parsed: Value =
            serde_json::from_str(&seed_parse_document("AAA-AAA-AAB").unwrap()).unwrap();
        assert_eq!(parsed["code"], "AAA-AAA-AAB");
        assert_eq!(parsed["value"], 1);

        // Non-canonical but parseable input round-trips to the canonical code.
        let lowercase: Value =
            serde_json::from_str(&seed_parse_document("aaa-aaa-aab").unwrap()).unwrap();
        assert_eq!(lowercase, parsed);
        let masked: Value =
            serde_json::from_str(&seed_parse_document(&format_input("aaaaaaaab")).unwrap())
                .unwrap();
        assert_eq!(masked, parsed);

        // Undashed lowercase is not a code by the game's own rules.
        assert!(seed_parse_document("aaaaaaaab").is_err());
        assert!(seed_parse_document("AAA-AAA-AA0").is_err());
    }

    #[test]
    fn start_decision_names_are_the_documented_ones() {
        use shpd_seedfinder_core::catalog::ItemKind;
        use shpd_seedfinder_core::query::{
            Requirement, SearchQuery, TierRequirement, UpgradeRequirement,
        };

        let requirement = |kind| Requirement {
            kind,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
        };
        let query = |kind| SearchQuery {
            requirements: vec![requirement(kind)],
            max_depth: 24,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        let target = encode_query(&query(ItemKind::Ring)).unwrap();
        let deeper = encode_query(&SearchQuery {
            max_depth: 9,
            ..query(ItemKind::Ring)
        })
        .unwrap();
        let armor = encode_query(&query(ItemKind::Armor)).unwrap();
        let mut narrowed_query = query(ItemKind::Armor);
        narrowed_query.requirements.push(Requirement {
            upgrade: UpgradeRequirement::AtLeast(2),
            ..requirement(ItemKind::Armor)
        });
        let narrowed = encode_query(&narrowed_query).unwrap();

        assert_eq!(
            decide_start_name(&target, Some(&target), false, true, None).unwrap(),
            "target-refine"
        );
        assert_eq!(
            decide_start_name(&deeper, Some(&target), false, true, None).unwrap(),
            "target-filter"
        );
        assert_eq!(
            decide_start_name(&armor, Some(&target), false, true, None).unwrap(),
            "detached"
        );
        assert_eq!(
            decide_start_name(&narrowed, Some(&target), false, true, Some(&armor)).unwrap(),
            "continue-detached"
        );
        // A missing Target anchors, and so does an empty Target Set the query
        // does not continue.
        assert_eq!(
            decide_start_name(&target, None, false, true, None).unwrap(),
            "anchor"
        );
        assert_eq!(
            decide_start_name(&deeper, Some(&target), true, true, None).unwrap(),
            "anchor"
        );

        assert!(decide_start_name(b"bad", Some(&target), false, true, None).is_err());
        assert!(decide_start_name(&target, Some(b"bad"), false, true, None).is_err());
        assert!(decide_start_name(&target, None, false, true, Some(b"bad")).is_err());
    }

    #[test]
    fn scout_match_envelope_indexes_the_scout_packet() {
        // Scouting is deterministic, so the marks index exactly the item list
        // the SSC2 packet of the same request carries.
        let seed = DungeonSeed::MIN;
        let world = production_scout_world(seed, Challenges::NONE).unwrap();
        let known = &world.items[0];
        let document = json!({
            "requirements": [{
                "item": item(known.item).stable_id,
                "max_depth": known.depth,
            }],
        });
        let query = encode_query(&json_query::decode(&document.to_string()).unwrap()).unwrap();

        let envelope: Value =
            serde_json::from_str(&scout_matches_document(b"AAA-AAA-AAA", &query).unwrap()).unwrap();
        assert_eq!(envelope["total_requirements"], 1);
        assert_eq!(envelope["matched_requirements"], 1);
        let matched = envelope["matched"].as_array().unwrap();
        assert_eq!(matched.len(), 1);
        let index = usize::try_from(matched[0].as_u64().unwrap()).unwrap();
        let packet = production_scout_packet(b"AAA-AAA-AAA").unwrap();
        let scouted = decode_scout_world(&packet).unwrap();
        assert!(index < scouted.items.len());
        assert_eq!(scouted.items[index].item, known.item);

        // An unsatisfiable requirement still reports the requirement count.
        let impossible = encode_query(
            &json_query::decode(
                r#"{"requirements":[{"item":"sword","max_depth":1}],"max_depth":1}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let envelope: Value =
            serde_json::from_str(&scout_matches_document(b"AAA-AAA-AAA", &impossible).unwrap())
                .unwrap();
        assert_eq!(envelope["total_requirements"], 1);
        assert!(envelope["matched"].as_array().unwrap().len() <= 1);

        assert!(scout_matches_document(b"AAA-AAA-AA0", &query).is_err());
        assert!(scout_matches_document(b"AAA-AAA-AAA", b"bad").is_err());
    }

    /// The frozen cross-platform fixtures: every bridge decodes exactly the
    /// documents every other platform decodes.
    const RESULTS_FIXTURES: [&str; 3] = [
        include_str!("../../seedfinder-core/tests/fixtures/results-export-v1.json"),
        include_str!(
            "../../seedfinder-core/tests/fixtures/results-export-v1-weapon-categories.json"
        ),
        include_str!("../../seedfinder-core/tests/fixtures/results-export-wandmaker-quest.json"),
    ];

    #[test]
    fn results_files_round_trip_through_the_frozen_fixtures() {
        for fixture in RESULTS_FIXTURES {
            let decoded: Value = serde_json::from_str(
                &results_decode_document(fixture, MAX_ACCEPTED_RESULTS).unwrap(),
            )
            .unwrap();
            assert_eq!(decoded["shpd_version"], "3.3.8");
            assert!(!decoded["seeds"].as_array().unwrap().is_empty());
            assert_eq!(decoded["dropped"], 0);

            let request = json!({
                "query": decoded["query"],
                "seeds": decoded["seeds"],
                "app_version": "test",
            });
            let encoded = results_encode_document(&request.to_string()).unwrap();
            let round_tripped: Value = serde_json::from_str(
                &results_decode_document(&encoded, MAX_ACCEPTED_RESULTS).unwrap(),
            )
            .unwrap();
            assert_eq!(round_tripped["query"], decoded["query"]);
            assert_eq!(round_tripped["seeds"], decoded["seeds"]);
            assert_eq!(round_tripped["dropped"], 0);
            assert_eq!(round_tripped["app_version"], "test");
        }
    }

    #[test]
    fn results_decoding_dedupes_caps_and_refuses_oversized_files() {
        let file = json!({
            "format": "seed-seeker-results",
            "query": {"requirements": [{"item": "sword"}]},
            "results": (0..MAX_ACCEPTED_RESULTS + 10)
                .map(|index| json!({
                    "seed": DungeonSeed::new(
                        u64::try_from(index % MAX_ACCEPTED_RESULTS).unwrap(),
                    )
                    .unwrap()
                    .to_code()
                }))
                .collect::<Vec<_>>(),
        });
        let decoded: Value = serde_json::from_str(
            &results_decode_document(&file.to_string(), MAX_ACCEPTED_RESULTS).unwrap(),
        )
        .unwrap();
        assert_eq!(
            decoded["seeds"].as_array().unwrap().len(),
            MAX_ACCEPTED_RESULTS
        );
        // Ten duplicates: importers report exactly what dedupe-and-cap removed.
        assert_eq!(decoded["dropped"], 10);
        assert!(decoded["app_version"].is_null());

        // The cap is the caller's own: a smaller one drops more.
        let smaller: Value =
            serde_json::from_str(&results_decode_document(&file.to_string(), 16).unwrap()).unwrap();
        assert_eq!(smaller["seeds"].as_array().unwrap().len(), 16);
        assert_eq!(smaller["dropped"], MAX_ACCEPTED_RESULTS + 10 - 16);

        let oversized = " ".repeat(results_export::MAX_FILE_BYTES + 1);
        let error = results_decode_document(&oversized, MAX_ACCEPTED_RESULTS).unwrap_err();
        assert!(error.contains("too large"), "{error}");
    }

    #[test]
    fn results_encoding_fails_on_invalid_queries_and_seed_codes() {
        let invalid_query = json!({"query": {"requirements": []}, "seeds": []});
        assert!(results_encode_document(&invalid_query.to_string()).is_err());

        let invalid_seed = json!({
            "query": {"requirements": [{"item": "sword"}]},
            "seeds": ["aaa-aaa-aab"],
        });
        let error = results_encode_document(&invalid_seed.to_string()).unwrap_err();
        assert!(error.contains("canonical"), "{error}");

        assert!(results_encode_document("not json").is_err());
        assert!(results_encode_document(r#"{"seeds":[]}"#).is_err());
    }

    #[test]
    fn engine_info_publishes_the_shared_constants() {
        // The bridges only serialize this document with their own result cap,
        // so pinning it here pins exactly what every native frontend reads.
        let info = engine_info::document(MAX_ACCEPTED_RESULTS);

        assert_eq!(info["shpdVersion"], shpd_seedfinder_core::SHPD_VERSION);
        assert_eq!(info["shpdCommit"], shpd_seedfinder_core::SHPD_COMMIT);
        assert_eq!(info["totalSeeds"], shpd_seedfinder_core::seed::TOTAL_SEEDS);
        assert_eq!(info["maxResults"], MAX_ACCEPTED_RESULTS);
        assert_eq!(
            info["limits"],
            json!({
                "maxDepth": 24,
                "exactTierMin": 2,
                "exactTierMax": 5,
                "boundedTierMin": 3,
                "boundedTierMax": 4,
                "identityGroupMax": 4,
                "maxUpgradeDefault": 3,
                "maxUpgradeRing": 4,
                "resultsFileMaxBytes": results_export::MAX_FILE_BYTES,
            })
        );
        assert_eq!(info["emptyBossFloors"], json!([5, 10, 15]));
        assert_eq!(
            info["questWindows"],
            json!({
                "ghost": [2, 4],
                "wandmaker": [7, 9],
                "blacksmith": [12, 14],
                "imp": [17, 19],
            })
        );
        assert_eq!(
            info["challenges"],
            json!([
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
        assert_eq!(info["searchStartStride"], 3_355_211_884_971_u64);
        assert_eq!(info.as_object().unwrap().len(), 9);
    }
}
