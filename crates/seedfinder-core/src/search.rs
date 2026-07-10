//! Deterministic, cancellable multicore traversal of seed ranges.

use std::cell::Cell;
use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::feasibility::QueryPlan;
use crate::model::{GeneratedWorld, WorldItem};
use crate::query::{QueryError, SearchQuery};
use crate::seed::{DungeonSeed, TOTAL_SEEDS};

/// Per-floor cancellation oracle consulted between floors of one seed.
///
/// Returning `false` promises that no continuation of the partial world can
/// satisfy the active query, letting the generator abandon the seed without
/// producing its remaining floors.
pub trait FloorGate: Sync {
    fn continue_after_floor(&self, completed_depth: u8, items_so_far: &[WorldItem]) -> bool;
}

/// Version-pinned world simulator used by the parallel search scheduler.
pub trait WorldGenerator: Sync {
    fn generate(&self, seed: DungeonSeed, max_depth: u8) -> GeneratedWorld;

    /// Generates an ordered batch. Implementors may override this to share
    /// setup or use SIMD while preserving one result for every input seed.
    fn generate_batch(&self, seeds: &[DungeonSeed], max_depth: u8) -> Vec<GeneratedWorld> {
        seeds
            .iter()
            .copied()
            .map(|seed| self.generate(seed, max_depth))
            .collect()
    }

    /// Generates an ordered batch under a [`FloorGate`], returning `None` for
    /// seeds the gate proved unable to match. The default ignores the gate,
    /// which is always correct because `None` is only an optimization.
    fn generate_batch_gated(
        &self,
        seeds: &[DungeonSeed],
        max_depth: u8,
        gate: &dyn FloorGate,
    ) -> Vec<Option<GeneratedWorld>> {
        let _ = gate;
        self.generate_batch(seeds, max_depth)
            .into_iter()
            .map(Some)
            .collect()
    }
}

/// Bounds and resource limits for one search.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchOptions {
    pub start_seed: u64,
    pub end_seed_exclusive: u64,
    pub workers: NonZeroUsize,
    pub chunk_size: NonZeroUsize,
    pub max_results: NonZeroUsize,
}

impl SearchOptions {
    #[must_use]
    pub fn available_parallelism() -> NonZeroUsize {
        std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN)
    }
}

/// Observable counters shared with UI/JNI polling code.
#[derive(Debug, Default)]
pub struct SearchProgress {
    tested: AtomicU64,
    cancelled: AtomicBool,
}

