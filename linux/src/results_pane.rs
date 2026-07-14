// SPDX-License-Identifier: GPL-3.0-or-later

//! Results pane: streaming search session, live statistics, and seed list.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::glib;
use shpd_seedfinder_core::query::SearchQuery;
use shpd_seedfinder_session::{
    MAX_ACCEPTED_RESULTS, NativeSession, STATE_CANCELLED, STATE_COMPLETED, STATE_FAILED,
    STATE_RUNNING,
};

use crate::format::{duration, estimate_duration, group_digits, probability_percent, seed_rate};

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const DRAIN_BATCH: usize = 256;

struct ActiveSearch {
    session: Rc<NativeSession>,
    matches: u64,
    last_tested: u64,
    last_tick: Instant,
    started: Instant,
    seeds_per_second: f64,
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
        self.active.borrow().is_some()
    }

    pub fn cancel(&self) {
        if let Some(active) = self.active.borrow().as_ref() {
            active.session.cancel();
        }
    }

    /// Starts a full-range production search; a failure to spawn is reported
    /// as a toast and leaves the pane idle.
    pub fn start(self: &Rc<Self>, query: SearchQuery) {
        if self.is_running() {
            return;
        }
        let session = match NativeSession::production(query) {
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
            matches: 0,
            last_tested: 0,
            last_tick: now,
            started: now,
            seeds_per_second: 0.0,
        }));

        let pane = Rc::clone(self);
        glib::timeout_add_local(POLL_INTERVAL, move || pane.tick());
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
        let diagnostic = if search_state == STATE_FAILED {
            active
                .session
                .take_failure_diagnostic()
                .unwrap_or_else(|| "unknown worker failure".to_owned())
        } else {
            String::new()
        };
        *active_slot = None;
        drop(active_slot);

        self.conclude(search_state, tested, matches, &diagnostic);
        glib::ControlFlow::Break
    }

    fn conclude(self: &Rc<Self>, search_state: i64, tested: u64, matches: u64, diagnostic: &str) {
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
                self.stats_line.set_label(&format!(
                    "{summary} · tested {} · {} match{}{cap_notice}",
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
                self.append_row(&code, seeds.len() + 1);
                seeds.push(code);
            }
            active.matches += worlds.len() as u64;
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
