//! Results-export documents: search results plus the query that found them.
//!
//! This module is the canonical implementation of the format described in
//! `docs/results-export-format.md`. Every frontend links this engine, so the
//! codec is exposed over FFI (`seedfinder_results_encode`/`_decode`), WASM
//! (`encode_results_file`/`decode_results_file`) and JNI
//! (`resultsEncode`/`resultsDecode`): frontends delegate to it instead of
//! re-implementing the schema.
//!
//! Compatibility contract:
//!
//! - The envelope is identified by `format` alone; the document carries no
//!   schema version. The guarantee the format makes is one-directional:
//!   whatever an older release exported, a newer one still imports.
//! - Readers ignore unknown envelope and per-result fields, including the
//!   `format_version` number releases up to 0.7.0 wrote, so files exported
//!   before this build keep importing unchanged.
//! - The embedded query reuses the [`crate::json_query`] document format and
//!   is validated strictly: unknown query fields, items, effects, or
//!   challenges fail the import instead of silently changing its meaning.
//!   That is also what a file from a *newer* app hits — it is rejected by
//!   name rather than misread, which is why no version number is needed.

use serde_json::{Map, Value, json};

use crate::json_query;
use crate::query::{MAX_IDENTITY_GROUP, MAX_LEVEL_SUM_GROUP, SearchQuery};
use crate::seed::DungeonSeed;

/// Identifies a Seed Seeker results file.
pub const FILE_FORMAT: &str = "seed-seeker-results";

/// Largest input [`decode`] accepts, in bytes. Part of the cross-platform
/// import contract: every frontend refuses larger files (a maximal legal file
/// is far smaller), and the engine enforces the same bound so no platform can
/// drift from it.
pub const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

/// Most seeds a restored results list holds, and the most a search session
/// on any platform retains. Part of the cross-platform contract: importers
/// dedupe then cap at exactly this many (see [`dedupe_and_cap`]), so a given
/// file restores the same list everywhere, and the engine publishes it as
/// `maxResults` in `engine_info` so no frontend keeps a copy.
pub const MAX_RESULTS: usize = 1_024;

/// One decoded results file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultsFile {
    /// App version that wrote the file, when declared. Informational.
    pub app_version: Option<String>,
    /// Upstream game version the exporting engine targeted. Informational.
    pub shpd_version: Option<String>,
    /// The validated query that produced the exported results.
    pub query: SearchQuery,
    /// The exported result seeds, in their exported order.
    pub seeds: Vec<DungeonSeed>,
}

/// Encodes a validated query and its result seeds as a pretty-printed results
/// document.
#[must_use]
pub fn encode(query: &SearchQuery, seeds: &[DungeonSeed], app_version: &str) -> String {
    let document = json!({
        "format": FILE_FORMAT,
        "app_version": app_version,
        "shpd_version": crate::SHPD_VERSION,
        "query": json_query::encode(query),
        "results": seeds
            .iter()
            .map(|seed| json!({ "seed": seed.to_code() }))
            .collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&document).unwrap_or_default()
}

/// Decodes and validates a results document.
///
/// # Errors
///
/// Returns a human-readable message for input above [`MAX_FILE_BYTES`], for
/// files that are not Seed Seeker results documents, and for files that
/// contain an invalid query or seed code.
pub fn decode(contents: &str) -> Result<ResultsFile, String> {
    if contents.len() > MAX_FILE_BYTES {
        return Err(
            "this file is too large to be a Seed Seeker results file (2 MiB limit)".to_owned(),
        );
    }
    let document: Value = serde_json::from_str(contents)
        .map_err(|error| format!("this is not a Seed Seeker results file: {error}"))?;
    let document = document
        .as_object()
        .ok_or("this is not a Seed Seeker results file: expected a JSON object")?;
    if document.get("format").and_then(Value::as_str) != Some(FILE_FORMAT) {
        return Err(format!(
            "this is not a Seed Seeker results file: missing \"format\": \"{FILE_FORMAT}\""
        ));
    }
    let query_value = document
        .get("query")
        .filter(|value| value.is_object())
        .ok_or("this results file is missing its \"query\" object")?;
    let query = json_query::decode(&query_value.to_string())
        .map_err(|error| format!("the query in this results file is not usable: {error}"))?;
    for (index, requirement) in query.requirements.iter().enumerate() {
        // The results format restricts same-item and combined-level groups
        // to what every app's editor can express (A..D), even though the
        // engine allows more.
        if requirement
            .identity_group
            .is_some_and(|group| group > MAX_IDENTITY_GROUP)
        {
            return Err(format!(
                "requirement {}: same-item group must be between 1 and {MAX_IDENTITY_GROUP} (A..D)",
                index + 1
            ));
        }
        if requirement
            .level_sum
            .is_some_and(|sum| sum.group > MAX_LEVEL_SUM_GROUP)
        {
            return Err(format!(
                "requirement {}: combined level group must be between 1 and \
                 {MAX_LEVEL_SUM_GROUP} (A..D)",
                index + 1
            ));
        }
    }
    let results = document
        .get("results")
        .and_then(Value::as_array)
        .ok_or("this results file is missing its \"results\" list")?;
    let seeds = results
        .iter()
        .enumerate()
        .map(|(index, entry)| decode_result_seed(index, entry))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ResultsFile {
        app_version: field_string(document, "app_version"),
        shpd_version: field_string(document, "shpd_version"),
        query,
        seeds,
    })
}

