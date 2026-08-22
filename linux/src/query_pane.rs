// SPDX-License-Identifier: GPL-3.0-or-later

//! Query-builder sidebar: requirements, search scope, and the search action.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;

use shpd_seedfinder_core::feasibility::Quest;
use shpd_seedfinder_core::main_world::normalize_floor_limit;
use shpd_seedfinder_core::query::MAX_SEARCH_DEPTH;
use shpd_seedfinder_core::quests::WandmakerQuestType;

use crate::state::{
    AppState, UiRequirement, floor_limit_skip_target, kind_icon, wandmaker_quest_label,
};
use crate::{glow, sprites};

/// Makes a floor-limit spin row skip the empty boss floors (5, 10, 15):
/// spinning up from 4 lands on 6, spinning down from 6 lands on 4, and typed
/// values snap down (10 means the first 10 floors, ≡ 9), since those floors
/// add no searchable items and are useless as limits.
pub fn skip_empty_boss_floors(row: &adw::SpinRow) {
    let previous = Cell::new(row.value());
    row.connect_value_notify(move |row| {
        let value = row.value();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let requested = value.round().clamp(0.0, f64::from(MAX_SEARCH_DEPTH)) as u8;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let anchor = previous
            .get()
            .round()
            .clamp(0.0, f64::from(MAX_SEARCH_DEPTH)) as u8;
        let target = floor_limit_skip_target(anchor, requested);
        if target != requested {
            // The corrected value re-enters this handler and, being a real
            // floor, records itself as the new anchor.
            row.set_value(f64::from(target));
            return;
        }
        previous.set(value);
    });
}

type KeyHandler = Box<dyn Fn(u64)>;

pub struct QueryPane {
    pub page: adw::NavigationPage,
    requirements_group: adw::PreferencesGroup,
    /// One boxed list per run of plain rows, one card per "any of these"
    /// group, in slot order.
    rows: gtk::Box,
    depth_row: adw::SpinRow,
    blacksmith_row: adw::SwitchRow,
    exclude_row: adw::SwitchRow,
    wandmaker_row: adw::ComboRow,
    fast_row: adw::SwitchRow,
    start_content: adw::ButtonContent,
    start_button: gtk::Button,
    challenges_button: gtk::Button,
    updating: Cell<bool>,
    on_edit: RefCell<Option<KeyHandler>>,
    on_remove: RefCell<Option<KeyHandler>>,
    on_add_alternative: RefCell<Option<KeyHandler>>,
    on_changed: RefCell<Option<Box<dyn Fn()>>>,
}