impl SearchProgress {
    #[must_use]
    pub fn tested(&self) -> u64 {
        self.tested.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Results and measured throughput for a completed or cancelled search.
#[derive(Clone, Debug)]
pub struct SearchOutcome {
    pub worlds: Vec<GeneratedWorld>,
    pub tested: u64,
    pub elapsed: Duration,
}

impl SearchOutcome {
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // A display-only rate does not need integer precision.
    pub fn seeds_per_second(&self) -> f64 {
        if self.elapsed.is_zero() {
            return 0.0;
        }
        self.tested as f64 / self.elapsed.as_secs_f64()
    }
}

/// Validates and searches a numeric seed interval on multiple worker threads.
/// Results are sorted by seed, independent of scheduling order.
///
/// # Errors
///
/// Returns [`SearchError`] for an invalid query or numeric interval.
///
/// # Panics
///
/// Panics if a custom [`WorldGenerator::generate_batch`] implementation
/// violates its one-output-per-input contract.
pub fn search_parallel<G: WorldGenerator>(
    generator: &G,
    query: &SearchQuery,
    options: SearchOptions,
    progress: &SearchProgress,
) -> Result<SearchOutcome, SearchError> {
    query.validate()?;
    if options.start_seed >= options.end_seed_exclusive || options.end_seed_exclusive > TOTAL_SEEDS
    {
        return Err(SearchError::InvalidSeedRange);
    }

    let started = Instant::now();
    let plan = QueryPlan::analyze(query);
    if plan.is_unsatisfiable() {
        return Ok(SearchOutcome {
            worlds: Vec::new(),
            tested: 0,
            elapsed: started.elapsed(),
        });
    }
    let generation_depth = plan.generation_depth();
    let cursor = AtomicU64::new(options.start_seed);
    let results = Mutex::new(Vec::new());
    let result_count = AtomicU64::new(0);
    let chunk_size = u64::try_from(options.chunk_size.get()).unwrap_or(1);
    let max_results = u64::try_from(options.max_results.get()).unwrap_or(u64::MAX);

    std::thread::scope(|scope| {
        for _ in 0..options.workers.get() {
            scope.spawn(|| {
                while !progress.is_cancelled() && result_count.load(Ordering::Acquire) < max_results
                {
                    let chunk_start = cursor.fetch_add(chunk_size, Ordering::Relaxed);
                    if chunk_start >= options.end_seed_exclusive {
                        break;
                    }
                    let chunk_end = chunk_start
                        .saturating_add(chunk_size)
                        .min(options.end_seed_exclusive);
                    let seeds = (chunk_start..chunk_end)
                        .map(|value| {
                            DungeonSeed::new(value).expect(
                                "a validated search interval contains only representable seeds",
                            )
                        })
                        .collect::<Vec<_>>();
                    let worlds = generator.generate_batch_gated(&seeds, generation_depth, &plan);
                    assert_eq!(
                        worlds.len(),
                        seeds.len(),
                        "WorldGenerator::generate_batch_gated must return one entry per seed"
                    );
                    let mut local_results = Vec::new();
                    let mut local_tested = 0_u64;
                    for world in worlds {
                        if progress.is_cancelled()
                            || result_count.load(Ordering::Acquire) >= max_results
                        {
                            break;
                        }
                        local_tested += 1;
                        let Some(world) = world else {
                            continue;
                        };
                        if query.matches(&world) {
                            let prior = result_count.fetch_add(1, Ordering::AcqRel);
                            if prior < max_results {
                                local_results.push(world);
                            }
                        }
                    }
                    progress.tested.fetch_add(local_tested, Ordering::Relaxed);
                    if !local_results.is_empty() {
                        results
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .extend(local_results);
                    }
                }
            });
        }
    });

    let mut worlds = results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    worlds.sort_unstable_by_key(|world| world.seed);
    worlds.truncate(options.max_results.get());
    Ok(SearchOutcome {
        worlds,
        tested: progress.tested(),
        elapsed: started.elapsed(),
    })
}

/// Starts a search on a coordinator thread and returns a cancellable handle.
/// This is the ownership shape used by the JNI layer.
pub fn spawn_search<G: WorldGenerator + Send + 'static>(
    generator: Arc<G>,
    query: SearchQuery,
    options: SearchOptions,
) -> SearchHandle {
    let progress = Arc::new(SearchProgress::default());
    let thread_progress = Arc::clone(&progress);
    let join = std::thread::spawn(move || {
        search_parallel(generator.as_ref(), &query, options, &thread_progress)
    });
    SearchHandle {
        progress,
        join: Some(join),
    }
}

/// Owned lifecycle for an asynchronous native search.
pub struct SearchHandle {
    progress: Arc<SearchProgress>,
    join: Option<std::thread::JoinHandle<Result<SearchOutcome, SearchError>>>,
}

/// Terminal state exposed by the non-blocking streaming search API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamingSearchState {
    Running,
    Completed,
    Cancelled,
    Failed,
}

/// Diagnostic retained for the first streaming worker panic.
///
/// The chunk bounds identify every seed which could have been executing when
/// the panic was raised. The panic payload is preserved when it is a string,
/// which is the shape produced by Rust assertions and generation invariants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamingSearchFailure {
    pub chunk_start: Option<u64>,
    pub chunk_end_exclusive: Option<u64>,
    pub message: String,
}

