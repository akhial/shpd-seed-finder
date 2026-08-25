// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure edits behind the requirement board, ported from the web design's
//! `relations.ts` so every frontend writes the same documents. Each edit
//! returns a new requirement list in the canonical encoding, so share links,
//! presets and results files round-trip; the board renders the *collapsed*
//! view that [`board_items`] derives from the flat list.
//!
//! Two ideas cover all three relationship kinds of the model:
//!
//! - an *either/or cluster* is several requirements sharing an
//!   [`UiRequirement::alternative_group`]: one slot, any member fills it;
//! - a *stack* is a chip (or a whole cluster) asking for more than one item of
//!   the same kind — the blacksmith's reforge fodder. Its extra copies never
//!   carry their own constraints. A stack of a concrete item encodes as plain
//!   repeated requirements; a wildcard or cluster stack encodes as bare copies
//!   tied to the anchor with an [`UiRequirement::identity_group`]; a stack with
//!   a *combined level* encodes as identical members sharing a [`LevelSum`]
//!   (each matched item counts upgrade + 1 towards the total, and members are
//!   optional, so "up to N items reaching T levels").
//!
//! Every row keeps its session key through every edit, so the board can find
//! the chip it was acting on however the list around it moved.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use shpd_seedfinder_core::catalog::ItemId;
use shpd_seedfinder_core::query::{
    EffectRequirement, LevelSum, MAX_IDENTITY_GROUP, MAX_LEVEL_SUM_GROUP, TierRequirement,
    UpgradeRequirement,
};

use crate::state::UiRequirement;

/// The most items one chip or cluster may ask for. Beyond three the board
/// stops reading as one thing, and no reforge chain needs more.
pub const STACK_MAX: usize = 3;

/// A board entry's identity, stable while the entry survives an edit: a chip
/// is named by its anchor row's session key, a cluster by its group.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BoardKey {
    Chip(u64),
    Cluster(u8),
}

/// One board entry: a chip, or an either/or cluster of chips.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardItem {
    pub key: BoardKey,
    /// Visible requirement indices: one for a chip, all members for a cluster.
    pub members: Vec<usize>,
    /// The cluster's alternative group, when this is a cluster.
    pub cluster: Option<u8>,
    /// Hidden copy indices behind the stack badge, in requirement order.
    pub extras: Vec<usize>,
    /// The stack's combined level, when one is set.
    pub total: Option<u8>,
}

impl BoardItem {
    /// How many items this asks for: its anchor plus the hidden copies.
    #[must_use]
    pub fn stack_count(&self) -> usize {
        1 + self.extras.len()
    }

    /// The requirement the badges and the editor act on.
    #[must_use]
    pub fn anchor(&self) -> usize {
        self.members[0]
    }
}

/// Whether a requirement constrains anything beyond its category; a stack's
/// extra copies are exactly the unconstrained ones. A per-item floor limit is
/// a placement bound, not an item property, and does not count.
fn is_bare(requirement: &UiRequirement) -> bool {
    requirement.to_core().is_bare()
}

/// Whether `copy` is the plain repeat of the named `item`. A floor limit is a
/// placement bound, not an item property, so a repeat that carries only one
/// still folds into its stack.
fn is_plain_item_copy(copy: &UiRequirement, item: ItemId) -> bool {
    copy.item == Some(item)
        && matches!(copy.tier, TierRequirement::Any)
        && matches!(copy.upgrade, UpgradeRequirement::Any)
        && matches!(copy.effect, EffectRequirement::Any)
        && !copy.require_uncursed
        && copy.source.is_none()
        && copy.identity_group.is_none()
        && copy.alternative_group.is_none()
        && copy.level_sum.is_none()
}

/// One combined-level group as the board reads it: the first member anchors,
/// the rest fold into its badge.
struct SumAnchor {
    anchor: usize,
    extras: Vec<usize>,
    total: u8,
}

/// Folds a level-sum group and an identity stack into the entry anchored at
/// `anchor_index`.
fn attach(
    item: &mut BoardItem,
    anchor_index: usize,
    requirements: &[UiRequirement],
    sums: &BTreeMap<u8, SumAnchor>,
    identity_extras: &HashMap<usize, Vec<usize>>,
) {
    if let Some(sum) = requirements[anchor_index].level_sum
        && let Some(group) = sums.get(&sum.group)
        && group.anchor == anchor_index
    {
        item.extras.extend(group.extras.iter().copied());
        item.total = Some(group.total);
    }
    if let Some(extras) = identity_extras.get(&anchor_index) {
        item.extras.extend(extras.iter().copied());
    }
}

