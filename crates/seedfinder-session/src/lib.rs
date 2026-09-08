//! Frontend-neutral search sessions, registry, and scout packet generation.

use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub mod json;

use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::feasibility::QueryPlan;
use shpd_seedfinder_core::main_world::{CanonicalMainWorldGenerator, ConfiguredMainWorldGenerator};
use shpd_seedfinder_core::model::GeneratedWorld;
use shpd_seedfinder_core::probability::estimate_match_probability;
use shpd_seedfinder_core::query::{ScoutMatches, SearchQuery, scout_matches};
pub use shpd_seedfinder_core::query::{StartDecision, decide_start};
pub use shpd_seedfinder_core::results_export::MAX_RESULTS;
pub use shpd_seedfinder_core::search::{PRODUCTION_SEARCH_START_STRIDE, SearchError};
use shpd_seedfinder_core::search::{
    SearchOptions, StreamingSearchHandle, StreamingSearchState, WorldGenerator,
    spawn_partial_streaming_search, spawn_rotated_streaming_search, spawn_streaming_search,
};
use shpd_seedfinder_core::seed::{DungeonSeed, TOTAL_SEEDS};
use shpd_seedfinder_core::wire::{
    WireError, decode_query, decode_scout_request, encode_results, encode_scout_world,
};

pub const STATE_RUNNING: i64 = 0;
pub const STATE_COMPLETED: i64 = 1;
pub const STATE_CANCELLED: i64 = 2;
pub const STATE_FAILED: i64 = 3;
pub const ERROR_NONE: i64 = 0;
pub const ERROR_SEARCH_WORKER_FAILED: i64 = 2_001;
pub const SEARCH_CHUNK_SIZE: usize = 4;

static REGISTRY: OnceLock<SessionRegistry> = OnceLock::new();
static CANONICAL_GENERATORS: OnceLock<Mutex<HashMap<u16, Arc<ConfiguredMainWorldGenerator>>>> =
    OnceLock::new();
static NEXT_PRODUCTION_SEARCH_START: OnceLock<AtomicU64> = OnceLock::new();

#[must_use]
pub fn registry() -> &'static SessionRegistry {
    REGISTRY.get_or_init(SessionRegistry::new)
}

fn canonical_generator(challenges: Challenges) -> Arc<ConfiguredMainWorldGenerator> {
    let generators = CANONICAL_GENERATORS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut generators = generators
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Arc::clone(
        generators
            .entry(challenges.bits())
            .or_insert_with(|| Arc::new(CanonicalMainWorldGenerator::with_challenges(challenges))),
    )
}

fn production_search_start() -> u64 {
    let next = NEXT_PRODUCTION_SEARCH_START.get_or_init(|| {
        let random_start = RandomState::new().hash_one(0_u8) % TOTAL_SEEDS;
        AtomicU64::new(random_start)
    });
    claim_production_search_start(next)
}

fn claim_production_search_start(next: &AtomicU64) -> u64 {
    next.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(advance_production_search_start(current))
    })
    .unwrap_or_else(|current| current)
}