impl QueryPane {
    #[allow(clippy::too_many_lines)] // Widget assembly is declarative and linear.
    pub fn new(menu: &gio::MenuModel) -> Rc<Self> {
        let presets_group = adw::PreferencesGroup::builder().title("Presets").build();
        let presets_row = adw::ActionRow::builder()
            .title("Manage presets")
            .subtitle("Load an included query or save the current one")
            .action_name("win.presets")
            .activatable(true)
            .build();
        presets_row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        presets_group.add(&presets_row);

        let add_button = gtk::Button::builder()
            .child(
                &adw::ButtonContent::builder()
                    .icon_name("list-add-symbolic")
                    .label("Add")
                    .build(),
            )
            .action_name("win.add-requirement")
            .css_classes(["flat"])
            .tooltip_text("Add Requirement")
            .build();
        let requirements_group = adw::PreferencesGroup::builder()
            .title("Requirements")
            .description("Every requirement must be satisfiable in the same run.")
            .header_suffix(&add_button)
            .build();
        let rows = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .build();
        requirements_group.add(&rows);

        let depth_row = adw::SpinRow::builder()
            .title("Floor limit")
            .subtitle("Search only the first floors")
            .adjustment(&gtk::Adjustment::new(
                f64::from(MAX_SEARCH_DEPTH),
                1.0,
                f64::from(MAX_SEARCH_DEPTH),
                1.0,
                5.0,
                0.0,
            ))
            .build();
        let blacksmith_row = adw::SwitchRow::builder()
            .title("Require accessible blacksmith")
            .subtitle("Always in range when searching 14 floors or more")
            .build();
        let exclude_row = adw::SwitchRow::builder()
            .title("Exclude Smith rewards")
            .subtitle(
                "Required items cannot come from the 2,000-favor Smith choice, \
                 leaving favor available for reforging",
            )
            .build();
        // Index zero is "Any"; the rest follow WandmakerQuestType::ALL.
        let wandmaker_labels = std::iter::once("Any")
            .chain(
                WandmakerQuestType::ALL
                    .into_iter()
                    .map(wandmaker_quest_label),
            )
            .collect::<Vec<_>>();
        let wandmaker_row = adw::ComboRow::builder()
            .title("Quest")
            .model(&gtk::StringList::new(&wandmaker_labels))
            .build();
        let wandmaker_group = adw::PreferencesGroup::builder().title("Wandmaker").build();
        wandmaker_group.add(&wandmaker_row);

        let scope_group = adw::PreferencesGroup::builder()
            .title("Search Scope")
            .build();
        scope_group.add(&depth_row);

        let blacksmith_group = adw::PreferencesGroup::builder().title("Blacksmith").build();
        blacksmith_group.add(&blacksmith_row);
        blacksmith_group.add(&exclude_row);

        let fast_row = adw::SwitchRow::builder()
            .title("Fast search")
            .subtitle(
                "Treats +3 weapons and armor as quest rewards only, skipping the rare \
                 Crypt and Sacrificial-fire prizes. Found seeds are always genuine",
            )
            .build();
        let performance_group = adw::PreferencesGroup::builder()
            .title("Performance")
            .build();
        performance_group.add(&fast_row);

        let preferences_page = adw::PreferencesPage::new();
        preferences_page.add(&presets_group);
        preferences_page.add(&requirements_group);
        preferences_page.add(&scope_group);
        preferences_page.add(&wandmaker_group);
        preferences_page.add(&blacksmith_group);
        preferences_page.add(&performance_group);

        let challenges_button = gtk::Button::builder()
            .css_classes(["flat", "caption"])
            .action_name("win.challenges")
            .halign(gtk::Align::Center)
            .visible(false)
            .build();
        let start_content = adw::ButtonContent::builder()
            .icon_name("media-playback-start-symbolic")
            .label("Start Search")
            .build();
        let start_button = gtk::Button::builder()
            .child(&start_content)
            .css_classes(["pill", "suggested-action"])
            .action_name("win.start-search")
            .build();
        let action_area = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(12)
            .margin_start(18)
            .margin_end(18)
            .build();
        action_area.append(&challenges_button);
        action_area.append(&start_button);

        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(menu)
            .primary(true)
            .tooltip_text("Main Menu")
            .build();
        let header_bar = adw::HeaderBar::new();
        header_bar.pack_end(&menu_button);

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header_bar);
        toolbar_view.add_bottom_bar(&action_area);
        toolbar_view.set_content(Some(&preferences_page));

        let nav_page = adw::NavigationPage::builder()
            .title("Seed Seeker")
            .tag("query")
            .child(&toolbar_view)
            .build();

        let pane = Rc::new(Self {
            page: nav_page,
            requirements_group,
            rows,
            depth_row,
            blacksmith_row,
            exclude_row,
            wandmaker_row,
            fast_row,
            start_content,
            start_button,
            challenges_button,
            updating: Cell::new(false),
            on_edit: RefCell::new(None),
            on_remove: RefCell::new(None),
            on_add_alternative: RefCell::new(None),
            on_changed: RefCell::new(None),
        });

