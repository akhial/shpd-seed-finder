// SPDX-License-Identifier: GPL-3.0-or-later

//! Results pane: streaming search session, live statistics, and seed list.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::glib;
use shpd_seedfinder_core::model::GeneratedWorld;
use shpd_seedfinder_core::query::SearchQuery;
use shpd_seedfinder_core::search::SearchError;
use shpd_seedfinder_core::seed::DungeonSeed;
use shpd_seedfinder_session::{
    MAX_ACCEPTED_RESULTS, NativeSession, STATE_CANCELLED, STATE_COMPLETED, STATE_FAILED,
    STATE_RUNNING, filter_matching_seeds,
};

use crate::format::{duration, estimate_duration, group_digits, probability_percent, seed_rate};

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const DRAIN_BATCH: usize = 256;

struct ActiveSearch {
    session: Rc<NativeSession>,
    query: SearchQuery,
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

/// Everything needed to refine a finished search: the query it ran and where
/// its traversal stopped. The engine guarantees every match in the region
/// before `resume_from` was delivered, so combining the filtered result list
/// with a scan of the `remaining` seeds loses nothing.
struct BaseRun {
    query: SearchQuery,
    resume_from: u64,
    remaining: u64,
}

/// An in-flight re-verification of the previous results on a worker thread.
struct PendingRefine {
    receiver: mpsc::Receiver<Result<Vec<GeneratedWorld>, SearchError>>,
    query: SearchQuery,
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
    progress_bar: gtk::ProgressBar,
    list: gtk::ListBox,
    seeds: RefCell<Vec<String>>,
    active: RefCell<Option<ActiveSearch>>,
    pending_refine: RefCell<Option<PendingRefine>>,
    base: RefCell<Option<BaseRun>>,
    toasts: adw::ToastOverlay,
    on_select: RefCell<Option<SelectHandler>>,
    on_finished: RefCell<Option<Box<dyn Fn()>>>,
}

type SelectHandler = Box<dyn Fn(&str)>;

impl ResultsPane {
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

        let progress_bar = gtk::ProgressBar::builder()
            .css_classes(["osd"])
            .valign(gtk::Align::Start)
            .visible(false)
            .build();
        let overlay = gtk::Overlay::builder().child(&stack).build();
        overlay.add_overlay(&progress_bar);

        let title = adw::WindowTitle::new("Results", "");
        let header_bar = adw::HeaderBar::builder().title_widget(&title).build();
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header_bar);
        toolbar_view.set_content(Some(&overlay));

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
            progress_bar,
            list,
            seeds: RefCell::new(Vec::new()),
            active: RefCell::new(None),
            pending_refine: RefCell::new(None),
            base: RefCell::new(None),
            toasts: toasts.clone(),
            on_select: RefCell::new(None),
            on_finished: RefCell::new(None),
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

    pub fn is_running(&self) -> bool {
        self.active.borrow().is_some() || self.pending_refine.borrow().is_some()
    }