const fn advance_production_search_start(current: u64) -> u64 {
    if current >= TOTAL_SEEDS - PRODUCTION_SEARCH_START_STRIDE {
        current - (TOTAL_SEEDS - PRODUCTION_SEARCH_START_STRIDE)
    } else {
        current + PRODUCTION_SEARCH_START_STRIDE
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScoutPacketError {
    Request(WireError),
    Response(WireError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScoutCallError {
    Packet(ScoutPacketError),
    Panicked,
}

/// Validates an `SSQ2` scout request (`magic[4]`, little-endian challenge
/// `u16`, remaining UTF-8 seed code) or a legacy raw-seed request, generates a
/// depth-24 world with the supplied generator, and encodes `SSC5`.
///
/// # Errors
///
/// Returns a request or response wire error.
pub fn scout_seed_packet<G: WorldGenerator + ?Sized>(
    generator: &G,
    request: &[u8],
) -> Result<Vec<u8>, ScoutPacketError> {
    let (seed, _) = decode_scout_request(request).map_err(ScoutPacketError::Request)?;
    let world = generator.generate(seed, 24);
    encode_scout_world(&world).map_err(ScoutPacketError::Response)
}

/// Performs [`scout_seed_packet`] while containing generator panics.
///
/// # Errors
///
/// Returns a packet error or [`ScoutCallError::Panicked`].
pub fn protected_scout_seed_packet<G: WorldGenerator + ?Sized>(
    generator: &G,
    request: &[u8],
) -> Result<Vec<u8>, ScoutCallError> {
    catch_unwind(AssertUnwindSafe(|| scout_seed_packet(generator, request)))
        .map_err(|_| ScoutCallError::Panicked)?
        .map_err(ScoutCallError::Packet)
}

/// Scouts one world with the canonical production generator selected by the
/// `SSQ2` challenge mask. Legacy raw UTF-8 seed requests use mask zero.
///
/// # Errors
///
/// Returns a packet error or a contained generation panic.
pub fn production_scout_packet(request: &[u8]) -> Result<Vec<u8>, ScoutCallError> {
    let (seed, challenges) = decode_scout_request(request)
        .map_err(ScoutPacketError::Request)
        .map_err(ScoutCallError::Packet)?;
    let world = production_scout_world(seed, challenges)?;
    encode_scout_world(&world)
        .map_err(ScoutPacketError::Response)
        .map_err(ScoutCallError::Packet)
}

/// Scouts one depth-24 world with the cached canonical production generator,
/// returning it typed for in-process Rust frontends. Generation panics are
/// contained like in [`production_scout_packet`].
///
/// # Errors
///
/// Returns [`ScoutCallError::Panicked`] when world generation panics.
pub fn production_scout_world(
    seed: DungeonSeed,
    challenges: Challenges,
) -> Result<GeneratedWorld, ScoutCallError> {
    let generator = canonical_generator(challenges);
    catch_unwind(AssertUnwindSafe(|| generator.generate(seed, 24)))
        .map_err(|_| ScoutCallError::Panicked)
}

/// Failure modes of [`production_scout_matches`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScoutMatchError {
    Request(WireError),
    Query(WireError),
    Panicked,
}

/// Scouts the world named by an `SSQ2` (or legacy raw seed) scout request and
/// reports which of its items satisfy the query in `query_packet`.
///
/// The world is generated exactly like [`production_scout_packet`]'s, so the
/// reported item indices address the item list of the `SSC5` packet that
/// request produces.
///
/// # Errors
///
/// Returns the scout request's or the query packet's decode error, or
/// [`ScoutMatchError::Panicked`] when world generation panics.
pub fn production_scout_matches(
    request: &[u8],
    query_packet: &[u8],
) -> Result<ScoutMatches, ScoutMatchError> {
    let (seed, challenges) = decode_scout_request(request).map_err(ScoutMatchError::Request)?;
    let query = decode_query(query_packet).map_err(ScoutMatchError::Query)?;
    let world = production_scout_world(seed, challenges).map_err(|_| ScoutMatchError::Panicked)?;
    Ok(scout_matches(&world, &query))
}

/// Logical processors available to search workers, never less than one.
/// Frontends read their worker-selector ceiling from here so the engine and
/// its UI agree on what "all cores" means.
#[must_use]
pub fn available_workers() -> usize {
    SearchOptions::available_parallelism().get()
}

/// The worker count a session actually spawns: the request clamped to the
/// host's parallelism, or every available core when the caller passes `None`.
fn effective_workers(requested: Option<NonZeroUsize>) -> NonZeroUsize {
    let available = SearchOptions::available_parallelism();
    requested.map_or(available, |workers| workers.min(available))
}

/// Re-verifies specific seeds against a full query, returning the matching
/// worlds in input order. This is the "filter existing results" half of
/// refining a search: frontends pass the seeds already on screen together with
/// the combined (old plus added requirements) query.
///
/// Generation runs on every available core; input order is preserved.
///
/// # Errors
///
/// Returns [`SearchError`] for an invalid query or a seed value outside the
/// seed space.
pub fn filter_matching_seeds(
    query: &SearchQuery,
    seed_values: &[u64],
) -> Result<Vec<GeneratedWorld>, SearchError> {
    query.validate()?;
    let seeds = seed_values
        .iter()
        .map(|&value| DungeonSeed::new(value).map_err(|_| SearchError::InvalidSeedRange))
        .collect::<Result<Vec<_>, _>>()?;
    let plan = QueryPlan::analyze(query);
    if plan.is_unsatisfiable() || seeds.is_empty() {
        return Ok(Vec::new());
    }
    let generator = canonical_generator(query.challenges);
    let depth = plan.generation_depth();
    let workers = SearchOptions::available_parallelism()
        .get()
        .min(seeds.len());
    let slice_len = seeds.len().div_ceil(workers);
    std::thread::scope(|scope| {
        let handles = seeds
            .chunks(slice_len)
            .map(|slice| {
                let generator = &generator;
                let plan = &plan;
                scope.spawn(move || {
                    // The catch keeps the panic on this side of the scope:
                    // an unwinding scoped thread would make `thread::scope`
                    // itself panic on exit, turning the error path below into
                    // a panic whenever more than one slice trips it.
                    catch_unwind(AssertUnwindSafe(|| {
                        generator
                            .generate_batch_gated(slice, depth, plan)
                            .into_iter()
                            .flatten()
                            .filter(|world| query.matches(world))
                            .collect::<Vec<_>>()
                    }))
                })
            })
            .collect::<Vec<_>>();
        let mut matched = Vec::new();
        for handle in handles {
            // A generator panic must surface as an error: silently treating
            // its slice as empty would drop genuine matches from the filter.
            matched.extend(
                handle
                    .join()
                    .ok()
                    .and_then(Result::ok)
                    .ok_or(SearchError::WorkerPanicked)?,
            );
        }
        Ok(matched)
    })
}

/// Packet form of [`filter_matching_seeds`] for wire frontends: decodes an
/// query request, filters `seed_values`, and encodes the surviving
/// seeds as an `SSR1` result packet. Generation panics are contained.
///
/// # Errors
///
/// Returns a request error, a spawn-shaped error for invalid seeds, or a
/// response encoding error.
pub fn production_filter_packet(
    request: &[u8],
    seed_values: &[u64],
) -> Result<Vec<u8>, FilterPacketError> {
    let query = decode_query(request).map_err(FilterPacketError::Request)?;
    let worlds = catch_unwind(AssertUnwindSafe(|| {
        filter_matching_seeds(&query, seed_values)
    }))
    .map_err(|_| FilterPacketError::Panicked)?
    .map_err(FilterPacketError::Filter)?;
    encode_results(&worlds).map_err(FilterPacketError::Response)
}

/// Failure modes of [`production_filter_packet`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FilterPacketError {
    Request(WireError),
    Filter(SearchError),
    Response(WireError),
    Panicked,
}

/// Decodes two query requests and reports whether `candidate`
/// continues `base`: an identical depth and challenge set, world
/// conditions (the blacksmith flags and the Wandmaker filter) at least as
/// strict as the base's, and every requirement of `base` covered by a distinct
/// candidate requirement at least as strict (equal or strengthened).
/// This is the soundness precondition for refining a search — only a
/// continuing query may filter a stopped session's delivered results and
/// resume its uncovered remainder. See [`SearchQuery::continues`].
///
/// # Errors
///
/// Returns the decode error of the first undecodable packet.
pub fn queries_continue(candidate: &[u8], base: &[u8]) -> Result<bool, WireError> {
    Ok(decode_query(candidate)?.continues(&decode_query(base)?))
}

/// Packet form of [`decide_start`]: the queries arrive as query requests, an
/// absent Target or detached base as `None`.
///
/// # Errors
///
/// Returns the decode error of the first undecodable packet.
pub fn decide_start_packets(
    candidate: &[u8],
    target: Option<&[u8]>,
    target_set_empty: bool,
    target_has_uncovered_seeds: bool,
    detached_base: Option<&[u8]>,
) -> Result<StartDecision, WireError> {
    let candidate = decode_query(candidate)?;
    let target = target.map(decode_query).transpose()?;
    let detached_base = detached_base.map(decode_query).transpose()?;
    Ok(decide_start(
        &candidate,
        target.as_ref(),
        target_set_empty,
        target_has_uncovered_seeds,
        detached_base.as_ref(),
    ))
}

pub struct NativeSession {
    search: StreamingSearchHandle,
    match_probability: f64,
    diagnostic_claimed: AtomicBool,
}

impl NativeSession {
    /// Starts a session using an injected generator and search range.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError`] if workers cannot be spawned.
    pub fn start<G: WorldGenerator + Send + 'static>(
        generator: &Arc<G>,
        query: SearchQuery,
        options: SearchOptions,
    ) -> Result<Self, SearchError> {
        let match_probability = estimate_match_probability(&query);
        spawn_streaming_search(generator, query, options).map(|search| Self {
            search,
            match_probability,
            diagnostic_claimed: AtomicBool::new(false),
        })
    }

    /// Starts the canonical full-range production search. `workers` is the
    /// number of search threads to spawn, clamped to the host's parallelism;
    /// `None` uses every available core.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError`] if workers cannot be spawned.
    pub fn production(
        query: SearchQuery,
        workers: Option<NonZeroUsize>,
    ) -> Result<Self, SearchError> {
        let match_probability = estimate_match_probability(&query);
        let options = SearchOptions {
            start_seed: 0,
            end_seed_exclusive: TOTAL_SEEDS,
            workers: effective_workers(workers),
            chunk_size: NonZeroUsize::new(SEARCH_CHUNK_SIZE).unwrap_or(NonZeroUsize::MIN),
            max_results: NonZeroUsize::new(MAX_RESULTS).unwrap_or(NonZeroUsize::MIN),
        };
        let generator = canonical_generator(query.challenges);
        spawn_rotated_streaming_search(&generator, query, options, production_search_start()).map(
            |search| Self {
                search,
                match_probability,
                diagnostic_claimed: AtomicBool::new(false),
            },
        )
    }

    /// Decodes an query request and starts a canonical production search.
    ///
    /// # Errors
    ///
    /// Distinguishes invalid wire requests from worker spawn failures.
    pub fn production_from_packet(
        request: &[u8],
        workers: Option<NonZeroUsize>,
    ) -> Result<Self, StartSessionError> {
        let query = decode_query(request).map_err(StartSessionError::Request)?;
        Self::production(query, workers).map_err(StartSessionError::Spawn)
    }

    /// Starts a production search which resumes a previous traversal: it scans
    /// only the `scan_len` seeds starting at `resume_from`, wrapping at the end
    /// of the seed space. Frontends refine a stopped or completed search by
    /// passing the previous session's [`Self::resume_hint`] values together
    /// with a strictly narrower query (the old requirements plus new ones).
    ///
    /// # Errors
    ///
    /// Returns [`SearchError`] for an invalid query, a resume position outside
    /// the seed space, or a scan length beyond it.
    pub fn production_resumed(
        query: SearchQuery,
        resume_from: u64,
        scan_len: u64,
        workers: Option<NonZeroUsize>,
    ) -> Result<Self, SearchError> {
        let match_probability = estimate_match_probability(&query);
        let options = SearchOptions {
            start_seed: 0,
            end_seed_exclusive: TOTAL_SEEDS,
            workers: effective_workers(workers),
            chunk_size: NonZeroUsize::new(SEARCH_CHUNK_SIZE).unwrap_or(NonZeroUsize::MIN),
            max_results: NonZeroUsize::new(MAX_RESULTS).unwrap_or(NonZeroUsize::MIN),
        };
        let generator = canonical_generator(query.challenges);
        spawn_partial_streaming_search(&generator, query, options, resume_from, scan_len).map(
            |search| Self {
                search,
                match_probability,
                diagnostic_claimed: AtomicBool::new(false),
            },
        )
    }

    /// Decodes an query request and starts a resumed production search.
    ///
    /// # Errors
    ///
    /// Distinguishes invalid wire requests from worker spawn failures.
    pub fn production_resumed_from_packet(
        request: &[u8],
        resume_from: u64,
        scan_len: u64,
        workers: Option<NonZeroUsize>,
    ) -> Result<Self, StartSessionError> {
        let query = decode_query(request).map_err(StartSessionError::Request)?;
        Self::production_resumed(query, resume_from, scan_len, workers)
            .map_err(StartSessionError::Spawn)
    }

    /// Where and how much a follow-up traversal must scan to finish this
    /// session's coverage of the seed space: `[resume_position, remaining]`.
    /// Exact once the session has stopped (any terminal status implies the
    /// workers have exited); meaningless while it is running — a running
    /// session's hint can overshoot the work actually done and must never be
    /// resumed from.
    #[must_use]
    pub fn resume_hint(&self) -> [i64; 2] {
        let coverage = self.search.resume_coverage();
        [
            i64::try_from(coverage.position).unwrap_or(i64::MAX),
            i64::try_from(coverage.remaining).unwrap_or(i64::MAX),
        ]
    }

    /// Drains at most `maximum` matches into an `SSR1` packet.
    ///
    /// # Errors
    ///
    /// Returns a wire error when the result count cannot be encoded.
    pub fn poll(&self, maximum: usize) -> Result<Vec<u8>, WireError> {
        encode_results(&self.search.drain_results(maximum))
    }

    /// Drains at most `maximum` matches as typed worlds. This is the in-process
    /// sibling of [`Self::poll`] for frontends linking the session layer as a
    /// Rust crate rather than over a wire boundary.
    #[must_use]
    pub fn drain_worlds(&self, maximum: usize) -> Vec<GeneratedWorld> {
        self.search.drain_results(maximum)
    }

    pub fn cancel(&self) {
        self.search.cancel();
    }

    #[must_use]
    pub fn status(&self) -> [i64; 5] {
        let (state, error) = match self.search.state() {
            StreamingSearchState::Running => (STATE_RUNNING, ERROR_NONE),
            StreamingSearchState::Completed => (STATE_COMPLETED, ERROR_NONE),
            StreamingSearchState::Cancelled => (STATE_CANCELLED, ERROR_NONE),
            StreamingSearchState::Failed => (STATE_FAILED, ERROR_SEARCH_WORKER_FAILED),
        };
        let tested = self.search.tested();
        [
            state,
            i64::try_from(tested).unwrap_or(i64::MAX),
            i64::try_from(self.search.total()).unwrap_or(i64::MAX),
            error,
            i64::from_ne_bytes(self.match_probability.to_bits().to_ne_bytes()),
        ]
    }

    #[must_use]
    pub fn take_failure_diagnostic(&self) -> Option<String> {
        if self.status()[0] != STATE_FAILED || self.diagnostic_claimed.swap(true, Ordering::AcqRel)
        {
            return None;
        }
        worker_failure_diagnostic(&self.search)
    }

    #[cfg(test)]
    fn is_finished(&self) -> bool {
        self.search.is_finished()
    }
}

#[derive(Debug)]
pub enum StartSessionError {
    Request(WireError),
    Spawn(SearchError),
}

#[must_use]
pub fn worker_failure_diagnostic(search: &StreamingSearchHandle) -> Option<String> {
    let failure = search.failure()?;
    let range = match (failure.chunk_start, failure.chunk_end_exclusive) {
        (Some(start), Some(end)) => {
            let first = DungeonSeed::new(start)
                .map_or_else(|_| start.to_string(), |seed| format!("{start} ({seed})"));
            let last = end.checked_sub(1).map_or_else(
                || "unknown".to_owned(),
                |value| {
                    DungeonSeed::new(value)
                        .map_or_else(|_| value.to_string(), |seed| format!("{value} ({seed})"))
                },
            );
            format!("{first}..={last}")
        }
        _ => "unknown".to_owned(),
    };
    let message = failure.message.replace('\0', "\\0").replace('\n', "\\n");
    Some(format!(
        "streaming worker panic in seed chunk {range}: {message}"
    ))
}

pub struct SessionRegistry {
    next_handle: AtomicI64,
    sessions: Mutex<HashMap<i64, Arc<NativeSession>>>,
}

impl SessionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_handle: AtomicI64::new(1),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn insert(&self, session: NativeSession) -> i64 {
        let session = Arc::new(session);
        loop {
            let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
            if handle == 0 {
                continue;
            }
            let mut guard = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let std::collections::hash_map::Entry::Vacant(entry) = guard.entry(handle) {
                entry.insert(Arc::clone(&session));
                return handle;
            }
        }
    }

    #[must_use]
    pub fn get(&self, handle: i64) -> Option<Arc<NativeSession>> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&handle)
            .cloned()
    }

    pub fn remove(&self, handle: i64) -> Option<Arc<NativeSession>> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&handle)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn close_session(registry: &SessionRegistry, handle: i64) -> bool {
    registry.remove(handle).is_some()
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, Instant};

    use shpd_seedfinder_core::catalog::{ItemId, ItemKind};
    use shpd_seedfinder_core::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
    use shpd_seedfinder_core::query::{
        EffectRequirement, Requirement, SearchQuery, TierRequirement, UpgradeRequirement,
    };
    use shpd_seedfinder_core::run::RingGems;
    use shpd_seedfinder_core::search::{SearchOptions, WorldGenerator};
    use shpd_seedfinder_core::seed::DungeonSeed;
    use shpd_seedfinder_core::wire::{WireError, decode_scout_world};

    use super::*;

    /// The request bytes a frontend sends for a query: its canonical JSON
    /// document.
    fn query_request(query: &SearchQuery) -> Vec<u8> {
        shpd_seedfinder_core::json_query::encode(query)
            .to_string()
            .into_bytes()
    }

    struct MatchingGenerator;
    impl WorldGenerator for MatchingGenerator {
        fn generate(&self, seed: DungeonSeed, _max_depth: u8) -> GeneratedWorld {
            matching_world(seed)
        }
    }
    struct PanickingGenerator;
    impl WorldGenerator for PanickingGenerator {
        fn generate(&self, _seed: DungeonSeed, _max_depth: u8) -> GeneratedWorld {
            panic!("intentional worker failure")
        }
    }
    #[derive(Default)]
    struct RecordingScoutGenerator {
        calls: AtomicUsize,
        inputs: Mutex<Vec<(DungeonSeed, u8)>>,
    }
    impl WorldGenerator for RecordingScoutGenerator {
        fn generate(&self, seed: DungeonSeed, max_depth: u8) -> GeneratedWorld {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.inputs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((seed, max_depth));
            matching_world(seed)
        }
    }
    #[derive(Default)]
    struct Gate {
        open: Mutex<bool>,
        changed: Condvar,
    }
    impl Gate {
        fn open(&self) {
            *self
                .open
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
            self.changed.notify_all();
        }
        fn wait(&self) {
            let guard = self
                .open
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            drop(
                self.changed
                    .wait_while(guard, |open| !*open)
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            );
        }
    }
    struct GatedGenerator {
        entered: Arc<Gate>,
        release: Arc<Gate>,
    }
    impl WorldGenerator for GatedGenerator {
        fn generate(&self, seed: DungeonSeed, _max_depth: u8) -> GeneratedWorld {
            self.entered.open();
            self.release.wait();
            matching_world(seed)
        }
    }
    fn matching_world(seed: DungeonSeed) -> GeneratedWorld {
        GeneratedWorld {
            feelings: Vec::new(),
            quests: shpd_seedfinder_core::quests::QuestSummary::default(),
            seed,
            items: vec![WorldItem {
                item: ItemId::WandFrost,
                upgrade: 2,
                effect: None,
                cursed: false,
                depth: 1,
                source: ItemSource::Heap,
                accessibility: Accessibility::Independent,
                secret: true,
            }],
            ring_gems: RingGems::UNSHUFFLED,
        }
    }
    fn query() -> SearchQuery {
        SearchQuery {
            requirements: vec![Requirement {
                kind: ItemKind::Wand,
                weapon_category: None,
                item: Some(ItemId::WandFrost),
                tier: TierRequirement::Any,
                upgrade: UpgradeRequirement::Exact(2),
                effect: EffectRequirement::Any,
                source: None,
                identity_group: None,
                max_depth: None,
                require_uncursed: false,
                alternative_group: None,
                level_sum: None,
            }],
            max_depth: 24,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
        }
    }

    #[test]
    fn production_generator_cache_is_keyed_by_challenge_mask() {
        let normal = canonical_generator(Challenges::NONE);
        let normal_again = canonical_generator(Challenges::NONE);
        let forbidden = canonical_generator(Challenges::NO_SCROLLS);
        assert!(Arc::ptr_eq(&normal, &normal_again));
        assert!(!Arc::ptr_eq(&normal, &forbidden));
    }
    fn options(end: u64, max: usize) -> SearchOptions {
        SearchOptions {
            start_seed: 0,
            end_seed_exclusive: end,
            workers: NonZeroUsize::MIN,
            chunk_size: NonZeroUsize::new(4).unwrap(),
            max_results: NonZeroUsize::new(max).unwrap(),
        }
    }
    fn wait(session: &NativeSession) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !session.is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
    fn count(packet: &[u8]) -> usize {
        assert_eq!(&packet[..4], b"SSR1");
        usize::from(u16::from_be_bytes([packet[4], packet[5]]))
    }

    #[test]
    fn production_search_starts_are_full_cycle_and_widely_spaced() {
        let next = AtomicU64::new(TOTAL_SEEDS - 1);
        let first = claim_production_search_start(&next);
        let second = claim_production_search_start(&next);
        let third = claim_production_search_start(&next);

        assert_eq!(first, TOTAL_SEEDS - 1);
        assert_eq!(second, advance_production_search_start(first));
        assert_eq!(third, advance_production_search_start(second));
        assert_ne!(first, second);
        assert_ne!(second, third);
        assert!(second < TOTAL_SEEDS && third < TOTAL_SEEDS);
    }

    #[test]
    fn polling_drains_terminal_results_before_reporting_completion() {
        let generator = Arc::new(MatchingGenerator);
        let session = NativeSession::start(&generator, query(), options(16, 32)).unwrap();
        wait(&session);
        let probability_bits =
            i64::from_ne_bytes(session.match_probability.to_bits().to_ne_bytes());
        assert_eq!(
            session.status(),
            [STATE_RUNNING, 16, 16, ERROR_NONE, probability_bits]
        );
        let mut drained = 0;
        while drained < 16 {
            let packet = session.poll(3).unwrap();
            let amount = count(&packet);
            assert!(amount <= 3);
            drained += amount;
        }
        assert_eq!(session.poll(3).unwrap(), b"SSR1\0\0");
        assert_eq!(
            session.status(),
            [STATE_COMPLETED, 16, 16, ERROR_NONE, probability_bits]
        );
    }
    #[test]
    fn cancellation_is_cooperative_and_has_no_error_code() {
        let entered = Arc::new(Gate::default());
        let release = Arc::new(Gate::default());
        let generator = Arc::new(GatedGenerator {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        });
        let session = NativeSession::start(&generator, query(), options(64, 64)).unwrap();
        entered.wait();
        session.cancel();
        session.cancel();
        release.open();
        wait(&session);
        let probability_bits =
            i64::from_ne_bytes(session.match_probability.to_bits().to_ne_bytes());
        assert_eq!(
            session.status(),
            [STATE_CANCELLED, 0, 64, ERROR_NONE, probability_bits]
        );
        assert_eq!(session.poll(8).unwrap(), b"SSR1\0\0");
    }
    #[test]
    fn a_worker_panic_has_a_stable_failure_code_and_one_diagnostic() {
        let generator = Arc::new(PanickingGenerator);
        let session = NativeSession::start(&generator, query(), options(4, 4)).unwrap();
        wait(&session);
        let status = session.status();
        let probability_bits =
            i64::from_ne_bytes(session.match_probability.to_bits().to_ne_bytes());
        assert_eq!(
            status,
            [
                STATE_FAILED,
                0,
                4,
                ERROR_SEARCH_WORKER_FAILED,
                probability_bits
            ]
        );
        let diagnostic = session.take_failure_diagnostic().unwrap();
        assert!(diagnostic.contains("0 (AAA-AAA-AAA)..=3 (AAA-AAA-AAD)"));
        assert!(diagnostic.contains("intentional worker failure"));
        assert!(session.take_failure_diagnostic().is_none());
    }
    #[test]
    fn typed_draining_matches_wire_polling_semantics() {
        let generator = Arc::new(MatchingGenerator);
        let session = NativeSession::start(&generator, query(), options(8, 8)).unwrap();
        wait(&session);
        let mut worlds = Vec::new();
        while worlds.len() < 8 {
            let drained = session.drain_worlds(3);
            assert!(drained.len() <= 3);
            worlds.extend(drained);
        }
        for world in &worlds {
            assert_eq!(world, &matching_world(world.seed));
        }
        assert!(session.drain_worlds(3).is_empty());
        assert_eq!(session.status()[0], STATE_COMPLETED);
    }

    #[test]
    fn filtering_reverifies_seeds_against_the_full_query() {
        // Seed AAA-AAA-AAF's canonical world is the fixture used by the scout
        // tests below; filter it against a query built from one of its own
        // items so the test does not depend on rare drops.
        let seed = DungeonSeed::from_code("AAA-AAA-AAF").unwrap();
        let world = production_scout_world(seed, Challenges::NONE).unwrap();
        let known = world
            .items
            .iter()
            .find(|entry| {
                shpd_seedfinder_core::catalog::item(entry.item).kind
                    != shpd_seedfinder_core::catalog::ItemKind::Trinket
            })
            .cloned()
            .unwrap();
        let definition = shpd_seedfinder_core::catalog::item(known.item);
        let satisfiable = SearchQuery {
            requirements: vec![Requirement {
                kind: definition.kind,
                weapon_category: None,
                item: Some(known.item),
                tier: TierRequirement::Any,
                upgrade: UpgradeRequirement::Any,
                effect: EffectRequirement::Any,
                source: None,
                identity_group: None,
                max_depth: None,
                require_uncursed: false,
                alternative_group: None,
                level_sum: None,
            }],
            max_depth: known.depth,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
        };

        let matches = filter_matching_seeds(&satisfiable, &[seed.value()]).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].seed, seed);

        // A stricter variant of the same query rejects the seed: no copy of
        // this item in the canonical world carries the missing upgrade level.
        let missing_upgrade = (1..=definition.kind.maximum_search_upgrade())
            .find(|&candidate| {
                world
                    .items
                    .iter()
                    .filter(|item| item.item == known.item)
                    .all(|item| item.upgrade != candidate)
            })
            .expect("the fixture world does not use every upgrade level");
        let mut rejecting = satisfiable.clone();
        rejecting.requirements[0].upgrade = UpgradeRequirement::Exact(missing_upgrade);
        assert!(
            filter_matching_seeds(&rejecting, &[seed.value()])
                .unwrap()
                .is_empty()
        );

        assert_eq!(
            filter_matching_seeds(&satisfiable, &[TOTAL_SEEDS]).unwrap_err(),
            shpd_seedfinder_core::search::SearchError::InvalidSeedRange
        );
        assert!(filter_matching_seeds(&satisfiable, &[]).unwrap().is_empty());
    }

    #[test]
    fn filter_packet_round_trips_ssr1_and_preserves_input_order() {
        let seed = DungeonSeed::from_code("AAA-AAA-AAF").unwrap();
        let world = production_scout_world(seed, Challenges::NONE).unwrap();
        let known = world.items.first().cloned().unwrap();
        let definition = shpd_seedfinder_core::catalog::item(known.item);
        let request = format!(
            r#"{{"max_depth":{},"requirements":[{{"item":"{}"}}]}}"#,
            known.depth, definition.stable_id
        );

        let packet = production_filter_packet(request.as_bytes(), &[seed.value()]).unwrap();
        assert_eq!(&packet[..4], b"SSR1");
        assert_eq!(u16::from_be_bytes([packet[4], packet[5]]), 1);
        assert_eq!(
            production_filter_packet(request.as_bytes(), &[]).unwrap(),
            b"SSR1\0\0"
        );
        assert!(matches!(
            production_filter_packet(b"bad!????????", &[seed.value()]),
            Err(FilterPacketError::Request(WireError::InvalidQueryDocument(
                _
            )))
        ));
    }

    #[test]
    fn unsatisfiable_resumed_session_hands_back_its_whole_arc() {
        // A +4 ring cannot exist by depth sixteen. The session completes
        // instantly without scanning, and the hint must return the entire
        // requested arc so a later satisfiable continuation still covers it.
        let impossible = SearchQuery {
            requirements: vec![Requirement {
                kind: shpd_seedfinder_core::catalog::ItemKind::Ring,
                weapon_category: None,
                item: None,
                tier: TierRequirement::Any,
                upgrade: UpgradeRequirement::Exact(4),
                effect: EffectRequirement::Any,
                require_uncursed: false,
                source: None,
                identity_group: None,
                max_depth: None,
                alternative_group: None,
                level_sum: None,
            }],
            max_depth: 16,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
        };
        let session = NativeSession::production_resumed(impossible, 42, 1_000, None).unwrap();
        wait(&session);
        assert_eq!(session.status()[0], STATE_COMPLETED);
        assert_eq!(session.resume_hint(), [42, 1_000]);
    }

    #[test]
    fn resumed_sessions_continue_a_stopped_traversal_without_losing_seeds() {
        let generator = Arc::new(MatchingGenerator);
        let session = NativeSession::start(&generator, query(), options(64, 16)).unwrap();
        wait(&session);
        // The result cap stopped the session early; the hint points at the
        // first seed whose outcome was not delivered.
        let [resume_from, remaining] = session.resume_hint();
        let resume_from = u64::try_from(resume_from).unwrap();
        let remaining = u64::try_from(remaining).unwrap();
        assert_eq!(resume_from, 16);
        assert_eq!(remaining, 48);
        let mut seen: Vec<u64> = session
            .drain_worlds(64)
            .into_iter()
            .map(|world| world.seed.value())
            .collect();

        let resumed = NativeSession::start(
            &generator,
            query(),
            SearchOptions {
                start_seed: 0,
                end_seed_exclusive: 64,
                workers: NonZeroUsize::MIN,
                chunk_size: NonZeroUsize::new(4).unwrap(),
                max_results: NonZeroUsize::new(64).unwrap(),
            },
        )
        .unwrap();
        drop(resumed);
        // Production-shaped resume path: scan only the remaining arc.
        let continued = {
            let options = SearchOptions {
                start_seed: 0,
                end_seed_exclusive: 64,
                workers: NonZeroUsize::MIN,
                chunk_size: NonZeroUsize::new(4).unwrap(),
                max_results: NonZeroUsize::new(64).unwrap(),
            };
            let handle = shpd_seedfinder_core::search::spawn_partial_streaming_search(
                &generator,
                query(),
                options,
                resume_from,
                remaining,
            )
            .unwrap();
            while !handle.is_finished() {
                std::thread::yield_now();
            }
            handle.drain_results(64)
        };
        seen.extend(continued.into_iter().map(|world| world.seed.value()));
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, (0..64).collect::<Vec<_>>());
    }

    /// One wildcard requirement of a kind: the shape the start decision reads.
    fn kind_requirement(kind: ItemKind) -> Requirement {
        Requirement {
            kind,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
        }
    }

    fn kind_query(kind: ItemKind) -> SearchQuery {
        SearchQuery {
            requirements: vec![kind_requirement(kind)],
            max_depth: 24,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
        }
    }

    #[test]
    fn starting_refines_an_extension_of_the_target_without_asking() {
        let base = kind_query(ItemKind::Ring);
        let mut extended = base.clone();
        extended.requirements.push(Requirement {
            upgrade: UpgradeRequirement::AtLeast(2),
            ..kind_requirement(ItemKind::Weapon)
        });

        // Adding a requirement after a concluded run refines it implicitly.
        assert_eq!(
            decide_start(&extended, Some(&base), false, true, None),
            StartDecision::TargetRefine,
            "an extending query must reuse the Target"
        );

        // Starting again with the query unchanged continues the session: the
        // filter keeps every seed and the scan picks up where it stopped. This
        // is the stop-then-start-again case, which must never wipe results.
        assert_eq!(
            decide_start(&base, Some(&base), false, true, None),
            StartDecision::TargetRefine,
            "an unchanged query must continue the previous run"
        );
        assert_eq!(
            decide_start(&extended, Some(&extended), false, true, None),
            StartDecision::TargetRefine
        );

        // A populated Target refines even with its range fully covered: the
        // filter half of the refine still has the whole Target Set to keep.
        assert_eq!(
            decide_start(&extended, Some(&base), false, false, None),
            StartDecision::TargetRefine
        );

        // Clearing the results drops the Target, so even an extending query
        // anchors a fresh session.
        assert_eq!(
            decide_start(&extended, None, false, true, None),
            StartDecision::Anchor
        );
    }

    #[test]
    fn queries_sharing_an_item_filter_the_target_set_without_scanning() {
        let base = kind_query(ItemKind::Ring);

        // A narrower scope breaks the continuation rule but still names a
        // ring, so the full Target Set is filtered instead of rescanned.
        let mut deeper = base.clone();
        deeper.max_depth = 9;
        assert!(!deeper.continues(&base));
        assert_eq!(
            decide_start(&deeper, Some(&base), false, true, None),
            StartDecision::TargetFilter
        );

        // Dropping back to fewer requirements is a filter too — the base is
        // the full Target Set, so loosening brings seeds back.
        let mut extended = base.clone();
        extended
            .requirements
            .push(kind_requirement(ItemKind::Weapon));
        assert_eq!(
            decide_start(&base, Some(&extended), false, true, None),
            StartDecision::TargetFilter
        );

        // An unrelated kind shares nothing and scans detached.
        assert_eq!(
            decide_start(&kind_query(ItemKind::Armor), Some(&base), false, true, None),
            StartDecision::Detached
        );
    }

    #[test]
    fn unrelated_queries_continue_only_the_detached_thread() {
        let target = kind_query(ItemKind::Ring);
        let detached = kind_query(ItemKind::Armor);

        // First unrelated query: a fresh detached scan.
        assert_eq!(
            decide_start(&detached, Some(&target), false, true, None),
            StartDecision::Detached
        );

        // Extending the detached run continues it instead of rescanning.
        let mut narrowed = detached.clone();
        narrowed.requirements.push(Requirement {
            upgrade: UpgradeRequirement::AtLeast(2),
            ..kind_requirement(ItemKind::Armor)
        });
        assert_eq!(
            decide_start(&narrowed, Some(&target), false, true, Some(&detached)),
            StartDecision::ContinueDetached
        );

        // But never when the last concluded run was not detached (or failed):
        // without a detached base, an unrelated query rescans.
        assert_eq!(
            decide_start(&narrowed, Some(&target), false, true, None),
            StartDecision::Detached
        );

        // And the Target always wins: a query continuing the Target refines
        // it even when it would also continue the detached run.
        assert_eq!(
            decide_start(&target, Some(&target), false, true, Some(&target)),
            StartDecision::TargetRefine
        );
    }

    #[test]
    fn an_empty_target_set_resumes_a_continuation_and_reanchors_otherwise() {
        let target = kind_query(ItemKind::Ring);

        // A continuing query still resumes the uncovered remainder.
        assert_eq!(
            decide_start(&target, Some(&target), true, true, None),
            StartDecision::TargetRefine
        );
        // With nothing left to scan either, the search re-anchors.
        assert_eq!(
            decide_start(&target, Some(&target), true, false, None),
            StartDecision::Anchor
        );
        // Any other query re-anchors: an empty set holds nothing worth
        // preserving, even for a query that shares the ring kind.
        let mut deeper = target.clone();
        deeper.max_depth = 9;
        assert_eq!(
            decide_start(&deeper, Some(&target), true, true, None),
            StartDecision::Anchor
        );
    }

    #[test]
    fn start_decisions_travel_as_query_documents_and_lowercase_names() {
        let target = kind_query(ItemKind::Ring);
        let detached = kind_query(ItemKind::Armor);
        let mut narrowed = detached.clone();
        narrowed.requirements.push(Requirement {
            upgrade: UpgradeRequirement::AtLeast(2),
            ..kind_requirement(ItemKind::Armor)
        });
        let mut deeper = target.clone();
        deeper.max_depth = 9;
        let packet = query_request;

        for (candidate, base, decision, name) in [
            (&target, None, StartDecision::TargetRefine, "target-refine"),
            (&deeper, None, StartDecision::TargetFilter, "target-filter"),
            (&detached, None, StartDecision::Detached, "detached"),
            (
                &narrowed,
                Some(&detached),
                StartDecision::ContinueDetached,
                "continue-detached",
            ),
        ] {
            let reported = decide_start_packets(
                &packet(candidate),
                Some(&packet(&target)),
                false,
                true,
                base.map(&packet).as_deref(),
            )
            .unwrap();
            assert_eq!(reported, decision);
            assert_eq!(reported.as_str(), name);
        }
        assert_eq!(
            decide_start_packets(&packet(&target), None, false, true, None).unwrap(),
            StartDecision::Anchor
        );
        assert_eq!(StartDecision::Anchor.as_str(), "anchor");

        // Any undecodable packet is reported rather than silently ignored.
        assert!(decide_start_packets(b"bad", None, false, true, None).is_err());
        assert!(decide_start_packets(&packet(&target), Some(b"bad"), false, true, None).is_err());
        assert!(decide_start_packets(&packet(&target), None, false, true, Some(b"bad")).is_err());
    }

    #[test]
    fn typed_production_scout_matches_the_packet_scout() {
        let seed = DungeonSeed::from_code("AAA-AAA-AAF").unwrap();
        let world = production_scout_world(seed, Challenges::NONE).unwrap();
        let packet = production_scout_packet(b"SSQ2\x00\x00AAA-AAA-AAF").unwrap();

        assert_eq!(world, decode_scout_world(&packet).unwrap());
        assert_eq!(&packet[..4], b"SSC5");
        assert!(world.items.iter().any(|entry| {
            shpd_seedfinder_core::catalog::item(entry.item).kind
                == shpd_seedfinder_core::catalog::ItemKind::Artifact
        }));
        assert_eq!(
            world
                .items
                .iter()
                .filter(|entry| {
                    shpd_seedfinder_core::catalog::item(entry.item).kind
                        == shpd_seedfinder_core::catalog::ItemKind::Trinket
                })
                .count(),
            4
        );

        let artifact_index = world
            .items
            .iter()
            .position(|entry| {
                shpd_seedfinder_core::catalog::item(entry.item).kind == ItemKind::Artifact
            })
            .unwrap();
        let artifact = &world.items[artifact_index];
        let mut query = kind_query(ItemKind::Artifact);
        query.requirements[0].item = Some(artifact.item);
        let matches =
            production_scout_matches(b"SSQ2\x00\x00AAA-AAA-AAF", &query_request(&query)).unwrap();
        assert_eq!(matches.matched.len(), world.items.len());
        assert_eq!(matches.matched_requirements, 1);
        assert!(matches.matched[artifact_index]);

        let challenged = production_scout_world(seed, Challenges::new(0x68).unwrap()).unwrap();
        assert_eq!(challenged.seed, world.seed);
        assert_ne!(challenged, world);
    }

    #[test]
    fn registry_close_removes_first_and_is_idempotent() {
        let generator = Arc::new(MatchingGenerator);
        let session = NativeSession::start(&generator, query(), options(4, 4)).unwrap();
        let registry = SessionRegistry::new();
        let handle = registry.insert(session);
        assert_ne!(handle, 0);
        assert_eq!(registry.len(), 1);
        assert!(registry.get(handle).is_some());
        assert!(close_session(&registry, handle));
        assert_eq!(registry.len(), 0);
        assert!(!close_session(&registry, handle));
    }
    #[test]
    fn scout_helper_validates_then_generates_one_depth_twenty_four_world() {
        let generator = RecordingScoutGenerator::default();
        let packet = scout_seed_packet(&generator, b"abc-def-ghi").unwrap();
        let seed = DungeonSeed::from_code("ABC-DEF-GHI").unwrap();
        assert_eq!(generator.calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            *generator
                .inputs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![(seed, 24)]
        );
        assert_eq!(decode_scout_world(&packet).unwrap(), matching_world(seed));
    }
    #[test]
    fn scout_helper_rejects_bad_input_without_running_generation() {
        let generator = RecordingScoutGenerator::default();
        assert_eq!(
            scout_seed_packet(&generator, b"AAA-AAA-AA0"),
            Err(ScoutPacketError::Request(WireError::InvalidSeedCode))
        );
        assert_eq!(
            scout_seed_packet(&generator, &[0xff]),
            Err(ScoutPacketError::Request(WireError::InvalidUtf8))
        );
        assert_eq!(generator.calls.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn production_scout_uses_the_request_challenge_mask() {
        let normal = production_scout_packet(b"SSQ2\x00\x00AAA-AAA-AAF").unwrap();
        let challenged = production_scout_packet(b"SSQ2\x68\x00AAA-AAA-AAF").unwrap();
        let normal = decode_scout_world(&normal).unwrap();
        let challenged = decode_scout_world(&challenged).unwrap();

        assert_eq!(normal.seed, challenged.seed);
        assert_ne!(normal, challenged);
    }
    #[test]
    fn protected_scout_helper_contains_generator_panics() {
        assert_eq!(
            protected_scout_seed_packet(&PanickingGenerator, b"AAA-AAA-AAA"),
            Err(ScoutCallError::Panicked)
        );
    }
}
