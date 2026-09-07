// SPDX-License-Identifier: GPL-3.0-or-later

//! Results pane: streaming search session, live statistics, and seed list.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::glib;
use shpd_seedfinder_core::feasibility::{QueryPlan, Quest};
use shpd_seedfinder_core::model::GeneratedWorld;
use shpd_seedfinder_core::query::{SearchQuery, StartDecision, decide_start};
use shpd_seedfinder_core::search::SearchError;
use shpd_seedfinder_core::seed::DungeonSeed;
use shpd_seedfinder_session::{
    MAX_RESULTS, NativeSession, STATE_CANCELLED, STATE_COMPLETED, STATE_FAILED, STATE_RUNNING,
    filter_matching_seeds,
};

use crate::format::{duration, group_digits, search_statistics};
use crate::result_navigation;

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const DRAIN_BATCH: usize = 256;

/// Most rows the seed list ever holds (docs/search-semantics.md). The
/// collection behind the list is uncapped — every find still reaches the
/// Target Set, refine filters, and export — but appending thousands of GTK
/// rows stalls the main loop, so only the first `DISPLAY_CAP` seeds are
/// listed. Deliberately equal to the engine's per-session accept cap.
const DISPLAY_CAP: usize = MAX_RESULTS;

struct ActiveSearch {
    session: Rc<NativeSession>,
    query: SearchQuery,
    /// How this run relates to the session's Target, fixed at start; the
    /// conclusion folds the run into the Target accordingly.
    mode: StartDecision,
    matches: u64,
    last_tested: u64,
    last_tick: Instant,
    started: Instant,
    seeds_per_second: f64,
    /// Every seed already listed, so a resumed traversal's small re-scan
    /// overlap never produces a duplicate row.
    seen: HashSet<String>,
    /// `(kept, previous)` when this search refines an earlier one.
    refined: Option<(u64, u64)>,
}

/// Everything needed to continue the last concluded run: the query it ran and
/// where its traversal stopped. The engine guarantees every match in the
/// region before `resume_from` was delivered, so combining the filtered
/// result list with a scan of the `remaining` seeds loses nothing.
struct BaseRun {
    query: SearchQuery,
    resume_from: u64,
    remaining: u64,
    /// Whether the run belonged to the detached thread (an unrelated scan or
    /// its continuation) — the only kind of base a later unrelated start may
    /// continue; related starts go through the Target instead.
    detached: bool,
}

/// The session's anchor (docs/search-semantics.md): the query, every found
/// seed, and the unscanned coverage of the first concluded search — or of an
/// import, whose coverage is empty. Grown by target refines, untouched by
/// filters and detached scans, and discarded only by Clear Results. `seeds`
/// is a superset of any related run's display, which is what lets a loosened
/// query bring seeds back.
struct Target {
    query: SearchQuery,
    seeds: Vec<String>,
    resume_from: u64,
    remaining: u64,
}

/// An in-flight re-verification of previously found seeds on a worker thread.
struct PendingRefine {
    receiver: mpsc::Receiver<Result<Vec<GeneratedWorld>, SearchError>>,
    query: SearchQuery,
    mode: StartDecision,
    resume_from: u64,
    remaining: u64,
    previous_matches: u64,
    started: Instant,
}

pub struct ResultsPane {
    pub page: adw::NavigationPage,
    title: adw::WindowTitle,
    stack: gtk::Stack,
    message_page: adw::StatusPage,
    stats_line: gtk::Label,
    progress_line: gtk::Label,
    list: gtk::ListBox,
    /// Every accepted seed of the current display run, uncapped and in
    /// traversal order. This collection — not the row widgets — feeds the
    /// Target, refine filters, export, and scout navigation; `list` holds
    /// rows for its first `DISPLAY_CAP` entries only.
    seeds: RefCell<Vec<String>>,
    active: RefCell<Option<ActiveSearch>>,
    pending_refine: RefCell<Option<PendingRefine>>,
    base: RefCell<Option<BaseRun>>,
    target: RefCell<Option<Target>>,
    /// Threads every search spawns, `None` meaning every core. The session
    /// clamps whatever it is given to the host's parallelism.
    workers: Cell<Option<NonZeroUsize>>,
    toasts: adw::ToastOverlay,
    on_select: RefCell<Option<SelectHandler>>,
    on_finished: RefCell<Option<Box<dyn Fn()>>>,
    on_results_changed: RefCell<Option<Box<dyn Fn()>>>,
}

type SelectHandler = Box<dyn Fn(&str)>;

