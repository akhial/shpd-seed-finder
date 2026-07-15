// SPDX-License-Identifier: GPL-3.0-or-later

//! Preset loading, saving, and deletion.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::persist::UserPreset;
use crate::state::AppState;
use crate::{persist, presets};

#[allow(clippy::too_many_lines)] // Dialog assembly is declarative and linear.
pub fn present(
    parent: &adw::ApplicationWindow,
    toasts: &adw::ToastOverlay,
    state: &Rc<RefCell<AppState>>,
    user_presets: &Rc<RefCell<Vec<UserPreset>>>,
    on_changed: &Rc<dyn Fn()>,
) {
    let dialog = adw::PreferencesDialog::builder()
        .title("Presets")
        .content_width(520)
        .build();
    let page = adw::PreferencesPage::new();

    let included_group = adw::PreferencesGroup::builder()
        .title("Included")
        .description("Ready-made queries shipped with Seed Seeker.")
        .build();
    for preset in presets::built_in() {
        let row = adw::ActionRow::builder()
            .title(preset.name)
            .subtitle("Included with the app")
            .build();
        let load = gtk::Button::builder()
            .label("Load")
            .valign(gtk::Align::Center)
            .build();
        row.add_suffix(&load);
        load.connect_clicked({
            let dialog = dialog.clone();
            let state = Rc::clone(state);
            let on_changed = Rc::clone(on_changed);
            move |_| {
                *state.borrow_mut() = preset.state.clone();
                on_changed();
                let _ = dialog.close();
            }
        });
        included_group.add(&row);
    }
    page.add(&included_group);

    let saved_group = adw::PreferencesGroup::builder()
        .title("Saved")
        .description(if user_presets.borrow().is_empty() {
            "No saved presets yet."
        } else {
            "Presets created on this device."
        })
        .build();
    for preset in user_presets.borrow().iter().cloned() {
        let row = adw::ActionRow::builder().title(&preset.name).build();
        let load = gtk::Button::builder()
            .label("Load")
            .valign(gtk::Align::Center)
            .build();
        let delete = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .tooltip_text("Delete Preset")
            .build();
        row.add_suffix(&load);
        row.add_suffix(&delete);

        load.connect_clicked({
            let dialog = dialog.clone();
            let state = Rc::clone(state);
            let on_changed = Rc::clone(on_changed);
            let preset_state = preset.state.clone();
            move |_| {
                *state.borrow_mut() = preset_state.clone();
                on_changed();
                let _ = dialog.close();
            }
        });
        delete.connect_clicked({
            let dialog = dialog.clone();
            let toasts = toasts.clone();
            let user_presets = Rc::clone(user_presets);
            let preset_name = preset.name.clone();
            move |_| {
                user_presets
                    .borrow_mut()
                    .retain(|saved| saved.name != preset_name);
                persist::save_presets(&user_presets.borrow());
                toasts.add_toast(adw::Toast::new("Preset deleted"));
                let _ = dialog.close();
            }
        });
        saved_group.add(&row);
    }
    page.add(&saved_group);

    let create_group = adw::PreferencesGroup::builder()
        .title("Save Current Query")
        .description("Using an existing name updates that saved preset.")
        .build();
    let name = adw::EntryRow::builder().title("Preset name").build();
    let save = gtk::Button::builder()
        .label("Save")
        .css_classes(["suggested-action"])
        .valign(gtk::Align::Center)
        .build();
    name.add_suffix(&save);
    save.connect_clicked({
        let dialog = dialog.clone();
        let name = name.clone();
        let state = Rc::clone(state);
        let toasts = toasts.clone();
        let user_presets = Rc::clone(user_presets);
        move |_| {
            let clean_name = name.text().trim().to_owned();
            if clean_name.is_empty() {
                toasts.add_toast(adw::Toast::new("Enter a preset name"));
                return;
            }
            if presets::built_in()
                .iter()
                .any(|preset| preset.name.eq_ignore_ascii_case(&clean_name))
            {
                toasts.add_toast(adw::Toast::new("That name belongs to an included preset"));
                return;
            }

            let mut saved = user_presets.borrow_mut();
            if let Some(existing) = saved
                .iter_mut()
                .find(|preset| preset.name.eq_ignore_ascii_case(&clean_name))
            {
                existing.state = state.borrow().clone();
            } else {
                saved.push(UserPreset {
                    name: clean_name,
                    state: state.borrow().clone(),
                });
            }
            persist::save_presets(&saved);
            drop(saved);
            toasts.add_toast(adw::Toast::new("Preset saved"));
            let _ = dialog.close();
        }
    });
    create_group.add(&name);
    page.add(&create_group);

    dialog.add(&page);
    dialog.present(Some(parent));
}