/// Deduplicates seeds (keeping the first occurrence) and caps the list at
/// `limit`, returning the kept seeds and how many entries were dropped.
/// All importers apply this rule so a given file restores the same list
/// everywhere.
#[must_use]
pub fn dedupe_and_cap(seeds: &[DungeonSeed], limit: usize) -> (Vec<DungeonSeed>, usize) {
    let mut seen = std::collections::HashSet::new();
    let mut kept = Vec::new();
    for seed in seeds {
        if kept.len() == limit {
            break;
        }
        if seen.insert(*seed) {
            kept.push(*seed);
        }
    }
    let dropped = seeds.len() - kept.len();
    (kept, dropped)
}

/// Encodes a results file from the bridge request `{"query": <canonical
/// query document>, "seeds": ["AAA-AAA-AAA", ...], "app_version": "..."}`
/// and returns the results-file text. This is the envelope every thin bridge
/// (C, JNI, wasm) hands its frontend, built here so each platform writes the
/// identical file.
///
/// # Errors
///
/// Returns a human-readable message for a malformed request, an invalid
/// query, or a seed code that is not in the canonical `XXX-XXX-XXX` form.
pub fn encode_document(request_json: &str) -> Result<String, String> {
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
        .map(|(index, entry)| request_seed(index, entry))
        .collect::<Result<Vec<_>, _>>()?;
    let app_version = request
        .get("app_version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(encode(&query, &seeds, app_version))
}

fn request_seed(index: usize, entry: &Value) -> Result<DungeonSeed, String> {
    let code = entry
        .as_str()
        .ok_or_else(|| format!("seed {}: expected a seed code string", index + 1))?;
    if !is_canonical_code(code) {
        return Err(format!(
            "seed {}: seed code must use the canonical XXX-XXX-XXX form",
            index + 1
        ));
    }
    DungeonSeed::from_code(code).map_err(|error| format!("seed {}: {error}", index + 1))
}

/// Decodes results-file text into the bridge document `{"query": <canonical
/// query document>, "seeds": [...], "dropped": <number>, "app_version": ...,
/// "shpd_version": ...}`. The seeds are already deduplicated and capped at
/// [`MAX_RESULTS`], so every platform restores the identical list, and
/// `dropped` counts the exported entries that step removed.
///
/// # Errors
///
/// Returns [`decode`]'s message: input above [`MAX_FILE_BYTES`], a file that
/// is not a results file, or an invalid query or seed code.
pub fn decode_document(contents: &str) -> Result<String, String> {
    let file = decode(contents)?;
    let (seeds, dropped) = dedupe_and_cap(&file.seeds, MAX_RESULTS);
    Ok(json!({
        "query": json_query::encode(&file.query),
        "seeds": seeds.iter().copied().map(DungeonSeed::to_code).collect::<Vec<_>>(),
        "dropped": dropped,
        "app_version": file.app_version,
        "shpd_version": file.shpd_version,
    })
    .to_string())
}

fn decode_result_seed(index: usize, entry: &Value) -> Result<DungeonSeed, String> {
    let code = entry
        .get("seed")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("result {}: missing \"seed\" code", index + 1))?;
    // Stricter than the interactive parser on purpose: files must carry the
    // canonical form so every platform accepts exactly the same documents.
    if !is_canonical_code(code) {
        return Err(format!(
            "result {}: seed code must use the canonical XXX-XXX-XXX form",
            index + 1
        ));
    }
    DungeonSeed::from_code(code).map_err(|error| format!("result {}: {error}", index + 1))
}

