// SPDX-License-Identifier: GPL-3.0-or-later

//! Query-builder sidebar: the requirement board, search scope, and the search
//! action.

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{cairo, gdk, gio, glib, pango};

use shpd_seedfinder_core::feasibility::Quest;
use shpd_seedfinder_core::main_world::normalize_floor_limit;
use shpd_seedfinder_core::query::{
    EffectRequirement, EffectSet, MAX_SEARCH_DEPTH, TierRequirement, UpgradeRequirement,
};
use shpd_seedfinder_core::quests::WandmakerQuestType;

use crate::relations::{BoardItem, STACK_MAX};
use crate::state::{
    AppState, UiRequirement, effect_label, floor_limit_skip_target, kind_icon,
    wandmaker_quest_label,
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

/// What the board asks the window to do with a requirement. Every variant
/// names rows by their session key, which survives the list moving under it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardAction {
    /// Open the editor on the row.
    Edit(u64),
    /// Make `source` an either/or alternative of `target`.
    Join { source: u64, target: u64 },
    /// Pull the row out of its cluster.
    Detach(u64),
    /// Delete the row: a cluster member alone, a lone chip with its stack.
    Remove(u64),
    /// Ask the row's board entry for `count` items.
    Count { key: u64, count: usize },
    /// Set or clear the entry's combined level.
    Total { key: u64, total: Option<u8> },
}

/// What the window does with one board gesture.
type BoardHandler = Box<dyn Fn(BoardAction)>;

/// Which number the stack popover is editing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StackField {
    Count,
    Total,
}

pub struct QueryPane {
    pub page: adw::NavigationPage,
    requirements_group: adw::PreferencesGroup,
    /// Holds the board and the drop-to-remove zone, and parents the popovers
    /// so a rebuild of the chips cannot pull them out from under the pointer.
    board_root: gtk::Box,
    /// One chip or cluster capsule per board entry, wrapping as they fill.
    board: adw::WrapBox,
    remove_revealer: gtk::Revealer,
    menu: gtk::PopoverMenu,
    /// The "How many" radio action, whose state is set to the chip's own
    /// count just before its menu opens so the right item is ticked.
    count_action: gio::SimpleAction,
    stack_popover: gtk::Popover,
    stack_title: gtk::Label,
    stack_spin: gtk::SpinButton,
    /// The row and number the stack popover is editing, and the value it
    /// opened on — the change lands when the popover closes, so spinning
    /// never rebuilds the board out from under the pointer.
    stack_target: Cell<Option<(u64, StackField)>>,
    stack_opened_on: Cell<f64>,
    depth_row: adw::SpinRow,
    blacksmith_row: adw::SwitchRow,
    exclude_row: adw::SwitchRow,
    wandmaker_row: adw::ComboRow,
    fast_row: adw::SwitchRow,
    start_content: adw::ButtonContent,
    start_button: gtk::Button,
    challenges_button: gtk::Button,
    updating: Cell<bool>,
    on_board: RefCell<Option<BoardHandler>>,
    on_changed: RefCell<Option<Box<dyn Fn()>>>,
}

