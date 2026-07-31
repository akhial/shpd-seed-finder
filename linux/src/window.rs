// SPDX-License-Identifier: GPL-3.0-or-later

//! Main window: an adaptive triple-pane layout built from two nested
//! navigation split views (query → results → seed), following GNOME's
//! multi-pane navigation pattern.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use shpd_seedfinder_core::results_export;
use shpd_seedfinder_core::seed::DungeonSeed;
use shpd_seedfinder_session::MAX_ACCEPTED_RESULTS;

/// Import size cap; a maximal legal results file is far below this.
const MAX_RESULTS_FILE_BYTES: usize = 2 * 1024 * 1024;

use crate::config::APP_NAME;
use crate::state::{AppState, UiRequirement};
use crate::{
    challenges_dialog, detail_pane, persist, presets_dialog, query_pane, requirement_editor,
    results_pane, update,
};

#[allow(clippy::too_many_lines)] // Linear assembly of panes, actions, and wiring.
pub fn present(app: &adw::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }

    let state = Rc::new(RefCell::new(persist::load()));
    let user_presets = Rc::new(RefCell::new(persist::load_presets()));
    // The query that produced the current results list, snapshotted at search
    // start (or import) so an export never reflects later editor changes.
    let exported_query: Rc<RefCell<Option<shpd_seedfinder_core::query::SearchQuery>>> =
        Rc::new(RefCell::new(None));
    let toasts = adw::ToastOverlay::new();

    let query = query_pane::QueryPane::new(build_menu().upcast_ref());
    let results = results_pane::ResultsPane::new(&toasts);
    let detail = detail_pane::DetailPane::new(&toasts);

    // Results and seed detail form the inner split view; the query sidebar
    // wraps both in the outer one. Nesting the two split views is the
    // libadwaita pattern for adaptive triple-pane layouts.
    let inner_split = adw::NavigationSplitView::builder()
        .sidebar(&results.page)
        .content(&detail.page)
        .min_sidebar_width(270.0)
        .max_sidebar_width(380.0)
        .sidebar_width_fraction(0.36)
        .build();
    let inner_page = adw::NavigationPage::builder()
        .title("Results")
        .child(&inner_split)
        .build();
    let outer_split = adw::NavigationSplitView::builder()
        .sidebar(&query.page)
        .content(&inner_page)
        .min_sidebar_width(300.0)
        .max_sidebar_width(420.0)
        .sidebar_width_fraction(0.3)
        .build();
    toasts.set_child(Some(&outer_split));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .content(&toasts)
        .default_width(1240)
        .default_height(760)
        .width_request(360)
        .height_request(360)
        .title(APP_NAME)
        .build();

    let medium = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        1000.0,
        adw::LengthUnit::Sp,
    ));
    medium.add_setter(&inner_split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(medium);
    let narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        620.0,
        adw::LengthUnit::Sp,
    ));
    narrow.add_setter(&inner_split, "collapsed", Some(&true.to_value()));
    narrow.add_setter(&outer_split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(narrow);

    // Actions and cross-pane wiring.

    let start_action = gio::SimpleAction::new("start-search", None);
    let refine_action = gio::SimpleAction::new("refine-search", None);
    let refresh_all: Rc<dyn Fn()> = Rc::new({
        let state = Rc::clone(&state);
        let query = Rc::clone(&query);
        let detail = Rc::clone(&detail);
        let results = Rc::clone(&results);
        let start_action = start_action.clone();
        let refine_action = refine_action.clone();
        move || {
            let snapshot = state.borrow();
            persist::save(&snapshot);
            query.refresh(&snapshot);
            detail.render(&snapshot);
            start_action.set_enabled(!snapshot.requirements.is_empty() || results.is_running());
            // Refining is offered only while the edited query strictly
            // extends the finished search: the found seeds then remain valid
            // candidates and the scan can pick up where it stopped.
            let refine_eligible = !results.is_running()
                && results.refine_target().is_some_and(|base| {
                    snapshot
                        .to_query()
                        .is_ok_and(|current| crate::state::extends_query(&current, &base))
                });
            refine_action.set_enabled(refine_eligible);
            query.set_refine_offered(refine_eligible);
        }
    });

    let edit_requirement: Rc<dyn Fn(UiRequirement, bool)> = Rc::new({
        let state = Rc::clone(&state);
        let refresh_all = Rc::clone(&refresh_all);
        let window = window.clone();
        move |requirement, is_new| {
            let state = Rc::clone(&state);
            let refresh_all = Rc::clone(&refresh_all);
            requirement_editor::present(&window, &requirement, is_new, move |result| {
                let mut state = state.borrow_mut();
                if let Some(slot) = state
                    .requirements
                    .iter_mut()
                    .find(|other| other.key == result.key)
                {
                    *slot = result;
                } else {
                    state.requirements.push(result);
                }
                drop(state);
                refresh_all();
            });
        }
    });

    query.connect_edit({
        let state = Rc::clone(&state);
        let edit_requirement = Rc::clone(&edit_requirement);
        move |key| {
            let requirement = state
                .borrow()
                .requirements
                .iter()
                .find(|requirement| requirement.key == key)
                .copied();
            if let Some(requirement) = requirement {
                edit_requirement(requirement, false);
            }
        }
    });
    query.connect_remove({
        let state = Rc::clone(&state);
        let refresh_all = Rc::clone(&refresh_all);
        move |key| {
            state
                .borrow_mut()
                .requirements
                .retain(|requirement| requirement.key != key);
            refresh_all();
        }
    });
    query.connect_changed({
        let state = Rc::clone(&state);
        let query = Rc::clone(&query);
        let refresh_all = Rc::clone(&refresh_all);
        move || {
            query.read_scope(&mut state.borrow_mut());
            refresh_all();
        }
    });

    results.connect_select({
        let state = Rc::clone(&state);
        let detail = Rc::clone(&detail);
        let results = Rc::clone(&results);
        let inner_split = inner_split.clone();
        let outer_split = outer_split.clone();
        move |seed| {
            detail.scout(Some(seed), &state.borrow());
            detail.set_result_position(results.position_of(seed));
            outer_split.set_show_content(true);
            inner_split.set_show_content(true);
        }
    });
    results.connect_finished({
        let query = Rc::clone(&query);
        let refresh_all = Rc::clone(&refresh_all);
        move || {
            query.set_running(false);
            // Re-derives the enabled actions, including whether the finished
            // search can now be refined.
            refresh_all();
        }
    });
    detail.connect_scout({
        let state = Rc::clone(&state);
        let detail = Rc::clone(&detail);
        let results = Rc::clone(&results);
        move || {
            detail.scout(None, &state.borrow());
            // A hand-entered seed may still be one of the results; keep the
            // position indicator honest either way.
            let position = detail
                .current_seed()
                .and_then(|seed| results.position_of(&seed));
            detail.set_result_position(position);
        }
    });

    // Keeps the seed pane's "Result N of M" indicator honest while a new
    // search clears the list or a running one streams matches in.
    results.connect_results_changed({
        let detail = Rc::clone(&detail);
        let results = Rc::clone(&results);
        move || {
            let position = detail
                .current_seed()
                .and_then(|seed| results.position_of(&seed));
            detail.set_result_position(position);
        }
    });

    // J/K step the scouted seed through the search results; row selection
    // then drives the scout above. The keys stay inert while a dialog is
    // presented, while an editable widget has focus, or while a collapsed
    // split view is showing another page (navigating would flip pages the
    // user is not on).
    let navigate_keys = gtk::EventControllerKey::new();
    navigate_keys.set_propagation_phase(gtk::PropagationPhase::Bubble);
    navigate_keys.connect_key_pressed({
        let window = window.clone();
        let results = Rc::clone(&results);
        let detail = Rc::clone(&detail);
        let inner_split = inner_split.clone();
        let outer_split = outer_split.clone();
        move |_, key, _, modifiers| {
            let delta = match key {
                gdk::Key::j | gdk::Key::J => 1,
                gdk::Key::k | gdk::Key::K => -1,
                _ => return glib::Propagation::Proceed,
            };
            if modifiers.intersects(
                gdk::ModifierType::CONTROL_MASK
                    | gdk::ModifierType::ALT_MASK
                    | gdk::ModifierType::SUPER_MASK,
            ) {
                return glib::Propagation::Proceed;
            }
            // Never navigate behind a modal (requirement editor, presets,
            // challenges, shortcuts are all in-window adw::Dialogs).
            if window.visible_dialog().is_some() {
                return glib::Propagation::Proceed;
            }
            // Bubble-phase keys rarely escape an editable widget, but a
            // focused entry must never lose typed letters to navigation.
            if GtkWindowExt::focus(&window)
                .is_some_and(|focus| focus.is::<gtk::Text>() || focus.is::<gtk::TextView>())
            {
                return glib::Propagation::Proceed;
            }
            // On collapsed layouts, act only while the seed page is visible.
            let seed_page_visible = (!outer_split.is_collapsed() || outer_split.shows_content())
                && (!inner_split.is_collapsed() || inner_split.shows_content());
            if !seed_page_visible {
                return glib::Propagation::Proceed;
            }
            let Some(seed) = detail.current_seed() else {
                return glib::Propagation::Proceed;
            };
            if results.select_step(&seed, delta) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });
    window.add_controller(navigate_keys);

    start_action.connect_activate({
        let state = Rc::clone(&state);
        let query = Rc::clone(&query);
        let results = Rc::clone(&results);
        let exported_query = Rc::clone(&exported_query);
        let toasts = toasts.clone();
        let inner_split = inner_split.clone();
        let outer_split = outer_split.clone();
        let refresh_all = Rc::clone(&refresh_all);
        move |_, _| {
            if results.is_running() {
                results.cancel();
                return;
            }
            match state.borrow().to_query() {
                Ok(search_query) => {
                    results.start(search_query.clone());
                    if results.is_running() {
                        exported_query.replace(Some(search_query));
                        query.set_running(true);
                        outer_split.set_show_content(true);
                        inner_split.set_show_content(false);
                        refresh_all();
                    }
                }
                Err(message) => toasts.add_toast(adw::Toast::new(&message)),
            }
        }
    });
    window.add_action(&start_action);

    refine_action.connect_activate({
        let state = Rc::clone(&state);
        let query = Rc::clone(&query);
        let results = Rc::clone(&results);
        let toasts = toasts.clone();
        let inner_split = inner_split.clone();
        let outer_split = outer_split.clone();
        let refresh_all = Rc::clone(&refresh_all);
        move |_, _| {
            if results.is_running() {
                return;
            }
            match state.borrow().to_query() {
                Ok(search_query) => {
                    results.refine(search_query);
                    if results.is_running() {
                        query.set_running(true);
                        outer_split.set_show_content(true);
                        inner_split.set_show_content(false);
                        refresh_all();
                    }
                }
                Err(message) => toasts.add_toast(adw::Toast::new(&message)),
            }
        }
    });
    window.add_action(&refine_action);

    let add_action = gio::SimpleAction::new("add-requirement", None);
    add_action.connect_activate({
        let state = Rc::clone(&state);
        let edit_requirement = Rc::clone(&edit_requirement);
        move |_, _| {
            let draft = UiRequirement::new(state.borrow_mut().claim_key());
            edit_requirement(draft, true);
        }
    });
    window.add_action(&add_action);

    let challenges_action = gio::SimpleAction::new("challenges", None);
    challenges_action.connect_activate({
        let state = Rc::clone(&state);
        let refresh_all = Rc::clone(&refresh_all);
        let window = window.clone();
        move |_, _| challenges_dialog::present(&window, &state, &refresh_all)
    });
    window.add_action(&challenges_action);

    let presets_action = gio::SimpleAction::new("presets", None);
    presets_action.connect_activate({
        let state = Rc::clone(&state);
        let user_presets = Rc::clone(&user_presets);
        let refresh_all = Rc::clone(&refresh_all);
        let results = Rc::clone(&results);
        let toasts = toasts.clone();
        let window = window.clone();
        move |_, _| {
            if results.is_running() {
                toasts.add_toast(adw::Toast::new("Stop the search before loading a preset"));
                return;
            }
            presets_dialog::present(&window, &toasts, &state, &user_presets, &refresh_all);
        }
    });
    window.add_action(&presets_action);

    let export_action = gio::SimpleAction::new("export-results", None);
    export_action.connect_activate({
        let results = Rc::clone(&results);
        let exported_query = Rc::clone(&exported_query);
        let toasts = toasts.clone();
        let window = window.clone();
        move |_, _| {
            if results.is_running() {
                toasts.add_toast(adw::Toast::new("Stop the search before exporting results"));
                return;
            }
            let codes = results.seed_codes();
            // Export the query snapshot captured when these results were
            // produced, never the live editor state.
            let query = exported_query.borrow().clone();
            let (Some(query), false) = (query, codes.is_empty()) else {
                toasts.add_toast(adw::Toast::new(
                    "Run a search first — there are no results to export yet",
                ));
                return;
            };
            let seeds = match codes
                .iter()
                .map(|code| DungeonSeed::from_code(code))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(seeds) => seeds,
                Err(error) => {
                    toasts.add_toast(adw::Toast::new(&format!("Cannot export: {error}")));
                    return;
                }
            };
            let contents = results_export::encode(&query, &seeds, env!("CARGO_PKG_VERSION"));
            let dialog = gtk::FileDialog::builder()
                .title("Export Results")
                .initial_name("seed-seeker-results.json")
                .build();
            let toasts = toasts.clone();
            dialog.save(Some(&window), gio::Cancellable::NONE, move |chosen| {
                // Cancelling the dialog is not an error worth reporting.
                let Ok(file) = chosen else { return };
                match file.replace_contents(
                    contents.as_bytes(),
                    None,
                    false,
                    gio::FileCreateFlags::REPLACE_DESTINATION,
                    gio::Cancellable::NONE,
                ) {
                    Ok(_) => toasts.add_toast(adw::Toast::new("Results exported")),
                    Err(error) => {
                        toasts.add_toast(adw::Toast::new(&format!("Export failed: {error}")));
                    }
                }
            });
        }
    });
    window.add_action(&export_action);

    let import_action = gio::SimpleAction::new("import-results", None);
    import_action.connect_activate({
        let state = Rc::clone(&state);
        let results = Rc::clone(&results);
        let refresh_all = Rc::clone(&refresh_all);
        let exported_query = Rc::clone(&exported_query);
        let toasts = toasts.clone();
        let window = window.clone();
        move |_, _| {
            if results.is_running() {
                toasts.add_toast(adw::Toast::new("Stop the search before importing results"));
                return;
            }
            let dialog = gtk::FileDialog::builder().title("Import Results").build();
            let state = Rc::clone(&state);
            let results = Rc::clone(&results);
            let refresh_all = Rc::clone(&refresh_all);
            let exported_query = Rc::clone(&exported_query);
            let toasts = toasts.clone();
            dialog.open(Some(&window), gio::Cancellable::NONE, move |chosen| {
                let Ok(file) = chosen else { return };
                file.load_contents_async(gio::Cancellable::NONE, move |loaded| {
                    let contents = match loaded {
                        Ok((bytes, _)) => bytes,
                        Err(error) => {
                            toasts.add_toast(adw::Toast::new(&format!("Import failed: {error}")));
                            return;
                        }
                    };
                    // A search may have started while the dialog was open.
                    if results.is_running() {
                        toasts.add_toast(adw::Toast::new(
                            "Stop the search before importing results",
                        ));
                        return;
                    }
                    if contents.len() > MAX_RESULTS_FILE_BYTES {
                        toasts.add_toast(adw::Toast::new(
                            "Import failed: this file is too large to be a results file (2 MiB limit)",
                        ));
                        return;
                    }
                    match results_export::decode(&String::from_utf8_lossy(&contents)) {
                        Ok(imported) => {
                            let (kept, dropped) = results_export::dedupe_and_cap(
                                &imported.seeds,
                                MAX_ACCEPTED_RESULTS,
                            );
                            *state.borrow_mut() = AppState::from_query(&imported.query);
                            exported_query.replace(Some(imported.query));
                            let codes: Vec<String> =
                                kept.iter().map(|seed| seed.to_code()).collect();
                            results.load_imported(&codes);
                            refresh_all();
                            let mut message = format!(
                                "Imported {} seed{}",
                                codes.len(),
                                if codes.len() == 1 { "" } else { "s" },
                            );
                            if dropped > 0 {
                                let _ = std::fmt::Write::write_fmt(
                                    &mut message,
                                    format_args!(
                                        " · {dropped} duplicate or over-limit entries dropped"
                                    ),
                                );
                            }
                            toasts.add_toast(adw::Toast::new(&message));
                            if let Some(file_version) = imported.shpd_version
                                && file_version != shpd_seedfinder_core::SHPD_VERSION
                            {
                                toasts.add_toast(adw::Toast::new(&format!(
                                    "Note: this file targets Shattered Pixel Dungeon \
                                     v{file_version}; this app targets v{} — seeds may \
                                     generate differently",
                                    shpd_seedfinder_core::SHPD_VERSION,
                                )));
                            }
                        }
                        Err(message) => {
                            toasts.add_toast(adw::Toast::new(&format!(
                                "Import failed: {message}"
                            )));
                        }
                    }
                });
            });
        }
    });
    window.add_action(&import_action);

    let focus_seed_action = gio::SimpleAction::new("focus-seed", None);
    focus_seed_action.connect_activate({
        let detail = Rc::clone(&detail);
        let inner_split = inner_split.clone();
        let outer_split = outer_split.clone();
        move |_, _| {
            outer_split.set_show_content(true);
            inner_split.set_show_content(true);
            detail.focus_entry();
        }
    });
    window.add_action(&focus_seed_action);

    let shortcuts_action = gio::SimpleAction::new("shortcuts", None);
    shortcuts_action.connect_activate({
        let window = window.clone();
        move |_, _| present_shortcuts(&window)
    });
    window.add_action(&shortcuts_action);

    refresh_all();
    window.present();
    update::check_on_startup(&window);
}

