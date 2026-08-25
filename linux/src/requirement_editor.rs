// SPDX-License-Identifier: GPL-3.0-or-later

//! Modal editor for one item requirement.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use shpd_seedfinder_core::catalog::{
    ALL_ARMOR_EFFECTS, ALL_WEAPON_EFFECTS, Effect, ITEMS, ItemDefinition, ItemId, ItemKind,
};
use shpd_seedfinder_core::main_world::normalize_floor_limit;
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::{
    BOUNDED_TIER_MAX, BOUNDED_TIER_MIN, EXACT_TIER_MAX, EXACT_TIER_MIN, EffectRequirement,
    EffectSet, MAX_SEARCH_DEPTH, TierRequirement, UpgradeRequirement,
};

use crate::query_pane::skip_empty_boss_floors;
use crate::relations::STACK_MAX;
use crate::state::{
    ALL_KIND_CHOICES, AppState, KindChoice, StackShape, UiRequirement, kind_choice_label,
    kind_choice_singular, source_label,
};

/// Positions of the effect picker's modes.
const EFFECT_ANY: u32 = 0;
const EFFECT_ANY_ENCHANTMENT: u32 = 1;
const EFFECT_SPECIFIC: u32 = 2;

/// One checkbox of the specific-effect list.
struct EffectCheck {
    effect: Effect,
    row: adw::ActionRow,
    check: gtk::CheckButton,
}

struct Editor {
    dialog: adw::Dialog,
    banner: adw::Banner,
    category: adw::ComboRow,
    item_row: adw::ComboRow,
    items: RefCell<Vec<Option<ItemId>>>,
    tier_row: adw::ComboRow,
    exact_tier: adw::SpinRow,
    bounded_tier: adw::ComboRow,
    upgrade_row: adw::ComboRow,
    exact_upgrade: adw::SpinRow,
    minimum_upgrade: adw::ComboRow,
    ring_minimum_upgrade: adw::SpinRow,
    upgrade_group: adw::PreferencesGroup,
    count_group: adw::PreferencesGroup,
    count_row: adw::SpinRow,
    copy_floor_switch: adw::SwitchRow,
    copy_floor_value: adw::SpinRow,
    levels_switch: adw::SwitchRow,
    levels_value: adw::SpinRow,
    effect_mode_group: adw::PreferencesGroup,
    effect_mode: adw::ComboRow,
    effect_group: adw::PreferencesGroup,
    effect_list: gtk::ListBox,
    effect_checks: RefCell<Vec<EffectCheck>>,
    uncursed: adw::SwitchRow,
    source_row: adw::ComboRow,
    floor_switch: adw::SwitchRow,
    floor_value: adw::SpinRow,
    updating: Cell<bool>,
    key: u64,
    /// The alternative group the row belongs to, kept so a saved member stays
    /// in its cluster.
    alternative_group: Option<u8>,
    /// A cluster member's stack belongs to the cluster, so that section is
    /// hidden for one.
    in_cluster: bool,
    /// The query the row is edited within, for cross-row validation.
    context: AppState,
}

/// Presents the editor over `parent`. `context` is the query the row lives in
/// and `stack` the shape of the board entry it anchors; `on_finish` receives
/// the edited requirement with the stack it asked for — how many items, their
/// combined level, and the floor limit of the extra copies — when the user
/// confirms, and cancelling never calls it.
pub fn present(
    parent: &adw::ApplicationWindow,
    context: &AppState,
    requirement: &UiRequirement,
    stack: StackShape,
    is_new: bool,
    on_finish: impl Fn(UiRequirement, usize, Option<u8>, Option<u8>) + 'static,
) {
    let editor = Rc::new(build(context.clone(), requirement, stack));
    connect(&editor);
    restore(&editor, requirement, stack);

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
    toolbar_view.add_top_bar(&editor.banner);
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
            let (result, count, total, copy_depth) = collect(&editor);
            match check(&editor, &result, count, total, copy_depth) {
                Ok(()) => {
                    editor.dialog.close();
                    on_finish(result, count, total, copy_depth);
                }
                Err(message) => {
                    editor.banner.set_title(&message);
                    editor.banner.set_revealed(true);
                }
            }
        }
    });
    editor.dialog.present(Some(parent));
}