/// The board's collapsed view of the flat requirement list: clusters group
/// alternatives, and a stack's copies fold into their anchor's badge.
#[must_use]
#[allow(clippy::too_many_lines)] // Three collapses read best in one pass.
pub fn board_items(requirements: &[UiRequirement]) -> Vec<BoardItem> {
    let mut hidden = vec![false; requirements.len()];

    // Combined-level groups: the first member anchors, the rest fold away.
    let mut sums: BTreeMap<u8, SumAnchor> = BTreeMap::new();
    for (index, requirement) in requirements.iter().enumerate() {
        let Some(sum) = requirement.level_sum else {
            continue;
        };
        sums.entry(sum.group)
            .and_modify(|group| group.extras.push(index))
            .or_insert_with(|| SumAnchor {
                anchor: index,
                extras: Vec::new(),
                total: sum.minimum_total,
            });
    }
    for group in sums.values() {
        for &index in &group.extras {
            hidden[index] = true;
        }
    }

    // Identity stacks: bare copies fold into the constrained unit (or the
    // first member when every member is bare). Groups with two constrained
    // units cannot collapse; validation reports them.
    let mut identity_extras: HashMap<usize, Vec<usize>> = HashMap::new();
    for members in identity_groups(requirements).values() {
        let constrained: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&index| !is_bare(&requirements[index]))
            .collect();
        let units: BTreeSet<(Option<u8>, usize)> = constrained
            .iter()
            .map(|&index| unit_of(requirements, index))
            .collect();
        if units.len() > 1 {
            continue;
        }
        let anchor = constrained.first().copied().unwrap_or(members[0]);
        // A cluster anchor labels every member; fold only the lone bare copies.
        let extras: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&index| {
                index != anchor
                    && requirements[index].alternative_group.is_none()
                    && is_bare(&requirements[index])
            })
            .collect();
        if extras.is_empty() {
            continue;
        }
        for &index in &extras {
            hidden[index] = true;
        }
        identity_extras.insert(anchor, extras);
    }

    // Walk the list building chips and clusters, folding plain item repeats
    // into the nearest earlier chip naming the same item.
    let mut items: Vec<BoardItem> = Vec::new();
    let mut clusters: BTreeMap<u8, usize> = BTreeMap::new();
    let mut chip_by_item: HashMap<ItemId, usize> = HashMap::new();
    for (index, requirement) in requirements.iter().enumerate() {
        if hidden[index] {
            continue;
        }
        if let Some(group) = requirement.alternative_group {
            if let Some(&position) = clusters.get(&group) {
                items[position].members.push(index);
                attach(
                    &mut items[position],
                    index,
                    requirements,
                    &sums,
                    &identity_extras,
                );
                continue;
            }
            let mut item = BoardItem {
                key: BoardKey::Cluster(group),
                members: vec![index],
                cluster: Some(group),
                extras: Vec::new(),
                total: None,
            };
            attach(&mut item, index, requirements, &sums, &identity_extras);
            clusters.insert(group, items.len());
            items.push(item);
            continue;
        }
        // A plain repeat of an earlier chip's item folds into that chip.
        if let Some(item_id) = requirement.item
            && is_plain_item_copy(requirement, item_id)
            && let Some(&position) = chip_by_item.get(&item_id)
            && items[position].total.is_none()
            && items[position].stack_count() < STACK_MAX
        {
            items[position].extras.push(index);
            continue;
        }
        let mut item = BoardItem {
            key: BoardKey::Chip(requirement.key),
            members: vec![index],
            cluster: None,
            extras: Vec::new(),
            total: None,
        };
        attach(&mut item, index, requirements, &sums, &identity_extras);
        if let Some(item_id) = requirement.item
            && requirement.level_sum.is_none()
        {
            chip_by_item.insert(item_id, items.len());
        }
        items.push(item);
    }
    // Single-member clusters render as chips.
    for item in &mut items {
        if item.cluster.is_some() && item.members.len() == 1 {
            item.cluster = None;
            item.key = BoardKey::Chip(requirements[item.members[0]].key);
        }
    }
    items
}

/// The number of visible board entries, for the pane's header count.
#[must_use]
pub fn board_count(requirements: &[UiRequirement]) -> usize {
    board_items(requirements).len()
}

/// The board entry holding the row `key`, if any.
#[must_use]
pub fn item_of_key(requirements: &[UiRequirement], key: u64) -> Option<BoardItem> {
    let index = requirements.iter().position(|row| row.key == key)?;
    board_items(requirements)
        .into_iter()
        .find(|item| item.members.contains(&index) || item.extras.contains(&index))
}

/// The unit a constrained identity member belongs to: a lone requirement, or
/// the whole alternative group it sits in.
fn unit_of(requirements: &[UiRequirement], index: usize) -> (Option<u8>, usize) {
    requirements[index]
        .alternative_group
        .map_or((None, index), |group| (Some(group), 0))
}

fn identity_groups(requirements: &[UiRequirement]) -> BTreeMap<u8, Vec<usize>> {
    let mut groups: BTreeMap<u8, Vec<usize>> = BTreeMap::new();
    for (index, requirement) in requirements.iter().enumerate() {
        if let Some(group) = requirement.identity_group {
            groups.entry(group).or_default().push(index);
        }
    }
    groups
}

/// The lowest group label from 1 to `maximum` that nothing uses yet.
fn free_group(used: impl IntoIterator<Item = Option<u8>>, maximum: u8) -> Option<u8> {
    let taken: BTreeSet<u8> = used.into_iter().flatten().collect();
    (1..=maximum).find(|group| !taken.contains(group))
}