impl ResultsPane {
    #[allow(clippy::too_many_lines)] // Widget assembly is declarative and linear.
    pub fn new(toasts: &adw::ToastOverlay) -> Rc<Self> {
        let empty_page = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title("Find Seeds")
            .description(
                "Add requirements, then start a search. \
                 Matching seeds appear here as they are found.",
            )
            .build();
        let message_page = adw::StatusPage::new();

        let stats_line = caption_label();
        let progress_line = caption_label();
        let status_area = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(3)
            .margin_top(9)
            .margin_bottom(9)
            .margin_start(12)
            .margin_end(12)
            .build();
        status_area.append(&stats_line);
        status_area.append(&progress_line);

        let list = gtk::ListBox::builder()
            .css_classes(["navigation-sidebar"])
            .build();
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        let results_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        results_box.append(&status_area);
        results_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        results_box.append(&scroller);

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        stack.add_named(&empty_page, Some("empty"));
        stack.add_named(&results_box, Some("results"));
        stack.add_named(&message_page, Some("message"));

        let title = adw::WindowTitle::new("Results", "");
        let header_bar = adw::HeaderBar::builder().title_widget(&title).build();
        let export_button = gtk::Button::builder()
            .icon_name("results-export-symbolic")
            .tooltip_text("Export Results…")
            .action_name("win.export-results")
            .build();
        let import_button = gtk::Button::builder()
            .icon_name("results-import-symbolic")
            .tooltip_text("Import Results…")
            .action_name("win.import-results")
            .build();
        let clear_button = gtk::Button::builder()
            .icon_name("clear-results-symbolic")
            .tooltip_text("Clear Results")
            .action_name("win.clear-results")
            .build();
        header_bar.pack_end(&export_button);
        header_bar.pack_end(&import_button);
        header_bar.pack_end(&clear_button);
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header_bar);
        toolbar_view.set_content(Some(&stack));

        let nav_page = adw::NavigationPage::builder()
            .title("Results")
            .tag("results")
            .child(&toolbar_view)
            .build();