fn build(context: AppState, requirement: &UiRequirement, stack: StackShape) -> Editor {
    let effect_list = gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::None)
        .build();
    let effect_group = adw::PreferencesGroup::builder()
        .title("Enchantments")
        .description("The item must carry one of the checked effects.")
        .build();
    effect_group.add(&effect_list);
    Editor {
        dialog: adw::Dialog::builder()
            .content_width(460)
            .content_height(700)
            .build(),
        banner: adw::Banner::new(""),
        category: combo_row(
            "Category",
            &ALL_KIND_CHOICES
                .iter()
                .map(|choice| kind_choice_label(*choice))
                .collect::<Vec<_>>(),
        ),
        item_row: searchable_combo_row("Item"),
        items: RefCell::new(vec![None]),
        tier_row: combo_row("Tier", &["Any tier", "Exactly", "At least", "At most"]),
        exact_tier: spin_row(
            "Exact tier",
            f64::from(EXACT_TIER_MIN),
            f64::from(EXACT_TIER_MIN),
            f64::from(EXACT_TIER_MAX),
        ),
        bounded_tier: combo_row("Minimum tier", &borrowed(&bounded_tier_labels())),
        upgrade_row: combo_row("Upgrade", &["Any", "Exactly", "At least"]),
        exact_upgrade: spin_row("Exactly", 1.0, 1.0, 4.0),
        minimum_upgrade: combo_row("Minimum upgrade", &["+1 or higher", "+2 or higher"]),
        ring_minimum_upgrade: spin_row("Minimum upgrade", 1.0, 1.0, 3.0),
        upgrade_group: adw::PreferencesGroup::builder()
            .title("Upgrade Level")
            .build(),
        count_group: adw::PreferencesGroup::builder()
            .title("Total Item Count")
            .description(
                "Ask for more than one item of this kind — reforge fodder for the \
                 blacksmith. The extra copies carry no constraints of their own.",
            )
            .build(),
        count_row: spin_row("How many", 1.0, 1.0, stack_maximum()),
        copy_floor_switch: adw::SwitchRow::builder()
            .title("Limit the extra copies to a floor")
            .build(),
        copy_floor_value: spin_row(
            "Copies within first … floors",
            4.0,
            1.0,
            f64::from(MAX_SEARCH_DEPTH),
        ),
        levels_switch: adw::SwitchRow::builder()
            .title("Count levels together")
            .subtitle("Any upgrade on each, as long as they add up")
            .build(),
        levels_value: spin_row("Levels reach", 1.0, 1.0, 4.0),
        effect_mode_group: adw::PreferencesGroup::builder()
            .title("Enchantment")
            .build(),
        effect_mode: combo_row("Enchantment", &["Any", "Any enchantment", "Specific…"]),
        effect_group,
        effect_list,
        effect_checks: RefCell::new(Vec::new()),
        uncursed: adw::SwitchRow::builder().title("Require uncursed").build(),
        source_row: combo_row(
            "Source",
            &std::iter::once("Any")
                .chain(ItemSource::ALL.iter().map(|source| source_label(*source)))
                .collect::<Vec<_>>(),
        ),
        floor_switch: adw::SwitchRow::builder()
            .title("Limit to a floor")
            .subtitle("Require this item within the first floors only")
            .build(),
        floor_value: spin_row(
            "Within first … floors",
            4.0,
            1.0,
            f64::from(MAX_SEARCH_DEPTH),
        ),
        updating: Cell::new(false),
        key: requirement.key,
        alternative_group: requirement.alternative_group,
        in_cluster: stack.in_cluster,
        context,
    }
}

/// The stack spinner's upper bound as the adjustment wants it.
#[allow(clippy::cast_precision_loss)] // STACK_MAX is 3.
fn stack_maximum() -> f64 {
    STACK_MAX as f64
}

