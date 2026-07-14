// SPDX-License-Identifier: GPL-3.0-or-later

//! Modal editor for one item requirement.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use shpd_seedfinder_core::catalog::{
    ALL_ARMOR_EFFECTS, ALL_WEAPON_EFFECTS, Effect, ITEMS, ItemDefinition, ItemId, ItemKind,
};
use shpd_seedfinder_core::query::{TierRequirement, UpgradeRequirement};

use crate::state::{
    ALL_KINDS, ALL_SOURCES, UiRequirement, kind_label, kind_singular, source_label,
};

struct Editor {
    dialog: adw::Dialog,
    category: adw::ComboRow,
    item_row: adw::ComboRow,
    items: RefCell<Vec<Option<ItemId>>>,
    tier_row: adw::ComboRow,
    exact_tier: adw::SpinRow,
    bounded_tier: adw::ComboRow,
    upgrade_row: adw::ComboRow,
    upgrade_value: adw::SpinRow,
    effect_row: adw::ComboRow,
    effects: RefCell<Vec<Option<Effect>>>,
    source_row: adw::ComboRow,
    group_row: adw::ComboRow,
    floor_switch: adw::SwitchRow,
    floor_value: adw::SpinRow,
    updating: Cell<bool>,
    key: u64,
}

/// Presents the editor over `parent`. `on_finish` receives the edited
/// requirement when the user confirms; cancelling never calls it.
pub fn present(
    parent: &adw::ApplicationWindow,
    requirement: &UiRequirement,
    is_new: bool,
    on_finish: impl Fn(UiRequirement) + 'static,
) {
    let editor = Rc::new(build(requirement));
    connect(&editor);
    restore(&editor, requirement);

    let header = adw::HeaderBar::builder()
        .show_start_title_buttons(false)
        .show_end_title_buttons(false)
        .build();
    let cancel = gtk::Button::with_label("Cancel");
    let confirm = gtk::Button::with_label(if is_new { "Add" } else { "Save" });
    confirm.add_css_class("suggested-action");
    header.pack_start(&cancel);
    header.pack_end(&confirm);

    let page = adw::PreferencesPage::new();
    for group in groups(&editor) {
        page.add(&group);
    }
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&page));

    editor.dialog.set_title(if is_new {
        "New Requirement"
    } else {
        "Edit Requirement"
    });
    editor.dialog.set_child(Some(&toolbar_view));
    editor.dialog.set_default_widget(Some(&confirm));

    cancel.connect_clicked({
        let dialog = editor.dialog.clone();
        move |_| {
            dialog.close();
        }
    });
    confirm.connect_clicked({
        let editor = Rc::clone(&editor);
        move |_| {
            let result = collect(&editor);
            if result.to_core().validate().is_ok() {
                editor.dialog.close();
                on_finish(result);
            }
        }
    });
    editor.dialog.present(Some(parent));
}

fn build(requirement: &UiRequirement) -> Editor {
    Editor {
        dialog: adw::Dialog::builder()
            .content_width(460)
            .content_height(640)
            .build(),
        category: combo_row(
            "Category",
            &ALL_KINDS
                .iter()
                .map(|kind| kind_label(*kind))
                .collect::<Vec<_>>(),
        ),
        item_row: searchable_combo_row("Item"),
        items: RefCell::new(vec![None]),
        tier_row: combo_row("Tier", &["Any tier", "Exactly", "At least", "At most"]),
        exact_tier: spin_row("Exact tier", 2.0, 2.0, 5.0),
        bounded_tier: combo_row("Minimum tier", &["Tier 3", "Tier 4"]),
        upgrade_row: combo_row("Upgrade", &["Any", "Exactly", "At least"]),
        upgrade_value: spin_row("Level", 1.0, 0.0, 4.0),
        effect_row: searchable_combo_row("Enchantment"),
        effects: RefCell::new(vec![None]),
        source_row: combo_row(
            "Source",
            &std::iter::once("Any")
                .chain(ALL_SOURCES.iter().map(|source| source_label(*source)))
                .collect::<Vec<_>>(),
        ),
        group_row: combo_row("Same-item group", &["None", "A", "B", "C", "D"]),
        floor_switch: adw::SwitchRow::builder()
            .title("Limit to a floor")
            .subtitle("Require this item within the first floors only")
            .build(),
        floor_value: spin_row("Within first … floors", 5.0, 1.0, 24.0),
        updating: Cell::new(false),
        key: requirement.key,
    }
}