        let pane = Rc::new(Self {
            page: nav_page,
            title,
            stack,
            message_page,
            stats_line,
            progress_line,
            list,
            seeds: RefCell::new(Vec::new()),
            active: RefCell::new(None),
            pending_refine: RefCell::new(None),
            base: RefCell::new(None),
            target: RefCell::new(None),
            workers: Cell::new(None),
            toasts: toasts.clone(),
            on_select: RefCell::new(None),
            on_finished: RefCell::new(None),
            on_results_changed: RefCell::new(None),
        });
        pane.list.connect_row_selected({
            let pane = Rc::clone(&pane);
            move |_, row| {
                let Some(row) = row else { return };
                let seeds = pane.seeds.borrow();
                let Some(seed) = seeds.get(row.index().unsigned_abs() as usize) else {
                    return;
                };
                if let Some(handler) = pane.on_select.borrow().as_ref() {
                    handler(seed);
                }
            }
        });
        pane
    }

    /// Runs when the user selects a found seed.
    pub fn connect_select(&self, handler: impl Fn(&str) + 'static) {
        self.on_select.replace(Some(Box::new(handler)));
    }

    /// Runs once whenever a search reaches a terminal state.
    pub fn connect_finished(&self, handler: impl Fn() + 'static) {
        self.on_finished.replace(Some(Box::new(handler)));
    }

    /// Runs whenever the seed list changes (cleared by a new search, or grown
    /// by streamed matches), so dependent views can refresh positions.
    pub fn connect_results_changed(&self, handler: impl Fn() + 'static) {
        self.on_results_changed.replace(Some(Box::new(handler)));
    }

    fn notify_results_changed(&self) {
        if let Some(handler) = self.on_results_changed.borrow().as_ref() {
            handler();
        }
    }

    pub fn is_running(&self) -> bool {
        self.active.borrow().is_some() || self.pending_refine.borrow().is_some()
    }

    /// Sets how many threads the next search spawns. A running search keeps
    /// the count it started with; the new one takes effect at the next start,
    /// resumed continuations included.
    pub fn set_worker_count(&self, workers: usize) {
        self.workers.set(NonZeroUsize::new(workers));
    }

    /// How a start request for `query` is served, per docs/search-semantics.md:
    /// its relationship to the session's Target picks between refining the
    /// Target Set, filtering it, continuing the last detached scan, and a
    /// fresh scan. The choice is silent — the user only ever presses one
    /// search action.
    fn decide(&self, query: &SearchQuery) -> StartDecision {
        let target_slot = self.target.borrow();
        let target = target_slot.as_ref();
        let base_slot = self.base.borrow();
        // A failed run never leaves a base behind, so `Some` here always
        // describes a concluded run.
        let detached_base = base_slot
            .as_ref()
            .filter(|base| base.detached)
            .map(|base| &base.query);
        decide_start(
            query,
            target.map(|target| &target.query),
            target.is_none_or(|target| target.seeds.is_empty()),
            target.is_some_and(|target| target.remaining > 0),
            detached_base,
        )
    }

    /// Whether there is anything for "Clear Results" to discard: listed seeds,
    /// a finished-run message, or the Target a later search could refine.
    pub fn can_clear(&self) -> bool {
        !self.is_running()
            && (!self.seeds.borrow().is_empty()
                || self.base.borrow().is_some()
                || self.target.borrow().is_some()
                || self
                    .stack
                    .visible_child_name()
                    .is_some_and(|name| name != "empty"))
    }

    /// Empties the seed list along with the Target behind it — the Target
    /// Query, the Target Set, and the coverage a later start would otherwise
    /// refine or resume — so the next search anchors a new session from
    /// scratch. This is the only action that discards the Target. Callers
    /// must ensure no search is running.
    pub fn clear(&self) {
        self.base.replace(None);
        self.target.replace(None);
        self.seeds.borrow_mut().clear();
        self.list.remove_all();
        self.progress_line.set_visible(false);
        self.stats_line.set_label("");
        self.title.set_subtitle("");
        self.stack.set_visible_child_name("empty");
        self.notify_results_changed();
    }

    /// 0-based position of `seed` among the found seeds with the total count,
    /// or `None` when it is not a search result.
    pub fn position_of(&self, seed: &str) -> Option<(usize, usize)> {
        result_navigation::position(&self.seeds.borrow(), seed)
    }

    /// Moves the selection `delta` rows from the row holding `seed`, clamped
    /// to the list; selecting a row scouts it through the select handler.
    /// Returns whether the selection moved.
    pub fn select_step(&self, seed: &str, delta: i64) -> bool {
        let Some(target) = result_navigation::step(&self.seeds.borrow(), seed, delta) else {
            return false;
        };
        // Rows exist only for the first DISPLAY_CAP seeds of the collection;
        // a step landing past the listed prefix leaves the selection alone.
        let Some(row) = self
            .list
            .row_at_index(i32::try_from(target).unwrap_or(i32::MAX))
        else {
            return false;
        };
        self.list.select_row(Some(&row));
        // Keeps the selected result in view without disturbing an entry's
        // focus: J/K only fire while no editable widget is focused.
        row.grab_focus();
        true
    }

    /// The currently listed seed codes, in display order.
    #[must_use]
    pub fn seed_codes(&self) -> Vec<String> {
        self.seeds.borrow().clone()
    }

    /// Replaces the list with seeds restored from an imported results file.
    /// The import replaces the session's Target: the imported query becomes
    /// the Target Query and the imported seeds the Target Set, with no
    /// coverage — refines of an import are filter-only. Callers must ensure
    /// no search is running.
    pub fn load_imported(&self, imported: &[String], query: &SearchQuery) {
        // Imported results carry no traversal state, so the previous
        // search's refine base no longer describes the listed seeds.
        self.base.replace(None);
        self.target.replace(Some(Target {
            query: query.clone(),
            seeds: imported.to_vec(),
            resume_from: 0,
            remaining: 0,
        }));
        self.list.remove_all();
        {
            let mut seeds = self.seeds.borrow_mut();
            seeds.clear();
            seeds.extend_from_slice(imported);
        }
        for (index, seed) in imported.iter().take(DISPLAY_CAP).enumerate() {
            self.append_row(seed, index + 1);
        }
        self.progress_line.set_visible(false);
        let count = imported.len() as u64;
        self.title.set_subtitle(&match count {
            0 => String::new(),
            1 => "1 seed".to_owned(),
            count => format!("{} seeds", group_digits(count)),
        });
        self.stats_line.set_label(&format!(
            "Imported · {} seed{}",
            group_digits(count),
            if count == 1 { "" } else { "s" },
        ));
        self.stack.set_visible_child_name("results");
        self.notify_results_changed();
    }

    pub fn cancel(self: &Rc<Self>) {
        if let Some(active) = self.active.borrow().as_ref() {
            active.session.cancel();
            return;
        }
        // Abandoning the re-verification phase of a refine: its worker thread
        // result is discarded by the poll loop finding the slot empty, and
        // the previous results stay listed untouched.
        if self.pending_refine.borrow_mut().take().is_some() {
            self.progress_line.set_visible(false);
            self.restore_count_subtitle();
            self.stats_line
                .set_label("Refine stopped · previous results unchanged");
            self.finish();
        }
    }

    fn restore_count_subtitle(&self) {
        let count = self.seeds.borrow().len() as u64;
        self.title.set_subtitle(&match count {
            0 => String::new(),
            1 => "1 seed".to_owned(),
            count => format!("{} seeds", group_digits(count)),
        });
    }

    /// Runs `query`, dispatching on its relationship to the session's Target
    /// (docs/search-semantics.md): a continuation refines the Target Set and
    /// resumes its coverage, a query sharing an item filters the full set,
    /// and an unrelated query scans the whole range without touching the
    /// Target — continuing the previous detached scan when that is sound.
    /// None of this is a user decision; only Clear Results discards anything.
    pub fn start_search(self: &Rc<Self>, query: SearchQuery) {
        if self.is_running() {
            return;
        }
        match self.decide(&query) {
            mode @ (StartDecision::TargetRefine | StartDecision::TargetFilter) => {
                let Some((target_query, seeds, resume_from, remaining)) =
                    self.target.borrow().as_ref().map(|target| {
                        (
                            target.query.clone(),
                            target.seeds.clone(),
                            target.resume_from,
                            target.remaining,
                        )
                    })
                else {
                    self.start_scan(query, StartDecision::Anchor);
                    return;
                };
                // Re-assert the equal-or-superset invariant here rather than
                // trusting the decision helper: the soundness of resuming
                // depends on it. A filter never scans at all.
                let (resume_from, remaining) = if mode == StartDecision::TargetRefine {
                    if !query.continues(&target_query) {
                        self.start_scan(query, StartDecision::Detached);
                        return;
                    }
                    (resume_from, remaining)
                } else {
                    (0, 0)
                };
                self.begin_filter(query, &seeds, resume_from, remaining, mode);
            }
            StartDecision::ContinueDetached => {
                // The classic pre-Target refine, scoped to the detached
                // thread: filter the last run's listed seeds and resume its
                // remainder. The Target is untouched throughout.
                let Some((base_query, resume_from, remaining)) = self
                    .base
                    .borrow()
                    .as_ref()
                    .map(|base| (base.query.clone(), base.resume_from, base.remaining))
                else {
                    self.start_scan(query, StartDecision::Detached);
                    return;
                };
                if !query.continues(&base_query) {
                    self.start_scan(query, StartDecision::Detached);
                    return;
                }
                let seeds = self.seeds.borrow().clone();
                self.begin_filter(
                    query,
                    &seeds,
                    resume_from,
                    remaining,
                    StartDecision::ContinueDetached,
                );
            }
            mode @ (StartDecision::Anchor | StartDecision::Detached) => {
                self.start_scan(query, mode);
            }
        }
    }

    /// Starts a full-range production search; a failure to spawn is reported
    /// as a toast and leaves the pane idle. An `Anchor` run establishes the
    /// Target when it concludes; a `Detached` run leaves it untouched.
    fn start_scan(self: &Rc<Self>, query: SearchQuery, mode: StartDecision) {
        self.base.replace(None);
        if mode == StartDecision::Detached {
            // The display and the Target Set diverge here: the earlier
            // results stay held by the Target until a related search.
            self.toasts.add_toast(adw::Toast::new(
                "Unrelated query — detached search from previous results",
            ));
        }
        let session = match NativeSession::production(query.clone(), self.workers.get()) {
            Ok(session) => Rc::new(session),
            Err(error) => {
                self.toasts.add_toast(adw::Toast::new(&format!(
                    "Could not start search: {error:?}"
                )));
                self.finish();
                return;
            }
        };
        self.seeds.borrow_mut().clear();
        self.list.remove_all();
        self.notify_results_changed();
        self.stack.set_visible_child_name("results");
        self.title.set_subtitle("Searching…");
        self.stats_line.set_label("Measuring search speed…");
        self.progress_line.set_label("Starting…");
        self.progress_line.set_visible(true);
        let now = Instant::now();
        self.active.replace(Some(ActiveSearch {
            session,
            query,
            mode,
            matches: 0,
            last_tested: 0,
            last_tick: now,
            started: now,
            seeds_per_second: 0.0,
            seen: HashSet::new(),
            refined: None,
        }));

        let pane = Rc::clone(self);
        glib::timeout_add_local(POLL_INTERVAL, move || pane.tick());
    }

    /// Refines without discarding: `seed_codes` — the full Target Set for
    /// target modes, the last detached run's listed seeds otherwise — are
    /// re-verified against `query` on a worker thread, the survivors replace
    /// the display, and the scan then resumes over exactly the `remaining`
    /// seeds the base traversal never covered (zero for a target filter).
    /// Basing target modes on the full set rather than the last run's
    /// survivors is what lets a loosened requirement bring seeds back. An
    /// unchanged query keeps every seed, so this is also how a stopped
    /// search is continued.
    fn begin_filter(
        self: &Rc<Self>,
        query: SearchQuery,
        seed_codes: &[String],
        resume_from: u64,
        remaining: u64,
        mode: StartDecision,
    ) {
        let seed_values: Vec<u64> = seed_codes
            .iter()
            .filter_map(|code| DungeonSeed::from_code(code).ok())
            .map(DungeonSeed::value)
            .collect();
        let previous_matches = seed_values.len() as u64;

        let (sender, receiver) = mpsc::channel();
        let filter_query = query.clone();
        std::thread::spawn(move || {
            let _ = sender.send(filter_matching_seeds(&filter_query, &seed_values));
        });
        self.pending_refine.replace(Some(PendingRefine {
            receiver,
            query,
            mode,
            resume_from,
            remaining,
            previous_matches,
            started: Instant::now(),
        }));

        self.stack.set_visible_child_name("results");
        self.title.set_subtitle("Refining…");
        self.stats_line.set_label(&format!(
            "Re-checking {} found seed{} against the current requirements…",
            group_digits(previous_matches),
            if previous_matches == 1 { "" } else { "s" },
        ));
        self.progress_line.set_visible(false);
        let pane = Rc::clone(self);
        glib::timeout_add_local(POLL_INTERVAL, move || pane.refine_tick());
    }

    fn refine_tick(self: &Rc<Self>) -> glib::ControlFlow {
        let outcome = {
            let pending_slot = self.pending_refine.borrow();
            let Some(pending) = pending_slot.as_ref() else {
                // Cancelled while filtering; the thread result is discarded.
                return glib::ControlFlow::Break;
            };
            match pending.receiver.try_recv() {
                Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
                Ok(result) => result.map_err(|error| format!("{error:?}")),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err("the verification thread stopped unexpectedly".to_owned())
                }
            }
        };
        let Some(pending) = self.pending_refine.borrow_mut().take() else {
            return glib::ControlFlow::Break;
        };

        let kept_worlds = match outcome {
            Ok(worlds) => worlds,
            Err(message) => {
                self.restore_count_subtitle();
                self.stats_line
                    .set_label("Refine failed · previous results unchanged");
                self.toasts.add_toast(adw::Toast::new(&format!(
                    "Could not re-verify the results: {message}"
                )));
                self.finish();
                return glib::ControlFlow::Break;
            }
        };

        // Replace the list with the surviving subset, in their original order.
        self.seeds.borrow_mut().clear();
        self.list.remove_all();
        let mut seen = HashSet::new();
        {
            let mut seeds = self.seeds.borrow_mut();
            for world in &kept_worlds {
                let code = world.seed.to_code();
                // Only the first DISPLAY_CAP survivors get a row; the rest
                // stay in the collection for the Target and later refines.
                if seeds.len() < DISPLAY_CAP {
                    self.append_row(&code, seeds.len() + 1);
                }
                seen.insert(code.clone());
                seeds.push(code);
            }
        }
        let kept = kept_worlds.len() as u64;

        if pending.remaining == 0 {
            // Nothing left to scan: a target filter never scans by design,
            // and for the other modes the base traversal already covered
            // every seed. The Target is untouched either way — for a target
            // refine the survivors were already members and the coverage was
            // already exhausted.
            self.base.replace(Some(BaseRun {
                query: pending.query,
                resume_from: pending.resume_from,
                remaining: 0,
                detached: pending.mode == StartDecision::ContinueDetached,
            }));
            self.conclude_refined_filter_only(kept, pending.previous_matches, pending.mode);
            return glib::ControlFlow::Break;
        }

        let session = match NativeSession::production_resumed(
            pending.query.clone(),
            pending.resume_from,
            pending.remaining,
            self.workers.get(),
        ) {
            Ok(session) => Rc::new(session),
            Err(error) => {
                self.restore_count_subtitle();
                self.stats_line
                    .set_label("Refine failed · kept seeds are listed");
                self.toasts.add_toast(adw::Toast::new(&format!(
                    "Could not resume the search: {error:?}"
                )));
                self.finish();
                return glib::ControlFlow::Break;
            }
        };
        self.title.set_subtitle("Searching…");
        self.stats_line.set_label("Measuring search speed…");
        self.progress_line.set_label("Resuming…");
        self.progress_line.set_visible(true);
        let now = Instant::now();
        self.active.replace(Some(ActiveSearch {
            session,
            query: pending.query,
            mode: pending.mode,
            matches: kept,
            last_tested: 0,
            last_tick: now,
            started: pending.started,
            seeds_per_second: 0.0,
            seen,
            refined: Some((kept, pending.previous_matches)),
        }));
        let pane = Rc::clone(self);
        glib::timeout_add_local(POLL_INTERVAL, move || pane.tick());
        glib::ControlFlow::Break
    }

    fn conclude_refined_filter_only(&self, kept: u64, previous: u64, mode: StartDecision) {
        self.progress_line.set_visible(false);
        self.restore_count_subtitle();
        // A target filter deliberately skipped scanning; the other filter-only
        // conclusions arrive here because their coverage was already complete.
        let filtered_only = mode == StartDecision::TargetFilter;
        if kept == 0 {
            let description = if filtered_only {
                format!(
                    "None of the {} found seeds satisfy the current requirements. \
                     They stay available for the next related search.",
                    group_digits(previous)
                )
            } else {
                format!(
                    "None of the {} previous seeds satisfy the current requirements, \
                     and the previous search had already covered every seed.",
                    group_digits(previous)
                )
            };
            self.show_message("edit-find-symbolic", "No Seeds Left", &description);
        } else {
            self.stats_line.set_label(if filtered_only {
                "Completed · filtered the found seeds without scanning"
            } else {
                "Completed · every seed was already scanned"
            });
            // A filter never scans, so its collection is exactly the kept
            // seeds and this session accepted nothing new.
            let collected = usize::try_from(kept).unwrap_or(usize::MAX);
            if let Some(notice) = conclusion_toast(Some((kept, previous)), collected, 0) {
                self.toasts.add_toast(adw::Toast::new(&notice));
            }
        }
        self.finish();
    }

    /// Folds a concluded scan into the Target (docs/search-semantics.md): an
    /// anchor establishes it from its own results and coverage, a target
    /// refine grows the set with the resumed scan's new finds and advances
    /// the coverage, and a detached run leaves it exactly as it was. Failed
    /// runs never reach this. The stored set is never capped by the display.
    fn settle_target(&self, mode: StartDecision, query: &SearchQuery, concluded: &BaseRun) {
        match mode {
            StartDecision::Anchor => {
                self.target.replace(Some(Target {
                    query: query.clone(),
                    seeds: self.seeds.borrow().clone(),
                    resume_from: concluded.resume_from,
                    remaining: concluded.remaining,
                }));
            }
            StartDecision::TargetRefine => {
                let mut target_slot = self.target.borrow_mut();
                if let Some(target) = target_slot.as_mut() {
                    // The filter's survivors were already members; only the
                    // new finds join the set.
                    let new_finds: Vec<String> = {
                        let known: HashSet<&String> = target.seeds.iter().collect();
                        self.seeds
                            .borrow()
                            .iter()
                            .filter(|code| !known.contains(code))
                            .cloned()
                            .collect()
                    };
                    target.seeds.extend(new_finds);
                    target.resume_from = concluded.resume_from;
                    target.remaining = concluded.remaining;
                }
            }
            StartDecision::TargetFilter
            | StartDecision::ContinueDetached
            | StartDecision::Detached => {}
        }
    }

    fn tick(self: &Rc<Self>) -> glib::ControlFlow {
        let mut active_slot = self.active.borrow_mut();
        let Some(active) = active_slot.as_mut() else {
            return glib::ControlFlow::Break;
        };

        Self::drain_matches(self, active);

        let status = active.session.status();
        let search_state = status[0];
        let tested = status[1].max(0).unsigned_abs();
        let probability = f64::from_bits(u64::from_ne_bytes(status[4].to_ne_bytes()));

        let now = Instant::now();
        let elapsed = now.duration_since(active.last_tick).as_secs_f64();
        if elapsed > 0.0 && tested >= active.last_tested {
            let instantaneous = precise(tested - active.last_tested) / elapsed;
            active.seeds_per_second = if active.seeds_per_second > 0.0 {
                0.7 * active.seeds_per_second + 0.3 * instantaneous
            } else {
                instantaneous
            };
        }
        active.last_tested = tested;
        active.last_tick = now;

        self.title.set_subtitle(&match active.matches {
            0 => "Searching…".to_owned(),
            1 => "1 seed".to_owned(),
            count => format!("{} seeds", group_digits(count)),
        });

        if search_state == STATE_RUNNING {
            self.stats_line
                .set_label(&search_statistics(probability, active.seeds_per_second));
            self.progress_line.set_label(&format!(
                "Tested {} · elapsed {}",
                group_digits(tested),
                duration(active.started.elapsed().as_secs_f64()),
            ));
            return glib::ControlFlow::Continue;
        }

        // Catch matches that raced the terminal state transition.
        Self::drain_matches(self, active);
        let matches = active.matches;
        let refined = active.refined;
        let diagnostic = if search_state == STATE_FAILED {
            active
                .session
                .take_failure_diagnostic()
                .unwrap_or_else(|| "unknown worker failure".to_owned())
        } else {
            String::new()
        };
        // A completed or stopped traversal can be refined later: remember its
        // query and the exact position a narrower follow-up scan resumes from.
        // A failed run leaves neither a base nor a Target behind — its
        // coverage is unknown.
        let mode = active.mode;
        // The engine proves some queries unsatisfiable before generating a
        // single world; the conclusion says so instead of reporting an
        // ordinary empty result.
        let unsatisfiable = QueryPlan::analyze(&active.query).is_unsatisfiable();
        let base =
            (search_state == STATE_COMPLETED || search_state == STATE_CANCELLED).then(|| {
                let [resume_from, remaining] = active.session.resume_hint();
                BaseRun {
                    query: active.query.clone(),
                    resume_from: resume_from.max(0).unsigned_abs(),
                    remaining: remaining.max(0).unsigned_abs(),
                    detached: matches!(
                        mode,
                        StartDecision::Detached | StartDecision::ContinueDetached
                    ),
                }
            });
        if let Some(concluded) = base.as_ref() {
            self.settle_target(mode, &active.query, concluded);
        }
        self.base.replace(base);
        *active_slot = None;
        drop(active_slot);

        self.conclude(
            search_state,
            tested,
            matches,
            refined,
            unsatisfiable,
            &diagnostic,
        );
        glib::ControlFlow::Break
    }

    fn conclude(
        self: &Rc<Self>,
        search_state: i64,
        tested: u64,
        matches: u64,
        refined: Option<(u64, u64)>,
        unsatisfiable: bool,
        diagnostic: &str,
    ) {
        self.title.set_subtitle(&match matches {
            0 => String::new(),
            1 => "1 seed".to_owned(),
            count => format!("{} seeds", group_digits(count)),
        });
        match search_state {
            STATE_FAILED => {
                self.show_message(
                    "computer-fail-symbolic",
                    "Search Failed",
                    &format!("The search stopped unexpectedly: {diagnostic}"),
                );
                self.toasts
                    .add_toast(adw::Toast::new("The search failed unexpectedly"));
            }
            STATE_COMPLETED if matches == 0 && unsatisfiable => {
                self.show_message(
                    "action-unavailable-symbolic",
                    "Impossible Query",
                    &format!(
                        "No seed can satisfy these requirements within the current floor \
                         limit. Quest-reward-only items need their quest floors in range: \
                         +3 wands floor {}, +3/+4 rings floor {}.",
                        Quest::Wandmaker.window().1,
                        Quest::Imp.window().1,
                    ),
                );
            }
            STATE_COMPLETED if matches == 0 => {
                self.show_message(
                    "edit-find-symbolic",
                    "No Seeds Found",
                    &format!(
                        "All {} seeds were tested without a match.",
                        group_digits(tested)
                    ),
                );
            }
            STATE_CANCELLED if matches == 0 => {
                self.show_message(
                    "media-playback-stop-symbolic",
                    "Search Stopped",
                    &format!(
                        "Tested {} seeds before stopping, without a match.",
                        group_digits(tested)
                    ),
                );
            }
            state => {
                let summary = if state == STATE_COMPLETED {
                    "Completed"
                } else {
                    "Stopped"
                };
                self.stats_line.set_label(&format!(
                    "{summary} · tested {} · {} match{}",
                    group_digits(tested),
                    group_digits(matches),
                    if matches == 1 { "" } else { "es" },
                ));
                self.progress_line.set_visible(false);
                // The engine's cap counts this session's accepts only, so a
                // resumed refine subtracts the survivors it started from.
                let new_finds = matches.saturating_sub(refined.map_or(0, |(kept, _)| kept));
                let collected = self.seeds.borrow().len();
                if let Some(notice) = conclusion_toast(refined, collected, new_finds) {
                    self.toasts.add_toast(adw::Toast::new(&notice));
                }
            }
        }
        self.finish();
    }

    fn show_message(&self, icon: &str, title: &str, description: &str) {
        self.message_page.set_icon_name(Some(icon));
        self.message_page.set_title(title);
        self.message_page.set_description(Some(description));
        self.stack.set_visible_child_name("message");
    }

    fn finish(&self) {
        if let Some(handler) = self.on_finished.borrow().as_ref() {
            handler();
        }
    }

    fn drain_matches(self: &Rc<Self>, active: &mut ActiveSearch) {
        let mut appended = false;
        loop {
            let worlds = active.session.drain_worlds(DRAIN_BATCH);
            if worlds.is_empty() {
                break;
            }
            let mut seeds = self.seeds.borrow_mut();
            for world in &worlds {
                let code = world.seed.to_code();
                // A resumed traversal may re-test a small overlap around the
                // previous stop position; keep each seed listed once.
                if !active.seen.insert(code.clone()) {
                    continue;
                }
                // Live finds past the display cap join the collection (and
                // through it the Target) without a row.
                if seeds.len() < DISPLAY_CAP {
                    self.append_row(&code, seeds.len() + 1);
                }
                seeds.push(code);
                active.matches += 1;
                appended = true;
            }
        }
        if appended {
            self.notify_results_changed();
        }
    }

    fn append_row(&self, seed_code: &str, position: usize) {
        let index_label = gtk::Label::builder()
            .label(position.to_string())
            .css_classes(["dim-label", "caption", "numeric"])
            .width_chars(4)
            .xalign(1.0)
            .build();
        let copy_button = gtk::Button::builder()
            .icon_name("edit-copy-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .tooltip_text("Copy Seed Code")
            .build();
        let row = adw::ActionRow::builder()
            .title(seed_code)
            .css_classes(["seed-row"])
            .build();
        row.add_prefix(&index_label);
        row.add_suffix(&copy_button);

        let toasts = self.toasts.clone();
        let seed = seed_code.to_owned();
        copy_button.connect_clicked(move |button| {
            button.clipboard().set_text(&seed);
            toasts.add_toast(adw::Toast::new(&format!("Copied {seed}")));
        });
        self.list.append(&row);
    }
}