fn next_alternative_group(requirements: &[UiRequirement]) -> u8 {
    requirements
        .iter()
        .filter_map(|row| row.alternative_group)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

/// The bare copy a stack of `anchor`'s kind grows by; it may carry its own
/// floor limit, the one bound that is a placement, not an item property. The
/// engine reads a stack's copies as *unconstrained*, so a copy names only the
/// family — never the anchor's melee/thrown narrowing, which the shared item
/// identity already implies.
fn bare_copy(
    anchor: &UiRequirement,
    identity_group: u8,
    key: u64,
    max_depth: Option<u8>,
) -> UiRequirement {
    UiRequirement {
        kind: anchor.kind,
        identity_group: Some(identity_group),
        max_depth,
        ..UiRequirement::new(key)
    }
}

/// The plain repeat a concrete stack of `anchor`'s item grows by.
fn plain_copy(anchor: &UiRequirement, key: u64, max_depth: Option<u8>) -> UiRequirement {
    UiRequirement {
        kind: anchor.kind,
        item: anchor.item,
        max_depth,
        ..UiRequirement::new(key)
    }
}

/// Rewrites the list into its canonical stack encoding and drops every group
/// that no longer says anything:
///
/// - a lone alternative, a lone identity label and a lone level-sum member
///   dissolve;
/// - a labelled cluster labels every one of its members;
/// - a stack anchored on a lone concrete chip carries plain repeats, not
///   identity labels.
///
/// Every operation funnels through this, so a deleted anchor can never leave
/// stale groups behind.
#[must_use]
pub fn normalize(requirements: &[UiRequirement]) -> Vec<UiRequirement> {
    let mut next = requirements.to_vec();
    // A cluster that holds an identity label spreads it to all its members.
    let mut cluster_label: BTreeMap<u8, u8> = BTreeMap::new();
    for row in &next {
        if let (Some(cluster), Some(label)) = (row.alternative_group, row.identity_group) {
            cluster_label.insert(cluster, label);
        }
    }
    for row in &mut next {
        if let Some(cluster) = row.alternative_group
            && let Some(&label) = cluster_label.get(&cluster)
        {
            row.identity_group = Some(label);
        }
    }
    // A stack anchored on a lone concrete chip encodes as plain repeats.
    for members in identity_groups(&next).values() {
        let constrained: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&index| !is_bare(&next[index]))
            .collect();
        let [anchor_index] = constrained[..] else {
            continue;
        };
        let anchor = next[anchor_index];
        if anchor.item.is_none() || anchor.alternative_group.is_some() {
            continue;
        }
        for &index in members {
            next[index] = if index == anchor_index {
                UiRequirement {
                    identity_group: None,
                    ..anchor
                }
            } else {
                plain_copy(&anchor, next[index].key, next[index].max_depth)
            };
        }
    }
    // Groups of one say nothing.
    let alternatives = counted(next.iter().map(|row| row.alternative_group));
    let identities = counted(next.iter().map(|row| row.identity_group));
    let level_sums = counted(next.iter().map(|row| row.level_sum.map(|sum| sum.group)));
    for row in &mut next {
        if row
            .alternative_group
            .is_some_and(|group| alternatives[&group] < 2)
        {
            row.alternative_group = None;
        }
        if row
            .identity_group
            .is_some_and(|group| identities[&group] < 2)
        {
            row.identity_group = None;
        }
        if row.level_sum.is_some_and(|sum| level_sums[&sum.group] < 2) {
            row.level_sum = None;
        }
    }
    next
}

fn counted(groups: impl IntoIterator<Item = Option<u8>>) -> BTreeMap<u8, usize> {
    let mut counts: BTreeMap<u8, usize> = BTreeMap::new();
    for group in groups.into_iter().flatten() {
        *counts.entry(group).or_default() += 1;
    }
    counts
}

/// Moves the requirement at `from` after the last requirement matching `after`.
fn move_after(
    requirements: &[UiRequirement],
    from: usize,
    after: impl Fn(&UiRequirement) -> bool,
) -> Vec<UiRequirement> {
    let moving = requirements[from];
    let rest: Vec<UiRequirement> = requirements
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != from)
        .map(|(_, row)| *row)
        .collect();
    let insert_at = rest.iter().rposition(after).map_or(0, |last| last + 1);
    let mut joined = rest[..insert_at].to_vec();
    joined.push(moving);
    joined.extend_from_slice(&rest[insert_at..]);
    joined
}

fn without(requirements: &[UiRequirement], doomed: &HashSet<usize>) -> Vec<UiRequirement> {
    requirements
        .iter()
        .enumerate()
        .filter(|(index, _)| !doomed.contains(index))
        .map(|(_, row)| *row)
        .collect()
}

/// The chip at `source` becomes an either/or alternative of the chip at
/// `target`. A combined level cannot travel into a cluster and is dropped; a
/// plain-repeat stack keeps its copies by trading them for identity labels,
/// which the cluster's members then share.
#[must_use]
pub fn join_alternatives(
    requirements: &[UiRequirement],
    source: usize,
    target: usize,
) -> Vec<UiRequirement> {
    if source == target {
        return requirements.to_vec();
    }
    let group = requirements[target]
        .alternative_group
        .unwrap_or_else(|| next_alternative_group(requirements));
    if requirements[source].alternative_group == Some(group) {
        return requirements.to_vec();
    }
    let source_key = requirements[source].key;
    let target_key = requirements[target].key;
    // A copy has to name the kind it copies, and a cluster spanning categories
    // names none — "ring or wand" is not a kind anything can be a copy of. So
    // a stack follows its chip into a cluster only while the cluster stays
    // within one category.
    let members: Vec<usize> = (0..requirements.len())
        .filter(|&index| {
            index == source
                || index == target
                || requirements[index].alternative_group == Some(group)
        })
        .collect();
    let one_category = members
        .iter()
        .all(|&index| requirements[index].kind == requirements[members[0]].kind);
    let mut next = requirements.to_vec();
    if one_category {
        // Trade plain repeats for identity copies so the stack survives the move.
        for index in [source, target] {
            let anchor = next[index];
            let Some(item_id) = anchor.item else { continue };
            if anchor.identity_group.is_some() {
                continue;
            }
            let copies: Vec<usize> = (0..next.len())
                .filter(|&other| other != index && is_plain_item_copy(&next[other], item_id))
                .collect();
            if copies.is_empty() {
                continue;
            }
            let Some(label) = free_group(
                next.iter().map(|row| row.identity_group),
                MAX_IDENTITY_GROUP,
            ) else {
                continue;
            };
            next[index].identity_group = Some(label);
            for other in copies {
                next[other] = bare_copy(&anchor, label, next[other].key, next[other].max_depth);
            }
        }
    } else {
        // The stacks let go: labelled copies are dropped and plain repeats stay
        // the standalone chips they already encode as. The chip's badge falling
        // back to ×1 is the visible half of this.
        let labels: BTreeSet<u8> = members
            .iter()
            .filter_map(|&index| requirements[index].identity_group)
            .collect();
        let keys: BTreeSet<u64> = members
            .iter()
            .map(|&index| requirements[index].key)
            .collect();
        next.retain(|row| {
            !row.identity_group
                .is_some_and(|label| labels.contains(&label) && !keys.contains(&row.key))
        });
        for row in &mut next {
            if row
                .identity_group
                .is_some_and(|label| labels.contains(&label))
            {
                row.identity_group = None;
            }
        }
    }
    let Some(moved_source) = next.iter().position(|row| row.key == source_key) else {
        return requirements.to_vec();
    };
    let moved_target = next.iter().position(|row| row.key == target_key);
    for (index, row) in next.iter_mut().enumerate() {
        if index == moved_source || Some(index) == moved_target {
            row.alternative_group = Some(group);
            row.level_sum = None;
        }
    }
    let moved = move_after(&next, moved_source, |row| {
        row.alternative_group == Some(group)
    });
    normalize(&moved)
}

