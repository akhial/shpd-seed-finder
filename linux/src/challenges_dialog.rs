// SPDX-License-Identifier: GPL-3.0-or-later

//! Challenge selection, presented like application preferences.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use shpd_seedfinder_core::challenges::Challenges;

use crate::state::{ALL_CHALLENGES, AppState};

/// Presents the challenges dialog; `on_changed` runs after every toggle.
pub fn present(
    parent: &adw::ApplicationWindow,
    state: &Rc<RefCell<AppState>>,
    on_changed: &Rc<dyn Fn()>,
) {
    let group = adw::PreferencesGroup::builder()
        .title("Challenges")
        .description("Searches and scouting simulate runs with the selected challenges enabled.")
        .build();
    for info in ALL_CHALLENGES {
        let row = adw::SwitchRow::builder()
            .title(info.label)
            .subtitle(if info.changes_generation {
                "Changes level generation"
            } else {
                "No effect on seed content"
            })
            .active(state.borrow().challenges.contains(info.challenge))
            .build();
        let state = Rc::clone(state);
        let on_changed = Rc::clone(on_changed);
        row.connect_active_notify(move |row| {
            {
                let mut state = state.borrow_mut();
                let bits = if row.is_active() {
                    state.challenges.bits() | info.challenge.bits()
                } else {
                    state.challenges.bits() & !info.challenge.bits()
                };
                state.challenges = Challenges::new(bits).unwrap_or(Challenges::NONE);
            }
            on_changed();
        });
        group.add(&row);
    }

    let page = adw::PreferencesPage::new();
    page.add(&group);
    let dialog = adw::PreferencesDialog::builder()
        .title("Challenges")
        .content_width(440)
        .build();
    dialog.add(&page);
    dialog.present(Some(parent));
}
