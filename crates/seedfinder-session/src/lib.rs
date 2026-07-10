//! Frontend-neutral search sessions, registry, and scout packet generation.

use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use shpd_seedfinder_core::main_world::CanonicalMainWorldGenerator;
use shpd_seedfinder_core::query::SearchQuery;
use shpd_seedfinder_core::search::{
    SearchError, SearchOptions, StreamingSearchHandle, StreamingSearchState, WorldGenerator,
    spawn_rotated_streaming_search, spawn_streaming_search,
};
use shpd_seedfinder_core::seed::{DungeonSeed, TOTAL_SEEDS};
use shpd_seedfinder_core::wire::{
    WireError, decode_query, decode_scout_seed, encode_results, encode_scout_world,
};

pub const STATE_RUNNING: i64 = 0;
pub const STATE_COMPLETED: i64 = 1;
pub const STATE_CANCELLED: i64 = 2;
pub const STATE_FAILED: i64 = 3;
pub const ERROR_NONE: i64 = 0;
pub const ERROR_SEARCH_WORKER_FAILED: i64 = 2_001;
pub const SEARCH_CHUNK_SIZE: usize = 4;
pub const MAX_ACCEPTED_RESULTS: usize = 1_024;

// Approximately one golden-ratio turn of the seed circle. TOTAL_SEEDS only
// has 2 and 13 as prime factors; this odd, non-multiple-of-13 stride is
// therefore coprime and visits every possible start before repeating.
const PRODUCTION_SEARCH_START_STRIDE: u64 = 3_355_211_884_971;

static REGISTRY: OnceLock<SessionRegistry> = OnceLock::new();
static CANONICAL_GENERATOR: OnceLock<Arc<CanonicalMainWorldGenerator>> = OnceLock::new();
static NEXT_PRODUCTION_SEARCH_START: OnceLock<AtomicU64> = OnceLock::new();

#[must_use]
pub fn registry() -> &'static SessionRegistry {
    REGISTRY.get_or_init(SessionRegistry::new)
}