/// The single status toast for a concluded run — a refine outcome, a list
/// notice, or both joined into one message (stacked toasts would hide one
/// behind the other) — or `None` when nothing is worth announcing. `refined`
/// is `(kept, previous)` when the run re-verified earlier seeds, `collected`
/// the full uncapped collection size, and `new_finds` how many seeds this
/// session accepted beyond the refine survivors. The list notice reports
/// truncation — the collection outgrew the `DISPLAY_CAP` listed rows — or,
/// for an untruncated run, that the engine's accept cap ended the session.
fn conclusion_toast(
    refined: Option<(u64, u64)>,
    collected: usize,
    new_finds: u64,
) -> Option<String> {
    let refined_notice = refined.map(|(kept, previous)| {
        format!(
            "Refined: kept {} of {} previous seed{}",
            group_digits(kept),
            group_digits(previous),
            if previous == 1 { "" } else { "s" },
        )
    });
    let limit_notice = if collected > DISPLAY_CAP {
        Some(format!(
            "listing the first {} of {} seeds",
            group_digits(DISPLAY_CAP as u64),
            group_digits(collected as u64),
        ))
    } else if new_finds >= MAX_RESULTS as u64 {
        Some(format!(
            "result limit reached ({} seeds)",
            group_digits(MAX_RESULTS as u64),
        ))
    } else {
        None
    };
    match (refined_notice, limit_notice) {
        (Some(refine), Some(limit)) => Some(format!("{refine} · {limit}")),
        (Some(refine), None) => Some(refine),
        (None, Some(limit)) => {
            let mut chars = limit.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
        }
        (None, None) => None,
    }
}