/// Pulls the chip at `index` out of its cluster; it leaves its stack behind.
#[must_use]
pub fn detach(requirements: &[UiRequirement], index: usize) -> Vec<UiRequirement> {
    let mut next = requirements.to_vec();
    next[index].alternative_group = None;
    next[index].identity_group = None;
    normalize(&next)
}

/// Deletes a whole board item: its members and its hidden copies.
#[must_use]
pub fn remove_item(requirements: &[UiRequirement], item: &BoardItem) -> Vec<UiRequirement> {
    let doomed: HashSet<usize> = item
        .members
        .iter()
        .chain(item.extras.iter())
        .copied()
        .collect();
    normalize(&without(requirements, &doomed))
}

/// Deletes one cluster member; the cluster and its stack live on without it.
#[must_use]
pub fn remove_member(requirements: &[UiRequirement], index: usize) -> Vec<UiRequirement> {
    normalize(&without(requirements, &HashSet::from([index])))
}

/// Whether the entry can ask for more than one item. A copy has to name the
/// kind it copies, and a cluster spanning two categories names none — "spear
/// or ring" is not a kind anything can be a copy of.
#[must_use]
pub fn can_stack(requirements: &[UiRequirement], item: &BoardItem) -> bool {
    let anchor = requirements[item.anchor()].kind;
    item.members
        .iter()
        .all(|&index| requirements[index].kind == anchor)
}

/// Sets how many items the board item anchored at `item` asks for. New copies
/// claim their session keys from `next_key`, the counter behind
/// [`crate::state::AppState::claim_key`].
#[must_use]
pub fn set_stack_count(
    requirements: &[UiRequirement],
    item: &BoardItem,
    count: usize,
    next_key: &mut u64,
) -> Vec<UiRequirement> {
    let wanted = count.clamp(1, STACK_MAX) - 1;
    if wanted == item.extras.len() || (wanted > item.extras.len() && !can_stack(requirements, item))
    {
        return requirements.to_vec();
    }
    if wanted < item.extras.len() {
        let doomed: HashSet<usize> = item.extras[wanted..].iter().copied().collect();
        return normalize(&without(requirements, &doomed));
    }
    let anchor = requirements[item.anchor()];
    let added = wanted - item.extras.len();
    // New copies keep to the floor limit the existing copies already carry.
    let inherited = item
        .extras
        .first()
        .and_then(|&index| requirements[index].max_depth);
    let mut next = requirements.to_vec();
    let template = if item.total.is_some() && anchor.level_sum.is_some() {
        anchor
    } else if item.cluster.is_none() && anchor.item.is_some() {
        plain_copy(&anchor, 0, inherited)
    } else {
        let Some(label) = anchor.identity_group.or_else(|| {
            free_group(
                next.iter().map(|row| row.identity_group),
                MAX_IDENTITY_GROUP,
            )
        }) else {
            return requirements.to_vec();
        };
        for &index in &item.members {
            next[index].identity_group = Some(label);
        }
        bare_copy(&anchor, label, 0, inherited)
    };
    let insert_at = item
        .members
        .iter()
        .chain(item.extras.iter())
        .max()
        .copied()
        .unwrap_or(0)
        + 1;
    for offset in 0..added {
        let key = *next_key;
        *next_key += 1;
        next.insert(insert_at + offset, UiRequirement { key, ..template });
    }
    normalize(&next)
}

/// The floor limit the stack's extra copies share (the first copy's, when a
/// hand-written document gave them different ones).
#[must_use]
pub fn copy_depth_of(requirements: &[UiRequirement], item: &BoardItem) -> Option<u8> {
    item.extras
        .first()
        .and_then(|&index| requirements[index].max_depth)
}

/// Sets or clears the floor limit of the stack's extra copies. The anchor
/// keeps its own limit: "the +3 one before floor 4, the rest wherever" and
/// "…the rest before floor 10" are both sayable. A combined-level stack has
/// identical members and no lone copies to bound.
#[must_use]
pub fn set_copy_depth(
    requirements: &[UiRequirement],
    item: &BoardItem,
    max_depth: Option<u8>,
) -> Vec<UiRequirement> {
    if item.total.is_some() {
        return requirements.to_vec();
    }
    let mut next = requirements.to_vec();
    for &index in &item.extras {
        next[index].max_depth = max_depth;
    }
    normalize(&next)
}

/// Sets or clears the stack's combined level. Only a lone concrete chip can
/// count levels; with a total the whole stack becomes identical optional
/// members ("up to N items reaching T levels"), without one it returns to an
/// anchor with plain repeats ("exactly N of the item").
#[must_use]
pub fn set_stack_total(
    requirements: &[UiRequirement],
    item: &BoardItem,
    total: Option<u8>,
) -> Vec<UiRequirement> {
    let anchor = requirements[item.anchor()];
    if item.cluster.is_some() || anchor.item.is_none() {
        return requirements.to_vec();
    }
    let indices: HashSet<usize> = std::iter::once(item.anchor())
        .chain(item.extras.iter().copied())
        .collect();
    let mut next = requirements.to_vec();
    let Some(total) = total else {
        for &index in &indices {
            next[index] = if index == item.anchor() {
                UiRequirement {
                    level_sum: None,
                    ..anchor
                }
            } else {
                plain_copy(&anchor, next[index].key, None)
            };
        }
        return normalize(&next);
    };
    let Some(group) = anchor.level_sum.map(|sum| sum.group).or_else(|| {
        free_group(
            requirements
                .iter()
                .map(|row| row.level_sum.map(|sum| sum.group)),
            MAX_LEVEL_SUM_GROUP,
        )
    }) else {
        return requirements.to_vec();
    };
    for &index in &indices {
        next[index] = UiRequirement {
            key: next[index].key,
            upgrade: UpgradeRequirement::Any,
            identity_group: None,
            level_sum: Some(LevelSum {
                group,
                minimum_total: total,
            }),
            ..anchor
        };
    }
    normalize(&next)
}