fn canonical_generator() -> &'static Arc<CanonicalMainWorldGenerator> {
    CANONICAL_GENERATOR.get_or_init(|| Arc::new(CanonicalMainWorldGenerator))
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoutPacketError {
    Request(WireError),
    Response(WireError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoutCallError {
    Packet(ScoutPacketError),
    Panicked,
}

/// Validates a scout request, generates a depth-24 world, and encodes `SSC1`.
///
/// # Errors
///
/// Returns a request or response wire error.
pub fn scout_seed_packet<G: WorldGenerator + ?Sized>(
    generator: &G,
    request: &[u8],
) -> Result<Vec<u8>, ScoutPacketError> {
    let seed = decode_scout_seed(request).map_err(ScoutPacketError::Request)?;
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

/// Scouts one world with the canonical production generator.
///
/// # Errors
///
/// Returns a packet error or a contained generation panic.
pub fn production_scout_packet(request: &[u8]) -> Result<Vec<u8>, ScoutCallError> {
    protected_scout_seed_packet(canonical_generator().as_ref(), request)
}

pub struct NativeSession {
    search: StreamingSearchHandle,
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
        spawn_streaming_search(generator, query, options).map(|search| Self {
            search,
            diagnostic_claimed: AtomicBool::new(false),
        })
    }

    /// Starts the canonical full-range production search.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError`] if workers cannot be spawned.
    pub fn production(query: SearchQuery) -> Result<Self, SearchError> {
        let options = SearchOptions {
            start_seed: 0,
            end_seed_exclusive: TOTAL_SEEDS,
            workers: SearchOptions::available_parallelism(),
            chunk_size: NonZeroUsize::new(SEARCH_CHUNK_SIZE).unwrap_or(NonZeroUsize::MIN),
            max_results: NonZeroUsize::new(MAX_ACCEPTED_RESULTS).unwrap_or(NonZeroUsize::MIN),
        };
        spawn_rotated_streaming_search(
            canonical_generator(),
            query,
            options,
            production_search_start(),
        )
        .map(|search| Self {
            search,
            diagnostic_claimed: AtomicBool::new(false),
        })
    }

    /// Decodes `SSF2` and starts a canonical production search.
    ///
    /// # Errors
    ///
    /// Distinguishes invalid wire requests from worker spawn failures.
    pub fn production_from_packet(request: &[u8]) -> Result<Self, StartSessionError> {
        let query = decode_query(request).map_err(StartSessionError::Request)?;
        Self::production(query).map_err(StartSessionError::Spawn)
    }

    /// Drains at most `maximum` matches into an `SSR1` packet.
    ///
    /// # Errors
    ///
    /// Returns a wire error when the result count cannot be encoded.
    pub fn poll(&self, maximum: usize) -> Result<Vec<u8>, WireError> {
        encode_results(&self.search.drain_results(maximum))
    }

    pub fn cancel(&self) {
        self.search.cancel();
    }

    #[must_use]
    pub fn status(&self) -> [i64; 4] {
        let (state, error) = match self.search.state() {
            StreamingSearchState::Running => (STATE_RUNNING, ERROR_NONE),
            StreamingSearchState::Completed => (STATE_COMPLETED, ERROR_NONE),
            StreamingSearchState::Cancelled => (STATE_CANCELLED, ERROR_NONE),
            StreamingSearchState::Failed => (STATE_FAILED, ERROR_SEARCH_WORKER_FAILED),
        };
        [
            state,
            i64::try_from(self.search.tested()).unwrap_or(i64::MAX),
            i64::try_from(self.search.total()).unwrap_or(i64::MAX),
            error,
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
    use shpd_seedfinder_core::query::{Requirement, SearchQuery, UpgradeRequirement};
    use shpd_seedfinder_core::search::{SearchOptions, WorldGenerator};
    use shpd_seedfinder_core::seed::DungeonSeed;
    use shpd_seedfinder_core::wire::{WireError, decode_scout_world};

    use super::*;

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
            seed,
            items: vec![WorldItem {
                item: ItemId::WandFrost,
                upgrade: 2,
                effect: None,
                cursed: false,
                depth: 1,
                source: ItemSource::Heap,
                accessibility: Accessibility::Independent,
            }],
        }
    }
    fn query() -> SearchQuery {
        SearchQuery {
            requirements: vec![Requirement {
                kind: ItemKind::Wand,
                item: Some(ItemId::WandFrost),
                upgrade: UpgradeRequirement::Exact(2),
                effect: None,
                source: None,
                identity_group: None,
            }],
            max_depth: 24,
            require_blacksmith: false,
            fast_mode: false,
        }
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
        assert_eq!(session.status(), [STATE_RUNNING, 16, 16, ERROR_NONE]);
        let mut drained = 0;
        while drained < 16 {
            let packet = session.poll(3).unwrap();
            let amount = count(&packet);
            assert!(amount <= 3);
            drained += amount;
        }
        assert_eq!(session.poll(3).unwrap(), b"SSR1\0\0");
        assert_eq!(session.status(), [STATE_COMPLETED, 16, 16, ERROR_NONE]);
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
        assert_eq!(session.status(), [STATE_CANCELLED, 0, 64, ERROR_NONE]);
        assert_eq!(session.poll(8).unwrap(), b"SSR1\0\0");
    }
    #[test]
    fn a_worker_panic_has_a_stable_failure_code_and_one_diagnostic() {
        let generator = Arc::new(PanickingGenerator);
        let session = NativeSession::start(&generator, query(), options(4, 4)).unwrap();
        wait(&session);
        let status = session.status();
        assert_eq!(status, [STATE_FAILED, 0, 4, ERROR_SEARCH_WORKER_FAILED]);
        let diagnostic = session.take_failure_diagnostic().unwrap();
        assert!(diagnostic.contains("0 (AAA-AAA-AAA)..=3 (AAA-AAA-AAD)"));
        assert!(diagnostic.contains("intentional worker failure"));
        assert!(session.take_failure_diagnostic().is_none());
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
    fn protected_scout_helper_contains_generator_panics() {
        assert_eq!(
            protected_scout_seed_packet(&PanickingGenerator, b"AAA-AAA-AAA"),
            Err(ScoutCallError::Panicked)
        );
    }
}
