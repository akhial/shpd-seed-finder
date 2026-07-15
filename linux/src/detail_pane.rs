// SPDX-License-Identifier: GPL-3.0-or-later

//! Seed pane: scout one seed and browse its item manifest by floor.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::rc::Rc;

use adw::prelude::*;
use shpd_seedfinder_core::catalog::{Effect, item};
use shpd_seedfinder_core::model::{Accessibility, GeneratedWorld, WorldItem};
use shpd_seedfinder_core::seed::DungeonSeed;
use shpd_seedfinder_session::production_scout_world;

use crate::scout_match::scout_match_indices;
use crate::state::{AppState, kind_icon, region, source_label};

pub struct DetailPane {
    pub page: adw::NavigationPage,
    entry: gtk::Entry,
    scout_button: gtk::Button,
    copy_button: gtk::Button,
    stack: gtk::Stack,
    summary_items: gtk::Label,
    summary_matches: gtk::Label,
    manifest_box: gtk::Box,
    world: RefCell<Option<GeneratedWorld>>,
    updating: Cell<bool>,
    toasts: adw::ToastOverlay,
    on_scout: RefCell<Option<Box<dyn Fn()>>>,
}

impl DetailPane {
    #[allow(clippy::too_many_lines)] // Widget assembly is declarative and linear.
    pub fn new(toasts: &adw::ToastOverlay) -> Rc<Self> {
        let entry = gtk::Entry::builder()
            .placeholder_text("AAA-AAA-AAA")
            .css_classes(["seed-entry"])
            .input_hints(gtk::InputHints::UPPERCASE_CHARS)
            .max_length(11)
            .hexpand(true)
            .build();
        let scout_button = gtk::Button::builder()
            .label("Scout")
            .css_classes(["suggested-action"])
            .sensitive(false)
            .build();
        let copy_button = gtk::Button::builder()
            .icon_name("edit-copy-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Copy Seed Code")
            .visible(false)
            .build();
        let entry_area = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .build();
        entry_area.append(&entry);
        entry_area.append(&scout_button);
        entry_area.append(&copy_button);
        let entry_clamp = adw::Clamp::builder()
            .child(&entry_area)
            .maximum_size(500)
            .build();

        let placeholder = adw::StatusPage::builder()
            .icon_name("mark-location-symbolic")
            .title("No Seed Scouted")
            .description(
                "Enter a seed code, or select a search result, \
                 to inspect its item manifest.",
            )
            .build();

        let summary_items = gtk::Label::builder()
            .css_classes(["caption", "dim-label", "numeric"])
            .xalign(0.0)
            .build();
        let summary_matches = gtk::Label::builder()
            .css_classes(["caption", "dim-label"])
            .xalign(0.0)
            .build();
        let summary_area = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .margin_start(12)
            .margin_end(12)
            .margin_top(9)
            .margin_bottom(3)
            .build();
        summary_area.append(&summary_items);
        summary_area.append(&summary_matches);

        let manifest_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(12)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();
        let manifest_clamp = adw::Clamp::builder()
            .child(&manifest_box)
            .maximum_size(600)
            .build();
        let manifest_scroller = gtk::ScrolledWindow::builder()
            .child(&manifest_clamp)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        let manifest_area = gtk::Box::new(gtk::Orientation::Vertical, 0);
        manifest_area.append(&summary_area);
        manifest_area.append(&manifest_scroller);

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        stack.add_named(&placeholder, Some("empty"));
        stack.add_named(&manifest_area, Some("manifest"));

        let header_bar = adw::HeaderBar::new();
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header_bar);
        toolbar_view.add_top_bar(&entry_clamp);
        toolbar_view.set_content(Some(&stack));

        let nav_page = adw::NavigationPage::builder()
            .title("Seed")
            .tag("seed")
            .child(&toolbar_view)
            .build();

        let pane = Rc::new(Self {
            page: nav_page,
            entry,
            scout_button,
            copy_button,
            stack,
            summary_items,
            summary_matches,
            manifest_box,
            world: RefCell::new(None),
            updating: Cell::new(false),
            toasts: toasts.clone(),
            on_scout: RefCell::new(None),
        });