/// Reports whether `code` is in the strictly canonical `XXX-XXX-XXX` form the
/// file format requires. Writers going through [`encode`] must check it too,
/// so a file exported on one platform imports on all of them.
#[must_use]
pub fn is_canonical_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 11
        && bytes.iter().enumerate().all(|(index, byte)| {
            if index == 3 || index == 7 {
                *byte == b'-'
            } else {
                byte.is_ascii_uppercase()
            }
        })
}

fn field_string(document: &Map<String, Value>, key: &str) -> Option<String> {
    document.get(key).and_then(Value::as_str).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::catalog::{ItemId, ItemKind};
    use crate::challenges::Challenges;
    use crate::model::ItemSource;
    use crate::query::{
        EffectRequirement, Requirement, SearchQuery, TierRequirement, UpgradeRequirement,
    };
    use crate::quests::WandmakerQuestType;
    use crate::seed::DungeonSeed;

    use super::{
        MAX_FILE_BYTES, MAX_RESULTS, decode, decode_document, dedupe_and_cap, encode,
        encode_document, is_canonical_code,
    };

    fn sample_query() -> SearchQuery {
        SearchQuery {
            requirements: vec![
                Requirement {
                    kind: ItemKind::Ring,
                    weapon_category: None,
                    item: Some(ItemId::RingWealth),
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::Exact(4),
                    effect: EffectRequirement::Any,
                    require_uncursed: false,
                    select_trinket: false,
                    source: Some(ItemSource::ImpReward),
                    identity_group: None,
                    max_depth: None,
                    alternative_group: None,
                    level_sum: None,
                },
                Requirement {
                    kind: ItemKind::Wand,
                    weapon_category: None,
                    item: None,
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::AtLeast(2),
                    effect: EffectRequirement::Any,
                    require_uncursed: true,
                    select_trinket: false,
                    source: None,
                    identity_group: Some(1),
                    max_depth: Some(9),
                    alternative_group: None,
                    level_sum: None,
                },
            ],
            max_depth: 21,
            challenges: Challenges::NO_HERBALISM | Challenges::DARKNESS,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
        }
    }

    fn seeds(codes: &[&str]) -> Vec<DungeonSeed> {
        codes
            .iter()
            .map(|code| DungeonSeed::from_code(code).unwrap())
            .collect()
    }

    #[test]
    fn encode_then_decode_round_trips_query_seeds_and_app_versions() {
        let query = sample_query();
        let exported = seeds(&["AAA-AAA-AAB", "ZZZ-ZZZ-ZZZ", "SEE-DSE-EKR"]);
        let contents = encode(&query, &exported, "0.6.1");
        let decoded = decode(&contents).unwrap();
        assert_eq!(decoded.app_version.as_deref(), Some("0.6.1"));
        assert_eq!(decoded.shpd_version.as_deref(), Some(crate::SHPD_VERSION));
        assert_eq!(decoded.query, query);
        assert_eq!(decoded.seeds, exported);
    }

    #[test]
    fn written_documents_carry_no_schema_version() {
        // The format is identified by "format" alone: a version number would
        // only serve readers older than the file, which is not a guarantee
        // this format makes.
        let query = SearchQuery {
            wandmaker_quest: Some(WandmakerQuestType::CorpseDust),
            ..sample_query()
        };
        let contents = encode(&query, &seeds(&["AAA-AAA-AAB"]), "0.6.1");
        assert!(!contents.contains("format_version"), "{contents}");
        assert_eq!(decode(&contents).unwrap().query, query);
    }

    /// The quest filter's own frozen fixture, decoded by every platform's
    /// codec alongside the older documents below.
    const WANDMAKER_QUEST_FIXTURE: &str =
        include_str!("../tests/fixtures/results-export-wandmaker-quest.json");

    #[test]
    fn wandmaker_quest_fixture_carries_the_quest() {
        let decoded = decode(WANDMAKER_QUEST_FIXTURE).unwrap();
        assert_eq!(decoded.query.max_depth, 9);
        assert_eq!(
            decoded.query.wandmaker_quest,
            Some(WandmakerQuestType::Rotberry)
        );
        assert_eq!(decoded.seeds, seeds(&["AAA-AAA-BUH", "ABC-DEF-GHI"]));
        assert_eq!(
            decode(&encode(&decoded.query, &decoded.seeds, "test"))
                .unwrap()
                .query,
            decoded.query
        );
    }

    /// A document written by hand and frozen, `"format_version": 1` and all:
    /// files exported by an older release must always stay readable. Do not
    /// edit the fixture; add new ones for new fields instead.
    const VERSION_1_FIXTURE: &str = include_str!("../tests/fixtures/results-export-v1.json");

    #[test]
    fn version_one_fixture_always_decodes() {
        let decoded = decode(VERSION_1_FIXTURE).unwrap();
        assert_eq!(decoded.app_version.as_deref(), Some("0.6.1"));
        assert_eq!(decoded.shpd_version.as_deref(), Some("3.3.8"));
        assert_eq!(decoded.query.max_depth, 12);
        assert_eq!(decoded.query.challenges, Challenges::NO_HERBALISM);
        assert!(decoded.query.require_blacksmith);
        assert_eq!(decoded.query.requirements.len(), 2);
        assert_eq!(
            decoded.query.requirements[0].item,
            Some(ItemId::RingTenacity)
        );
        assert_eq!(decoded.query.requirements[1].kind, ItemKind::Wand);
        assert_eq!(decoded.seeds, seeds(&["AAA-AAA-BUH", "ABC-DEF-GHI"]));
    }

    /// Narrowed weapon kinds (`melee_weapon`/`thrown_weapon`), pinned by
    /// another frozen document; every importer must accept them.
    const WEAPON_CATEGORIES_FIXTURE: &str =
        include_str!("../tests/fixtures/results-export-v1-weapon-categories.json");

    #[test]
    fn weapon_category_fixture_decodes_and_round_trips() {
        use crate::catalog::WeaponCategory;

        let decoded = decode(WEAPON_CATEGORIES_FIXTURE).unwrap();
        assert_eq!(decoded.query.requirements.len(), 3);
        assert_eq!(
            decoded.query.requirements[0].weapon_category,
            Some(WeaponCategory::Thrown)
        );
        assert_eq!(
            decoded.query.requirements[1].weapon_category,
            Some(WeaponCategory::Melee)
        );
        assert_eq!(decoded.query.requirements[1].item, Some(ItemId::Sword));
        assert_eq!(decoded.query.requirements[2].weapon_category, None);
        assert_eq!(decoded.seeds, seeds(&["AAA-AAA-ACO"]));

        // Re-encoding must keep the narrowing: widening "thrown_weapon" back
        // to "weapon" would silently change the query's meaning on import.
        let encoded = encode(&decoded.query, &decoded.seeds, "test");
        let round_tripped = decode(&encoded).unwrap();
        assert_eq!(round_tripped.query, decoded.query);
        assert_eq!(round_tripped.seeds, decoded.seeds);
    }

    #[test]
    fn unknown_envelope_and_result_fields_are_ignored() {
        let contents = r#"{
            "format": "seed-seeker-results",
            "format_version": 1,
            "exported_at": "2031-01-01T00:00:00Z",
            "future_minor_field": {"nested": true},
            "query": {"requirements": [{"item": "sword"}]},
            "results": [
                {"seed": "AAA-AAA-AAB", "future_note": "still fine"}
            ]
        }"#;
        let decoded = decode(contents).unwrap();
        assert_eq!(decoded.seeds, seeds(&["AAA-AAA-AAB"]));
        assert!(decoded.app_version.is_none());
    }

    #[test]
    fn any_declared_format_version_is_ignored() {
        // `format_version` is just another unknown envelope field now: an old
        // file's 1 and a hypothetical future 99 are read the same way, and a
        // file with none at all is the normal case.
        for version in ["1", "2", "99", "0", "1.5", "true", "\"1\"", "-1"] {
            let contents = format!(
                r#"{{"format":"seed-seeker-results","format_version":{version},
                    "query":{{"requirements":[{{"item":"sword"}}]}},
                    "results":[{{"seed":"AAA-AAA-AAB"}}]}}"#
            );
            let decoded = decode(&contents).unwrap_or_else(|error| panic!("{version}: {error}"));
            assert_eq!(decoded.seeds, seeds(&["AAA-AAA-AAB"]));
        }
    }

    #[test]
    fn foreign_and_malformed_files_are_rejected_clearly() {
        for contents in ["not json at all", "[]", "{}", r#"{"format":"other"}"#] {
            let error = decode(contents).unwrap_err();
            assert!(
                error.contains("not a Seed Seeker results file"),
                "{contents}: {error}"
            );
        }
    }

    #[test]
    fn unknown_query_content_fails_instead_of_changing_meaning() {
        let unknown_item = r#"{
            "format": "seed-seeker-results",
            "query": {"requirements": [{"item": "item_from_the_future"}]},
            "results": []
        }"#;
        let error = decode(unknown_item).unwrap_err();
        assert!(error.contains("query"), "{error}");
        assert!(error.contains("item_from_the_future"), "{error}");

        let unknown_field = r#"{
            "format": "seed-seeker-results",
            "query": {"requirements": [{"item": "sword"}], "wished_luck": 7},
            "results": []
        }"#;
        assert!(decode(unknown_field).is_err());
    }

    #[test]
    fn invalid_seed_codes_name_the_offending_result() {
        let contents = r#"{
            "format": "seed-seeker-results",
            "query": {"requirements": [{"item": "sword"}]},
            "results": [{"seed": "AAA-AAA-AAB"}, {"seed": "AAA-AAA-AA0"}]
        }"#;
        let error = decode(contents).unwrap_err();
        assert!(error.starts_with("result 2:"), "{error}");
    }

    #[test]
    fn only_canonical_seed_codes_are_accepted() {
        // The interactive parser tolerates these; the file format must not,
        // or files would import on some platforms and fail on others.
        for code in ["aaa-aaa-aab", "AAAAAAAAB", "AAA AAA AAB", " AAA-AAA-AAB"] {
            let contents = format!(
                r#"{{"format":"seed-seeker-results",
                    "query":{{"requirements":[{{"item":"sword"}}]}},
                    "results":[{{"seed":"{code}"}}]}}"#
            );
            let error = decode(&contents).unwrap_err();
            assert!(error.contains("canonical"), "{code}: {error}");
        }
    }

    #[test]
    fn wrong_typed_query_fields_are_rejected() {
        for query in [
            r#"{"requirements":[{"item":"sword"}],"max_depth":"12"}"#,
            r#"{"requirements":[{"item":42}]}"#,
            r#"{"requirements":[{"item":"sword"}],"challenges":"barren_land"}"#,
            r#"{"requirements":[{"item":"sword","upgrade":true}]}"#,
        ] {
            let contents =
                format!(r#"{{"format":"seed-seeker-results","query":{query},"results":[]}}"#);
            assert!(decode(&contents).is_err(), "{query}");
        }
    }

    #[test]
    fn same_item_groups_above_four_are_rejected() {
        let contents = r#"{
            "format": "seed-seeker-results",
            "query": {"requirements": [{"kind": "wand", "identity_group": 5}]},
            "results": []
        }"#;
        let error = decode(contents).unwrap_err();
        assert!(error.contains("1 and 4"), "{error}");
    }

    #[test]
    fn oversized_files_are_refused_before_parsing() {
        // The engine enforces the shared import cap so no frontend has to.
        let padding = " ".repeat(MAX_FILE_BYTES);
        let contents = format!(
            r#"{{"format":"seed-seeker-results",{padding}
                "query":{{"requirements":[{{"item":"sword"}}]}},
                "results":[{{"seed":"AAA-AAA-AAB"}}]}}"#
        );
        assert!(contents.len() > MAX_FILE_BYTES);
        let error = decode(&contents).unwrap_err();
        assert!(error.contains("too large"), "{error}");
        assert!(error.contains("2 MiB"), "{error}");

        // A document of exactly the cap is still parsed (and here rejected on
        // its contents, not its size).
        let padded = "not json".to_owned() + &" ".repeat(MAX_FILE_BYTES - "not json".len());
        assert_eq!(padded.len(), MAX_FILE_BYTES);
        assert!(decode(&padded).unwrap_err().contains("not a Seed Seeker"));
    }

    #[test]
    fn canonical_seed_codes_are_recognized_for_writers() {
        assert!(is_canonical_code("AAA-AAA-AAB"));
        for code in ["aaa-aaa-aab", "AAAAAAAAB", "AAA AAA AAB", " AAA-AAA-AAB"] {
            assert!(!is_canonical_code(code), "{code}");
        }
    }

    #[test]
    fn importers_dedupe_then_cap_preserving_first_occurrences() {
        let raw = seeds(&["AAA-AAA-AAC", "AAA-AAA-AAB", "AAA-AAA-AAC"]);
        let (kept, dropped) = dedupe_and_cap(&raw, 1_024);
        assert_eq!(kept, seeds(&["AAA-AAA-AAC", "AAA-AAA-AAB"]));
        assert_eq!(dropped, 1);

        let many: Vec<_> = (0..1_500)
            .map(|value| DungeonSeed::new(value).unwrap())
            .collect();
        let (kept, dropped) = dedupe_and_cap(&many, 1_024);
        assert_eq!(kept.len(), 1_024);
        assert_eq!(dropped, 476);
    }

    /// The frozen cross-platform fixtures: every bridge decodes exactly the
    /// documents every other platform decodes.
    const BRIDGE_FIXTURES: [&str; 3] = [
        include_str!("../tests/fixtures/results-export-v1.json"),
        include_str!("../tests/fixtures/results-export-v1-weapon-categories.json"),
        include_str!("../tests/fixtures/results-export-wandmaker-quest.json"),
    ];

    #[test]
    fn bridge_documents_round_trip_through_the_frozen_fixtures() {
        for fixture in BRIDGE_FIXTURES {
            let decoded: Value = serde_json::from_str(&decode_document(fixture).unwrap()).unwrap();
            assert_eq!(decoded["shpd_version"], "3.3.8");
            assert!(!decoded["seeds"].as_array().unwrap().is_empty());
            assert_eq!(decoded["dropped"], 0);

            let request = json!({
                "query": decoded["query"],
                "seeds": decoded["seeds"],
                "app_version": "test",
            });
            let encoded = encode_document(&request.to_string()).unwrap();
            let round_tripped: Value =
                serde_json::from_str(&decode_document(&encoded).unwrap()).unwrap();
            assert_eq!(round_tripped["query"], decoded["query"]);
            assert_eq!(round_tripped["seeds"], decoded["seeds"]);
            assert_eq!(round_tripped["dropped"], 0);
            assert_eq!(round_tripped["app_version"], "test");
        }
    }

    #[test]
    fn bridge_decoding_dedupes_caps_and_refuses_oversized_files() {
        let file = json!({
            "format": "seed-seeker-results",
            "query": {"requirements": [{"item": "sword"}]},
            "results": (0..MAX_RESULTS + 10)
                .map(|index| json!({
                    "seed": DungeonSeed::new(u64::try_from(index % MAX_RESULTS).unwrap())
                        .unwrap()
                        .to_code()
                }))
                .collect::<Vec<_>>(),
        });
        let decoded: Value =
            serde_json::from_str(&decode_document(&file.to_string()).unwrap()).unwrap();
        assert_eq!(decoded["seeds"].as_array().unwrap().len(), MAX_RESULTS);
        // Ten duplicates: importers report exactly what dedupe-and-cap removed.
        assert_eq!(decoded["dropped"], 10);
        assert!(decoded["app_version"].is_null());

        let oversized = " ".repeat(MAX_FILE_BYTES + 1);
        let error = decode_document(&oversized).unwrap_err();
        assert!(error.contains("too large"), "{error}");
    }

    #[test]
    fn bridge_encoding_fails_on_invalid_queries_and_seed_codes() {
        let invalid_query = json!({"query": {"requirements": []}, "seeds": []});
        assert!(encode_document(&invalid_query.to_string()).is_err());

        let invalid_seed = json!({
            "query": {"requirements": [{"item": "sword"}]},
            "seeds": ["aaa-aaa-aab"],
        });
        let error = encode_document(&invalid_seed.to_string()).unwrap_err();
        assert!(error.contains("canonical"), "{error}");

        assert!(encode_document("not json").is_err());
        assert!(encode_document(r#"{"seeds":[]}"#).is_err());
    }
}