fn build_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let query_section = gio::Menu::new();
    query_section.append(Some("_Presets…"), Some("win.presets"));
    query_section.append(Some("_Challenges…"), Some("win.challenges"));
    menu.append_section(None, &query_section);
    let results_section = gio::Menu::new();
    results_section.append(Some("_Import Results…"), Some("win.import-results"));
    results_section.append(Some("_Export Results…"), Some("win.export-results"));
    menu.append_section(None, &results_section);
    let app_section = gio::Menu::new();
    app_section.append(Some("_Keyboard Shortcuts"), Some("win.shortcuts"));
    app_section.append(Some("_About Seed Seeker"), Some("app.about"));
    menu.append_section(None, &app_section);
    menu
}

fn present_shortcuts(window: &adw::ApplicationWindow) {
    let section = adw::ShortcutsSection::new(None);
    for (title, accelerator) in [
        ("Add requirement", "<primary>n"),
        ("Start or stop the search", "<primary>Return"),
        ("Refine the results", "<primary><shift>Return"),
        ("Enter a seed code", "<primary>l"),
        ("Scout the next search result", "j"),
        ("Scout the previous search result", "k"),
        ("Challenges", "<primary>comma"),
        ("Keyboard shortcuts", "<primary>question"),
        ("Quit", "<primary>q"),
    ] {
        section.add(adw::ShortcutsItem::new(title, accelerator));
    }
    let dialog = adw::ShortcutsDialog::new();
    dialog.add(section);
    dialog.present(Some(window));
}