        pane.entry.connect_changed({
            let pane = Rc::clone(&pane);
            move |entry| {
                if pane.updating.get() {
                    return;
                }
                pane.updating.set(true);
                let formatted = format_seed_input(&entry.text());
                if formatted != entry.text() {
                    entry.set_text(&formatted);
                    entry.set_position(-1);
                }
                pane.scout_button
                    .set_sensitive(DungeonSeed::from_code(&formatted).is_ok());
                pane.updating.set(false);
            }
        });
        pane.entry.connect_activate({
            let pane = Rc::clone(&pane);
            move |_| pane.request_scout()
        });
        pane.scout_button.connect_clicked({
            let pane = Rc::clone(&pane);
            move |_| pane.request_scout()
        });
        pane.copy_button.connect_clicked({
            let pane = Rc::clone(&pane);
            move |button| {
                if let Some(world) = pane.world.borrow().as_ref() {
                    let code = world.seed.to_code();
                    button.clipboard().set_text(&code);
                    pane.toasts
                        .add_toast(adw::Toast::new(&format!("Copied {code}")));
                }
            }
        });
        pane
    }

    /// Runs when the user asks to scout the entered seed; the window owns the
    /// query state and calls back into [`Self::scout`].
    pub fn connect_scout(&self, handler: impl Fn() + 'static) {
        self.on_scout.replace(Some(Box::new(handler)));
    }

    fn request_scout(&self) {
        if let Some(handler) = self.on_scout.borrow().as_ref() {
            handler();
        }
    }

    pub fn focus_entry(&self) {
        self.entry.grab_focus();
    }

    /// Scouts the seed in the entry, or `code` when given (also filling the
    /// entry), and renders its manifest against the current requirements.
    pub fn scout(&self, code: Option<&str>, state: &AppState) {
        if let Some(code) = code {
            self.updating.set(true);
            self.entry.set_text(&format_seed_input(code));
            self.scout_button.set_sensitive(true);
            self.updating.set(false);
        }
        let text = self.entry.text();
        let Ok(seed) = DungeonSeed::from_code(text.trim()) else {
            self.toasts
                .add_toast(adw::Toast::new("Seed codes use the AAA-AAA-AAA format"));
            return;
        };
        let Ok(world) = production_scout_world(seed, state.challenges) else {
            self.toasts.add_toast(adw::Toast::new(
                "World generation failed for this seed; please report it",
            ));
            return;
        };
        self.world.replace(Some(world));
        self.render(state);
    }

    /// Re-renders the manifest, e.g. after the requirements changed.
    pub fn render(&self, state: &AppState) {
        let world = self.world.borrow();
        let Some(world) = world.as_ref() else {
            self.stack.set_visible_child_name("empty");
            self.copy_button.set_visible(false);
            return;
        };
        self.stack.set_visible_child_name("manifest");
        self.copy_button.set_visible(true);

        let matches = scout_match_indices(
            &world.items,
            &state.requirements,
            state.max_depth,
            state.exclude_blacksmith_rewards,
        );
        let mut by_depth: BTreeMap<u8, Vec<usize>> = BTreeMap::new();
        for (index, world_item) in world.items.iter().enumerate() {
            by_depth.entry(world_item.depth).or_default().push(index);
        }

        self.summary_items.set_label(&format!(
            "{} items across {} floors",
            world.items.len(),
            by_depth.len()
        ));
        if state.requirements.is_empty() {
            self.summary_matches.set_label("");
        } else {
            self.summary_matches.set_label(&format!(
                "· {} requirement match{}",
                matches.len(),
                if matches.len() == 1 { "" } else { "es" }
            ));
        }
        if matches.is_empty() {
            self.summary_matches.remove_css_class("success");
            self.summary_matches.add_css_class("dim-label");
        } else {
            self.summary_matches.remove_css_class("dim-label");
            self.summary_matches.add_css_class("success");
        }

        while let Some(child) = self.manifest_box.first_child() {
            self.manifest_box.remove(&child);
        }
        for (depth, indices) in &by_depth {
            let group = adw::PreferencesGroup::builder()
                .title(format!("Floor {depth}"))
                .description(region(*depth))
                .build();
            for index in indices {
                group.add(&item_row(&world.items[*index], matches.contains(index)));
            }
            self.manifest_box.append(&group);
        }
    }
}

fn item_row(world_item: &WorldItem, matched: bool) -> adw::ActionRow {
    let mut subtitle = source_label(world_item.source).to_owned();
    match world_item.accessibility {
        Accessibility::Independent => {}
        Accessibility::Choice { group, option } => {
            let _ = write!(
                subtitle,
                "\nOne reward of choice group {group} (option {})",
                option + 1
            );
        }
        Accessibility::Scenarios { group, .. } => {
            let _ = write!(
                subtitle,
                "\nOnly in some outcomes of scenario group {group}"
            );
        }
    }

    let definition = item(world_item.item);
    let row = adw::ActionRow::builder()
        .title(gtk::glib::markup_escape_text(definition.name))
        .subtitle(gtk::glib::markup_escape_text(&subtitle))
        .build();
    row.add_prefix(&gtk::Image::from_icon_name(kind_icon(definition.kind)));

    if world_item.upgrade > 0 {
        let upgrade = gtk::Label::builder()
            .label(format!("+{}", world_item.upgrade))
            .css_classes(["caption-heading", "success"])
            .valign(gtk::Align::Center)
            .build();
        row.add_suffix(&upgrade);
    }
    if let Some(effect) = world_item.effect {
        let cursed_effect = match effect {
            Effect::Weapon(weapon_effect) => weapon_effect.is_curse(),
            Effect::Armor(armor_effect) => armor_effect.is_curse(),
        };
        row.add_suffix(&tag(
            effect.wire_name(),
            if cursed_effect { "error" } else { "accent" },
        ));
    }
    if world_item.cursed {
        row.add_suffix(&tag("Cursed", "error"));
    }
    if matched {
        let badge = tag("Match", "success");
        badge.set_tooltip_text(Some(
            "Selected as part of a jointly obtainable requirement match",
        ));
        row.add_suffix(&badge);
    }
    row
}

fn tag(label: &str, color: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(label)
        .css_classes(["tag", color])
        .valign(gtk::Align::Center)
        .build()
}

/// Canonicalizes seed input as the user types: uppercase base-26 letters in
/// dash-separated groups of three.
fn format_seed_input(input: &str) -> String {
    let letters: Vec<char> = input
        .chars()
        .filter(char::is_ascii_alphabetic)
        .map(|character| character.to_ascii_uppercase())
        .take(9)
        .collect();
    let mut output = String::with_capacity(11);
    for (index, letter) in letters.iter().enumerate() {
        if index == 3 || index == 6 {
            output.push('-');
        }
        output.push(*letter);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::format_seed_input;

    #[test]
    fn seed_input_is_canonicalized_while_typing() {
        assert_eq!(format_seed_input(""), "");
        assert_eq!(format_seed_input("swl"), "SWL");
        assert_eq!(format_seed_input("swlk"), "SWL-K");
        assert_eq!(format_seed_input("swl-kgn-qfd"), "SWL-KGN-QFD");
        assert_eq!(format_seed_input("s1w!l kg"), "SWL-KG");
        assert_eq!(format_seed_input("abcdefghijkl"), "ABC-DEF-GHI");
    }
}