    /// The query of the last finished search whose traversal can still be
    /// refined, if any.
    pub fn refine_target(&self) -> Option<SearchQuery> {
        self.base.borrow().as_ref().map(|base| base.query.clone())
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
            self.progress_bar.set_visible(false);
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

    /// Starts a full-range production search; a failure to spawn is reported
    /// as a toast and leaves the pane idle.
    pub fn start(self: &Rc<Self>, query: SearchQuery) {
        if self.is_running() {
            return;
        }
        self.base.replace(None);
        let session = match NativeSession::production(query.clone()) {
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
        self.stack.set_visible_child_name("results");
        self.title.set_subtitle("Searching…");
        self.stats_line.set_label("Measuring search speed…");
        self.progress_line.set_label("Starting…");
        self.progress_line.set_visible(true);
        self.progress_bar.set_fraction(0.0);
        self.progress_bar.set_visible(true);
        let now = Instant::now();
        self.active.replace(Some(ActiveSearch {
            session,
            query,
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

    /// Narrows the last finished search without discarding it: the listed
    /// seeds are re-verified against `query` on a worker thread, and the scan
    /// then resumes over exactly the seeds the previous traversal never
    /// covered. The caller ensures `query` extends the finished search's
    /// query (see [`crate::state::extends_query`]).
    pub fn refine(self: &Rc<Self>, query: SearchQuery) {
        if self.is_running() {
            return;
        }
        let Some(base) = self.base.borrow().as_ref().map(|base| BaseRun {
            query: base.query.clone(),
            resume_from: base.resume_from,
            remaining: base.remaining,
        }) else {
            return;
        };
        // Re-assert the superset invariant here rather than trusting the
        // action's enabled flag: filter-and-resume is only sound when the
        // refined query strictly extends the finished one.
        if !crate::state::extends_query(&query, &base.query) {
            self.toasts.add_toast(adw::Toast::new(
                "Refining requires only added requirements; start a new search instead",
            ));
            return;
        }
        let seed_values: Vec<u64> = self
            .seeds
            .borrow()
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
            resume_from: base.resume_from,
            remaining: base.remaining,
            previous_matches,
            started: Instant::now(),
        }));

        self.stack.set_visible_child_name("results");
        self.title.set_subtitle("Refining…");
        self.stats_line.set_label(&format!(
            "Re-checking {} found seed{} against the added requirements…",
            group_digits(previous_matches),
            if previous_matches == 1 { "" } else { "s" },
        ));
        self.progress_line.set_visible(false);
        self.progress_bar.set_fraction(0.0);
        self.progress_bar.set_visible(true);
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
                Err(mpsc::TryRecvError::Empty) => {
                    self.progress_bar.pulse();
                    return glib::ControlFlow::Continue;
                }
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
                self.progress_bar.set_visible(false);
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
                self.append_row(&code, seeds.len() + 1);
                seen.insert(code.clone());
                seeds.push(code);
            }
        }
        let kept = kept_worlds.len() as u64;

        if pending.remaining == 0 {
            // The previous traversal already covered every seed; the filtered
            // subset is the complete refined result.
            self.base.replace(Some(BaseRun {
                query: pending.query,
                resume_from: pending.resume_from,
                remaining: 0,
            }));
            self.conclude_refined_filter_only(kept, pending.previous_matches);
            return glib::ControlFlow::Break;
        }

        let session = match NativeSession::production_resumed(
            pending.query.clone(),
            pending.resume_from,
            pending.remaining,
        ) {
            Ok(session) => Rc::new(session),
            Err(error) => {
                self.progress_bar.set_visible(false);
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

    fn conclude_refined_filter_only(&self, kept: u64, previous: u64) {
        self.progress_bar.set_visible(false);
        self.progress_line.set_visible(false);
        self.restore_count_subtitle();
        if kept == 0 {
            self.show_message(
                "edit-find-symbolic",
                "No Seeds Left",
                &format!(
                    "None of the {} previous seeds satisfy the added requirements, \
                     and the previous search had already covered every seed.",
                    group_digits(previous)
                ),
            );
        } else {
            self.stats_line.set_label(&format!(
                "Refined · kept {} of {} previous seed{} · every seed was already scanned",
                group_digits(kept),
                group_digits(previous),
                if previous == 1 { "" } else { "s" },
            ));
        }
        self.finish();
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
        let total = status[2].max(1).unsigned_abs();
        let probability = f64::from_bits(u64::from_ne_bytes(status[4].to_ne_bytes()));
        let probability = (probability > 0.0 && probability.is_finite()).then_some(probability);

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

        self.progress_bar
            .set_fraction((precise(tested) / precise(total)).clamp(0.0, 1.0));
        self.title.set_subtitle(&match active.matches {
            0 => "Searching…".to_owned(),
            1 => "1 seed".to_owned(),
            count => format!("{} seeds", group_digits(count)),
        });

        if search_state == STATE_RUNNING {
            let time_to_seed = probability
                .filter(|_| active.seeds_per_second > 0.0)
                .map(|probability| 1.0 / probability / active.seeds_per_second);
            self.stats_line.set_label(&format!(
                "Match probability {} · {} seeds/s · ~{} to a match",
                probability_percent(probability),
                seed_rate(active.seeds_per_second),
                estimate_duration(time_to_seed),
            ));
            self.progress_line.set_label(&format!(
                "Tested {} of {} · elapsed {}",
                group_digits(tested),
                group_digits(total),
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
        let base =
            (search_state == STATE_COMPLETED || search_state == STATE_CANCELLED).then(|| {
                let [resume_from, remaining] = active.session.resume_hint();
                BaseRun {
                    query: active.query.clone(),
                    resume_from: resume_from.max(0).unsigned_abs(),
                    remaining: remaining.max(0).unsigned_abs(),
                }
            });
        self.base.replace(base);
        *active_slot = None;
        drop(active_slot);

        self.conclude(search_state, tested, matches, refined, &diagnostic);
        glib::ControlFlow::Break
    }

    fn conclude(
        self: &Rc<Self>,
        search_state: i64,
        tested: u64,
        matches: u64,
        refined: Option<(u64, u64)>,
        diagnostic: &str,
    ) {
        self.progress_bar.set_visible(false);
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
            STATE_COMPLETED if tested == 0 && matches == 0 => {
                // The engine proves some queries unsatisfiable before testing
                // a single seed; surface that instead of a zero-result search.
                self.show_message(
                    "action-unavailable-symbolic",
                    "Impossible Query",
                    "No seed can satisfy these requirements within the current floor \
                     limit. Quest-reward-only items need their quest floors in range: \
                     +3 wands floor 9, +3/+4 rings floor 19.",
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
                let cap_notice = if matches >= MAX_ACCEPTED_RESULTS as u64 {
                    " · result limit reached"
                } else {
                    ""
                };
                let refined_notice = refined.map_or(String::new(), |(kept, previous)| {
                    format!(
                        " · kept {} of {} previous",
                        group_digits(kept),
                        group_digits(previous)
                    )
                });
                self.stats_line.set_label(&format!(
                    "{summary} · tested {} · {} match{}{refined_notice}{cap_notice}",
                    group_digits(tested),
                    group_digits(matches),
                    if matches == 1 { "" } else { "es" },
                ));
                self.progress_line.set_visible(false);
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
                self.append_row(&code, seeds.len() + 1);
                seeds.push(code);
                active.matches += 1;
            }
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