fn groups(editor: &Rc<Editor>) -> Vec<adw::PreferencesGroup> {
    let item_group = adw::PreferencesGroup::builder().title("Item").build();
    item_group.add(&editor.category);
    item_group.add(&editor.item_row);
    item_group.add(&editor.tier_row);
    item_group.add(&editor.exact_tier);
    item_group.add(&editor.bounded_tier);

    editor.upgrade_group.add(&editor.upgrade_row);
    editor.upgrade_group.add(&editor.exact_upgrade);
    editor.upgrade_group.add(&editor.minimum_upgrade);
    editor.upgrade_group.add(&editor.ring_minimum_upgrade);

    editor.count_group.add(&editor.count_row);
    editor.count_group.add(&editor.copy_floor_switch);
    editor.count_group.add(&editor.copy_floor_value);
    editor.count_group.add(&editor.levels_switch);
    editor.count_group.add(&editor.levels_value);

    editor.effect_mode_group.add(&editor.effect_mode);

    let details_group = adw::PreferencesGroup::builder().title("Details").build();
    details_group.add(&editor.uncursed);
    details_group.add(&editor.source_row);
    details_group.add(&editor.floor_switch);
    details_group.add(&editor.floor_value);

    vec![
        item_group,
        editor.upgrade_group.clone(),
        editor.count_group.clone(),
        editor.effect_mode_group.clone(),
        editor.effect_group.clone(),
        details_group,
    ]
}

fn connect(editor: &Rc<Editor>) {
    editor
        .category
        .connect_selected_notify(hook(Rc::clone(editor), |editor| {
            // Keep selections that remain valid under the new category (for
            // example, switching Weapon to Thrown with a shuriken pinned);
            // anything absent from the repopulated lists falls back to Any.
            populate_items(editor, selected_item(editor));
            populate_effects(editor, selected_effect(editor));
            editor.tier_row.set_selected(0);
            normalize_upgrades(editor);
            refresh_levels_range(editor);
            refresh_visibility(editor);
        }));
    editor
        .item_row
        .connect_selected_notify(hook(Rc::clone(editor), |editor| {
            if selected_item(editor).is_some() {
                editor.tier_row.set_selected(0);
            } else {
                // Only a named item can count its levels together.
                editor.levels_switch.set_active(false);
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
            normalize_upgrades(editor);
            refresh_levels_range(editor);
            refresh_visibility(editor);
        }));
    editor
        .count_row
        .connect_value_notify(hook(Rc::clone(editor), |editor| {
            if selected_count(editor) < 2 {
                editor.levels_switch.set_active(false);
                editor.copy_floor_switch.set_active(false);
            }
            refresh_levels_range(editor);
            refresh_visibility(editor);
        }));
    for row in [&editor.levels_switch, &editor.copy_floor_switch] {
        row.connect_active_notify(hook(Rc::clone(editor), |editor| {
            refresh_levels_range(editor);
            refresh_visibility(editor);
        }));
    }
    editor
        .levels_value
        .connect_value_notify(hook(Rc::clone(editor), refresh_levels_range));
    editor
        .effect_mode
        .connect_selected_notify(hook(Rc::clone(editor), refresh_visibility));
    editor
        .uncursed
        .connect_active_notify(hook(Rc::clone(editor), apply_curse_visibility));
    editor
        .floor_switch
        .connect_active_notify(hook(Rc::clone(editor), refresh_visibility));
    skip_empty_boss_floors(&editor.floor_value);
    skip_empty_boss_floors(&editor.copy_floor_value);
}

/// Wraps a handler so programmatic updates never re-enter it. Any edit also
/// retires the validation message of the previous save attempt.
fn hook<W>(editor: Rc<Editor>, handler: fn(&Rc<Editor>)) -> impl Fn(&W) {
    move |_| {
        if editor.updating.get() {
            return;
        }
        editor.updating.set(true);
        editor.banner.set_revealed(false);
        handler(&editor);
        editor.updating.set(false);
    }
}

fn restore(editor: &Rc<Editor>, requirement: &UiRequirement, stack: StackShape) {
    editor.updating.set(true);
    let kind_index = ALL_KIND_CHOICES
        .iter()
        .position(|choice| *choice == requirement.kind_choice())
        .unwrap_or(0);
    editor
        .category
        .set_selected(u32::try_from(kind_index).unwrap_or(0));
    editor.uncursed.set_active(requirement.require_uncursed);
    populate_items(editor, requirement.item);
    populate_effects(editor, requirement.effect);
    normalize_upgrades(editor);
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
            let maximum = selected_kind(editor).maximum_search_upgrade();
            editor
                .exact_upgrade
                .set_value(f64::from(upgrade.clamp(1, maximum)));
        }
        UpgradeRequirement::AtLeast(upgrade) => {
            editor.upgrade_row.set_selected(2);
            set_minimum_upgrade(editor, upgrade);
        }
    }
    let source_index = requirement
        .source
        .and_then(|source| ItemSource::ALL.iter().position(|other| *other == source))
        .map_or(0, |index| index + 1);
    editor
        .source_row
        .set_selected(u32::try_from(source_index).unwrap_or(0));
    #[allow(clippy::cast_precision_loss)] // The count is 1..=STACK_MAX.
    let count = stack.count.clamp(1, STACK_MAX) as f64;
    editor.count_row.set_value(count);
    if let Some(total) = stack.total {
        editor.levels_switch.set_active(true);
        editor.levels_value.set_value(f64::from(total));
    }
    if let Some(depth) = stack.copy_depth {
        editor.copy_floor_switch.set_active(true);
        editor
            .copy_floor_value
            .set_value(f64::from(normalize_floor_limit(depth)));
    }
    refresh_levels_range(editor);
    if let Some(depth) = requirement.max_depth {
        editor.floor_switch.set_active(true);
        editor
            .floor_value
            .set_value(f64::from(normalize_floor_limit(depth)));
    }
    refresh_visibility(editor);
    editor.updating.set(false);
}

