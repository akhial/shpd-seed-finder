//! Thin JSON and cooperative-search adapter for browser WebAssembly.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use shpd_seedfinder_core::catalog::{Effect, ItemKind, item};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::deep_link;
use shpd_seedfinder_core::feasibility::QueryPlan;
use shpd_seedfinder_core::json_query;
use shpd_seedfinder_core::main_world::{
    CanonicalMainWorldGenerator, ConfiguredMainWorldGenerator, generate_main_world_with_challenges,
};
use shpd_seedfinder_core::model::{Accessibility, ItemSource, WorldItem};
use shpd_seedfinder_core::probability::estimate_match_probability;
use shpd_seedfinder_core::query::{SearchQuery, scout_matches};
use shpd_seedfinder_core::quests::{
    BlacksmithQuestType, GhostQuestType, ImpTarget, QuestSummary, WandmakerQuestType,
};
use shpd_seedfinder_core::results_export;
use shpd_seedfinder_core::search::WorldGenerator;
use shpd_seedfinder_core::seed::{DungeonSeed, TOTAL_SEEDS};
use shpd_seedfinder_core::{SHPD_COMMIT, SHPD_VERSION};
use wasm_bindgen::prelude::*;

/// Maximum number of matches retained by one browser search session.
pub const MAX_RESULTS: usize = 1_024;
const SEARCH_BATCH_SIZE: u64 = 256;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineInfo {
    shpd_version: &'static str,
    shpd_commit: &'static str,
    total_seeds: u64,
    max_results: usize,
}

#[derive(Serialize)]
struct SeedOutput {
    code: String,
    value: u64,
}