fn caption_label() -> gtk::Label {
    gtk::Label::builder()
        .css_classes(["caption", "dim-label", "numeric"])
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .build()
}

// Seed counts stay far below 2^53, so the f64 progress math is exact enough
// for display purposes.
#[allow(clippy::cast_precision_loss)]
const fn precise(value: u64) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests {
    use super::{DISPLAY_CAP, conclusion_toast};

    const CAP: u64 = DISPLAY_CAP as u64;

    #[test]
    fn quiet_run_shows_no_toast() {
        assert_eq!(conclusion_toast(None, 12, 12), None);
    }

    #[test]
    fn plain_refine_reports_the_kept_count() {
        assert_eq!(
            conclusion_toast(Some((3, 10)), 3, 0),
            Some("Refined: kept 3 of 10 previous seeds".to_owned()),
        );
    }

    #[test]
    fn refine_of_a_single_seed_stays_singular() {
        assert_eq!(
            conclusion_toast(Some((1, 1)), 1, 0),
            Some("Refined: kept 1 of 1 previous seed".to_owned()),
        );
    }

    #[test]
    fn fresh_scan_hitting_the_engine_cap_reports_the_limit() {
        assert_eq!(
            conclusion_toast(None, DISPLAY_CAP, CAP),
            Some("Result limit reached (1\u{202f}024 seeds)".to_owned()),
        );
    }

    #[test]
    fn survivors_alone_filling_the_display_are_not_truncation() {
        // Exactly DISPLAY_CAP collected seeds all have rows, and a session
        // that accepted nothing new never hit the engine cap.
        assert_eq!(
            conclusion_toast(Some((CAP, 2_000)), DISPLAY_CAP, 0),
            Some("Refined: kept 1\u{202f}024 of 2\u{202f}000 previous seeds".to_owned()),
        );
    }

    #[test]
    fn accumulated_refine_reports_truncation_in_one_toast() {
        assert_eq!(
            conclusion_toast(Some((900, 1_500)), 1_900, 1_000),
            Some(
                "Refined: kept 900 of 1\u{202f}500 previous seeds · \
                 listing the first 1\u{202f}024 of 1\u{202f}900 seeds"
                    .to_owned()
            ),
        );
    }

    #[test]
    fn truncation_outranks_the_engine_cap_notice() {
        assert_eq!(
            conclusion_toast(Some((500, 500)), 1_524, CAP),
            Some(
                "Refined: kept 500 of 500 previous seeds · \
                 listing the first 1\u{202f}024 of 1\u{202f}524 seeds"
                    .to_owned()
            ),
        );
    }

    #[test]
    fn capped_resume_without_truncation_reports_the_limit() {
        assert_eq!(
            conclusion_toast(Some((0, 500)), DISPLAY_CAP, CAP),
            Some(
                "Refined: kept 0 of 500 previous seeds · \
                 result limit reached (1\u{202f}024 seeds)"
                    .to_owned()
            ),
        );
    }

    #[test]
    fn oversized_filter_survivor_set_reports_truncation_alone() {
        // A target filter of a grown Target Set: no scan, no engine cap,
        // but more survivors than rows.
        assert_eq!(
            conclusion_toast(Some((3_000, 5_116)), 3_000, 0),
            Some(
                "Refined: kept 3\u{202f}000 of 5\u{202f}116 previous seeds · \
                 listing the first 1\u{202f}024 of 3\u{202f}000 seeds"
                    .to_owned()
            ),
        );
    }
}