        skip_empty_boss_floors(&pane.depth_row);
        pane.depth_row.connect_value_notify({
            let pane = Rc::clone(&pane);
            move |_| pane.notify_changed()
        });
        for row in [&pane.blacksmith_row, &pane.exclude_row, &pane.fast_row] {
            row.connect_active_notify({
                let pane = Rc::clone(&pane);
                move |_| pane.notify_changed()
            });
        }
        pane.wandmaker_row.connect_selected_notify({
            let pane = Rc::clone(&pane);
            move |_| pane.notify_changed()
        });
        pane
    }

    fn notify_changed(&self) {
        if self.updating.get() {
            return;
        }
        if let Some(handler) = self.on_changed.borrow().as_ref() {
            handler();
        }
    }

    pub fn connect_edit(&self, handler: impl Fn(u64) + 'static) {
        self.on_edit.replace(Some(Box::new(handler)));
    }

    pub fn connect_remove(&self, handler: impl Fn(u64) + 'static) {
        self.on_remove.replace(Some(Box::new(handler)));
    }

    /// Runs when the user asks for an alternative to the row `key`.
    pub fn connect_add_alternative(&self, handler: impl Fn(u64) + 'static) {
        self.on_add_alternative.replace(Some(Box::new(handler)));
    }

    fn call(handler: &RefCell<Option<KeyHandler>>, key: u64) {
        if let Some(handler) = handler.borrow().as_ref() {
            handler(key);
        }
    }

    /// Runs after the user changes any scope or performance control.
    pub fn connect_changed(&self, handler: impl Fn() + 'static) {
        self.on_changed.replace(Some(Box::new(handler)));
    }

    /// Copies the scope and performance controls into `state`.
    pub fn read_scope(&self, state: &mut AppState) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let depth = self.depth_row.value().round() as u8;
        state.max_depth = normalize_floor_limit(depth.clamp(1, MAX_SEARCH_DEPTH));
        state.require_blacksmith = self.blacksmith_row.is_active();
        state.exclude_blacksmith_rewards = self.exclude_row.is_active();
        state.wandmaker_quest = usize::try_from(self.wandmaker_row.selected())
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| WandmakerQuestType::ALL.get(index).copied());
        state.fast_mode = self.fast_row.is_active();
    }

    /// Rebuilds every control from `state` without echoing change signals.
    pub fn refresh(self: &Rc<Self>, state: &AppState) {
        self.updating.set(true);
        self.depth_row
            .set_value(f64::from(normalize_floor_limit(state.max_depth)));
        self.blacksmith_row.set_active(state.require_blacksmith);
        // Past the Blacksmith's last possible floor every seed has the
        // quest, so the filter has nothing left to exclude.
        self.blacksmith_row
            .set_sensitive(state.max_depth < Quest::Blacksmith.window().1);
        self.exclude_row
            .set_active(state.exclude_blacksmith_rewards);
        self.wandmaker_row.set_selected(
            state
                .wandmaker_quest
                .map_or(0, |variant| u32::from(variant.wire_id())),
        );
        self.fast_row.set_active(state.fast_mode);
        self.rebuild_rows(state);
        let enabled = state.challenges.bits().count_ones();
        self.challenges_button.set_visible(enabled > 0);
        self.challenges_button.set_label(&format!(
            "{enabled} challenge{} enabled",
            if enabled == 1 { "" } else { "s" }
        ));
        self.updating.set(false);
    }

    fn rebuild_rows(self: &Rc<Self>, state: &AppState) {
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        self.requirements_group
            .set_title(&requirements_title(state.slot_count()));
        if state.requirements.is_empty() {
            let list = boxed_list();
            let row = adw::ActionRow::builder()
                .title("No requirements yet")
                .subtitle("Add one to describe the item you are hunting for")
                .build();
            row.add_css_class("dim-label");
            list.append(&row);
            self.rows.append(&list);
            return;
        }
        // Consecutive plain rows share one boxed list; every "any of these"
        // group is a card of its own, so the sidebar reads slot by slot.
        let mut plain: Option<gtk::ListBox> = None;
        for slot in state.slots() {
            if let [index] = slot[..] {
                let list = plain.get_or_insert_with(|| {
                    let list = boxed_list();
                    self.rows.append(&list);
                    list
                });
                list.append(&self.requirement_row(&state.requirements[index], false));
            } else {
                plain = None;
                self.rows.append(&self.alternative_card(state, &slot));
            }
        }
    }

    /// One "any of these" slot: a header with its own add action, then the
    /// members separated by "or".
    fn alternative_card(self: &Rc<Self>, state: &AppState, members: &[usize]) -> gtk::ListBox {
        let card = boxed_list();
        let add_button = gtk::Button::builder()
            .child(
                &adw::ButtonContent::builder()
                    .icon_name("list-add-symbolic")
                    .label("Alternative")
                    .build(),
            )
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .tooltip_text("Add Alternative")
            .build();
        let header = adw::ActionRow::builder()
            .title("Any of these")
            .subtitle("One of these items satisfies the requirement")
            .build();
        header.add_css_class("property");
        header.add_suffix(&add_button);
        card.append(&header);
        if let Some(first) = members.first().copied() {
            let key = state.requirements[first].key;
            add_button.connect_clicked({
                let pane = Rc::clone(self);
                move |_| Self::call(&pane.on_add_alternative, key)
            });
        }
        for (position, index) in members.iter().enumerate() {
            if position > 0 {
                card.append(&or_separator());
            }
            card.append(&self.requirement_row(&state.requirements[*index], true));
        }
        card
    }

    /// One requirement as an activatable row with its remove action; rows
    /// outside a group also offer to fork into an "any of these" group (a
    /// member's card carries that action instead).
    fn requirement_row(
        self: &Rc<Self>,
        requirement: &UiRequirement,
        in_group: bool,
    ) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(gtk::glib::markup_escape_text(&requirement.title()))
            .subtitle(gtk::glib::markup_escape_text(&requirement.subtitle()))
            .activatable(true)
            .build();
        row.add_prefix(&requirement_prefix(requirement));
        let key = requirement.key;
        if !in_group {
            let fork_button = gtk::Button::builder()
                .icon_name("add-alternative-symbolic")
                .css_classes(["flat"])
                .valign(gtk::Align::Center)
                .tooltip_text("Add Alternative")
                .build();
            fork_button.connect_clicked({
                let pane = Rc::clone(self);
                move |_| Self::call(&pane.on_add_alternative, key)
            });
            row.add_suffix(&fork_button);
        }
        let remove_button = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .tooltip_text(if in_group {
                "Remove Alternative"
            } else {
                "Remove Requirement"
            })
            .build();
        row.add_suffix(&remove_button);
        row.connect_activated({
            let pane = Rc::clone(self);
            move |_| Self::call(&pane.on_edit, key)
        });
        remove_button.connect_clicked({
            let pane = Rc::clone(self);
            move |_| Self::call(&pane.on_remove, key)
        });
        row
    }

    /// Flips the search action between its start and stop presentation.
    pub fn set_running(&self, running: bool) {
        if running {
            self.start_content
                .set_icon_name("media-playback-stop-symbolic");
            self.start_content.set_label("Stop Search");
            self.start_button.remove_css_class("suggested-action");
            self.start_button.add_css_class("destructive-action");
        } else {
            self.start_content
                .set_icon_name("media-playback-start-symbolic");
            self.start_content.set_label("Start Search");
            self.start_button.remove_css_class("destructive-action");
            self.start_button.add_css_class("suggested-action");
        }
    }
}