fn groups(editor: &Rc<Editor>) -> Vec<adw::PreferencesGroup> {
    let item_group = adw::PreferencesGroup::builder().title("Item").build();
    item_group.add(&editor.category);
    item_group.add(&editor.item_row);
    item_group.add(&editor.tier_row);
    item_group.add(&editor.exact_tier);
    item_group.add(&editor.bounded_tier);

    let upgrade_group = adw::PreferencesGroup::builder()
        .title("Upgrade Level")
        .build();
    upgrade_group.add(&editor.upgrade_row);
    upgrade_group.add(&editor.upgrade_value);

    let details_group = adw::PreferencesGroup::builder()
        .title("Details")
        .description("Same-item group members must resolve to the same item.")
        .build();
    details_group.add(&editor.effect_row);
    details_group.add(&editor.source_row);
    details_group.add(&editor.group_row);
    details_group.add(&editor.floor_switch);
    details_group.add(&editor.floor_value);

    vec![item_group, upgrade_group, details_group]
}

fn connect(editor: &Rc<Editor>) {
    editor
        .category
        .connect_selected_notify(hook(Rc::clone(editor), |editor| {
            populate_items(editor, None);
            populate_effects(editor, None);
            editor.tier_row.set_selected(0);
            clamp_upgrade(editor);
            refresh_visibility(editor);
        }));
    editor
        .item_row
        .connect_selected_notify(hook(Rc::clone(editor), |editor| {
            if selected_item(editor).is_some() {
                editor.tier_row.set_selected(0);
            }
            refresh_visibility(editor);
        }));
    editor
        .tier_row
        .connect_selected_notify(hook(Rc::clone(editor), refresh_visibility));
    editor
        .exact_tier
        .connect_value_notify(hook(Rc::clone(editor), |editor| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let tier = editor.exact_tier.value().round() as u8;
            editor
                .bounded_tier
                .set_selected(u32::from(tier.clamp(3, 4) - 3));
        }));
    editor
        .bounded_tier
        .connect_selected_notify(hook(Rc::clone(editor), |editor| {
            editor
                .exact_tier
                .set_value(f64::from(editor.bounded_tier.selected() + 3));
        }));
    editor
        .upgrade_row
        .connect_selected_notify(hook(Rc::clone(editor), |editor| {
            clamp_upgrade(editor);
            refresh_visibility(editor);
        }));
    editor
        .floor_switch
        .connect_active_notify(hook(Rc::clone(editor), refresh_visibility));
}

/// Wraps a handler so programmatic updates never re-enter it.
fn hook<W>(editor: Rc<Editor>, handler: fn(&Rc<Editor>)) -> impl Fn(&W) {
    move |_| {
        if editor.updating.get() {
            return;
        }
        editor.updating.set(true);
        handler(&editor);
        editor.updating.set(false);
    }
}

fn restore(editor: &Rc<Editor>, requirement: &UiRequirement) {
    editor.updating.set(true);
    let kind_index = ALL_KINDS
        .iter()
        .position(|kind| *kind == requirement.kind)
        .unwrap_or(0);
    editor
        .category
        .set_selected(u32::try_from(kind_index).unwrap_or(0));
    populate_items(editor, requirement.item);
    populate_effects(editor, requirement.effect);
    match requirement.tier {
        TierRequirement::Any => editor.tier_row.set_selected(0),
        TierRequirement::Exact(tier) => {
            editor.tier_row.set_selected(1);
            set_tier_value(editor, tier);
        }
        TierRequirement::AtLeast(tier) => {
            editor.tier_row.set_selected(2);
            set_tier_value(editor, tier);
        }
        TierRequirement::AtMost(tier) => {
            editor.tier_row.set_selected(3);
            set_tier_value(editor, tier);
        }
    }
    match requirement.upgrade {
        UpgradeRequirement::Any => editor.upgrade_row.set_selected(0),
        UpgradeRequirement::Exact(upgrade) => {
            editor.upgrade_row.set_selected(1);
            clamp_upgrade(editor);
            editor.upgrade_value.set_value(f64::from(upgrade));
        }
        UpgradeRequirement::AtLeast(upgrade) => {
            editor.upgrade_row.set_selected(2);
            clamp_upgrade(editor);
            editor.upgrade_value.set_value(f64::from(upgrade));
        }
    }
    let source_index = requirement
        .source
        .and_then(|source| ALL_SOURCES.iter().position(|other| *other == source))
        .map_or(0, |index| index + 1);
    editor
        .source_row
        .set_selected(u32::try_from(source_index).unwrap_or(0));
    editor
        .group_row
        .set_selected(u32::from(requirement.identity_group.unwrap_or(0)));
    if let Some(depth) = requirement.max_depth {
        editor.floor_switch.set_active(true);
        editor.floor_value.set_value(f64::from(depth));
    }
    refresh_visibility(editor);
    editor.updating.set(false);
}