#[derive(Debug)]
struct StreamingShared {
    cursor: AtomicU64,
    end_seed_exclusive: u64,
    total: u64,
    chunk_size: u64,
    max_results: u64,
    tested: AtomicU64,
    accepted: AtomicU64,
    cancelled: AtomicBool,
    failed: AtomicBool,
    failure: Mutex<Option<StreamingSearchFailure>>,
    active_workers: AtomicUsize,
    results: Mutex<VecDeque<GeneratedWorld>>,
}

impl StreamingShared {
    fn state(&self) -> StreamingSearchState {
        if self.failed.load(Ordering::Acquire) {
            StreamingSearchState::Failed
        } else if self.active_workers.load(Ordering::Acquire) != 0 {
            StreamingSearchState::Running
        } else if self.cancelled.load(Ordering::Acquire) {
            StreamingSearchState::Cancelled
        } else if !self
            .results
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
        {
            // Android polls results before status and stops polling as soon as
            // status becomes terminal. Keep the session observable as running
            // until every match produced by the final worker has been drained.
            StreamingSearchState::Running
        } else {
            StreamingSearchState::Completed
        }
    }

    fn record_failure(&self, failure: StreamingSearchFailure) {
        let mut retained = self
            .failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if retained.is_none() {
            *retained = Some(failure);
        }
        drop(retained);
        self.failed.store(true, Ordering::Release);
        self.cancelled.store(true, Ordering::Release);
    }
}