impl From<DungeonSeed> for SeedOutput {
    fn from(seed: DungeonSeed) -> Self {
        Self {
            code: seed.to_code(),
            value: seed.value(),
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum AnalysisOutput {
    Invalid {
        valid: bool,
        error: String,
    },
    Valid {
        valid: bool,
        probability: Option<f64>,
        impossible: bool,
        notes: Vec<String>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScoutRequest {
    seed: String,
    #[serde(default)]
    challenges: Vec<FileChallenge>,
    #[serde(default)]
    query: Option<Value>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileChallenge {
    OnDiet,
    FaithIsMyArmor,
    Pharmacophobia,
    BarrenLand,
    SwarmIntelligence,
    IntoDarkness,
    ForbiddenRunes,
    HostileChampions,
    BadderBosses,
}

impl From<FileChallenge> for Challenges {
    fn from(value: FileChallenge) -> Self {
        match value {
            FileChallenge::OnDiet => Self::NO_FOOD,
            FileChallenge::FaithIsMyArmor => Self::NO_ARMOR,
            FileChallenge::Pharmacophobia => Self::NO_HEALING,
            FileChallenge::BarrenLand => Self::NO_HERBALISM,
            FileChallenge::SwarmIntelligence => Self::SWARM_INTELLIGENCE,
            FileChallenge::IntoDarkness => Self::DARKNESS,
            FileChallenge::ForbiddenRunes => Self::NO_SCROLLS,
            FileChallenge::HostileChampions => Self::CHAMPION_ENEMIES,
            FileChallenge::BadderBosses => Self::STRONGER_BOSSES,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScoutOutput {
    seed: SeedOutput,
    quests: Vec<ScoutQuestOutput>,
    items: Vec<ScoutItemOutput>,
    matched_requirements: usize,
    total_requirements: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScoutQuestOutput {
    quest: &'static str,
    variant: &'static str,
    depth: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScoutItemOutput {
    id: &'static str,
    name: &'static str,
    category: &'static str,
    sprite_index: u16,
    upgrade: u8,
    effect: Option<EffectOutput>,
    cursed: bool,
    secret: bool,
    depth: u8,
    source: &'static str,
    accessibility: AccessibilityOutput,
    matched: bool,
}

#[derive(Serialize)]
struct EffectOutput {
    name: &'static str,
    kind: &'static str,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AccessibilityOutput {
    Independent,
    Choice { group: u16, option: u8 },
    Scenarios { group: u16, mask: String },
}

#[derive(Serialize)]
struct AdvanceOutput {
    state: &'static str,
    tested: u64,
    matches: Vec<SeedOutput>,
}

/// Returns the pinned engine version and browser-session limits as JSON.
#[wasm_bindgen]
#[must_use]
pub fn engine_info() -> String {
    to_json(&EngineInfo {
        shpd_version: SHPD_VERSION,
        shpd_commit: SHPD_COMMIT,
        total_seeds: TOTAL_SEEDS,
        max_results: MAX_RESULTS,
    })
}

/// Formats partial interactive seed input as uppercase groups of three.
#[wasm_bindgen]
#[must_use]
pub fn format_seed_code(input: &str) -> String {
    let mut output = String::with_capacity(11);
    for (index, byte) in input
        .bytes()
        .filter(u8::is_ascii_alphabetic)
        .take(9)
        .enumerate()
    {
        if index == 3 || index == 6 {
            output.push('-');
        }
        output.push(char::from(byte.to_ascii_uppercase()));
    }
    output
}

/// Parses a seed using the core game's seed-code semantics and returns JSON.
///
/// # Errors
///
/// Returns a JavaScript error when the input is not a valid seed code.
#[wasm_bindgen]
pub fn parse_seed_code(input: &str) -> Result<String, JsError> {
    parse_seed_code_impl(input).map_err(|error| JsError::new(&error))
}

/// Encodes a canonical query document as a full shareable web link.
///
/// # Errors
///
/// Returns a JavaScript error for malformed documents and invalid queries.
#[wasm_bindgen]
pub fn encode_share_link(query_json: &str) -> Result<String, JsError> {
    encode_share_link_impl(query_json).map_err(|error| JsError::new(&error))
}

/// Decodes share-link text — a full link, custom-scheme link, or bare code —
/// into the canonical query JSON document.
///
/// # Errors
///
/// Returns a JavaScript error when the text carries no valid share code.
#[wasm_bindgen]
pub fn decode_share_text(text: &str) -> Result<String, JsError> {
    decode_share_text_impl(text).map_err(|error| JsError::new(&error))
}

/// Encodes a results file from `{"query": <canonical query document>,
/// "seeds": ["AAA-AAA-AAA", ...], "app_version": "..."}`, returning the file's
/// JSON text. The codec is `crates/seedfinder-core/src/results_export.rs`,
/// specified in `docs/results-export-format.md`.
///
/// # Errors
///
/// Returns a JavaScript error for a malformed request, an invalid query, or a
/// seed code that is not in the canonical `XXX-XXX-XXX` form.
#[wasm_bindgen]
pub fn encode_results_file(request_json: &str) -> Result<String, JsError> {
    encode_results_file_impl(request_json).map_err(|error| JsError::new(&error))
}

/// Decodes results-file text into `{"query": <canonical query document>,
/// "seeds": [...], "dropped": <number>, "app_version": ..., "shpd_version":
/// ...}`. The seeds are already deduplicated and capped at the shared result
/// limit, so every platform restores the identical list, and `dropped` counts
/// the exported entries that step removed.
///
/// # Errors
///
/// Returns a JavaScript error for input above the 2 MiB import cap, for files
/// that are not results files, and for an invalid query or seed code.
#[wasm_bindgen]
pub fn decode_results_file(contents: &str) -> Result<String, JsError> {
    decode_results_file_impl(contents).map_err(|error| JsError::new(&error))
}

/// Decodes and analyzes a query without throwing or panicking on bad input.
#[wasm_bindgen]
#[must_use]
pub fn analyze_query(query_json: &str) -> String {
    let query = match json_query::decode(query_json) {
        Ok(query) => query,
        Err(error) => {
            return to_json(&AnalysisOutput::Invalid {
                valid: false,
                error,
            });
        }
    };
    let plan = QueryPlan::analyze(&query);
    let impossible = plan.is_unsatisfiable();
    let probability = (!impossible)
        .then(|| estimate_match_probability(&query))
        .filter(|value| value.is_finite());
    let notes = if impossible {
        vec!["No seed can satisfy this combination of requirements.".to_owned()]
    } else {
        Vec::new()
    };
    to_json(&AnalysisOutput::Valid {
        valid: true,
        probability,
        impossible,
        notes,
    })
}

/// Generates and describes one complete depth-24 world as JSON.
///
/// # Errors
///
/// Returns a JavaScript error for malformed requests, invalid seeds or
/// queries, and world-generation failures.
#[wasm_bindgen]
pub fn scout(request_json: &str) -> Result<String, JsError> {
    scout_impl(request_json).map_err(|error| JsError::new(&error))
}

/// Re-verifies specific seeds against a full query using the same
/// authoritative matcher as the search path, returning the matching seeds as
/// a JSON array of `{code, value}` in input order. This backs the "refine"
/// flow: existing result seeds are filtered by the combined query instead of
/// trusting stale metadata.
///
/// # Errors
///
/// Returns a JavaScript error for an invalid query or seed value.
#[wasm_bindgen]
#[allow(clippy::needless_pass_by_value)] // wasm-bindgen requires an owned Vec.
pub fn filter_seeds(query_json: &str, seed_values: Vec<f64>) -> Result<String, JsError> {
    filter_seeds_impl(query_json, &seed_values).map_err(|error| JsError::new(&error))
}

/// Reports whether the query in `candidate_json` continues the one in
/// `base_json`: an identical depth, challenge set and fast mode, world
/// conditions (the blacksmith flags and the Wandmaker filter) at least as
/// strict as the base's, and every base requirement covered by a distinct candidate
/// requirement at least as strict (equal or strengthened). Only a continuing
/// query may reuse a stopped search's results and coverage remainder (the
/// filter-and-resume refine flow).
///
/// # Errors
///
/// Returns a JavaScript error when either query fails to decode.
#[wasm_bindgen]
pub fn query_continues(candidate_json: &str, base_json: &str) -> Result<bool, JsError> {
    query_continues_impl(candidate_json, base_json).map_err(|error| JsError::new(&error))
}

fn query_continues_impl(candidate_json: &str, base_json: &str) -> Result<bool, String> {
    Ok(json_query::decode(candidate_json)?.continues(&json_query::decode(base_json)?))
}

/// Cooperative, single-threaded browser search state.
#[wasm_bindgen]
pub struct SearchSession {
    query: SearchQuery,
    plan: QueryPlan,
    generator: ConfiguredMainWorldGenerator,
    cursor: u64,
    end_seed_exclusive: u64,
    tested: u64,
    accepted: usize,
    completed: bool,
}

#[wasm_bindgen]
impl SearchSession {
    /// Creates a validated cooperative search over a half-open numeric range.
    ///
    /// # Errors
    ///
    /// Returns a JavaScript error for an invalid query, fractional or
    /// non-finite bounds, or a range outside the core seed space.
    #[wasm_bindgen(constructor)]
    pub fn new(
        query_json: &str,
        start_seed: f64,
        end_seed_exclusive: f64,
    ) -> Result<SearchSession, JsError> {
        Self::new_impl(query_json, start_seed, end_seed_exclusive)
            .map_err(|error| JsError::new(&error))
    }

    /// Tests at most `max_seeds` more seeds and returns only newly found matches.
    #[must_use]
    pub fn advance(&mut self, max_seeds: u32) -> String {
        let mut matches = Vec::new();
        let mut remaining =
            u64::from(max_seeds).min(self.end_seed_exclusive.saturating_sub(self.cursor));

        while remaining > 0 && !self.completed {
            let batch_len = remaining.min(SEARCH_BATCH_SIZE);
            let batch_end = self.cursor + batch_len;
            let seeds = (self.cursor..batch_end)
                .filter_map(|value| DungeonSeed::new(value).ok())
                .collect::<Vec<_>>();
            let worlds = self.generator.generate_batch_gated(
                &seeds,
                self.plan.generation_depth(),
                &self.plan,
            );
            for world in worlds {
                self.cursor += 1;
                self.tested += 1;
                if let Some(world) = world
                    && self.query.matches(&world)
                {
                    matches.push(world.seed.into());
                    self.accepted += 1;
                    if self.accepted == MAX_RESULTS {
                        self.completed = true;
                        break;
                    }
                }
            }
            remaining = remaining.saturating_sub(batch_len);
            if self.cursor == self.end_seed_exclusive {
                self.completed = true;
            }
        }

        to_json(&AdvanceOutput {
            state: if self.completed {
                "completed"
            } else {
                "running"
            },
            tested: self.tested,
            matches,
        })
    }
}

impl SearchSession {
    fn new_impl(
        query_json: &str,
        start_seed: f64,
        end_seed_exclusive: f64,
    ) -> Result<Self, String> {
        let query = json_query::decode(query_json)?;
        let start_seed = seed_bound(start_seed, false)?;
        let end_seed_exclusive = seed_bound(end_seed_exclusive, true)?;
        if start_seed >= end_seed_exclusive {
            return Err("start_seed must be less than end_seed_exclusive".to_owned());
        }
        let plan = QueryPlan::analyze(&query);
        let completed = plan.is_unsatisfiable();
        Ok(Self {
            generator: CanonicalMainWorldGenerator::with_challenges(query.challenges),
            query,
            plan,
            cursor: if completed {
                end_seed_exclusive
            } else {
                start_seed
            },
            end_seed_exclusive,
            tested: 0,
            accepted: 0,
            completed,
        })
    }
}

fn filter_seeds_impl(query_json: &str, seed_values: &[f64]) -> Result<String, String> {
    let query = json_query::decode(query_json)?;
    let seeds = seed_values
        .iter()
        .map(|&value| {
            seed_bound(value, false)
                .and_then(|value| DungeonSeed::new(value).map_err(|error| error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let plan = QueryPlan::analyze(&query);
    if plan.is_unsatisfiable() {
        return Ok(to_json::<Vec<SeedOutput>>(&Vec::new()));
    }
    let generator = CanonicalMainWorldGenerator::with_challenges(query.challenges);
    let worlds = generator.generate_batch_gated(&seeds, plan.generation_depth(), &plan);
    let matches = worlds
        .into_iter()
        .flatten()
        .filter(|world| query.matches(world))
        .map(|world| SeedOutput::from(world.seed))
        .collect::<Vec<_>>();
    Ok(to_json(&matches))
}

fn encode_share_link_impl(query_json: &str) -> Result<String, String> {
    let query = json_query::decode(query_json)?;
    deep_link::encode_link(&query)
}

fn decode_share_text_impl(text: &str) -> Result<String, String> {
    let query = deep_link::decode_text(text)?;
    Ok(json_query::encode(&query).to_string())
}

fn encode_results_file_impl(request_json: &str) -> Result<String, String> {
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

fn decode_results_file_impl(contents: &str) -> Result<String, String> {
    let file = results_export::decode(contents)?;
    let (seeds, dropped) = results_export::dedupe_and_cap(&file.seeds, MAX_RESULTS);
    Ok(serde_json::json!({
        "query": json_query::encode(&file.query),
        "seeds": seeds.iter().copied().map(DungeonSeed::to_code).collect::<Vec<_>>(),
        "dropped": dropped,
        "app_version": file.app_version,
        "shpd_version": file.shpd_version,
    })
    .to_string())
}

fn parse_seed_code_impl(input: &str) -> Result<String, String> {
    let seed = DungeonSeed::from_code(input).map_err(|error| error.to_string())?;
    Ok(to_json(&SeedOutput::from(seed)))
}

fn scout_impl(request_json: &str) -> Result<String, String> {
    let request: ScoutRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid scout request JSON: {error}"))?;
    let formatted_seed = format_seed_code(&request.seed);
    let seed = DungeonSeed::from_code(&formatted_seed).map_err(|error| error.to_string())?;
    let challenges = request
        .challenges
        .into_iter()
        .fold(Challenges::NONE, |mask, challenge| mask | challenge.into());
    let query = request
        .query
        .map(|value| json_query::decode(&value.to_string()))
        .transpose()?;
    let world = generate_main_world_with_challenges(seed, 24, challenges)
        .map_err(|error| format!("world generation failed: {error}"))?;
    let marks = query.as_ref().map(|query| scout_matches(&world, query));
    let matched_requirements = marks.as_ref().map_or(0, |marks| marks.matched_requirements);
    let total_requirements = marks.as_ref().map_or(0, |marks| marks.total_requirements);
    let matched = marks.map_or_else(|| vec![false; world.items.len()], |marks| marks.matched);
    let items = world
        .items
        .iter()
        .zip(matched)
        .map(|(world_item, matched)| scout_item_output(world_item, matched))
        .collect();
    Ok(to_json(&ScoutOutput {
        seed: seed.into(),
        quests: scout_quest_outputs(world.quests),
        items,
        matched_requirements,
        total_requirements,
    }))
}

fn scout_quest_outputs(quests: QuestSummary) -> Vec<ScoutQuestOutput> {
    let mut output = Vec::with_capacity(4);
    if let Some(quest) = quests.ghost {
        output.push(ScoutQuestOutput {
            quest: "ghost",
            variant: match quest.variant {
                GhostQuestType::FetidRat => "fetid_rat",
                GhostQuestType::GnollTrickster => "gnoll_trickster",
                GhostQuestType::GreatCrab => "great_crab",
            },
            depth: quest.depth,
        });
    }
    if let Some(quest) = quests.wandmaker {
        output.push(ScoutQuestOutput {
            quest: "wandmaker",
            variant: match quest.variant {
                WandmakerQuestType::CorpseDust => "corpse_dust",
                WandmakerQuestType::ElementalEmbers => "elemental_embers",
                WandmakerQuestType::Rotberry => "rotberry",
            },
            depth: quest.depth,
        });
    }
    if let Some(quest) = quests.blacksmith {
        output.push(ScoutQuestOutput {
            quest: "blacksmith",
            variant: match quest.variant {
                BlacksmithQuestType::Crystal => "crystal",
                BlacksmithQuestType::Gnoll => "gnoll",
            },
            depth: quest.depth,
        });
    }
    if let Some(quest) = quests.imp {
        output.push(ScoutQuestOutput {
            quest: "imp",
            variant: match quest.variant {
                ImpTarget::Monk => "monk",
                ImpTarget::Golem => "golem",
            },
            depth: quest.depth,
        });
    }
    output
}

fn scout_item_output(world_item: &WorldItem, matched: bool) -> ScoutItemOutput {
    let definition = item(world_item.item);
    ScoutItemOutput {
        id: definition.stable_id,
        name: definition.name,
        category: item_kind_name(definition.kind),
        sprite_index: definition.sprite_index,
        upgrade: world_item.upgrade,
        effect: world_item.effect.map(effect_output),
        cursed: world_item.cursed,
        secret: world_item.secret,
        depth: world_item.depth,
        source: item_source_name(world_item.source),
        accessibility: accessibility_output(world_item.accessibility),
        matched,
    }
}

const fn item_kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Weapon => "weapon",
        ItemKind::Armor => "armor",
        ItemKind::Wand => "wand",
        ItemKind::Ring => "ring",
    }
}

const fn item_source_name(source: ItemSource) -> &'static str {
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

const fn effect_output(effect: Effect) -> EffectOutput {
    EffectOutput {
        name: effect.wire_name(),
        kind: if effect.is_curse() {
            "curse"
        } else {
            "enchantment"
        },
    }
}

fn accessibility_output(accessibility: Accessibility) -> AccessibilityOutput {
    match accessibility {
        Accessibility::Independent => AccessibilityOutput::Independent,
        Accessibility::Choice { group, option } => AccessibilityOutput::Choice { group, option },
        Accessibility::Scenarios { group, mask } => AccessibilityOutput::Scenarios {
            group,
            mask: format!("0x{mask:x}"),
        },
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn seed_bound(value: f64, allow_total: bool) -> Result<u64, String> {
    // Every permitted value is below 2^53 and therefore exactly representable.
    let upper = if allow_total {
        TOTAL_SEEDS as f64
    } else {
        (TOTAL_SEEDS - 1) as f64
    };
    if !value.is_finite() || value.fract() != 0.0 || value < 0.0 || value > upper {
        return Err(if allow_total {
            format!("seed bound must be an integer in 0..={TOTAL_SEEDS}")
        } else {
            format!("seed bound must be an integer in 0..{}", TOTAL_SEEDS - 1)
        });
    }
    Ok(value as u64)
}

fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| r#"{"error":"JSON serialization failed"}"#.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::num::NonZeroUsize;

    use serde::Deserialize;
    use serde_json::{Value, json};
    use shpd_seedfinder_core::catalog::item;
    use shpd_seedfinder_core::json_query;
    use shpd_seedfinder_core::main_world::{CanonicalMainWorldGenerator, generate_main_world};
    use shpd_seedfinder_core::search::{SearchOptions, SearchProgress, search_parallel};
    use shpd_seedfinder_core::seed::{DungeonSeed, TOTAL_SEEDS};

    use super::{
        MAX_RESULTS, SearchSession, analyze_query, decode_results_file_impl,
        decode_share_text_impl, encode_results_file_impl, encode_share_link_impl, engine_info,
        filter_seeds_impl, format_seed_code, parse_seed_code_impl, query_continues_impl,
        scout_impl,
    };

    /// The frozen cross-platform fixtures: the browser codec decodes exactly
    /// the documents every other platform decodes.
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
            let decoded: Value = serde_json::from_str(&decode_results_file_impl(fixture).unwrap())
                .unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(decoded["shpd_version"], "3.3.8");
            assert!(!decoded["seeds"].as_array().unwrap().is_empty());
            assert_eq!(decoded["dropped"], 0);

            let request = json!({
                "query": decoded["query"],
                "seeds": decoded["seeds"],
                "app_version": "test",
            });
            let encoded = encode_results_file_impl(&request.to_string()).unwrap();
            let round_tripped: Value =
                serde_json::from_str(&decode_results_file_impl(&encoded).unwrap()).unwrap();
            assert_eq!(round_tripped["query"], decoded["query"]);
            assert_eq!(round_tripped["seeds"], decoded["seeds"]);
            assert_eq!(round_tripped["dropped"], 0);
            assert_eq!(round_tripped["app_version"], "test");
        }
    }

    #[test]
    fn results_decoding_dedupes_caps_and_refuses_oversized_files() {
        let duplicated = json!({
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
            serde_json::from_str(&decode_results_file_impl(&duplicated.to_string()).unwrap())
                .unwrap();
        assert_eq!(decoded["seeds"].as_array().unwrap().len(), MAX_RESULTS);
        // Ten duplicates: importers report exactly what dedupe-and-cap removed.
        assert_eq!(decoded["dropped"], 10);
        assert!(decoded["app_version"].is_null());

        let oversized = " ".repeat(super::results_export::MAX_FILE_BYTES + 1);
        let error = decode_results_file_impl(&oversized).unwrap_err();
        assert!(error.contains("too large"), "{error}");
    }

    #[test]
    fn results_encoding_fails_on_invalid_queries_and_seed_codes() {
        let invalid_query = json!({"query": {"requirements": []}, "seeds": []});
        assert!(encode_results_file_impl(&invalid_query.to_string()).is_err());

        let invalid_seed = json!({
            "query": {"requirements": [{"item": "sword"}]},
            "seeds": ["aaa-aaa-aab"],
        });
        let error = encode_results_file_impl(&invalid_seed.to_string()).unwrap_err();
        assert!(error.contains("canonical"), "{error}");

        assert!(encode_results_file_impl("not json").is_err());
        assert!(encode_results_file_impl(r#"{"seeds":[]}"#).is_err());
    }

    #[test]
    fn query_continuation_matches_scope_and_requirement_multiset() {
        let base = r#"{"requirements":[{"kind":"ring","upgrade":{"at_least":1}}],"max_depth":6}"#;
        let narrowed = r#"{"requirements":[{"kind":"ring","upgrade":{"at_least":1}},{"kind":"wand"}],"max_depth":6}"#;
        let rescoped =
            r#"{"requirements":[{"kind":"ring","upgrade":{"at_least":1}}],"max_depth":7}"#;
        assert!(query_continues_impl(base, base).unwrap());
        assert!(query_continues_impl(narrowed, base).unwrap());
        assert!(!query_continues_impl(base, narrowed).unwrap());
        assert!(!query_continues_impl(rescoped, base).unwrap());
        assert!(query_continues_impl("not json", base).is_err());
    }

    #[test]
    fn share_links_round_trip_the_canonical_document() {
        let document = r#"{"requirements":[{"item":"wand_fireblast","upgrade":{"at_least":3}}]}"#;
        let link = encode_share_link_impl(document).unwrap();
        assert_eq!(link, "https://shpd-seed-seeker.web.app/#q=EAGWhMA");
        // Decoding returns the canonical document, which spells out the kind.
        let canonical = r#"{"requirements":[{"item":"wand_fireblast","kind":"wand","upgrade":{"at_least":3}}]}"#;
        assert_eq!(decode_share_text_impl(&link).unwrap(), canonical);
        assert_eq!(decode_share_text_impl("EAGWhMA").unwrap(), canonical);
        assert!(encode_share_link_impl(r#"{"requirements":[]}"#).is_err());
        assert!(decode_share_text_impl("https://example.com/").is_err());
    }

    #[test]
    fn engine_info_reports_core_constants() {
        let info: Value = serde_json::from_str(&engine_info()).unwrap();
        assert_eq!(info["shpdVersion"], shpd_seedfinder_core::SHPD_VERSION);
        assert_eq!(info["shpdCommit"], shpd_seedfinder_core::SHPD_COMMIT);
        assert_eq!(info["totalSeeds"], TOTAL_SEEDS);
        assert_eq!(info["maxResults"], MAX_RESULTS);
    }

    #[test]
    fn interactive_seed_formatting_handles_partial_and_garbage_input() {
        assert_eq!(format_seed_code("a"), "A");
        assert_eq!(format_seed_code("abcD"), "ABC-D");
        assert_eq!(format_seed_code("abc-def-ghi"), "ABC-DEF-GHI");
        assert_eq!(format_seed_code(" 1a!b@c#d$e%f^g&h*i extra"), "ABC-DEF-GHI");
        assert_eq!(format_seed_code("åa😀b"), "AB");
    }

    #[test]
    fn seed_parsing_accepts_core_forms_and_rejects_invalid_codes() {
        let parsed: Value =
            serde_json::from_str(&parse_seed_code_impl("aaa-aaa-aab").unwrap()).unwrap();
        assert_eq!(parsed, json!({"code":"AAA-AAA-AAB","value":1}));
        assert!(parse_seed_code_impl("aaaaaaaaa").is_err());
        assert!(parse_seed_code_impl("AAA-AAA-AA0").is_err());
    }

    #[test]
    fn query_analysis_covers_valid_invalid_and_impossible_inputs() {
        let valid: Value = serde_json::from_str(&analyze_query(
            r#"{"requirements":[{"item":"wand_fireblast","upgrade":{"at_least":2}}]}"#,
        ))
        .unwrap();
        assert_eq!(valid["valid"], true);
        assert_eq!(valid["impossible"], false);
        assert!(valid["probability"].as_f64().is_some());

        let invalid: Value = serde_json::from_str(&analyze_query("not json")).unwrap();
        assert_eq!(invalid["valid"], false);
        assert!(invalid["error"].as_str().unwrap().contains("invalid JSON"));

        let impossible: Value = serde_json::from_str(&analyze_query(
            r#"{"requirements":[{"kind":"ring","upgrade":4,"uncursed":true}]}"#,
        ))
        .unwrap();
        assert_eq!(impossible["valid"], true);
        assert_eq!(impossible["impossible"], true);
        assert!(impossible["probability"].is_null());
        assert!(!impossible["notes"].as_array().unwrap().is_empty());
    }

    #[derive(Deserialize)]
    struct AndroidCatalog {
        entries: Vec<AndroidEntry>,
    }

    #[derive(Deserialize)]
    struct AndroidEntry {
        id: String,
        sprite: u16,
        #[serde(default)]
        class: Option<String>,
    }

    #[test]
    fn scout_matches_canonical_world_and_android_catalog() {
        use shpd_seedfinder_core::catalog::{WeaponCategory, item_by_stable_id};

        let output: Value =
            serde_json::from_str(&scout_impl(r#"{"seed":"AAA-AAA-AAA"}"#).unwrap()).unwrap();
        let world = generate_main_world(DungeonSeed::MIN, 24).unwrap();
        let output_items = output["items"].as_array().unwrap();
        assert_eq!(output_items.len(), world.items.len());

        assert_eq!(
            output["quests"],
            serde_json::json!([
                { "quest": "ghost", "variant": "great_crab", "depth": 4 },
                { "quest": "wandmaker", "variant": "elemental_embers", "depth": 9 },
                { "quest": "blacksmith", "variant": "crystal", "depth": 13 },
                { "quest": "imp", "variant": "golem", "depth": 19 },
            ])
        );

        let catalog: AndroidCatalog = serde_json::from_str(include_str!(
            "../../../android/app/src/main/assets/third_party/shattered-pixel-dungeon/catalog-v3.3.8.json"
        ))
        .unwrap();
        let sprites = catalog
            .entries
            .iter()
            .map(|entry| (entry.id.clone(), entry.sprite))
            .collect::<BTreeMap<_, _>>();
        for (output_item, world_item) in output_items.iter().zip(&world.items) {
            let definition = item(world_item.item);
            assert_eq!(output_item["id"], definition.stable_id);
            assert_eq!(output_item["depth"], world_item.depth);
            assert_eq!(output_item["secret"], world_item.secret);
            assert_eq!(
                output_item["spriteIndex"],
                sprites.get(definition.stable_id).copied().unwrap()
            );
        }
        // The canonical seed hides part of its loot behind secret rooms.
        assert!(output_items.iter().any(|item| item["secret"] == true));

        // The asset's melee/thrown classes mirror the core catalog exactly.
        for entry in &catalog.entries {
            let expected =
                item_by_stable_id(&entry.id)
                    .unwrap()
                    .weapon_category()
                    .map(|category| match category {
                        WeaponCategory::Melee => "melee",
                        WeaponCategory::Thrown => "thrown",
                    });
            assert_eq!(entry.class.as_deref(), expected, "{}", entry.id);
        }
    }

    #[test]
    fn scout_query_marks_a_matching_known_item() {
        let world = generate_main_world(DungeonSeed::MIN, 24).unwrap();
        let known = &world.items[0];
        let definition = item(known.item);
        let request = json!({
            "seed": "AAAAAAAAA",
            "query": {
                "requirements": [{
                    "item": definition.stable_id,
                    "max_depth": known.depth
                }]
            }
        });
        let output: Value =
            serde_json::from_str(&scout_impl(&request.to_string()).unwrap()).unwrap();
        assert_eq!(output["totalRequirements"], 1);
        assert_eq!(output["matchedRequirements"], 1);
        let matched = output["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["matched"] == true)
            .collect::<Vec<_>>();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0]["id"], definition.stable_id);
    }

    #[test]
    fn filtering_seeds_agrees_with_the_search_matcher() {
        // Find a couple of matches in a small range first, then check that
        // filtering keeps exactly those seeds among a mixed candidate list.
        let query_json =
            r#"{"requirements":[{"kind":"ring","upgrade":{"at_least":1}}],"max_depth":6}"#;
        let query = json_query::decode(query_json).unwrap();
        let matches = search_parallel(
            &CanonicalMainWorldGenerator,
            &query,
            SearchOptions {
                start_seed: 0,
                end_seed_exclusive: 2_000,
                workers: NonZeroUsize::MIN,
                chunk_size: NonZeroUsize::new(64).unwrap(),
                max_results: NonZeroUsize::new(MAX_RESULTS).unwrap(),
            },
            &SearchProgress::default(),
        )
        .unwrap()
        .worlds
        .into_iter()
        .map(|world| world.seed.value())
        .collect::<Vec<_>>();
        assert!(!matches.is_empty());

        #[allow(clippy::cast_precision_loss)]
        let candidates = (0..2_000_u64).map(|seed| seed as f64).collect::<Vec<_>>();
        let output: Value =
            serde_json::from_str(&filter_seeds_impl(query_json, &candidates).unwrap()).unwrap();
        let kept = output
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["value"].as_u64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(kept, matches);

        assert!(filter_seeds_impl("not json", &[0.0]).is_err());
        assert!(filter_seeds_impl(query_json, &[-1.0]).is_err());
        assert_eq!(filter_seeds_impl(query_json, &[]).unwrap(), "[]");
    }

    #[test]
    fn cooperative_search_matches_one_worker_parallel_search() {
        let query_json = r#"{
            "max_depth": 9,
            "requirements":[{
                "item":"wand_fireblast",
                "upgrade":{"exact":3},
                "source":"wandmaker_reward"
            }]
        }"#;
        let query = json_query::decode(query_json).unwrap();
        let parallel = search_parallel(
            &CanonicalMainWorldGenerator,
            &query,
            SearchOptions {
                start_seed: 0,
                end_seed_exclusive: 20_000,
                workers: NonZeroUsize::MIN,
                chunk_size: NonZeroUsize::new(137).unwrap(),
                max_results: NonZeroUsize::new(MAX_RESULTS).unwrap(),
            },
            &SearchProgress::default(),
        )
        .unwrap()
        .worlds
        .into_iter()
        .map(|world| world.seed.value())
        .collect::<Vec<_>>();

        let mut session = SearchSession::new_impl(query_json, 0.0, 20_000.0).unwrap();
        let mut cooperative = Vec::new();
        loop {
            let output: Value = serde_json::from_str(&session.advance(113)).unwrap();
            cooperative.extend(
                output["matches"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|seed| seed["value"].as_u64().unwrap()),
            );
            if output["state"] == "completed" {
                break;
            }
        }
        assert_eq!(cooperative, parallel);
    }
}
