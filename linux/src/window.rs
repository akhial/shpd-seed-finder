// SPDX-License-Identifier: GPL-3.0-or-later

//! Main window: an adaptive triple-pane layout built from two nested
//! navigation split views (query → results → seed), following GNOME's
//! multi-pane navigation pattern.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;

use crate::config::APP_NAME;
use crate::state::UiRequirement;
use crate::{
    challenges_dialog, detail_pane, persist, query_pane, requirement_editor, results_pane,
};

#[allow(clippy::too_many_lines)] // Linear assembly of panes, actions, and wiring.
pub fn present(app: &adw::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }

    let state = Rc::new(RefCell::new(persist::load()));
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
    let refresh_all: Rc<dyn Fn()> = Rc::new({
        let state = Rc::clone(&state);
        let query = Rc::clone(&query);
        let detail = Rc::clone(&detail);
        let results = Rc::clone(&results);
        let start_action = start_action.clone();
        move || {
            let snapshot = state.borrow();
            persist::save(&snapshot);
            query.refresh(&snapshot);
            detail.render(&snapshot);
            start_action.set_enabled(!snapshot.requirements.is_empty() || results.is_running());
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
        let inner_split = inner_split.clone();
        let outer_split = outer_split.clone();
        move |seed| {
            detail.scout(Some(seed), &state.borrow());
            outer_split.set_show_content(true);
            inner_split.set_show_content(true);
        }
    });
    results.connect_finished({
        let query = Rc::clone(&query);
        let state = Rc::clone(&state);
        let start_action = start_action.clone();
        move || {
            query.set_running(false);
            start_action.set_enabled(!state.borrow().requirements.is_empty());
        }
    });
    detail.connect_scout({
        let state = Rc::clone(&state);
        let detail = Rc::clone(&detail);
        move || detail.scout(None, &state.borrow())
    });

    start_action.connect_activate({
        let state = Rc::clone(&state);
        let query = Rc::clone(&query);
        let results = Rc::clone(&results);
        let toasts = toasts.clone();
        let inner_split = inner_split.clone();
        let outer_split = outer_split.clone();
        move |_, _| {
            if results.is_running() {
                results.cancel();
                return;
            }
            match state.borrow().to_query() {
                Ok(search_query) => {
                    results.start(search_query);
                    if results.is_running() {
                        query.set_running(true);
                        outer_split.set_show_content(true);
                        inner_split.set_show_content(false);
                    }
                }
                Err(message) => toasts.add_toast(adw::Toast::new(&message)),
            }
        }
    });
    window.add_action(&start_action);

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
}

fn build_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let query_section = gio::Menu::new();
    query_section.append(Some("_Challenges…"), Some("win.challenges"));
    menu.append_section(None, &query_section);
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
        ("Enter a seed code", "<primary>l"),
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