/// A multicore search whose progress and matches can be polled without
/// blocking for completion. Dropping the handle cooperatively cancels and
/// joins every worker.
pub struct StreamingSearchHandle {
    shared: Arc<StreamingShared>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl StreamingSearchHandle {
    #[must_use]
    pub fn state(&self) -> StreamingSearchState {
        self.shared.state()
    }

    #[must_use]
    pub fn tested(&self) -> u64 {
        self.shared.tested.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn total(&self) -> u64 {
        self.shared.total
    }

    #[must_use]
    pub fn accepted(&self) -> u64 {
        self.shared.accepted.load(Ordering::Relaxed)
    }

    /// Returns the first retained worker panic, if one occurred.
    #[must_use]
    pub fn failure(&self) -> Option<StreamingSearchFailure> {
        self.shared
            .failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Drains up to `maximum` matches which have already completed. It never
    /// waits for a worker or a future result.
    pub fn drain_results(&self, maximum: usize) -> Vec<GeneratedWorld> {
        let mut queue = self
            .shared
            .results
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = maximum.min(queue.len());
        queue.drain(..count).collect()
    }

    pub fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.shared.active_workers.load(Ordering::Acquire) == 0
    }
}

impl Drop for StreamingSearchHandle {
    fn drop(&mut self) {
        self.cancel();
        for worker in self.workers.drain(..) {
            if worker.join().is_err() {
                self.shared.failed.store(true, Ordering::Release);
            }
        }
    }
}

/// Starts a non-blocking multicore traversal suitable for JNI polling.
///
/// Results within an individual claimed chunk retain numeric seed order.
/// Chunks may finish in a different order because they run concurrently.
///
/// # Errors
///
/// Returns [`SearchError`] before starting any worker for an invalid query or
/// numeric seed interval.
pub fn spawn_streaming_search<G: WorldGenerator + Send + 'static>(
    generator: &Arc<G>,
    query: SearchQuery,
    options: SearchOptions,
) -> Result<StreamingSearchHandle, SearchError> {
    query.validate()?;
    if options.start_seed >= options.end_seed_exclusive || options.end_seed_exclusive > TOTAL_SEEDS
    {
        return Err(SearchError::InvalidSeedRange);
    }

    let plan = Arc::new(QueryPlan::analyze(&query));
    let shared = Arc::new(StreamingShared {
        // An impossible query is complete before any worker claims a chunk.
        cursor: AtomicU64::new(if plan.is_unsatisfiable() {
            options.end_seed_exclusive
        } else {
            options.start_seed
        }),
        end_seed_exclusive: options.end_seed_exclusive,
        total: options.end_seed_exclusive - options.start_seed,
        chunk_size: u64::try_from(options.chunk_size.get()).unwrap_or(1),
        max_results: u64::try_from(options.max_results.get()).unwrap_or(u64::MAX),
        tested: AtomicU64::new(0),
        accepted: AtomicU64::new(0),
        cancelled: AtomicBool::new(false),
        failed: AtomicBool::new(false),
        failure: Mutex::new(None),
        active_workers: AtomicUsize::new(options.workers.get()),
        results: Mutex::new(VecDeque::new()),
    });
    let query = Arc::new(query);
    let mut workers = Vec::with_capacity(options.workers.get());

    for _ in 0..options.workers.get() {
        let worker_generator = Arc::clone(generator);
        let worker_query = Arc::clone(&query);
        let worker_plan = Arc::clone(&plan);
        let worker_shared = Arc::clone(&shared);
        workers.push(std::thread::spawn(move || {
            let active_chunk = Cell::new(None);
            let worker_result = catch_unwind(AssertUnwindSafe(|| {
                streaming_worker(
                    worker_generator.as_ref(),
                    worker_query.as_ref(),
                    worker_plan.as_ref(),
                    worker_shared.as_ref(),
                    &active_chunk,
                );
            }));
            if let Err(payload) = worker_result {
                let chunk = active_chunk.get();
                worker_shared.record_failure(StreamingSearchFailure {
                    chunk_start: chunk.map(|(start, _)| start),
                    chunk_end_exclusive: chunk.map(|(_, end)| end),
                    message: panic_payload_message(payload.as_ref()),
                });
            }
            worker_shared.active_workers.fetch_sub(1, Ordering::AcqRel);
        }));
    }

    Ok(StreamingSearchHandle { shared, workers })
}

fn streaming_worker<G: WorldGenerator>(
    generator: &G,
    query: &SearchQuery,
    plan: &QueryPlan,
    shared: &StreamingShared,
    active_chunk: &Cell<Option<(u64, u64)>>,
) {
    let generation_depth = plan.generation_depth();
    while !shared.cancelled.load(Ordering::Acquire)
        && shared.accepted.load(Ordering::Acquire) < shared.max_results
    {
        let chunk_start = shared
            .cursor
            .fetch_add(shared.chunk_size, Ordering::Relaxed);
        if chunk_start >= shared.end_seed_exclusive {
            return;
        }
        let chunk_end = chunk_start
            .saturating_add(shared.chunk_size)
            .min(shared.end_seed_exclusive);
        active_chunk.set(Some((chunk_start, chunk_end)));
        let seeds: Vec<_> = (chunk_start..chunk_end)
            .map(|value| {
                DungeonSeed::new(value)
                    .expect("a validated search interval only contains representable seeds")
            })
            .collect();
        let worlds = generator.generate_batch_gated(&seeds, generation_depth, plan);
        assert_eq!(
            worlds.len(),
            seeds.len(),
            "WorldGenerator::generate_batch_gated must return one entry per seed"
        );

        let mut local_results = Vec::new();
        let mut local_tested = 0_u64;
        for world in worlds {
            if shared.cancelled.load(Ordering::Acquire)
                || shared.accepted.load(Ordering::Acquire) >= shared.max_results
            {
                break;
            }
            local_tested += 1;
            let Some(world) = world else {
                continue;
            };
            if query.matches(&world) {
                if shared
                    .accepted
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |accepted| {
                        (accepted < shared.max_results).then_some(accepted + 1)
                    })
                    .is_ok()
                {
                    local_results.push(world);
                } else {
                    break;
                }
            }
        }
        shared.tested.fetch_add(local_tested, Ordering::Relaxed);
        if !local_results.is_empty() {
            shared
                .results
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend(local_results);
        }
        active_chunk.set(None);
    }
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

impl SearchHandle {
    #[must_use]
    pub fn progress(&self) -> &Arc<SearchProgress> {
        &self.progress
    }

    pub fn cancel(&self) {
        self.progress.cancel();
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.join
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }

