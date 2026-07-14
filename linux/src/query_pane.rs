// SPDX-License-Identifier: GPL-3.0-or-later

//! Query-builder sidebar: requirements, search scope, and the search action.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;

use crate::state::{AppState, kind_icon};

type KeyHandler = Box<dyn Fn(u64)>;

pub struct QueryPane {
    pub page: adw::NavigationPage,
    list: gtk::ListBox,
    depth_row: adw::SpinRow,
    blacksmith_row: adw::SwitchRow,
    exclude_row: adw::SwitchRow,
    fast_row: adw::SwitchRow,
    start_content: adw::ButtonContent,
    start_button: gtk::Button,
    challenges_button: gtk::Button,
    updating: Cell<bool>,
    on_edit: RefCell<Option<KeyHandler>>,
    on_remove: RefCell<Option<KeyHandler>>,
    on_changed: RefCell<Option<Box<dyn Fn()>>>,
}

impl QueryPane {
    #[allow(clippy::too_many_lines)] // Widget assembly is declarative and linear.
    pub fn new(menu: &gio::MenuModel) -> Rc<Self> {
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
        let list = gtk::ListBox::builder()
            .css_classes(["boxed-list"])
            .selection_mode(gtk::SelectionMode::None)
            .build();
        requirements_group.add(&list);

        let depth_row = adw::SpinRow::builder()
            .title("Floor limit")
            .subtitle("Search only the first floors")
            .adjustment(&gtk::Adjustment::new(24.0, 1.0, 24.0, 1.0, 5.0, 0.0))
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
        let scope_group = adw::PreferencesGroup::builder()
            .title("Search Scope")
            .build();
        scope_group.add(&depth_row);
        scope_group.add(&blacksmith_row);
        scope_group.add(&exclude_row);

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
        preferences_page.add(&requirements_group);
        preferences_page.add(&scope_group);
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
            list,
            depth_row,
            blacksmith_row,
            exclude_row,
            fast_row,
            start_content,
            start_button,
            challenges_button,
            updating: Cell::new(false),
            on_edit: RefCell::new(None),
            on_remove: RefCell::new(None),
            on_changed: RefCell::new(None),
        });

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

    /// Runs after the user changes any scope or performance control.
    pub fn connect_changed(&self, handler: impl Fn() + 'static) {
        self.on_changed.replace(Some(Box::new(handler)));
    }

    /// Copies the scope and performance controls into `state`.
    pub fn read_scope(&self, state: &mut AppState) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let depth = self.depth_row.value().round() as u8;
        state.max_depth = depth.clamp(1, 24);
        state.require_blacksmith = self.blacksmith_row.is_active();
        state.exclude_blacksmith_rewards = self.exclude_row.is_active();
        state.fast_mode = self.fast_row.is_active();
    }

    /// Rebuilds every control from `state` without echoing change signals.
    pub fn refresh(self: &Rc<Self>, state: &AppState) {
        self.updating.set(true);
        self.depth_row.set_value(f64::from(state.max_depth));
        self.blacksmith_row.set_active(state.require_blacksmith);
        self.blacksmith_row.set_sensitive(state.max_depth < 14);
        self.exclude_row
            .set_active(state.exclude_blacksmith_rewards);
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
        self.list.remove_all();
        if state.requirements.is_empty() {
            let row = adw::ActionRow::builder()
                .title("No requirements yet")
                .subtitle("Add one to describe the item you are hunting for")
                .build();
            row.add_css_class("dim-label");
            self.list.append(&row);
            return;
        }
        for requirement in &state.requirements {
            let remove_button = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .css_classes(["flat"])
                .valign(gtk::Align::Center)
                .tooltip_text("Remove Requirement")
                .build();
            let row = adw::ActionRow::builder()
                .title(gtk::glib::markup_escape_text(&requirement.title()))
                .subtitle(gtk::glib::markup_escape_text(&requirement.subtitle()))
                .activatable(true)
                .build();
            row.add_prefix(&gtk::Image::from_icon_name(kind_icon(requirement.kind)));
            row.add_suffix(&remove_button);

            let key = requirement.key;
            row.connect_activated({
                let pane = Rc::clone(self);
                move |_| {
                    if let Some(handler) = pane.on_edit.borrow().as_ref() {
                        handler(key);
                    }
                }
            });
            remove_button.connect_clicked({
                let pane = Rc::clone(self);
                move |_| {
                    if let Some(handler) = pane.on_remove.borrow().as_ref() {
                        handler(key);
                    }
                }
            });
            self.list.append(&row);
        }
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