fn collect(editor: &Rc<Editor>) -> UiRequirement {
    let kind = selected_kind(editor);
    let item = selected_item(editor);
    let tier_eligible = item.is_none() && matches!(kind, ItemKind::Weapon | ItemKind::Armor);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let exact_tier = editor.exact_tier.value().round() as u8;
    let bounded_tier = u8::try_from(editor.bounded_tier.selected() + 3).unwrap_or(3);
    let tier = match editor.tier_row.selected() {
        1 if tier_eligible => TierRequirement::Exact(exact_tier),
        2 if tier_eligible => TierRequirement::AtLeast(bounded_tier),
        3 if tier_eligible => TierRequirement::AtMost(bounded_tier),
        _ => TierRequirement::Any,
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let upgrade_value = editor.upgrade_value.value().round() as u8;
    let upgrade = match editor.upgrade_row.selected() {
        1 => UpgradeRequirement::Exact(upgrade_value),
        2 => UpgradeRequirement::AtLeast(upgrade_value),
        _ => UpgradeRequirement::Any,
    };
    let effect = if matches!(kind, ItemKind::Weapon | ItemKind::Armor) {
        editor
            .effects
            .borrow()
            .get(editor.effect_row.selected() as usize)
            .copied()
            .flatten()
    } else {
        None
    };
    let source = match editor.source_row.selected() {
        0 => None,
        index => ALL_SOURCES.get(index as usize - 1).copied(),
    };
    let identity_group = match editor.group_row.selected() {
        0 => None,
        group => u8::try_from(group).ok(),
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let max_depth = editor
        .floor_switch
        .is_active()
        .then(|| editor.floor_value.value().round() as u8);
    UiRequirement {
        key: editor.key,
        kind,
        item,
        tier,
        upgrade,
        effect,
        source,
        identity_group,
        max_depth,
    }
}

fn selected_kind(editor: &Rc<Editor>) -> ItemKind {
    ALL_KINDS
        .get(editor.category.selected() as usize)
        .copied()
        .unwrap_or(ItemKind::Weapon)
}

fn selected_item(editor: &Rc<Editor>) -> Option<ItemId> {
    editor
        .items
        .borrow()
        .get(editor.item_row.selected() as usize)
        .copied()
        .flatten()
}

fn set_tier_value(editor: &Rc<Editor>, tier: u8) {
    editor.exact_tier.set_value(f64::from(tier));
    editor
        .bounded_tier
        .set_selected(u32::from(tier.clamp(3, 4) - 3));
}

/// Items offered for one family. Tier-1 equipment is starting gear and never
/// spawns in the dungeon, so it is not searchable.
fn searchable_items(kind: ItemKind) -> Vec<&'static ItemDefinition> {
    let mut items: Vec<_> = ITEMS
        .iter()
        .filter(|definition| definition.kind == kind && definition.tier != Some(1))
        .collect();
    if matches!(kind, ItemKind::Weapon | ItemKind::Armor) {
        items.sort_by_key(|definition| definition.tier);
    }
    items
}

fn populate_items(editor: &Rc<Editor>, selection: Option<ItemId>) {
    let kind = selected_kind(editor);
    let mut ids = vec![None];
    let mut labels = vec![format!("Any {}", kind_singular(kind))];
    for definition in searchable_items(kind) {
        ids.push(Some(definition.id));
        labels.push(match definition.tier {
            Some(tier) => format!("{} · Tier {tier}", definition.name),
            None => definition.name.to_owned(),
        });
    }
    let selected = selection
        .and_then(|wanted| ids.iter().position(|id| *id == Some(wanted)))
        .unwrap_or(0);
    editor.items.replace(ids);
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    editor
        .item_row
        .set_model(Some(&gtk::StringList::new(&labels)));
    editor
        .item_row
        .set_selected(u32::try_from(selected).unwrap_or(0));
}

fn populate_effects(editor: &Rc<Editor>, selection: Option<Effect>) {
    let kind = selected_kind(editor);
    editor.effect_row.set_title(if kind == ItemKind::Armor {
        "Glyph"
    } else {
        "Enchantment"
    });
    let mut effects = vec![None];
    let mut labels = vec!["Any".to_owned()];
    match kind {
        ItemKind::Weapon => {
            for effect in ALL_WEAPON_EFFECTS {
                effects.push(Some(Effect::Weapon(*effect)));
                labels.push(effect_label(effect.wire_name(), effect.is_curse()));
            }
        }
        ItemKind::Armor => {
            for effect in ALL_ARMOR_EFFECTS {
                effects.push(Some(Effect::Armor(*effect)));
                labels.push(effect_label(effect.wire_name(), effect.is_curse()));
            }
        }
        ItemKind::Wand | ItemKind::Ring => {}
    }
    let selected = selection
        .and_then(|wanted| effects.iter().position(|effect| *effect == Some(wanted)))
        .unwrap_or(0);
    editor.effects.replace(effects);
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    editor
        .effect_row
        .set_model(Some(&gtk::StringList::new(&labels)));
    editor
        .effect_row
        .set_selected(u32::try_from(selected).unwrap_or(0));
}

fn effect_label(name: &str, is_curse: bool) -> String {
    if is_curse {
        format!("{name} · curse")
    } else {
        name.to_owned()
    }
}

fn clamp_upgrade(editor: &Rc<Editor>) {
    let maximum = f64::from(selected_kind(editor).maximum_search_upgrade());
    let minimum = if editor.upgrade_row.selected() == 1 {
        1.0
    } else {
        0.0
    };
    let adjustment = editor.upgrade_value.adjustment();
    adjustment.set_lower(minimum);
    adjustment.set_upper(maximum);
    editor
        .upgrade_value
        .set_value(editor.upgrade_value.value().clamp(minimum, maximum));
}

fn refresh_visibility(editor: &Rc<Editor>) {
    let kind = selected_kind(editor);
    let wildcard_equipment =
        selected_item(editor).is_none() && matches!(kind, ItemKind::Weapon | ItemKind::Armor);
    let tier_mode = editor.tier_row.selected();
    editor.tier_row.set_visible(wildcard_equipment);
    editor
        .exact_tier
        .set_visible(wildcard_equipment && tier_mode == 1);
    editor
        .bounded_tier
        .set_visible(wildcard_equipment && matches!(tier_mode, 2 | 3));
    editor.bounded_tier.set_title(if tier_mode == 2 {
        "Minimum tier"
    } else {
        "Maximum tier"
    });
    editor
        .upgrade_value
        .set_visible(editor.upgrade_row.selected() != 0);
    editor
        .upgrade_value
        .set_title(if editor.upgrade_row.selected() == 1 {
            "Exactly"
        } else {
            "At least"
        });
    editor
        .effect_row
        .set_visible(matches!(kind, ItemKind::Weapon | ItemKind::Armor));
    editor
        .floor_value
        .set_visible(editor.floor_switch.is_active());
}

fn combo_row(title: &str, options: &[&str]) -> adw::ComboRow {
    adw::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(options))
        .build()
}

fn searchable_combo_row(title: &str) -> adw::ComboRow {
    let row = adw::ComboRow::builder().title(title).build();
    row.set_expression(Some(&gtk::PropertyExpression::new(
        gtk::StringObject::static_type(),
        None::<gtk::Expression>,
        "string",
    )));
    row.set_enable_search(true);
    row
}

fn spin_row(title: &str, value: f64, lower: f64, upper: f64) -> adw::SpinRow {
    adw::SpinRow::builder()
        .title(title)
        .adjustment(&gtk::Adjustment::new(value, lower, upper, 1.0, 1.0, 0.0))
        .build()
}