/// The editor's result: the row itself, then the stack it asks for — how
/// many items, their combined level, and the floor limit of the extra copies.
fn collect(editor: &Rc<Editor>) -> (UiRequirement, usize, Option<u8>, Option<u8>) {
    let (kind, weapon_category) = selected_choice(editor);
    let item = selected_item(editor);
    let tier_eligible = item.is_none() && matches!(kind, ItemKind::Weapon | ItemKind::Armor);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let exact_tier = editor.exact_tier.value().round() as u8;
    let bounded_tier = u8::try_from(editor.bounded_tier.selected())
        .map_or(BOUNDED_TIER_MIN, |offset| {
            BOUNDED_TIER_MIN.saturating_add(offset)
        });
    let tier = match editor.tier_row.selected() {
        1 if tier_eligible => TierRequirement::Exact(exact_tier),
        2 if tier_eligible => TierRequirement::AtLeast(bounded_tier),
        3 if tier_eligible => TierRequirement::AtMost(bounded_tier),
        _ => TierRequirement::Any,
    };
    let upgrade = selected_upgrade(editor);
    let source = match editor.source_row.selected() {
        0 => None,
        index => ItemSource::ALL.get(index as usize - 1).copied(),
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let max_depth = editor
        .floor_switch
        .is_active()
        .then(|| normalize_floor_limit(editor.floor_value.value().round() as u8));
    let requirement = UiRequirement {
        key: editor.key,
        kind,
        weapon_category,
        item,
        tier,
        upgrade,
        effect: selected_effect(editor),
        require_uncursed: editor.uncursed.is_active(),
        source,
        // The stack's own encoding carries these; the board rebuilds them
        // from the count and total this returns.
        identity_group: None,
        max_depth,
        alternative_group: editor.alternative_group,
        level_sum: None,
    };
    let count = selected_count(editor);
    let total = countable_levels(editor).then(|| selected_total(editor));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let copy_depth = (count > 1 && total.is_none() && editor.copy_floor_switch.is_active())
        .then(|| normalize_floor_limit(editor.copy_floor_value.value().round() as u8));
    (requirement, count, total, copy_depth)
}

/// The editor's own checks before the engine's: the specific-effect list
/// needs a selection, and the whole query must stay valid with `result`
/// stored.
fn check(
    editor: &Rc<Editor>,
    result: &UiRequirement,
    count: usize,
    total: Option<u8>,
    copy_depth: Option<u8>,
) -> Result<(), String> {
    if enchantable(selected_kind(editor))
        && editor.effect_mode.selected() == EFFECT_SPECIFIC
        && checked_effects(editor).is_empty()
    {
        return Err(if selected_kind(editor) == ItemKind::Armor {
            "Choose at least one glyph or curse".to_owned()
        } else {
            "Choose at least one enchantment or curse".to_owned()
        });
    }
    editor
        .context
        .validate_draft(result, count, total, copy_depth)
}

fn selected_choice(editor: &Rc<Editor>) -> KindChoice {
    ALL_KIND_CHOICES
        .get(editor.category.selected() as usize)
        .copied()
        .unwrap_or((ItemKind::Weapon, None))
}

fn selected_kind(editor: &Rc<Editor>) -> ItemKind {
    selected_choice(editor).0
}

const fn enchantable(kind: ItemKind) -> bool {
    matches!(kind, ItemKind::Weapon | ItemKind::Armor)
}

fn selected_item(editor: &Rc<Editor>) -> Option<ItemId> {
    editor
        .items
        .borrow()
        .get(editor.item_row.selected() as usize)
        .copied()
        .flatten()
}

fn selected_upgrade(editor: &Rc<Editor>) -> UpgradeRequirement {
    let kind = selected_kind(editor);
    match editor.upgrade_row.selected() {
        1 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let value = editor.exact_upgrade.value().round() as u8;
            UpgradeRequirement::Exact(value)
        }
        2 => {
            let value = if kind == ItemKind::Ring {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let value = editor.ring_minimum_upgrade.value().round() as u8;
                value
            } else {
                u8::try_from(editor.minimum_upgrade.selected() + 1).unwrap_or(1)
            };
            UpgradeRequirement::AtLeast(value)
        }
        _ => UpgradeRequirement::Any,
    }
}