impl QueryPane {
    #[allow(clippy::too_many_lines)] // Widget assembly is declarative and linear.
    pub fn new(menu_model: &gio::MenuModel) -> Rc<Self> {
        let presets_group = adw::PreferencesGroup::builder().title("Presets").build();
        let presets_row = adw::ActionRow::builder()
            .title("Manage presets")
            .subtitle("Load an included query or save the current one")
            .action_name("win.presets")
            .activatable(true)
            .build();
        presets_row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        presets_group.add(&presets_row);

        let requirements_group = adw::PreferencesGroup::builder()
            .title("Requirements")
            .description(
                "Every requirement must be satisfiable in the same run. \
                 Drop one chip on another for an either/or.",
            )
            .build();
        let board = adw::WrapBox::builder()
            .child_spacing(6)
            .line_spacing(6)
            .build();
        let remove_zone = gtk::Box::builder()
            .halign(gtk::Align::Fill)
            .spacing(6)
            .css_classes(["remove-zone"])
            .build();
        let remove_icon = gtk::Image::from_icon_name("user-trash-symbolic");
        let remove_label = gtk::Label::builder()
            .label("Drop to remove")
            .hexpand(true)
            .build();
        remove_zone.append(&remove_icon);
        remove_zone.append(&remove_label);
        let remove_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .child(&remove_zone)
            .build();
        let board_root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        board_root.append(&board);
        board_root.append(&remove_revealer);
        requirements_group.add(&board_root);

        let stack_title = gtk::Label::builder()
            .css_classes(["caption-heading"])
            .halign(gtk::Align::Start)
            .build();
        let stack_spin = gtk::SpinButton::builder()
            .adjustment(&gtk::Adjustment::new(1.0, 1.0, 3.0, 1.0, 1.0, 0.0))
            .numeric(true)
            .build();
        let stack_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        stack_box.append(&stack_title);
        stack_box.append(&stack_spin);
        let stack_popover = gtk::Popover::builder().child(&stack_box).build();

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
                "Treats +3 weapons and armor as quest rewards only — the Ghost, the \
                 Blacksmith and the Imp's vault — skipping the rare Crypt, \
                 Sacrificial-fire and special-room chest prizes, so such a search \
                 ends at floor 19 rather than the floor limit. Found seeds are \
                 always genuine",
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
            .menu_model(menu_model)
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
            board_root,
            board,
            remove_revealer,
            menu: gtk::PopoverMenu::from_model(None::<&gio::Menu>),
            count_action: gio::SimpleAction::new_stateful(
                "count",
                Some(count_variant_type()),
                &(0_u64, 0_u64).to_variant(),
            ),
            stack_popover,
            stack_title,
            stack_spin,
            stack_target: Cell::new(None),
            stack_opened_on: Cell::new(1.0),
            depth_row,
            blacksmith_row,
            exclude_row,
            wandmaker_row,
            fast_row,
            start_content,
            start_button,
            challenges_button,
            updating: Cell::new(false),
            on_board: RefCell::new(None),
            on_changed: RefCell::new(None),
        });

        pane.menu.set_parent(&pane.board_root);
        pane.menu.set_position(gtk::PositionType::Bottom);
        pane.menu.set_has_arrow(true);
        pane.stack_popover.set_parent(&pane.board_root);
        pane.stack_popover.set_position(gtk::PositionType::Bottom);
        pane.board_root
            .insert_action_group("board", Some(&pane.board_actions()));
        pane.stack_popover.connect_closed({
            let pane = Rc::clone(&pane);
            move |_| pane.commit_stack()
        });
        // Dropping a cluster member on the board's own background — anywhere
        // no chip sits — pulls it out of its cluster.
        pane.board
            .add_controller(pane.drop_target(move |pane, key| {
                pane.emit(BoardAction::Detach(key));
            }));
        remove_zone.add_controller(pane.drop_target(move |pane, key| {
            pane.emit(BoardAction::Remove(key));
        }));

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

    /// Runs when the board asks for a change to the requirements.
    pub fn connect_board(&self, handler: impl Fn(BoardAction) + 'static) {
        self.on_board.replace(Some(Box::new(handler)));
    }

    /// Runs after the user changes any scope or performance control.
    pub fn connect_changed(&self, handler: impl Fn() + 'static) {
        self.on_changed.replace(Some(Box::new(handler)));
    }

    fn emit(&self, action: BoardAction) {
        if let Some(handler) = self.on_board.borrow().as_ref() {
            handler(action);
        }
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
        self.rebuild_board(state);
        let enabled = state.challenges.bits().count_ones();
        self.challenges_button.set_visible(enabled > 0);
        self.challenges_button.set_label(&format!(
            "{enabled} challenge{} enabled",
            if enabled == 1 { "" } else { "s" }
        ));
        self.updating.set(false);
    }

    fn rebuild_board(self: &Rc<Self>, state: &AppState) {
        // A drop rebuilds the board, and the chip that was in flight — along
        // with the drag it was carrying — goes with it, so the bin is put
        // away here rather than waiting for a drag that may never end.
        self.remove_revealer.set_reveal_child(false);
        self.board.remove_all();
        self.requirements_group
            .set_title(&requirements_title(state.board_count()));
        let items = state.board();
        if items.is_empty() {
            let empty = gtk::Label::builder()
                .label("Nothing yet — add the item you are hunting for")
                .css_classes(["dim-label"])
                .build();
            self.board.append(&empty);
        }
        for item in &items {
            if item.cluster.is_some() {
                self.board.append(&self.cluster(state, item, &items));
            } else {
                self.board
                    .append(&self.chip(state, item.anchor(), item, &items, false));
            }
        }
        let add = gtk::Button::builder()
            .child(
                &adw::ButtonContent::builder()
                    .icon_name("list-add-symbolic")
                    .label("Add")
                    .build(),
            )
            .css_classes(["chip", "chip-add"])
            .action_name("win.add-requirement")
            .tooltip_text("Add Requirement")
            .build();
        self.board.append(&add);
    }

    /// One either/or cluster: its members share a dashed capsule, with the
    /// stack badges at the trailing edge, where they speak for the whole
    /// capsule rather than for any one member.
    fn cluster(
        self: &Rc<Self>,
        state: &AppState,
        item: &BoardItem,
        items: &[BoardItem],
    ) -> gtk::Widget {
        let capsule = gtk::Box::builder()
            .spacing(2)
            .css_classes(["cluster"])
            .build();
        for (position, index) in item.members.iter().enumerate() {
            if position > 0 {
                capsule.append(
                    &gtk::Label::builder()
                        .label("or")
                        .css_classes(["cluster-or"])
                        .build(),
                );
            }
            capsule.append(&self.chip(state, *index, item, items, true));
        }
        for badge in self.badges(state, item) {
            capsule.append(&badge);
        }
        let key = state.requirements[item.anchor()].key;
        capsule.add_controller(self.drop_target(move |pane, source| {
            pane.emit(BoardAction::Join {
                source,
                target: key,
            });
        }));
        capsule.upcast()
    }

    /// One requirement as a chip: its sprite, its name, and the tiny tags that
    /// qualify it. A chip standing on its own also carries the badges of its
    /// stack; inside a cluster those belong to the capsule.
    fn chip(
        self: &Rc<Self>,
        state: &AppState,
        index: usize,
        item: &BoardItem,
        items: &[BoardItem],
        in_cluster: bool,
    ) -> gtk::Widget {
        let requirement = &state.requirements[index];
        let chip = gtk::Box::builder()
            .spacing(6)
            .css_classes(["chip"])
            .focusable(true)
            .accessible_role(gtk::AccessibleRole::Button)
            .tooltip_text(chip_tooltip(state, index, item))
            .build();
        if requirement.to_core().validate().is_err() {
            chip.add_css_class("chip-error");
        }
        chip.append(&requirement_prefix(requirement));
        chip.append(
            &gtk::Label::builder()
                .label(requirement.chip_name())
                .ellipsize(pango::EllipsizeMode::End)
                .max_width_chars(18)
                .build(),
        );
        for (text, class) in chip_tags(requirement) {
            chip.append(
                &gtk::Label::builder()
                    .label(text)
                    .css_classes(["chip-tag", class])
                    .build(),
            );
        }
        if let Some(badge) = effect_badge(requirement) {
            chip.append(&badge);
        }
        if requirement.require_uncursed {
            chip.append(
                &gtk::Label::builder()
                    .label("\u{2713}")
                    .tooltip_text("Uncursed")
                    .css_classes(["chip-tag", "chip-tag-soft"])
                    .build(),
            );
        }
        if !in_cluster {
            for badge in self.badges(state, item) {
                chip.append(&badge);
            }
        }
        self.wire_chip(&chip, state, index, item, items);
        chip.upcast()
    }

    /// The gestures every chip answers to: activate to edit, drag onto another
    /// chip for an either/or, the context menu for the same in words.
    fn wire_chip(
        self: &Rc<Self>,
        chip: &gtk::Box,
        state: &AppState,
        index: usize,
        item: &BoardItem,
        items: &[BoardItem],
    ) {
        let key = state.requirements[index].key;
        let count = item.stack_count();
        let menu = Self::chip_menu(state, index, item, items);

        let click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_PRIMARY)
            .build();
        click.connect_released({
            let pane = Rc::clone(self);
            move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                pane.emit(BoardAction::Edit(key));
            }
        });
        chip.add_controller(click);

        let secondary = gtk::GestureClick::builder()
            .button(gdk::BUTTON_SECONDARY)
            .build();
        secondary.connect_pressed({
            let pane = Rc::clone(self);
            let menu = menu.clone();
            move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                if let Some(chip) = gesture.widget() {
                    pane.show_menu(&chip, &menu, key, count);
                }
            }
        });
        chip.add_controller(secondary);

        let long_press = gtk::GestureLongPress::new();
        long_press.connect_pressed({
            let pane = Rc::clone(self);
            let menu = menu.clone();
            move |gesture, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                if let Some(chip) = gesture.widget() {
                    pane.show_menu(&chip, &menu, key, count);
                }
            }
        });
        chip.add_controller(long_press);

        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed({
            let pane = Rc::clone(self);
            let menu = menu.clone();
            move |controller, keyval, _, modifiers| {
                let context_key = keyval == gdk::Key::Menu
                    || (keyval == gdk::Key::F10
                        && modifiers.contains(gdk::ModifierType::SHIFT_MASK));
                if context_key {
                    if let Some(chip) = controller.widget() {
                        pane.show_menu(&chip, &menu, key, count);
                    }
                } else if matches!(
                    keyval,
                    gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space
                ) {
                    pane.emit(BoardAction::Edit(key));
                } else if matches!(keyval, gdk::Key::Delete | gdk::Key::BackSpace) {
                    pane.emit(BoardAction::Remove(key));
                } else {
                    return glib::Propagation::Proceed;
                }
                glib::Propagation::Stop
            }
        });
        chip.add_controller(keys);

        let drag = gtk::DragSource::builder()
            .actions(gdk::DragAction::MOVE)
            .build();
        drag.connect_prepare(move |source, _, _| {
            let widget = source.widget()?;
            source.set_icon(Some(&gtk::WidgetPaintable::new(Some(&widget))), 0, 0);
            Some(gdk::ContentProvider::for_value(&key.to_value()))
        });
        drag.connect_drag_begin({
            let pane = Rc::clone(self);
            move |source, _| {
                if let Some(widget) = source.widget() {
                    widget.add_css_class("chip-dragging");
                }
                pane.remove_revealer.set_reveal_child(true);
            }
        });
        drag.connect_drag_end({
            let pane = Rc::clone(self);
            move |source, _, _| {
                if let Some(widget) = source.widget() {
                    widget.remove_css_class("chip-dragging");
                }
                pane.remove_revealer.set_reveal_child(false);
            }
        });
        chip.add_controller(drag);

        chip.add_controller(self.drop_target(move |pane, source| {
            pane.emit(BoardAction::Join {
                source,
                target: key,
            });
        }));
    }

    /// A drop zone for a chip in flight, lit while the pointer is over it.
    fn drop_target(self: &Rc<Self>, dropped: impl Fn(&Rc<Self>, u64) + 'static) -> gtk::DropTarget {
        let target = gtk::DropTarget::new(u64::static_type(), gdk::DragAction::MOVE);
        target.connect_enter(|target, _, _| {
            if let Some(widget) = target.widget() {
                widget.add_css_class("drop-target");
            }
            gdk::DragAction::MOVE
        });
        target.connect_leave(|target| {
            if let Some(widget) = target.widget() {
                widget.remove_css_class("drop-target");
            }
        });
        let pane = Rc::clone(self);
        target.connect_drop(move |target, value, _, _| {
            if let Some(widget) = target.widget() {
                widget.remove_css_class("drop-target");
            }
            let Ok(source) = value.get::<u64>() else {
                return false;
            };
            dropped(&pane, source);
            true
        });
        target
    }

    /// The badges of one board entry: how many items it asks for, and the
    /// combined level they reach together.
    fn badges(self: &Rc<Self>, state: &AppState, item: &BoardItem) -> Vec<gtk::Widget> {
        let key = state.requirements[item.anchor()].key;
        let count = item.stack_count();
        let mut badges: Vec<gtk::Widget> = Vec::new();
        if count > 1 || item.total.is_some() {
            let label = if item.total.is_some() {
                format!("\u{2264}{count}")
            } else {
                format!("\u{d7}{count}")
            };
            let button = gtk::Button::builder()
                .label(label)
                .css_classes(["stack-badge"])
                .valign(gtk::Align::Center)
                .tooltip_text(if item.total.is_some() {
                    format!("Up to {count} items")
                } else {
                    format!("{count} of the same kind")
                })
                .build();
            button.connect_clicked({
                let pane = Rc::clone(self);
                #[allow(clippy::cast_precision_loss)] // At most STACK_MAX.
                let count = count as f64;
                move |button| {
                    pane.open_stack_popover(
                        button,
                        key,
                        StackField::Count,
                        "How many",
                        count,
                        stack_maximum(),
                    );
                }
            });
            badges.push(button.upcast());
        }
        if let Some(total) = item.total {
            let button = gtk::Button::builder()
                .label(format!("\u{3a3} \u{2265} {total}"))
                .css_classes(["stack-badge", "stack-badge-total"])
                .valign(gtk::Align::Center)
                .tooltip_text(format!(
                    "Levels add to at least {total} (a +0 item counts 1)"
                ))
                .build();
            let capacity = f64::from(levels_capacity(state, item));
            button.connect_clicked({
                let pane = Rc::clone(self);
                move |button| {
                    pane.open_stack_popover(
                        button,
                        key,
                        StackField::Total,
                        "Combined level",
                        f64::from(total),
                        capacity,
                    );
                }
            });
            badges.push(button.upcast());
        }
        badges
    }

    fn open_stack_popover(
        self: &Rc<Self>,
        badge: &gtk::Button,
        key: u64,
        field: StackField,
        title: &str,
        value: f64,
        maximum: f64,
    ) {
        self.stack_target.set(None);
        self.stack_title.set_label(title);
        let adjustment = self.stack_spin.adjustment();
        adjustment.set_lower(1.0);
        adjustment.set_upper(maximum.max(1.0));
        self.stack_spin.set_value(value);
        self.stack_opened_on.set(value);
        self.stack_target.set(Some((key, field)));
        self.point_at(&self.stack_popover, badge.upcast_ref());
        self.stack_popover.popup();
    }

    /// Applies what the stack popover was left showing. Doing this on close
    /// rather than on every step keeps the board still while the user spins.
    fn commit_stack(&self) {
        let Some((key, field)) = self.stack_target.take() else {
            return;
        };
        let value = self.stack_spin.value().round();
        if (value - self.stack_opened_on.get()).abs() < f64::EPSILON {
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        match field {
            StackField::Count => self.emit(BoardAction::Count {
                key,
                count: value.max(1.0) as usize,
            }),
            StackField::Total => self.emit(BoardAction::Total {
                key,
                total: Some(value.max(1.0) as u8),
            }),
        }
    }

    fn show_menu(self: &Rc<Self>, chip: &gtk::Widget, menu: &gio::Menu, key: u64, count: usize) {
        self.count_action
            .set_state(&(key, count as u64).to_variant());
        self.menu.set_menu_model(Some(menu));
        self.point_at(self.menu.upcast_ref(), chip);
        self.menu.popup();
    }

    /// Aims a popover at `widget`, in the board's own coordinates.
    fn point_at(&self, popover: &gtk::Popover, widget: &gtk::Widget) {
        if let Some(bounds) = widget.compute_bounds(&self.board_root) {
            #[allow(clippy::cast_possible_truncation)]
            popover.set_pointing_to(Some(&gdk::Rectangle::new(
                bounds.x() as i32,
                bounds.y() as i32,
                bounds.width() as i32,
                bounds.height() as i32,
            )));
        }
    }

    /// The chip's context menu: every gesture of the board said in words, for
    /// the keyboard, for touch, and for anyone who would rather not drag.
    fn chip_menu(
        state: &AppState,
        index: usize,
        item: &BoardItem,
        items: &[BoardItem],
    ) -> gio::Menu {
        let key = state.requirements[index].key;
        let anchor = state.requirements[item.anchor()];
        let menu = gio::Menu::new();
        let first = gio::Menu::new();
        first.append_item(&menu_item("_Edit…", "board.edit", &key.to_variant()));
        // Either/or with every other entry on the board, named as it reads.
        let peers = gio::Menu::new();
        for other in items {
            if other.key == item.key {
                continue;
            }
            let target = state.requirements[other.anchor()].key;
            let name = if other.cluster.is_some() {
                other
                    .members
                    .iter()
                    .map(|member| state.requirements[*member].chip_name())
                    .collect::<Vec<_>>()
                    .join(" or ")
            } else {
                state.requirements[other.anchor()].chip_name()
            };
            peers.append_item(&menu_item(&name, "board.join", &(key, target).to_variant()));
        }
        if peers.n_items() > 0 {
            first.append_submenu(Some("_Either/or with…"), &peers);
        }
        menu.append_section(None, &first);

        // A cluster spanning two categories cannot anchor a stack, so it is
        // not offered one.
        if state.can_stack(key) {
            let counts = gio::Menu::new();
            for count in 1..=u64::try_from(STACK_MAX).unwrap_or(3) {
                counts.append_item(&menu_item(
                    &count.to_string(),
                    "board.count",
                    &(key, count).to_variant(),
                ));
            }
            menu.append_section(Some("How many"), &counts);
        }

        // Only a lone chip naming one item can count its copies' levels.
        if item.cluster.is_none() && anchor.item.is_some() && item.stack_count() > 1 {
            let levels = gio::Menu::new();
            levels.append_item(&menu_item(
                if item.total.is_some() {
                    "Stop counting _levels"
                } else {
                    "Count _levels together"
                },
                "board.total",
                &key.to_variant(),
            ));
            menu.append_section(None, &levels);
        }
        if item.cluster.is_some() {
            let alone = gio::Menu::new();
            alone.append_item(&menu_item("On its _own", "board.detach", &key.to_variant()));
            menu.append_section(None, &alone);
        }
        let last = gio::Menu::new();
        last.append_item(&menu_item("_Remove", "board.remove", &key.to_variant()));
        menu.append_section(None, &last);
        menu
    }

    /// The actions the chip menus fire, all of them naming their row by key.
    fn board_actions(self: &Rc<Self>) -> gio::SimpleActionGroup {
        let group = gio::SimpleActionGroup::new();
        for (name, action) in [
            ("edit", BoardAction::Edit as fn(u64) -> BoardAction),
            ("detach", BoardAction::Detach),
            ("remove", BoardAction::Remove),
        ] {
            let entry = gio::SimpleAction::new(name, Some(glib::VariantTy::UINT64));
            entry.connect_activate({
                let pane = Rc::clone(self);
                move |_, target| {
                    if let Some(key) = target.and_then(glib::Variant::get::<u64>) {
                        pane.emit(action(key));
                    }
                }
            });
            group.add_action(&entry);
        }
        let join = gio::SimpleAction::new("join", Some(count_variant_type()));
        join.connect_activate({
            let pane = Rc::clone(self);
            move |_, target| {
                if let Some((source, target)) = target.and_then(glib::Variant::get::<(u64, u64)>) {
                    pane.emit(BoardAction::Join { source, target });
                }
            }
        });
        group.add_action(&join);
        // A stateful action with a target renders as a row of radio items; the
        // state is set to the chip's own count just before the menu opens.
        let count = self.count_action.clone();
        count.connect_activate({
            let pane = Rc::clone(self);
            move |action, target| {
                let Some((key, count)) = target.and_then(glib::Variant::get::<(u64, u64)>) else {
                    return;
                };
                action.set_state(&(key, count).to_variant());
                pane.emit(BoardAction::Count {
                    key,
                    count: usize::try_from(count).unwrap_or(1),
                });
            }
        });
        group.add_action(&count);
        let total = gio::SimpleAction::new("total", Some(glib::VariantTy::UINT64));
        total.connect_activate({
            let pane = Rc::clone(self);
            move |_, target| {
                if let Some(key) = target.and_then(glib::Variant::get::<u64>) {
                    pane.emit(BoardAction::Total { key, total: None });
                }
            }
        });
        group.add_action(&total);
        group
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

/// The stack spinner's upper bound as an adjustment wants it.
#[allow(clippy::cast_precision_loss)] // STACK_MAX is 3.
fn stack_maximum() -> f64 {
    STACK_MAX as f64
}

/// The variant type of the actions that name a row and a number together.
fn count_variant_type() -> &'static glib::VariantTy {
    glib::VariantTy::new("(tt)").expect("(tt) is a valid variant type")
}

/// A menu item bound to a board action with the row it acts on as its target.
/// A u64 target cannot be spelled in a detailed action name, so it is set as a
/// value instead.
fn menu_item(label: &str, action: &str, target: &glib::Variant) -> gio::MenuItem {
    let entry = gio::MenuItem::new(Some(label), None);
    entry.set_action_and_target_value(Some(action), Some(target));
    entry
}

/// The most levels a combined-level stack could reach: each item counts its
/// upgrade plus one, and its members may carry any upgrade.
fn levels_capacity(state: &AppState, item: &BoardItem) -> u8 {
    let anchor = state.requirements[item.anchor()];
    let per_item = anchor.to_core().upgrade_ceiling() + 1;
    u8::try_from(item.stack_count())
        .unwrap_or(1)
        .saturating_mul(per_item)
        .max(1)
}

/// The tiny qualifiers beside a chip's name: tier, upgrade, floor. A named
/// item pins its own tier, so only a wildcard shows one.
fn chip_tags(requirement: &UiRequirement) -> Vec<(String, &'static str)> {
    let mut tags = Vec::new();
    if requirement.item.is_none() {
        match requirement.tier {
            TierRequirement::Any => {}
            TierRequirement::Exact(tier) => tags.push((format!("T{tier}"), "chip-tag-plain")),
            TierRequirement::AtLeast(tier) => tags.push((format!("T{tier}+"), "chip-tag-plain")),
            TierRequirement::AtMost(tier) => {
                tags.push((format!("T\u{2264}{tier}"), "chip-tag-plain"));
            }
        }
    }
    match requirement.upgrade {
        UpgradeRequirement::Any => {}
        UpgradeRequirement::Exact(upgrade) => tags.push((format!("+{upgrade}"), "chip-tag-up")),
        UpgradeRequirement::AtLeast(upgrade) => {
            tags.push((format!("+{upgrade}\u{2191}"), "chip-tag-up"));
        }
    }
    if let Some(depth) = requirement.max_depth {
        tags.push((format!("F\u{2264}{depth}"), "chip-tag-plain"));
    }
    tags
}

/// The effect badge, for what a pulsing sprite cannot say on its own: several
/// effects at once, "any enchantment", which settles on no colour, or an
/// effect on a wildcard chip, which has no sprite of its own to pulse.
fn effect_badge(requirement: &UiRequirement) -> Option<gtk::Widget> {
    let EffectRequirement::OneOf(set) = requirement.effect else {
        return None;
    };
    let label = effect_label(requirement.effect)?;
    // "Any enchantment" settles on no colour of its own, so it wears them all.
    if EffectSet::enchantments(set.family()) == Some(set) {
        return Some(effect_dot(None, &label));
    }
    if set.count() > 1 {
        let count = gtk::Label::builder()
            .label(set.count().to_string())
            .tooltip_text(label)
            .css_classes(["effect-count"])
            .valign(gtk::Align::Center)
            .build();
        return Some(count.upcast());
    }
    // A single effect — enchantment or curse — already pulses on a real
    // sprite, and the tooltip names it; a badge would only say it twice.
    // A wildcard chip shows the family's icon, which cannot pulse, so there
    // the dot is all the colour the effect gets.
    if requirement.item.is_some() {
        return None;
    }
    Some(effect_dot(glow::effect(set.effects().next()), &label))
}

/// The dot standing in for an effect: its glow colour, or the rainbow of "any
/// enchantment", which is every colour and so none.
fn effect_dot(glow: Option<glow::Glow>, label: &str) -> gtk::Widget {
    const SIZE: i32 = 12;
    let area = gtk::DrawingArea::builder()
        .content_width(SIZE)
        .content_height(SIZE)
        .valign(gtk::Align::Center)
        .tooltip_text(label)
        .accessible_role(gtk::AccessibleRole::Img)
        .build();
    area.set_draw_func(move |_, context, width, height| {
        let radius = f64::from(width.min(height)) / 2.0;
        context.arc(
            f64::from(width) / 2.0,
            f64::from(height) / 2.0,
            radius,
            0.0,
            std::f64::consts::TAU,
        );
        if let Some(glow) = glow {
            let (red, green, blue) = glow.rgb();
            context.set_source_rgb(red, green, blue);
        } else {
            let rainbow = cairo::LinearGradient::new(0.0, 0.0, f64::from(width), 0.0);
            for (offset, red, green, blue) in [
                (0.0, 1.0, 0.33, 0.33),
                (0.2, 1.0, 1.0, 0.33),
                (0.4, 0.33, 1.0, 0.33),
                (0.6, 0.33, 1.0, 1.0),
                (0.8, 0.33, 0.33, 1.0),
                (1.0, 1.0, 0.33, 1.0),
            ] {
                rainbow.add_color_stop_rgb(offset, red, green, blue);
            }
            let _ = context.set_source(&rainbow);
        }
        let _ = context.fill();
    });
    area.upcast()
}

/// Everything the chip is too small to say: what it asks of one item, how it
/// relates to the chips around it, and why it cannot be searched for.
fn chip_tooltip(state: &AppState, index: usize, item: &BoardItem) -> String {
    let requirement = &state.requirements[index];
    let mut text = requirement.title();
    let _ = write!(text, "\n{}", requirement.subtitle());
    if item.cluster.is_some() {
        let peers: Vec<String> = item
            .members
            .iter()
            .filter(|member| **member != index)
            .map(|member| state.requirements[*member].chip_name())
            .collect();
        let _ = write!(text, "\nor {}", peers.join(", "));
    }
    if let Some(total) = item.total {
        let _ = write!(
            text,
            "\n\u{3a3} up to {} \u{2014} levels add to \u{2265} {total}",
            item.stack_count()
        );
    } else if item.stack_count() > 1 {
        // The chip's own bounds (+3, F≤4) describe one copy, not the extras.
        let mut depths: Vec<Option<u8>> = item
            .extras
            .iter()
            .map(|extra| state.requirements[*extra].max_depth)
            .collect();
        depths.sort_unstable();
        depths.dedup();
        let floors = match depths.as_slice() {
            [Some(depth)] => format!("floors 1\u{2013}{depth}"),
            [None] => "any floor".to_owned(),
            _ => "own floor limits".to_owned(),
        };
        let _ = write!(
            text,
            "\n\u{d7} {} of the same kind \u{2014} the extra copies: any upgrade, {floors}",
            item.stack_count()
        );
    }
    if let Err(error) = requirement.to_core().validate() {
        let _ = write!(text, "\n{error}");
    }
    text
}

/// The chip icon for one requirement: the item's real sprite once a concrete
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

/// The requirements header, counting board entries: a cluster is one
/// requirement however many alternatives it lists, and a stack is one however
/// many items it asks for.
fn requirements_title(entries: usize) -> String {
    if entries == 0 {
        "Requirements".to_owned()
    } else {
        format!("Requirements ({entries})")
    }
}

#[cfg(test)]
mod tests {
    use super::requirements_title;

    #[test]
    fn requirements_header_counts_board_entries() {
        assert_eq!(requirements_title(0), "Requirements");
        assert_eq!(requirements_title(3), "Requirements (3)");
    }
}