/// The row icon for one requirement: the item's real sprite once a concrete
/// item is pinned, pulsing the enchantment or curse the requirement asks for,
/// and otherwise the family's symbolic icon — a wildcard requirement depicts no
/// particular item.
fn requirement_prefix(requirement: &UiRequirement) -> gtk::Widget {
    match requirement.item {
        Some(item_id) => sprites::item_image(
            shpd_seedfinder_core::catalog::item(item_id),
            glow::effect(requirement.pinned_effect()),
        ),
        None => {
            gtk::Image::from_icon_name(kind_icon(requirement.kind, requirement.weapon_category))
                .upcast()
        }
    }
}

fn boxed_list() -> gtk::ListBox {
    gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::None)
        .build()
}

/// The inert "or" row between the members of an "any of these" card.
fn or_separator() -> gtk::ListBoxRow {
    let label = gtk::Label::builder()
        .label("OR")
        .css_classes(["caption-heading", "dim-label"])
        .margin_top(2)
        .margin_bottom(2)
        .build();
    gtk::ListBoxRow::builder()
        .activatable(false)
        .selectable(false)
        .focusable(false)
        .child(&label)
        .build()
}

/// The requirements header, counting slots: an "any of these" group is one
/// requirement however many alternatives it lists.
fn requirements_title(slots: usize) -> String {
    if slots == 0 {
        "Requirements".to_owned()
    } else {
        format!("Requirements ({slots})")
    }
}

#[cfg(test)]
mod tests {
    use super::requirements_title;

    #[test]
    fn requirements_header_counts_slots() {
        assert_eq!(requirements_title(0), "Requirements");
        assert_eq!(requirements_title(3), "Requirements (3)");
    }
}