/// The effect predicate the picker currently describes. Wands and rings
/// carry no effects; an empty specific selection reads as the wildcard and
/// is refused by [`check`] instead.
fn selected_effect(editor: &Rc<Editor>) -> EffectRequirement {
    let kind = selected_kind(editor);
    if !enchantable(kind) {
        return EffectRequirement::Any;
    }
    match editor.effect_mode.selected() {
        EFFECT_ANY_ENCHANTMENT => {
            EffectSet::enchantments(kind).map_or(EffectRequirement::Any, EffectRequirement::OneOf)
        }
        EFFECT_SPECIFIC => EffectSet::from_effects(checked_effects(editor))
            .map_or(EffectRequirement::Any, EffectRequirement::OneOf),
        _ => EffectRequirement::Any,
    }
}

/// The checked effects of the specific list, skipping curses hidden by the
/// uncursed switch.
fn checked_effects(editor: &Rc<Editor>) -> Vec<Effect> {
    editor
        .effect_checks
        .borrow()
        .iter()
        .filter(|entry| entry.row.is_visible() && entry.check.is_active())
        .map(|entry| entry.effect)
        .collect()
}

/// How many items the row asks for; a cluster member leaves its stack to the
/// cluster and always speaks for one.
fn selected_count(editor: &Rc<Editor>) -> usize {
    if editor.in_cluster {
        return 1;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = editor.count_row.value().round().max(1.0) as usize;
    count.clamp(1, STACK_MAX)
}

/// Whether the row is a stack of a named item whose levels count together —
/// the only shape a combined level can describe.
fn countable_levels(editor: &Rc<Editor>) -> bool {
    !editor.in_cluster
        && selected_item(editor).is_some()
        && selected_count(editor) > 1
        && editor.levels_switch.is_active()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn selected_total(editor: &Rc<Editor>) -> u8 {
    editor.levels_value.value().round().max(1.0) as u8
}

/// The most levels the stack could reach: every item counts its upgrade plus
/// one, and a member of a combined-level stack may carry any upgrade.
fn levels_capacity(editor: &Rc<Editor>) -> u8 {
    let per_item = selected_kind(editor).maximum_search_upgrade() + 1;
    u8::try_from(selected_count(editor))
        .unwrap_or(1)
        .saturating_mul(per_item)
        .max(1)
}

fn set_tier_value(editor: &Rc<Editor>, tier: u8) {
    editor.exact_tier.set_value(f64::from(tier));
    editor
        .bounded_tier
        .set_selected(u32::from(tier.clamp(3, 4) - 3));
}

/// Items offered for one category choice. Tier-1 equipment is starting gear
/// and never spawns in the dungeon, so it is not searchable.
fn searchable_items(choice: KindChoice) -> Vec<&'static ItemDefinition> {
    let (kind, weapon_category) = choice;
    let mut items: Vec<_> = ITEMS
        .iter()
        .filter(|definition| {
            definition.kind == kind
                && definition.tier != Some(1)
                && weapon_category
                    .is_none_or(|category| definition.weapon_category() == Some(category))
        })
        .collect();
    if matches!(kind, ItemKind::Weapon | ItemKind::Armor) {
        items.sort_by_key(|definition| definition.tier);
    }
    items
}

fn populate_items(editor: &Rc<Editor>, selection: Option<ItemId>) {
    let choice = selected_choice(editor);
    let mut ids = vec![None];
    let mut labels = vec![format!("Any {}", kind_choice_singular(choice))];
    for definition in searchable_items(choice) {
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

/// Rebuilds the effect picker for the selected category and restores
/// `selection` onto it: a set of another family falls back to Any.
fn populate_effects(editor: &Rc<Editor>, selection: EffectRequirement) {
    let kind = selected_kind(editor);
    let (mode_title, list_title) = if kind == ItemKind::Armor {
        ("Glyph", "Glyphs")
    } else {
        ("Enchantment", "Enchantments")
    };
    editor.effect_mode_group.set_title(mode_title);
    editor.effect_mode.set_title(mode_title);
    editor.effect_group.set_title(list_title);
    editor.effect_list.remove_all();

    let family: Vec<Effect> = match kind {
        ItemKind::Weapon => ALL_WEAPON_EFFECTS
            .iter()
            .map(|effect| Effect::Weapon(*effect))
            .collect(),
        ItemKind::Armor => ALL_ARMOR_EFFECTS
            .iter()
            .map(|effect| Effect::Armor(*effect))
            .collect(),
        ItemKind::Wand | ItemKind::Ring => Vec::new(),
    };
    let selected_set = match selection {
        EffectRequirement::OneOf(set) if set.family() == kind => Some(set),
        _ => None,
    };
    let checks: Vec<EffectCheck> = family
        .into_iter()
        .map(|effect| {
            let check = gtk::CheckButton::builder()
                .active(selected_set.is_some_and(|set| set.contains(effect)))
                .valign(gtk::Align::Center)
                .build();
            let row = adw::ActionRow::builder()
                .title(effect_label(effect.wire_name(), effect.is_curse()))
                .activatable_widget(&check)
                .build();
            row.add_prefix(&check);
            check.connect_toggled({
                let banner = editor.banner.clone();
                move |_| banner.set_revealed(false)
            });
            editor.effect_list.append(&row);
            EffectCheck { effect, row, check }
        })
        .collect();
    editor.effect_checks.replace(checks);
    apply_curse_visibility(editor);

    let mode = match selected_set {
        None => EFFECT_ANY,
        Some(set) if EffectSet::enchantments(kind) == Some(set) => EFFECT_ANY_ENCHANTMENT,
        Some(_) => EFFECT_SPECIFIC,
    };
    editor.effect_mode.set_selected(mode);
}

/// Hides (and unchecks) the curse rows while the item must be uncursed.
fn apply_curse_visibility(editor: &Rc<Editor>) {
    let hide_curses = editor.uncursed.is_active();
    for entry in editor.effect_checks.borrow().iter() {
        if !entry.effect.is_curse() {
            continue;
        }
        entry.row.set_visible(!hide_curses);
        if hide_curses {
            entry.check.set_active(false);
        }
    }
}

fn effect_label(name: &str, is_curse: bool) -> String {
    if is_curse {
        format!("{name} · curse")
    } else {
        name.to_owned()
    }
}

fn normalize_upgrades(editor: &Rc<Editor>) {
    let maximum_upgrade = selected_kind(editor).maximum_search_upgrade();
    let maximum = f64::from(maximum_upgrade);
    let adjustment = editor.exact_upgrade.adjustment();
    adjustment.set_lower(1.0);
    adjustment.set_upper(maximum);
    editor
        .exact_upgrade
        .set_value(editor.exact_upgrade.value().clamp(1.0, maximum));
    let minimum = u8::try_from(editor.minimum_upgrade.selected() + 1).unwrap_or(1);
    populate_minimum_upgrades(editor, minimum);
    let ring_adjustment = editor.ring_minimum_upgrade.adjustment();
    ring_adjustment.set_lower(1.0);
    ring_adjustment.set_upper(f64::from(maximum_upgrade - 1));
    editor.ring_minimum_upgrade.set_value(
        editor
            .ring_minimum_upgrade
            .value()
            .clamp(1.0, f64::from(maximum_upgrade - 1)),
    );
}

/// Bounds the combined level by what the stack could carry together, and
/// spells the value out the way the chip's badge reads it.
fn refresh_levels_range(editor: &Rc<Editor>) {
    let capacity = levels_capacity(editor);
    let adjustment = editor.levels_value.adjustment();
    adjustment.set_lower(1.0);
    adjustment.set_upper(f64::from(capacity));
    editor
        .levels_value
        .set_value(editor.levels_value.value().clamp(1.0, f64::from(capacity)));
    editor.levels_value.set_subtitle(&format!(
        "\u{2265} {} across up to {}",
        selected_total(editor),
        selected_count(editor)
    ));
}

fn populate_minimum_upgrades(editor: &Rc<Editor>, selection: u8) {
    let maximum = selected_kind(editor).maximum_search_upgrade();
    let labels: Vec<_> = (1..maximum)
        .map(|upgrade| format!("+{upgrade} or higher"))
        .collect();
    let label_refs: Vec<_> = labels.iter().map(String::as_str).collect();
    editor
        .minimum_upgrade
        .set_model(Some(&gtk::StringList::new(&label_refs)));
    editor
        .minimum_upgrade
        .set_selected(u32::from(selection.clamp(1, maximum - 1) - 1));
}

fn set_minimum_upgrade(editor: &Rc<Editor>, upgrade: u8) {
    populate_minimum_upgrades(editor, upgrade);
    let maximum = selected_kind(editor).maximum_search_upgrade();
    editor
        .ring_minimum_upgrade
        .set_value(f64::from(upgrade.clamp(1, maximum - 1)));
}

fn refresh_visibility(editor: &Rc<Editor>) {
    let kind = selected_kind(editor);
    let wildcard_equipment = selected_item(editor).is_none() && enchantable(kind);
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
        .exact_upgrade
        .set_visible(editor.upgrade_row.selected() == 1);
    editor
        .minimum_upgrade
        .set_visible(editor.upgrade_row.selected() == 2 && kind != ItemKind::Ring);
    editor
        .ring_minimum_upgrade
        .set_visible(editor.upgrade_row.selected() == 2 && kind == ItemKind::Ring);
    // A stack of two or more may bound its extra copies, or count their
    // levels together when they are copies of one named item. A combined
    // level speaks for the whole stack, so the per-item upgrade steps aside.
    let counting_levels = countable_levels(editor);
    let stacked = !editor.in_cluster && selected_count(editor) > 1;
    editor.upgrade_group.set_visible(!counting_levels);
    editor.count_group.set_visible(!editor.in_cluster);
    editor
        .copy_floor_switch
        .set_visible(stacked && !counting_levels);
    editor
        .copy_floor_value
        .set_visible(stacked && !counting_levels && editor.copy_floor_switch.is_active());
    editor
        .levels_switch
        .set_visible(stacked && selected_item(editor).is_some());
    editor.levels_value.set_visible(counting_levels);
    editor.effect_mode_group.set_visible(enchantable(kind));
    editor
        .effect_group
        .set_visible(enchantable(kind) && editor.effect_mode.selected() == EFFECT_SPECIFIC);
    editor
        .floor_value
        .set_visible(editor.floor_switch.is_active());
}

/// The tier labels of the at-least/at-most picker, one per tier the engine
/// accepts as a bound.
fn bounded_tier_labels() -> Vec<String> {
    (BOUNDED_TIER_MIN..=BOUNDED_TIER_MAX)
        .map(|tier| format!("Tier {tier}"))
        .collect()
}

fn borrowed(labels: &[String]) -> Vec<&str> {
    labels.iter().map(String::as_str).collect()
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