/// Applies the editor's result: the anchor's own fields plus the stack's
/// shape. `index` is the edited anchor, or `None` for a new chip. Editing a
/// cluster member leaves the stack's count and total to the cluster.
#[must_use]
pub fn apply_edit(
    requirements: &[UiRequirement],
    index: Option<usize>,
    requirement: UiRequirement,
    count: usize,
    total: Option<u8>,
    copy_depth: Option<u8>,
    next_key: &mut u64,
) -> Vec<UiRequirement> {
    let anchor_key = requirement.key;
    let mut next = match index {
        None => {
            let mut next = requirements.to_vec();
            next.push(requirement);
            next
        }
        Some(index) => {
            let current = requirements[index];
            // The copies belonged to the chip as it was, and the edit may have
            // changed the very kind they copy — so the stack comes down here
            // and is rebuilt below from the count and total the editor
            // returned. A cluster member leaves its stack to the cluster and
            // keeps its copies.
            let doomed: HashSet<usize> = if current.alternative_group.is_some() {
                HashSet::new()
            } else {
                board_items(requirements)
                    .into_iter()
                    .find(|entry| entry.members.contains(&index))
                    .map(|entry| entry.extras.into_iter().collect())
                    .unwrap_or_default()
            };
            let mut next = requirements.to_vec();
            next[index] = UiRequirement {
                alternative_group: current.alternative_group,
                ..requirement
            };
            without(&next, &doomed)
        }
    };
    next = normalize(&next);
    let Some(item) = item_of_key(&next, anchor_key) else {
        return next;
    };
    if item.cluster.is_some() {
        return next;
    }
    let item = if item.total.is_some() && total.is_none() {
        next = set_stack_total(&next, &item, None);
        match item_of_key(&next, anchor_key) {
            Some(item) => item,
            None => return next,
        }
    } else {
        item
    };
    next = set_stack_count(&next, &item, count, next_key);
    let Some(item) = item_of_key(&next, anchor_key) else {
        return next;
    };
    if total.is_some() {
        set_stack_total(&next, &item, total)
    } else {
        set_copy_depth(&next, &item, copy_depth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shpd_seedfinder_core::catalog::{ItemKind, WeaponCategory};
    use shpd_seedfinder_core::query::SearchQuery;

    /// A weapon requirement with the given patch applied, keyed in order.
    fn row(key: u64, patch: impl FnOnce(&mut UiRequirement)) -> UiRequirement {
        let mut requirement = UiRequirement::new(key);
        patch(&mut requirement);
        requirement
    }

    /// Every row's item, or its kind when it names none.
    fn names(requirements: &[UiRequirement]) -> Vec<String> {
        requirements
            .iter()
            .map(|row| {
                row.item
                    .map_or_else(|| format!("{:?}", row.kind), |item| format!("{item:?}"))
            })
            .collect()
    }

    /// The board entry holding the row at `index`.
    fn item_at(requirements: &[UiRequirement], index: usize) -> BoardItem {
        board_items(requirements)
            .into_iter()
            .find(|entry| entry.members.contains(&index))
            .expect("a board item must hold the row")
    }

    /// The whole list as the engine would check it.
    fn valid(requirements: &[UiRequirement]) -> Result<(), String> {
        SearchQuery {
            requirements: requirements.iter().map(|row| row.to_core()).collect(),
            max_depth: 24,
            challenges: shpd_seedfinder_core::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        }
        .validate()
        .map_err(|error| error.to_string())
    }

    #[test]
    fn dropping_a_chip_on_another_makes_one_slot_placed_after_the_target() {
        let base = vec![
            row(1, |r| r.item = Some(ItemId::Spear)),
            row(2, |r| r.kind = ItemKind::Armor),
            row(3, |r| r.item = Some(ItemId::Shuriken)),
        ];
        let next = join_alternatives(&base, 2, 0);
        assert_eq!(names(&next), ["Spear", "Shuriken", "Armor"]);
        assert_eq!(next[0].alternative_group, next[1].alternative_group);
        assert_eq!(
            board_items(&next)
                .iter()
                .map(|item| item.members.clone())
                .collect::<Vec<_>>(),
            vec![vec![0, 1], vec![2]]
        );
        assert!(valid(&next).is_ok());
    }

    #[test]
    fn joining_a_cluster_drops_a_combined_level_and_leaving_a_pair_dissolves_it() {
        let sum = LevelSum {
            group: 1,
            minimum_total: 3,
        };
        let member = |key| {
            row(key, |r| {
                r.kind = ItemKind::Ring;
                r.item = Some(ItemId::RingMight);
                r.level_sum = Some(sum);
            })
        };
        let base = vec![
            member(1),
            member(2),
            row(3, |r| r.item = Some(ItemId::Shuriken)),
        ];
        let next = join_alternatives(&base, 0, 2);
        assert!(next.iter().all(|row| row.level_sum.is_none()));
        let shuriken = next
            .iter()
            .position(|row| row.item == Some(ItemId::Shuriken))
            .unwrap();
        let out = detach(&next, shuriken);
        assert!(out.iter().all(|row| row.alternative_group.is_none()));
    }

    #[test]
    fn a_concrete_stack_encodes_as_plain_repeats() {
        let base = vec![
            row(1, |r| {
                r.kind = ItemKind::Ring;
                r.item = Some(ItemId::RingMight);
                r.upgrade = UpgradeRequirement::Exact(2);
            }),
            row(2, |r| r.kind = ItemKind::Wand),
        ];
        let mut next_key = 3;
        let next = set_stack_count(&base, &item_at(&base, 0), 3, &mut next_key);
        assert_eq!(next.len(), 4);
        assert_eq!(
            next.iter()
                .filter(|row| row.item == Some(ItemId::RingMight))
                .count(),
            3
        );
        assert!(next.iter().all(|row| row.identity_group.is_none()));
        // The board folds the repeats back into one ×3 chip.
        let board = board_items(&next);
        assert_eq!(board.len(), 2);
        assert_eq!(board[0].stack_count(), 3);
        assert_eq!(board[0].total, None);
        assert!(valid(&next).is_ok());
        // Every copy keeps a key of its own.
        let all: BTreeSet<u64> = next.iter().map(|row| row.key).collect();
        assert_eq!(all.len(), next.len());
    }

    #[test]
    fn a_wildcard_stack_encodes_as_bare_copies_sharing_an_identity_group() {
        let base = vec![row(1, |r| {
            r.kind = ItemKind::Wand;
            r.upgrade = UpgradeRequirement::AtLeast(1);
        })];
        let mut next_key = 2;
        let next = set_stack_count(&base, &item_at(&base, 0), 3, &mut next_key);
        assert_eq!(next.len(), 3);
        assert_eq!(next[0].identity_group, Some(1));
        assert!(next.iter().all(|row| row.identity_group == Some(1)));
        assert!(next[1..].iter().all(|row| row.kind == ItemKind::Wand
            && row.item.is_none()
            && matches!(row.upgrade, UpgradeRequirement::Any)));
        assert!(valid(&next).is_ok());
        assert_eq!(board_items(&next)[0].stack_count(), 3);
        // Shrinking to one dissolves the group entirely.
        let shrunk = set_stack_count(&next, &item_at(&next, 0), 1, &mut next_key);
        assert_eq!(shrunk.len(), 1);
        assert_eq!(shrunk[0].identity_group, None);
    }

    #[test]
    fn an_either_or_cluster_anchors_a_stack_and_every_member_carries_the_label() {
        let base = join_alternatives(
            &[
                row(1, |r| r.item = Some(ItemId::RunicBlade)),
                row(2, |r| r.item = Some(ItemId::WarHammer)),
            ],
            1,
            0,
        );
        let mut next_key = 3;
        let next = set_stack_count(&base, &item_at(&base, 0), 3, &mut next_key);
        assert_eq!(next.len(), 4);
        assert_eq!(
            next.iter()
                .filter(|row| row.identity_group == Some(1))
                .count(),
            4
        );
        assert_eq!(
            next.iter()
                .filter(|row| row.alternative_group.is_some())
                .count(),
            2
        );
        assert!(valid(&next).is_ok());
        let board = board_items(&next);
        assert_eq!(board.len(), 1);
        assert!(board[0].cluster.is_some());
        assert_eq!(board[0].stack_count(), 3);
        // Removing one cluster member keeps the stack on the survivor.
        let dissolved = remove_member(&next, 1);
        let board = board_items(&dissolved);
        assert_eq!(board.len(), 1);
        assert_eq!(board[0].stack_count(), 3);
        assert!(valid(&dissolved).is_ok());
    }

    #[test]
    fn a_plain_repeat_stack_trades_its_copies_for_labels_when_it_joins_a_cluster() {
        let start = vec![
            row(1, |r| r.item = Some(ItemId::Spear)),
            row(2, |r| r.item = Some(ItemId::Mace)),
        ];
        let mut next_key = 3;
        let base = set_stack_count(&start, &item_at(&start, 0), 2, &mut next_key);
        let mace = base
            .iter()
            .position(|row| row.item == Some(ItemId::Mace))
            .unwrap();
        let next = join_alternatives(&base, mace, 0);
        // The copy is now a bare weapon tied to the whole cluster.
        let copies: Vec<&UiRequirement> = next.iter().filter(|row| row.item.is_none()).collect();
        assert_eq!(copies.len(), 1);
        let label = copies[0].identity_group;
        assert!(label.is_some());
        assert!(
            next.iter()
                .filter(|row| row.alternative_group.is_some())
                .all(|row| row.identity_group == label)
        );
        assert!(valid(&next).is_ok());
    }

    #[test]
    fn deleting_the_anchor_deletes_its_copies_and_leaves_no_stale_groups() {
        let start = vec![
            row(1, |r| r.kind = ItemKind::Wand),
            row(2, |r| r.kind = ItemKind::Armor),
        ];
        let mut next_key = 3;
        let wildcard = set_stack_count(&start, &item_at(&start, 0), 3, &mut next_key);
        let after = remove_item(&wildcard, &item_at(&wildcard, 0));
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].kind, ItemKind::Armor);
        assert!(after.iter().all(|row| row.identity_group.is_none()));

        let start = vec![row(1, |r| {
            r.kind = ItemKind::Ring;
            r.item = Some(ItemId::RingMight);
        })];
        let mut next_key = 2;
        let stacked = set_stack_count(&start, &item_at(&start, 0), 2, &mut next_key);
        let total = set_stack_total(&stacked, &item_at(&stacked, 0), Some(3));
        let after = remove_item(&total, &item_at(&total, 0));
        assert!(after.is_empty());
    }

    #[test]
    fn ejecting_a_member_from_a_stacked_cluster_strips_its_label() {
        let base = join_alternatives(
            &[
                row(1, |r| r.item = Some(ItemId::Spear)),
                row(2, |r| r.item = Some(ItemId::Mace)),
            ],
            1,
            0,
        );
        let mut next_key = 3;
        let base = set_stack_count(&base, &item_at(&base, 0), 2, &mut next_key);
        let ejected = detach(&base, 0);
        let spear = ejected
            .iter()
            .find(|row| row.item == Some(ItemId::Spear))
            .unwrap();
        assert_eq!(spear.alternative_group, None);
        assert_eq!(spear.identity_group, None);
        assert!(valid(&ejected).is_ok());
    }

    #[test]
    fn a_total_turns_the_stack_into_identical_optional_members() {
        let start = vec![row(1, |r| {
            r.kind = ItemKind::Ring;
            r.item = Some(ItemId::RingMight);
            r.upgrade = UpgradeRequirement::Exact(2);
        })];
        let mut next_key = 2;
        let base = set_stack_count(&start, &item_at(&start, 0), 2, &mut next_key);
        let next = set_stack_total(&base, &item_at(&base, 0), Some(3));
        assert_eq!(next.len(), 2);
        assert!(next.iter().all(|row| row.level_sum
            == Some(LevelSum {
                group: 1,
                minimum_total: 3
            })));
        // The total speaks for the stack: per-member upgrades reset to any.
        assert!(
            next.iter()
                .all(|row| matches!(row.upgrade, UpgradeRequirement::Any))
        );
        let board = board_items(&next);
        assert_eq!(board.len(), 1);
        assert_eq!(board[0].total, Some(3));
        assert_eq!(board[0].stack_count(), 2);
        assert!(valid(&next).is_ok());
        // Clearing the total returns to plain repeats.
        let cleared = set_stack_total(&next, &board_items(&next)[0], None);
        assert!(cleared.iter().all(|row| row.level_sum.is_none()));
        assert_eq!(board_items(&cleared)[0].stack_count(), 2);
    }

    #[test]
    fn a_loaded_level_sum_document_collapses_back_into_one_chip() {
        let sum = LevelSum {
            group: 2,
            minimum_total: 4,
        };
        let member = |key| {
            row(key, |r| {
                r.kind = ItemKind::Ring;
                r.item = Some(ItemId::RingMight);
                r.level_sum = Some(sum);
            })
        };
        let loaded = vec![member(1), member(2), row(3, |r| r.kind = ItemKind::Wand)];
        let board = board_items(&loaded);
        assert_eq!(board.len(), 2);
        assert_eq!(board[0].total, Some(4));
        assert_eq!(board[0].stack_count(), 2);
    }

    #[test]
    fn the_editor_applies_count_and_total_and_rebuilds_the_stack() {
        let mut next_key = 1;
        let mut key = || {
            let key = next_key;
            next_key += 1;
            key
        };
        let ring = row(key(), |r| {
            r.kind = ItemKind::Ring;
            r.item = Some(ItemId::RingMight);
        });
        let mut requirements = apply_edit(&[], None, ring, 2, Some(3), None, &mut next_key);
        assert_eq!(requirements.len(), 2);
        assert!(
            requirements
                .iter()
                .all(|row| row.level_sum.map(|sum| sum.minimum_total) == Some(3))
        );
        // Raising the count keeps the total; clearing it returns plain repeats.
        requirements = apply_edit(
            &requirements,
            Some(0),
            requirements[0],
            3,
            Some(5),
            None,
            &mut next_key,
        );
        assert_eq!(requirements.len(), 3);
        assert!(
            requirements
                .iter()
                .all(|row| row.level_sum.map(|sum| sum.minimum_total) == Some(5))
        );
        requirements = apply_edit(
            &requirements,
            Some(0),
            requirements[0],
            2,
            None,
            None,
            &mut next_key,
        );
        assert_eq!(requirements.len(), 2);
        assert!(requirements.iter().all(|row| row.level_sum.is_none()));
        assert_eq!(
            requirements
                .iter()
                .filter(|row| row.item == Some(ItemId::RingMight))
                .count(),
            2
        );
        assert!(valid(&requirements).is_ok());
    }

    #[test]
    fn the_editor_rebuilds_the_copies_when_the_edit_changes_the_category() {
        let mut next_key = 2;
        let wand = row(1, |r| r.kind = ItemKind::Wand);
        let mut requirements = apply_edit(&[], None, wand, 3, None, None, &mut next_key);
        assert!(requirements.iter().all(|row| row.kind == ItemKind::Wand));
        // The old copies named wands; the edited chip asks for rings, so the
        // stack comes down and is rebuilt rather than keeping stale wands.
        let ring = row(1, |r| r.kind = ItemKind::Ring);
        requirements = apply_edit(&requirements, Some(0), ring, 3, None, None, &mut next_key);
        assert_eq!(requirements.len(), 3);
        assert!(requirements.iter().all(|row| row.kind == ItemKind::Ring));
        assert!(valid(&requirements).is_ok());
    }

    #[test]
    fn the_editor_shrinking_a_level_sum_stack_drops_its_orphaned_members() {
        let mut next_key = 2;
        let ring = row(1, |r| {
            r.kind = ItemKind::Ring;
            r.item = Some(ItemId::RingMight);
        });
        let mut requirements = apply_edit(&[], None, ring, 3, Some(4), None, &mut next_key);
        assert_eq!(requirements.len(), 3);
        requirements = apply_edit(&requirements, Some(0), ring, 1, None, None, &mut next_key);
        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].level_sum, None);
    }

    #[test]
    fn a_cluster_spanning_two_categories_refuses_a_stack() {
        let base = join_alternatives(
            &[
                row(1, |r| r.item = Some(ItemId::Spear)),
                row(2, |r| r.kind = ItemKind::Ring),
            ],
            1,
            0,
        );
        let mut next_key = 3;
        let next = set_stack_count(&base, &item_at(&base, 0), 2, &mut next_key);
        assert_eq!(next.len(), 2);
        assert_eq!(item_at(&next, 0).stack_count(), 1);
        assert!(valid(&next).is_ok());
    }

    #[test]
    fn a_stack_does_not_follow_its_chip_into_a_cluster_of_another_category() {
        // A copy has to name the kind it copies, and "ring or wand" names none,
        // so the second ring stays the standalone chip it already encodes as.
        let mut next_key = 2;
        let ring = row(1, |r| {
            r.kind = ItemKind::Ring;
            r.item = Some(ItemId::RingMight);
        });
        let mut requirements = apply_edit(&[], None, ring, 2, None, None, &mut next_key);
        // The last key the stack claimed, plus one, for the chip joining it.
        let key = next_key;
        requirements.push(row(key, |r| r.kind = ItemKind::Wand));
        let wand = requirements.len() - 1;
        let joined = join_alternatives(&requirements, 0, wand);
        assert!(joined.iter().all(|row| row.identity_group.is_none()));
        assert!(valid(&joined).is_ok());
        assert_eq!(board_items(&joined).len(), 2);
    }

    #[test]
    fn a_wildcard_stack_lets_go_when_its_chip_joins_another_category() {
        // The labelled copies cannot name "wand or ring", so they are dropped
        // rather than left describing an impossible identity group.
        let mut next_key = 2;
        let wand = row(1, |r| {
            r.kind = ItemKind::Wand;
            r.upgrade = UpgradeRequirement::AtLeast(2);
        });
        let mut requirements = apply_edit(&[], None, wand, 3, None, None, &mut next_key);
        assert_eq!(requirements.len(), 3);
        // The last key the stack claimed, plus one, for the chip joining it.
        let key = next_key;
        requirements.push(row(key, |r| r.kind = ItemKind::Ring));
        let ring = requirements.len() - 1;
        let joined = join_alternatives(&requirements, 0, ring);
        assert_eq!(joined.len(), 2);
        assert!(joined.iter().all(|row| row.identity_group.is_none()));
        assert!(valid(&joined).is_ok());
    }

    #[test]
    fn the_anchor_and_its_copies_carry_independent_floor_limits() {
        let mut next_key = 2;
        let armor = row(1, |r| {
            r.kind = ItemKind::Armor;
            r.item = Some(ItemId::PlateArmor);
            r.upgrade = UpgradeRequirement::Exact(3);
            r.max_depth = Some(4);
        });
        let requirements = apply_edit(&[], None, armor, 2, None, Some(9), &mut next_key);
        assert_eq!(requirements.len(), 2);
        assert_eq!(requirements[0].max_depth, Some(4));
        assert_eq!(requirements[1].max_depth, Some(9));
        // Still one chip: a repeat with only a floor limit folds into its stack.
        let board = board_items(&requirements);
        assert_eq!(board.len(), 1);
        assert_eq!(board[0].stack_count(), 2);
        assert_eq!(copy_depth_of(&requirements, &board[0]), Some(9));
        assert!(valid(&requirements).is_ok());
    }

    #[test]
    fn unlimited_copies_stay_unlimited_while_the_anchor_is_floor_bound() {
        let mut next_key = 2;
        let armor = row(1, |r| {
            r.kind = ItemKind::Armor;
            r.upgrade = UpgradeRequirement::Exact(3);
            r.max_depth = Some(4);
        });
        let requirements = apply_edit(&[], None, armor, 2, None, None, &mut next_key);
        assert_eq!(requirements[0].max_depth, Some(4));
        assert_eq!(requirements[1].max_depth, None);
        assert_eq!(
            requirements[1].identity_group,
            requirements[0].identity_group
        );
        assert!(valid(&requirements).is_ok());
    }

    #[test]
    fn a_wildcard_stack_limits_its_bare_copies_without_constraining_them() {
        let mut next_key = 2;
        let wand = row(1, |r| {
            r.kind = ItemKind::Wand;
            r.upgrade = UpgradeRequirement::AtLeast(2);
        });
        let mut requirements = apply_edit(&[], None, wand, 2, None, Some(9), &mut next_key);
        assert!(
            requirements[1..]
                .iter()
                .all(|row| row.max_depth == Some(9)
                    && matches!(row.upgrade, UpgradeRequirement::Any))
        );
        assert!(valid(&requirements).is_ok());
        // Growing the stack from the chip badge keeps the copies' floor.
        requirements = set_stack_count(&requirements, &item_at(&requirements, 0), 3, &mut next_key);
        assert_eq!(requirements.len(), 3);
        assert!(requirements[1..].iter().all(|row| row.max_depth == Some(9)));
    }

    #[test]
    fn editing_away_the_limit_clears_it_from_every_copy() {
        let mut next_key = 2;
        let sword = row(1, |r| r.item = Some(ItemId::Longsword));
        let mut requirements = apply_edit(&[], None, sword, 3, None, Some(6), &mut next_key);
        assert!(requirements[1..].iter().all(|row| row.max_depth == Some(6)));
        requirements = apply_edit(&requirements, Some(0), sword, 3, None, None, &mut next_key);
        assert!(requirements.iter().all(|row| row.max_depth.is_none()));
    }

    #[test]
    fn the_copies_keep_their_floor_when_the_stack_follows_its_chip_into_a_cluster() {
        let mut next_key = 2;
        let might = row(1, |r| {
            r.kind = ItemKind::Ring;
            r.item = Some(ItemId::RingMight);
        });
        let mut requirements = apply_edit(&[], None, might, 2, None, Some(7), &mut next_key);
        // The last key the stack claimed, plus one, for the chip joining it.
        let key = next_key;
        requirements.push(row(key, |r| {
            r.kind = ItemKind::Ring;
            r.item = Some(ItemId::RingHaste);
        }));
        let haste = requirements.len() - 1;
        let joined = join_alternatives(&requirements, 0, haste);
        let copy = joined.iter().find(|row| row.item.is_none()).unwrap();
        assert_eq!(copy.max_depth, Some(7));
        assert!(valid(&joined).is_ok());
    }

    #[test]
    fn a_narrowed_weapon_stack_keeps_its_copies_plain_for_the_engine() {
        // The engine reads a copy that names a weapon class as a *second*
        // constrained member of the stack, so the copy names only the family.
        let mut next_key = 2;
        let thrown = row(1, |r| {
            r.weapon_category = Some(WeaponCategory::Thrown);
            r.upgrade = UpgradeRequirement::AtLeast(1);
        });
        let requirements = apply_edit(&[], None, thrown, 2, None, None, &mut next_key);
        assert_eq!(requirements.len(), 2);
        assert_eq!(requirements[1].weapon_category, None);
        assert!(valid(&requirements).is_ok());
    }
}