    /// Waits for worker completion.
    ///
    /// # Errors
    ///
    /// Returns the search validation error, or [`SearchError::WorkerPanicked`]
    /// if the coordinator thread failed unexpectedly.
    pub fn join(mut self) -> Result<SearchOutcome, SearchError> {
        self.join
            .take()
            .ok_or(SearchError::AlreadyJoined)?
            .join()
            .map_err(|_| SearchError::WorkerPanicked)?
    }
}

impl Drop for SearchHandle {
    fn drop(&mut self) {
        self.cancel();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Search setup or worker error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchError {
    InvalidQuery(QueryError),
    InvalidSeedRange,
    AlreadyJoined,
    WorkerPanicked,
}

impl From<QueryError> for SearchError {
    fn from(error: QueryError) -> Self {
        Self::InvalidQuery(error)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::Arc;

    use crate::catalog::{ItemId, ItemKind};
    use crate::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
    use crate::query::{Requirement, SearchQuery};

    use super::{
        SearchOptions, SearchProgress, StreamingSearchState, WorldGenerator, search_parallel,
        spawn_streaming_search,
    };

    struct DivisibleGenerator;

    impl WorldGenerator for DivisibleGenerator {
        fn generate(&self, seed: crate::seed::DungeonSeed, _max_depth: u8) -> GeneratedWorld {
            let items = if seed.value() % 17 == 0 {
                vec![WorldItem {
                    item: ItemId::WandFrost,
                    upgrade: 2,
                    effect: None,
                    cursed: false,
                    depth: 1,
                    source: ItemSource::Heap,
                    accessibility: Accessibility::Independent,
                }]
            } else {
                Vec::new()
            };
            GeneratedWorld { seed, items }
        }
    }

    #[test]
    fn parallel_results_are_sorted_and_bounded() {
        let query = SearchQuery {
            requirements: vec![Requirement {
                kind: ItemKind::Wand,
                item: Some(ItemId::WandFrost),
                upgrade: crate::query::UpgradeRequirement::Exact(2),
                effect: None,
                source: None,
                identity_group: None,
            }],
            max_depth: 4,
            require_blacksmith: false,
            fast_mode: false,
        };
        let options = SearchOptions {
            start_seed: 0,
            end_seed_exclusive: 10_000,
            workers: NonZeroUsize::new(4).unwrap(),
            chunk_size: NonZeroUsize::new(31).unwrap(),
            max_results: NonZeroUsize::new(20).unwrap(),
        };
        let progress = SearchProgress::default();
        let outcome = search_parallel(&DivisibleGenerator, &query, options, &progress).unwrap();
        assert_eq!(outcome.worlds.len(), 20);
        assert!(
            outcome
                .worlds
                .windows(2)
                .all(|pair| pair[0].seed < pair[1].seed)
        );
        assert!(
            outcome
                .worlds
                .iter()
                .all(|world| world.seed.value() % 17 == 0)
        );
        assert!(outcome.tested >= 20);
    }

    #[test]
    fn streaming_search_uses_batch_hook_and_drains_without_blocking() {
        struct BatchOnlyGenerator;

        impl WorldGenerator for BatchOnlyGenerator {
            fn generate(&self, _seed: crate::seed::DungeonSeed, _max_depth: u8) -> GeneratedWorld {
                panic!("the streaming scheduler should use generate_batch")
            }

            fn generate_batch(
                &self,
                seeds: &[crate::seed::DungeonSeed],
                _max_depth: u8,
            ) -> Vec<GeneratedWorld> {
                seeds
                    .iter()
                    .copied()
                    .map(|seed| GeneratedWorld {
                        seed,
                        items: (seed.value() % 17 == 0)
                            .then_some(WorldItem {
                                item: ItemId::WandFrost,
                                upgrade: 2,
                                effect: None,
                                cursed: false,
                                depth: 1,
                                source: ItemSource::Heap,
                                accessibility: Accessibility::Independent,
                            })
                            .into_iter()
                            .collect(),
                    })
                    .collect()
            }
        }

        let query = SearchQuery {
            requirements: vec![Requirement {
                kind: ItemKind::Wand,
                item: Some(ItemId::WandFrost),
                upgrade: crate::query::UpgradeRequirement::Exact(2),
                effect: None,
                source: None,
                identity_group: None,
            }],
            max_depth: 4,
            require_blacksmith: false,
            fast_mode: false,
        };
        let options = SearchOptions {
            start_seed: 0,
            end_seed_exclusive: 1_000,
            workers: NonZeroUsize::MIN,
            chunk_size: NonZeroUsize::new(64).unwrap(),
            max_results: NonZeroUsize::new(4).unwrap(),
        };
        let generator = Arc::new(BatchOnlyGenerator);
        let handle = spawn_streaming_search(&generator, query, options).unwrap();
        let mut worlds = Vec::new();
        while !handle.is_finished() {
            worlds.extend(handle.drain_results(2));
            std::thread::yield_now();
        }
        worlds.extend(handle.drain_results(10));

        assert_eq!(handle.state(), StreamingSearchState::Completed);
        assert_eq!(handle.total(), 1_000);
        assert_eq!(handle.accepted(), 4);
        assert_eq!(worlds.len(), 4);
        assert_eq!(
            worlds
                .iter()
                .map(|world| world.seed.value())
                .collect::<Vec<_>>(),
            vec![0, 17, 34, 51]
        );
    }

    #[test]
    fn streaming_status_stays_running_until_terminal_results_are_drained() {
        let query = SearchQuery {
            requirements: vec![Requirement {
                kind: ItemKind::Wand,
                item: Some(ItemId::WandFrost),
                upgrade: crate::query::UpgradeRequirement::Exact(2),
                effect: None,
                source: None,
                identity_group: None,
            }],
            max_depth: 4,
            require_blacksmith: false,
            fast_mode: false,
        };
        let options = SearchOptions {
            start_seed: 0,
            end_seed_exclusive: 1,
            workers: NonZeroUsize::MIN,
            chunk_size: NonZeroUsize::MIN,
            max_results: NonZeroUsize::MIN,
        };
        let generator = Arc::new(DivisibleGenerator);
        let handle = spawn_streaming_search(&generator, query, options).unwrap();
        while !handle.is_finished() {
            std::thread::yield_now();
        }

        assert_eq!(handle.state(), StreamingSearchState::Running);
        assert_eq!(handle.drain_results(1).len(), 1);
        assert_eq!(handle.state(), StreamingSearchState::Completed);
    }

    #[test]
    fn streaming_failure_retains_claimed_chunk_and_panic_message() {
        struct PanicAtSix;

        impl WorldGenerator for PanicAtSix {
            fn generate(&self, seed: crate::seed::DungeonSeed, _max_depth: u8) -> GeneratedWorld {
                assert_ne!(seed.value(), 6, "fixture panic at seed six");
                GeneratedWorld {
                    seed,
                    items: Vec::new(),
                }
            }
        }

        let query = SearchQuery {
            requirements: vec![Requirement {
                kind: ItemKind::Wand,
                item: Some(ItemId::WandFrost),
                upgrade: crate::query::UpgradeRequirement::Exact(2),
                effect: None,
                source: None,
                identity_group: None,
            }],
            max_depth: 4,
            require_blacksmith: false,
            fast_mode: false,
        };
        let options = SearchOptions {
            start_seed: 0,
            end_seed_exclusive: 12,
            workers: NonZeroUsize::MIN,
            chunk_size: NonZeroUsize::new(4).unwrap(),
            max_results: NonZeroUsize::MIN,
        };
        let generator = Arc::new(PanicAtSix);
        let handle = spawn_streaming_search(&generator, query, options).unwrap();
        while !handle.is_finished() {
            std::thread::yield_now();
        }

        assert_eq!(handle.state(), StreamingSearchState::Failed);
        assert_eq!(handle.tested(), 4);
        let failure = handle.failure().unwrap();
        assert_eq!(failure.chunk_start, Some(4));
        assert_eq!(failure.chunk_end_exclusive, Some(8));
        assert!(failure.message.contains("fixture panic at seed six"));
    }
}
