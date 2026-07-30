//! Multi-item query validation and accessibility-aware matching.

use std::collections::BTreeMap;
use std::fmt;

use crate::catalog::{
    ALL_ARMOR_EFFECTS, ALL_WEAPON_EFFECTS, Effect, ItemId, ItemKind, WeaponCategory, item,
};
use crate::challenges::Challenges;
use crate::model::{GeneratedWorld, ItemSource, WorldItem};

/// Highest upgrade any generated item can carry (+4 rings from the Imp).
const MAX_ITEM_UPGRADE: u16 = 4;

/// Upgrade predicate attached to one item requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpgradeRequirement {
    Any,
    Exact(u8),
    AtLeast(u8),
}

/// Optional tier predicate for tiered equipment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TierRequirement {
    Any,
    Exact(u8),
    AtLeast(u8),
    AtMost(u8),
}

impl TierRequirement {
    /// Whether a tiered item satisfies this predicate. Untiered items never do.
    #[must_use]
    pub fn matches(self, tier: Option<u8>) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(wanted) => tier == Some(wanted),
            Self::AtLeast(minimum) => tier.is_some_and(|tier| tier >= minimum),
            Self::AtMost(maximum) => tier.is_some_and(|tier| tier <= maximum),
        }
    }
}

impl UpgradeRequirement {
    const fn matches(self, upgrade: u8) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(wanted) => upgrade == wanted,
            Self::AtLeast(minimum) => upgrade >= minimum,
        }
    }
}

/// Non-empty set of same-family effects, stored as a bitmask over the
/// family's upstream catalog ordering. Only weapons and armor carry effects.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EffectSet {
    family: ItemKind,
    mask: u32,
}

impl EffectSet {
    /// Builds a set from one effect.
    #[must_use]
    pub fn single(effect: Effect) -> Self {
        Self {
            family: effect_family(effect),
            mask: 1 << effect_index(effect),
        }
    }

    /// Builds a set from effects that must all belong to one family.
    /// Returns `None` for an empty iterator or mixed families.
    pub fn from_effects<I: IntoIterator<Item = Effect>>(effects: I) -> Option<Self> {
        let mut combined: Option<Self> = None;
        for effect in effects {
            let single = Self::single(effect);
            combined = Some(match combined {
                None => single,
                Some(existing) if existing.family == single.family => Self {
                    family: existing.family,
                    mask: existing.mask | single.mask,
                },
                Some(_) => return None,
            });
        }
        combined
    }

    /// Every non-curse enchantment or glyph of the family, or `None` for
    /// families that never carry effects.
    #[must_use]
    pub fn enchantments(kind: ItemKind) -> Option<Self> {
        Self::from_effects(family_effects(kind)?.filter(|effect| !effect.is_curse()))
    }

    /// The item family whose effects this set draws from.
    #[must_use]
    pub const fn family(self) -> ItemKind {
        self.family
    }

    #[must_use]
    pub fn contains(self, effect: Effect) -> bool {
        effect_family(effect) == self.family && self.mask & (1 << effect_index(effect)) != 0
    }

    /// Iterates the member effects in upstream catalog order.
    pub fn effects(self) -> impl Iterator<Item = Effect> {
        family_effects(self.family)
            .into_iter()
            .flatten()
            .filter(move |effect| self.contains(*effect))
    }

    /// The subset shared with `other`, or `None` when nothing overlaps.
    #[must_use]
    pub fn intersection(self, other: Self) -> Option<Self> {
        if self.family != other.family {
            return None;
        }
        let mask = self.mask & other.mask;
        (mask != 0).then_some(Self {
            family: self.family,
            mask,
        })
    }

    /// The set without curse-type effects, or `None` when only curses remain.
    #[must_use]
    pub fn without_curses(self) -> Option<Self> {
        Self::from_effects(self.effects().filter(|effect| !effect.is_curse()))
    }

    #[must_use]
    pub fn is_curses_only(self) -> bool {
        self.without_curses().is_none()
    }

    #[must_use]
    pub const fn len(self) -> u32 {
        self.mask.count_ones()
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.mask == 0
    }
}

const fn effect_family(effect: Effect) -> ItemKind {
    match effect {
        Effect::Weapon(_) => ItemKind::Weapon,
        Effect::Armor(_) => ItemKind::Armor,
    }
}

const fn effect_index(effect: Effect) -> u8 {
    match effect {
        Effect::Weapon(effect) => effect as u8,
        Effect::Armor(effect) => effect as u8,
    }
}

fn family_effects(kind: ItemKind) -> Option<Box<dyn Iterator<Item = Effect>>> {
    match kind {
        ItemKind::Weapon => Some(Box::new(
            ALL_WEAPON_EFFECTS.iter().copied().map(Effect::Weapon),
        )),
        ItemKind::Armor => Some(Box::new(
            ALL_ARMOR_EFFECTS.iter().copied().map(Effect::Armor),
        )),
        ItemKind::Wand | ItemKind::Ring => None,
    }
}

/// Effect predicate attached to one item requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectRequirement {
    /// Wildcard: any effect, or none at all.
    Any,
    /// The item must carry one of these effects. "Any enchantment" is the
    /// full non-curse set from [`EffectSet::enchantments`].
    OneOf(EffectSet),
}

impl EffectRequirement {
    #[must_use]
    pub fn matches(self, effect: Option<Effect>) -> bool {
        match self {
            Self::Any => true,
            Self::OneOf(set) => effect.is_some_and(|effect| set.contains(effect)),
        }
    }
}

/// Minimum combined upgrade level shared by every requirement in one group.
///
/// All members of a group must be satisfied by distinct items whose upgrade
/// levels sum to at least `minimum_total`, on top of each member's own
/// upgrade predicate. Combine with [`Requirement::identity_group`] to demand
/// copies of the same item, such as two rings of one type totalling +2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpgradeSum {
    /// Non-zero group label shared by the participating requirements.
    pub group: u8,
    /// Inclusive lower bound on the members' combined upgrade levels.
    pub minimum_total: u8,
}

/// One required item. `None` fields are wildcards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Requirement {
    pub kind: ItemKind,
    /// Optional melee/thrown narrowing; only meaningful for weapon
    /// requirements. `None` matches both, preserving the pre-existing
    /// "any weapon" semantics.
    pub weapon_category: Option<WeaponCategory>,
    pub item: Option<ItemId>,
    pub tier: TierRequirement,
    pub upgrade: UpgradeRequirement,
    pub effect: EffectRequirement,
    /// Whether cursed candidate items are ineligible for this requirement.
    pub require_uncursed: bool,
    pub source: Option<ItemSource>,
    /// Requirements in the same non-zero group must resolve to the same item ID.
    pub identity_group: Option<u8>,
    /// Optional inclusive floor limit for this item, independent of the query's
    /// overall generation limit.
    pub max_depth: Option<u8>,
    /// Requirements in the same non-zero group are alternatives: one matching
    /// item satisfies the whole group.
    pub alternative_group: Option<u8>,
    /// Optional combined-upgrade constraint shared with other requirements.
    pub upgrade_sum: Option<UpgradeSum>,
}

impl Requirement {
    #[must_use]
    pub fn matches(self, candidate: &WorldItem) -> bool {
        self.matching_identity(candidate).is_some()
    }

    fn matching_identity(self, candidate: &WorldItem) -> Option<ItemId> {
        let identity = match self.item {
            None => candidate.item,
            Some(wanted) if wanted == candidate.item => candidate.item,
            Some(_) => return None,
        };
        let definition = item(identity);
        (definition.kind == self.kind
            && self
                .weapon_category
                .is_none_or(|wanted| definition.weapon_category() == Some(wanted))
            && self.tier.matches(definition.tier)
            && self.upgrade.matches(candidate.upgrade)
            && self.effect.matches(candidate.effect)
            && (!self.require_uncursed || !candidate.cursed)
            && self.source.is_none_or(|wanted| wanted == candidate.source))
        .then_some(identity)
    }

    /// The highest upgrade level an item satisfying this requirement can
    /// carry, used to bound combined-upgrade groups.
    fn maximum_upgrade(self) -> u8 {
        match self.upgrade {
            UpgradeRequirement::Exact(wanted) => wanted,
            UpgradeRequirement::Any | UpgradeRequirement::AtLeast(_) => {
                self.kind.maximum_search_upgrade()
            }
        }
    }

    /// Checks that an item/effect/upgrade combination is meaningful.
    ///
    /// # Errors
    ///
    /// Returns a validation error for a category mismatch, an effect intended
    /// for another family, an upgrade outside the UI's family-specific range,
    /// or an inconsistent group label.
    pub fn validate(self) -> Result<(), QueryError> {
        if self
            .item
            .is_some_and(|item_id| item(item_id).kind != self.kind)
        {
            return Err(QueryError::ItemKindMismatch);
        }
        if let Some(category) = self.weapon_category {
            if self.kind != ItemKind::Weapon
                || self
                    .item
                    .is_some_and(|item_id| item_id.weapon_category() != Some(category))
            {
                return Err(QueryError::InvalidWeaponCategory);
            }
        }
        let tierable =
            self.item.is_none() && matches!(self.kind, ItemKind::Weapon | ItemKind::Armor);
        let valid_tier = match self.tier {
            TierRequirement::Any => true,
            TierRequirement::Exact(tier) => tierable && (2..=5).contains(&tier),
            TierRequirement::AtLeast(tier) | TierRequirement::AtMost(tier) => {
                tierable && (3..=4).contains(&tier)
            }
        };
        if !valid_tier {
            return Err(QueryError::InvalidTier);
        }
        let maximum = self.kind.maximum_search_upgrade();
        let valid_upgrade = match self.upgrade {
            UpgradeRequirement::Any => true,
            UpgradeRequirement::Exact(upgrade) => (1..=maximum).contains(&upgrade),
            UpgradeRequirement::AtLeast(upgrade) => upgrade <= maximum,
        };
        if !valid_upgrade {
            return Err(QueryError::InvalidUpgrade);
        }
        if self.identity_group == Some(0) {
            return Err(QueryError::InvalidIdentityGroup);
        }
        if self.alternative_group == Some(0) {
            return Err(QueryError::InvalidAlternativeGroup);
        }
        if self
            .upgrade_sum
            .is_some_and(|sum| sum.group == 0 || sum.minimum_total == 0)
        {
            return Err(QueryError::InvalidUpgradeSum);
        }
        if self.alternative_group.is_some() && self.upgrade_sum.is_some() {
            return Err(QueryError::UpgradeSumInsideAlternative);
        }
        if self
            .max_depth
            .is_some_and(|depth| !(1..=24).contains(&depth))
        {
            return Err(QueryError::InvalidDepth);
        }
        match (self.kind, self.effect) {
            (_, EffectRequirement::Any) => {}
            (kind, EffectRequirement::OneOf(set)) if set.family() == kind => {}
            _ => return Err(QueryError::EffectKindMismatch),
        }
        if self.require_uncursed {
            if let EffectRequirement::OneOf(set) = self.effect {
                if set.is_curses_only() {
                    return Err(QueryError::UncursedWithCurse);
                }
            }
        }
        Ok(())
    }
}

/// All requirements must be obtainable together in the same generated world.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchQuery {
    pub requirements: Vec<Requirement>,
    pub max_depth: u8,
    /// Upstream v3.3.8 challenge mask used while generating candidate worlds.
    pub challenges: Challenges,
    /// Whether an accessible blacksmith room must exist within `max_depth`.
    pub require_blacksmith: bool,
    /// Whether Blacksmith "Smith" rewards are ineligible to satisfy item
    /// requirements. The room may still be required separately for reforging.
    pub exclude_blacksmith_rewards: bool,
    /// Trades exhaustiveness for speed: +3 weapon/armor requirements are
    /// assumed to come from quest rewards, ignoring the far rarer Crypt and
    /// Sacrificial-fire prizes. Matches are still always genuine, but seeds
    /// whose only qualifying item comes from those rooms are skipped. See
    /// [`crate::feasibility`].
    pub fast_mode: bool,
}

/// One identity-group member seen during validation: its alternative group,
/// category, and concrete item (when named).
type IdentityMember = (Option<u8>, ItemKind, Option<ItemId>);

impl SearchQuery {
    /// Validates bounds and every requirement.
    ///
    /// # Errors
    ///
    /// Returns a [`QueryError`] when no requirements are present, the selected
    /// depth is outside the main dungeon, or a requirement is inconsistent.
    pub fn validate(&self) -> Result<(), QueryError> {
        if self.requirements.is_empty() {
            return Err(QueryError::Empty);
        }
        if !(1..=24).contains(&self.max_depth) {
            return Err(QueryError::InvalidDepth);
        }
        let mut identity_groups: BTreeMap<u8, Vec<IdentityMember>> = BTreeMap::new();
        let mut upgrade_sums: BTreeMap<u8, (u8, u16)> = BTreeMap::new();
        for requirement in &self.requirements {
            requirement.validate()?;
            if let Some(group) = requirement.identity_group {
                let members = identity_groups.entry(group).or_default();
                for &(alternative, kind, item) in members.iter() {
                    // Alternatives of one slot are never assigned together,
                    // so they may disagree; every other pair must agree.
                    if alternative.is_some() && alternative == requirement.alternative_group {
                        continue;
                    }
                    if kind != requirement.kind
                        || item
                            .zip(requirement.item)
                            .is_some_and(|(left, right)| left != right)
                    {
                        return Err(QueryError::InconsistentIdentityGroup);
                    }
                }
                members.push((
                    requirement.alternative_group,
                    requirement.kind,
                    requirement.item,
                ));
            }
            if let Some(sum) = requirement.upgrade_sum {
                let (minimum_total, reachable) = upgrade_sums
                    .entry(sum.group)
                    .or_insert((sum.minimum_total, 0));
                if *minimum_total != sum.minimum_total {
                    return Err(QueryError::InconsistentUpgradeSum);
                }
                *reachable += u16::from(requirement.maximum_upgrade());
            }
        }
        for (minimum_total, reachable) in upgrade_sums.values() {
            if u16::from(*minimum_total) > *reachable {
                return Err(QueryError::UnattainableUpgradeSum);
            }
        }
        Ok(())
    }

    /// Matches requirements as an AND query while respecting distinct item
    /// instances, alternative groups, combined-upgrade totals, and mutually
    /// exclusive quest/chest reward branches.
    #[must_use]
    pub fn matches(&self, world: &GeneratedWorld) -> bool {
        if self.require_blacksmith
            && !world.items.iter().any(|candidate| {
                candidate.depth <= self.max_depth
                    && candidate.source == ItemSource::BlacksmithReward
            })
        {
            return false;
        }

        let mut assignment = Assignment::prepare(self, world);
        if assignment.slots.len() > world.items.len()
            || assignment.slots.first().is_some_and(Vec::is_empty)
        {
            return false;
        }
        assignment.satisfies_every_slot(0)
    }
}

/// Query slots resolved against one world's items: alternatives collapse to
/// one slot, and every slot must be served by a distinct item.
struct Assignment<'query> {
    items: &'query [WorldItem],
    /// Candidate matches per slot: `(item index, matched identity, member)`.
    slots: Vec<Vec<(usize, ItemId, &'query Requirement)>>,
    /// Combined-upgrade group sizes and totals, keyed by group label.
    sum_groups: BTreeMap<u8, SumGroup>,
    used: Vec<bool>,
    scenarios: BTreeMap<u16, u64>,
    identities: BTreeMap<u8, ItemId>,
    /// Assigned member count and accumulated upgrade total per sum group.
    sums: BTreeMap<u8, (u16, u16)>,
}

/// Size and required total of one combined-upgrade group.
#[derive(Clone, Copy, Debug)]
struct SumGroup {
    members: u16,
    minimum_total: u16,
}

impl<'query> Assignment<'query> {
    /// Builds per-slot candidate lists, most constrained slot first.
    fn prepare(query: &'query SearchQuery, world: &'query GeneratedWorld) -> Self {
        let mut slot_of_group: BTreeMap<u8, usize> = BTreeMap::new();
        let mut slots: Vec<Vec<(usize, ItemId, &'query Requirement)>> = Vec::new();
        let mut sum_groups: BTreeMap<u8, SumGroup> = BTreeMap::new();
        for requirement in &query.requirements {
            let slot = if let Some(group) = requirement.alternative_group {
                *slot_of_group.entry(group).or_insert_with(|| {
                    slots.push(Vec::new());
                    slots.len() - 1
                })
            } else {
                slots.push(Vec::new());
                slots.len() - 1
            };
            if let Some(sum) = requirement.upgrade_sum {
                let group = sum_groups.entry(sum.group).or_insert(SumGroup {
                    members: 0,
                    minimum_total: u16::from(sum.minimum_total),
                });
                group.members += 1;
            }
            for (index, candidate) in world.items.iter().enumerate() {
                if candidate.depth <= query.max_depth
                    && candidate.depth <= requirement.max_depth.unwrap_or(query.max_depth)
                    && (!query.exclude_blacksmith_rewards
                        || candidate.source != ItemSource::BlacksmithReward)
                {
                    if let Some(identity) = requirement.matching_identity(candidate) {
                        slots[slot].push((index, identity, requirement));
                    }
                }
            }
        }
        // Fail early by assigning the most constrained slot first.
        slots.sort_by_key(Vec::len);
        Self {
            items: &world.items,
            slots,
            sum_groups,
            used: vec![false; world.items.len()],
            scenarios: BTreeMap::new(),
            identities: BTreeMap::new(),
            sums: BTreeMap::new(),
        }
    }

    /// Depth-first assignment requiring every slot to hold a distinct item.
    fn satisfies_every_slot(&mut self, slot: usize) -> bool {
        if slot == self.slots.len() {
            return true;
        }
        for candidate in 0..self.slots[slot].len() {
            let (item_index, identity, requirement) = self.slots[slot][candidate];
            let Some(undo) = self.assign(item_index, identity, requirement) else {
                continue;
            };
            if self.satisfies_every_slot(slot + 1) {
                return true;
            }
            self.unassign(undo);
        }
        false
    }

    /// Places one item into one slot when every cross-item constraint still
    /// holds, returning the state needed to undo the placement.
    fn assign(
        &mut self,
        item_index: usize,
        identity: ItemId,
        requirement: &Requirement,
    ) -> Option<Undo> {
        if self.used[item_index] {
            return None;
        }
        let mut undo = Undo {
            item_index,
            identity: None,
            scenario: None,
            sum: None,
        };
        if let Some(group) = requirement.identity_group {
            if self
                .identities
                .get(&group)
                .is_some_and(|wanted| *wanted != identity)
            {
                return None;
            }
            undo.identity = Some((group, self.identities.insert(group, identity)));
        }
        if let Some((group, item_scenarios)) =
            self.items[item_index].accessibility.scenario_constraint()
        {
            let compatible =
                self.scenarios.get(&group).copied().unwrap_or(u64::MAX) & item_scenarios;
            if compatible == 0 {
                self.unassign(undo);
                return None;
            }
            undo.scenario = Some((group, self.scenarios.insert(group, compatible)));
        }
        if let Some(sum) = requirement.upgrade_sum {
            let group = self
                .sum_groups
                .get(&sum.group)
                .copied()
                .unwrap_or(SumGroup {
                    members: 0,
                    minimum_total: 0,
                });
            let (assigned, total) = self.sums.get(&sum.group).copied().unwrap_or((0, 0));
            let assigned = assigned + 1;
            let total = total + u16::from(self.items[item_index].upgrade);
            let remaining = group.members.saturating_sub(assigned);
            let unreachable = total + remaining * MAX_ITEM_UPGRADE < group.minimum_total;
            if unreachable {
                self.unassign(undo);
                return None;
            }
            undo.sum = Some((sum.group, self.sums.insert(sum.group, (assigned, total))));
        }
        self.used[item_index] = true;
        Some(undo)
    }

    fn unassign(&mut self, undo: Undo) {
        self.used[undo.item_index] = false;
        rewind(&mut self.sums, undo.sum);
        rewind(&mut self.scenarios, undo.scenario);
        rewind(&mut self.identities, undo.identity);
    }

    /// Combined-upgrade groups that are not fully assigned or fall short of
    /// their total: their assigned members do not count as satisfied.
    fn failed_sum_groups(&self) -> Vec<u8> {
        self.sums
            .iter()
            .filter(|(label, (count, total))| {
                let group = self.sum_groups.get(label).copied().unwrap_or(SumGroup {
                    members: 0,
                    minimum_total: 0,
                });
                *count < group.members || *total < group.minimum_total
            })
            .map(|(label, _)| *label)
            .collect()
    }
}

#[derive(Clone, Copy)]
struct Undo {
    item_index: usize,
    identity: Option<(u8, Option<ItemId>)>,
    scenario: Option<(u16, Option<u64>)>,
    sum: Option<(u8, Option<(u16, u16)>)>,
}

fn rewind<K: Ord, V>(map: &mut BTreeMap<K, V>, previous: Option<(K, Option<V>)>) {
    if let Some((key, previous)) = previous {
        if let Some(previous) = previous {
            map.insert(key, previous);
        } else {
            map.remove(&key);
        }
    }
}

/// Indices of the world items used by an assignment that satisfies as many
/// query slots as possible, for scout-style match highlighting.
///
/// Alternatives count once per group, and combined-upgrade groups count only
/// when every member is assigned and the total is met — a lone ring of a
/// wanted pair is not highlighted. Ties keep the first maximal assignment
/// found. The query does not have to be satisfiable in full.
#[must_use]
pub fn best_match_indices(query: &SearchQuery, world: &GeneratedWorld) -> Vec<usize> {
    if query.requirements.is_empty() {
        return Vec::new();
    }
    let mut search = BestSubset {
        assignment: Assignment::prepare(query, world),
        selected: Vec::new(),
        best: Vec::new(),
        best_score: 0,
    };
    search.visit(0);
    search.best
}

struct BestSubset<'query> {
    assignment: Assignment<'query>,
    /// Assigned items with the combined-upgrade group they serve, if any.
    selected: Vec<(usize, Option<u8>)>,
    best: Vec<usize>,
    best_score: usize,
}

impl BestSubset<'_> {
    fn visit(&mut self, slot: usize) {
        if slot == self.assignment.slots.len() {
            // Items serving an incomplete or short combined-upgrade group do
            // not count and are not highlighted.
            let failed = self.assignment.failed_sum_groups();
            let counted: Vec<usize> = self
                .selected
                .iter()
                .filter(|(_, sum_group)| sum_group.is_none_or(|group| !failed.contains(&group)))
                .map(|(item_index, _)| *item_index)
                .collect();
            if counted.len() > self.best_score {
                self.best_score = counted.len();
                self.best = counted;
            }
            return;
        }
        // The remaining slots bound what this branch can still add.
        if self.selected.len() + (self.assignment.slots.len() - slot) <= self.best_score {
            return;
        }
        for candidate in 0..self.assignment.slots[slot].len() {
            let (item_index, identity, requirement) = self.assignment.slots[slot][candidate];
            let Some(undo) = self.assignment.assign(item_index, identity, requirement) else {
                continue;
            };
            self.selected
                .push((item_index, requirement.upgrade_sum.map(|sum| sum.group)));
            self.visit(slot + 1);
            self.selected.pop();
            self.assignment.unassign(undo);
        }
        self.visit(slot + 1);
    }
}

/// Invalid user query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryError {
    Empty,
    InvalidDepth,
    InvalidUpgrade,
    InvalidTier,
    ItemKindMismatch,
    InvalidWeaponCategory,
    EffectKindMismatch,
    UncursedWithCurse,
    InvalidIdentityGroup,
    InconsistentIdentityGroup,
    InvalidAlternativeGroup,
    InvalidUpgradeSum,
    InconsistentUpgradeSum,
    UnattainableUpgradeSum,
    UpgradeSumInsideAlternative,
}

impl fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "at least one item requirement is needed",
            Self::InvalidDepth => "maximum depth must be between 1 and 24",
            Self::InvalidUpgrade => "upgrade must be +1, +2, or +3 (+4 for rings)",
            Self::InvalidTier => {
                "tier filters require a wildcard weapon or armor and a non-redundant tier"
            }
            Self::ItemKindMismatch => "selected item is in a different category",
            Self::InvalidWeaponCategory => {
                "melee/thrown filters require a weapon requirement and a matching item"
            }
            Self::EffectKindMismatch => "selected enchantment or glyph is inapplicable",
            Self::UncursedWithCurse => "an uncursed item cannot be limited to curses",
            Self::InvalidIdentityGroup => "identity group zero is reserved for no group",
            Self::InconsistentIdentityGroup => {
                "linked item requirements must use the same category and item"
            }
            Self::InvalidAlternativeGroup => "alternative group zero is reserved for no group",
            Self::InvalidUpgradeSum => "combined upgrade groups need a non-zero group and total",
            Self::InconsistentUpgradeSum => {
                "requirements in a combined upgrade group must agree on the total"
            }
            Self::UnattainableUpgradeSum => {
                "the combined upgrade total exceeds what the group's items can carry"
            }
            Self::UpgradeSumInsideAlternative => {
                "a combined upgrade group cannot include alternative requirements"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for QueryError {}

#[cfg(test)]
mod tests {
    use crate::catalog::{ArmorEffect, Effect, ItemId, ItemKind, WeaponCategory, WeaponEffect};
    use crate::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
    use crate::seed::DungeonSeed;

    use super::{
        EffectRequirement, EffectSet, QueryError, Requirement, SearchQuery, TierRequirement,
        UpgradeRequirement, UpgradeSum, best_match_indices,
    };

    fn world_item(item: ItemId, accessibility: Accessibility) -> WorldItem {
        WorldItem {
            item,
            upgrade: 2,
            effect: None,
            cursed: false,
            depth: 3,
            source: ItemSource::GhostReward,
            accessibility,
        }
    }

    fn requirement(item: ItemId) -> Requirement {
        Requirement {
            kind: crate::catalog::item(item).kind,
            weapon_category: None,
            item: Some(item),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(2),
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            upgrade_sum: None,
        }
    }

    fn query(requirements: Vec<Requirement>, max_depth: u8) -> SearchQuery {
        SearchQuery {
            requirements,
            max_depth,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        }
    }

    fn world(items: Vec<WorldItem>) -> GeneratedWorld {
        GeneratedWorld {
            seed: DungeonSeed::MIN,
            items,
        }
    }

    #[test]
    fn and_query_requires_distinct_item_occurrences() {
        let two_swords = query(
            vec![requirement(ItemId::Sword), requirement(ItemId::Sword)],
            4,
        );
        let one = world(vec![world_item(ItemId::Sword, Accessibility::Independent)]);
        assert!(!two_swords.matches(&one));
        let two = world(vec![
            world_item(ItemId::Sword, Accessibility::Independent),
            world_item(ItemId::Sword, Accessibility::Independent),
        ]);
        assert!(two_swords.matches(&two));
    }

    #[test]
    fn uncursed_requirement_rejects_cursed_copies() {
        let mut candidate = world_item(ItemId::Sword, Accessibility::Independent);
        let mut wanted = requirement(ItemId::Sword);
        wanted.require_uncursed = true;

        assert!(wanted.matches(&candidate));
        candidate.cursed = true;
        assert!(!wanted.matches(&candidate));
        wanted.require_uncursed = false;
        assert!(wanted.matches(&candidate));
    }

    #[test]
    fn requirement_floor_limit_is_inclusive() {
        let sample = world(vec![world_item(ItemId::Sword, Accessibility::Independent)]);
        let mut limited = requirement(ItemId::Sword);
        limited.max_depth = Some(2);
        let mut floor_limited = query(vec![limited], 24);
        assert!(!floor_limited.matches(&sample));
        floor_limited.requirements[0].max_depth = Some(3);
        assert!(floor_limited.matches(&sample));
    }

    #[test]
    fn mutually_exclusive_rewards_cannot_satisfy_and_query() {
        let both = query(
            vec![requirement(ItemId::Sword), requirement(ItemId::MailArmor)],
            4,
        );
        let exclusive = world(vec![
            world_item(
                ItemId::Sword,
                Accessibility::Choice {
                    group: 1,
                    option: 0,
                },
            ),
            world_item(
                ItemId::MailArmor,
                Accessibility::Choice {
                    group: 1,
                    option: 1,
                },
            ),
        ]);
        assert!(!both.matches(&exclusive));
    }

    #[test]
    fn same_choice_option_and_independent_rewards_can_match() {
        let both = query(
            vec![requirement(ItemId::Sword), requirement(ItemId::MailArmor)],
            4,
        );
        let compatible = world(vec![
            world_item(
                ItemId::Sword,
                Accessibility::Choice {
                    group: 2,
                    option: 0,
                },
            ),
            world_item(
                ItemId::MailArmor,
                Accessibility::Choice {
                    group: 2,
                    option: 0,
                },
            ),
        ]);
        assert!(both.matches(&compatible));
    }

    #[test]
    fn scenario_masks_model_prerequisite_paths_without_false_choices() {
        let sword = world_item(
            ItemId::Sword,
            Accessibility::Scenarios {
                group: 7,
                mask: 0b0011,
            },
        );
        let armor = world_item(
            ItemId::MailArmor,
            Accessibility::Scenarios {
                group: 7,
                mask: 0b0110,
            },
        );
        let wand = world_item(
            ItemId::WandFrost,
            Accessibility::Scenarios {
                group: 7,
                mask: 0b1100,
            },
        );
        let sample = world(vec![sword, armor, wand]);

        let compatible = query(
            vec![requirement(ItemId::Sword), requirement(ItemId::MailArmor)],
            4,
        );
        assert!(compatible.matches(&sample));

        let incompatible = query(
            vec![requirement(ItemId::Sword), requirement(ItemId::WandFrost)],
            4,
        );
        assert!(!incompatible.matches(&sample));
    }

    #[test]
    fn validation_rejects_wrong_category() {
        let invalid = Requirement {
            kind: ItemKind::Wand,
            ..requirement(ItemId::Sword)
        };
        assert_eq!(invalid.validate(), Err(QueryError::ItemKindMismatch));
    }

    #[test]
    fn weapon_category_narrows_wildcard_weapon_requirements() {
        let any_weapon = Requirement {
            item: None,
            upgrade: UpgradeRequirement::Any,
            ..requirement(ItemId::Sword)
        };
        let melee = Requirement {
            weapon_category: Some(WeaponCategory::Melee),
            ..any_weapon
        };
        let thrown = Requirement {
            weapon_category: Some(WeaponCategory::Thrown),
            ..any_weapon
        };
        let sword = world_item(ItemId::Sword, Accessibility::Independent);
        let shuriken = world_item(ItemId::Shuriken, Accessibility::Independent);
        let dart = world_item(ItemId::PoisonDart, Accessibility::Independent);

        assert!(any_weapon.matches(&sword));
        assert!(any_weapon.matches(&shuriken));
        assert!(melee.matches(&sword));
        assert!(!melee.matches(&shuriken));
        assert!(!melee.matches(&dart));
        assert!(!thrown.matches(&sword));
        assert!(thrown.matches(&shuriken));
        assert!(thrown.matches(&dart));

        // Tier filters compose with the category filter.
        let tier_five_thrown = Requirement {
            tier: TierRequirement::Exact(5),
            ..thrown
        };
        assert_eq!(tier_five_thrown.validate(), Ok(()));
        assert!(tier_five_thrown.matches(&world_item(
            ItemId::ThrowingHammer,
            Accessibility::Independent
        )));
        assert!(
            !tier_five_thrown.matches(&world_item(ItemId::Greatsword, Accessibility::Independent))
        );
        assert!(!tier_five_thrown.matches(&shuriken));
    }

    #[test]
    fn weapon_category_validation_requires_a_consistent_weapon() {
        use crate::catalog::WeaponCategory;

        let melee_wand = Requirement {
            weapon_category: Some(WeaponCategory::Melee),
            ..requirement(ItemId::WandFrost)
        };
        assert_eq!(
            melee_wand.validate(),
            Err(QueryError::InvalidWeaponCategory)
        );

        let melee_shuriken = Requirement {
            weapon_category: Some(WeaponCategory::Melee),
            ..requirement(ItemId::Shuriken)
        };
        assert_eq!(
            melee_shuriken.validate(),
            Err(QueryError::InvalidWeaponCategory)
        );

        let thrown_shuriken = Requirement {
            weapon_category: Some(WeaponCategory::Thrown),
            ..requirement(ItemId::Shuriken)
        };
        assert_eq!(thrown_shuriken.validate(), Ok(()));
    }

    #[test]
    fn validation_rejects_uncursed_items_limited_to_curses() {
        let invalid = Requirement {
            effect: EffectRequirement::OneOf(EffectSet::single(Effect::Weapon(
                WeaponEffect::Displacing,
            ))),
            require_uncursed: true,
            ..requirement(ItemId::Sword)
        };
        assert_eq!(invalid.validate(), Err(QueryError::UncursedWithCurse));

        let mixed = Requirement {
            effect: EffectRequirement::OneOf(
                EffectSet::from_effects([
                    Effect::Weapon(WeaponEffect::Displacing),
                    Effect::Weapon(WeaponEffect::Blazing),
                ])
                .unwrap(),
            ),
            require_uncursed: true,
            ..requirement(ItemId::Sword)
        };
        assert_eq!(mixed.validate(), Ok(()));
    }

    #[test]
    fn plus_four_is_valid_only_for_rings() {
        let ring = Requirement {
            upgrade: UpgradeRequirement::Exact(4),
            ..requirement(ItemId::RingSharpshooting)
        };
        assert_eq!(ring.validate(), Ok(()));

        let wand = Requirement {
            upgrade: UpgradeRequirement::Exact(4),
            ..requirement(ItemId::WandFrost)
        };
        assert_eq!(wand.validate(), Err(QueryError::InvalidUpgrade));
    }

    #[test]
    fn tier_predicates_match_exact_minimum_and_maximum_tiers() {
        let tier_five = Requirement {
            kind: ItemKind::Weapon,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Exact(5),
            ..requirement(ItemId::Sword)
        };
        assert!(tier_five.matches(&world_item(ItemId::Greatsword, Accessibility::Independent)));
        assert!(!tier_five.matches(&world_item(ItemId::Longsword, Accessibility::Independent)));

        let tier_four_plus = Requirement {
            tier: TierRequirement::AtLeast(4),
            ..tier_five
        };
        assert!(tier_four_plus.matches(&world_item(ItemId::Longsword, Accessibility::Independent)));
        assert!(
            tier_four_plus.matches(&world_item(ItemId::Greatsword, Accessibility::Independent))
        );
        assert!(!tier_four_plus.matches(&world_item(ItemId::Sword, Accessibility::Independent)));

        let tier_four_or_lower = Requirement {
            tier: TierRequirement::AtMost(4),
            ..tier_five
        };
        assert!(
            tier_four_or_lower.matches(&world_item(ItemId::Longsword, Accessibility::Independent))
        );
        assert!(tier_four_or_lower.matches(&world_item(ItemId::Sword, Accessibility::Independent)));
        assert!(
            !tier_four_or_lower
                .matches(&world_item(ItemId::Greatsword, Accessibility::Independent))
        );

        let invalid = Requirement {
            kind: ItemKind::Wand,
            item: None,
            ..tier_five
        };
        assert_eq!(invalid.validate(), Err(QueryError::InvalidTier));

        let tier_one = Requirement {
            tier: TierRequirement::Exact(1),
            ..tier_five
        };
        assert_eq!(tier_one.validate(), Err(QueryError::InvalidTier));

        for redundant in [
            TierRequirement::AtLeast(2),
            TierRequirement::AtLeast(5),
            TierRequirement::AtMost(2),
            TierRequirement::AtMost(5),
        ] {
            assert_eq!(
                Requirement {
                    tier: redundant,
                    ..tier_five
                }
                .validate(),
                Err(QueryError::InvalidTier)
            );
        }
    }

    #[test]
    fn linked_wands_require_distinct_copies_and_a_blacksmith_in_range() {
        let linked = |upgrade, source| Requirement {
            kind: ItemKind::Wand,
            weapon_category: None,
            item: None,
            upgrade,
            source,
            identity_group: Some(1),
            ..requirement(ItemId::WandFrost)
        };
        let mut staff = query(
            vec![
                linked(
                    UpgradeRequirement::Exact(3),
                    Some(ItemSource::WandmakerReward),
                ),
                linked(UpgradeRequirement::AtLeast(0), None),
                linked(UpgradeRequirement::AtLeast(0), None),
                Requirement {
                    kind: ItemKind::Wand,
                    weapon_category: None,
                    item: None,
                    upgrade: UpgradeRequirement::Exact(1),
                    ..requirement(ItemId::WandFrost)
                },
            ],
            14,
        );
        staff.require_blacksmith = true;
        let make = |item, upgrade, depth, source| WorldItem {
            item,
            upgrade,
            effect: None,
            cursed: false,
            depth,
            source,
            accessibility: Accessibility::Independent,
        };
        let sample = world(vec![
            make(ItemId::WandFrost, 3, 7, ItemSource::WandmakerReward),
            make(ItemId::WandFrost, 0, 2, ItemSource::Heap),
            make(ItemId::WandFrost, 1, 4, ItemSource::Chest),
            make(ItemId::WandLightning, 1, 5, ItemSource::Heap),
            make(ItemId::Sword, 2, 13, ItemSource::BlacksmithReward),
        ]);

        assert_eq!(staff.validate(), Ok(()));
        assert!(staff.matches(&sample));

        let mut wrong_type = sample.clone();
        wrong_type.items[2].item = ItemId::WandLightning;
        assert!(!staff.matches(&wrong_type));

        staff.max_depth = 12;
        assert!(!staff.matches(&sample));
    }

    #[test]
    fn smith_rewards_can_be_excluded_without_hiding_the_blacksmith() {
        let mut reforging = query(vec![requirement(ItemId::Sword)], 14);
        reforging.require_blacksmith = true;
        reforging.exclude_blacksmith_rewards = true;
        let make = |source| WorldItem {
            item: ItemId::Sword,
            upgrade: 2,
            effect: None,
            cursed: false,
            depth: 13,
            source,
            accessibility: Accessibility::Independent,
        };
        let smith_only = world(vec![make(ItemSource::BlacksmithReward)]);

        assert!(!reforging.matches(&smith_only));

        let mut reforging_setup = smith_only.clone();
        reforging_setup.items.push(make(ItemSource::Heap));
        assert!(reforging.matches(&reforging_setup));

        reforging.require_blacksmith = false;
        let no_blacksmith = world(vec![make(ItemSource::Heap)]);
        assert!(reforging.matches(&no_blacksmith));
    }

    #[test]
    fn wildcard_does_not_hide_conflicting_concrete_identity_group_members() {
        let linked = |item| Requirement {
            kind: ItemKind::Wand,
            weapon_category: None,
            item,
            upgrade: UpgradeRequirement::Any,
            identity_group: Some(1),
            ..requirement(ItemId::WandFrost)
        };
        let inconsistent = query(
            vec![
                linked(Some(ItemId::WandFrost)),
                linked(None),
                linked(Some(ItemId::WandLightning)),
            ],
            24,
        );
        assert_eq!(
            inconsistent.validate(),
            Err(QueryError::InconsistentIdentityGroup)
        );
    }

    #[test]
    fn identity_groups_may_disagree_across_alternatives_of_one_slot() {
        let linked = |item, alternative_group| Requirement {
            kind: ItemKind::Wand,
            item,
            upgrade: UpgradeRequirement::Any,
            identity_group: Some(1),
            alternative_group,
            ..requirement(ItemId::WandFrost)
        };
        // Alternatives are never assigned together, so they may name
        // different items while sharing an identity group.
        let alternatives = query(
            vec![
                linked(Some(ItemId::WandFrost), Some(1)),
                linked(Some(ItemId::WandLightning), Some(1)),
                linked(None, None),
            ],
            24,
        );
        assert_eq!(alternatives.validate(), Ok(()));
        // A concrete requirement outside the slot still has to agree with
        // every member it could be assigned alongside.
        let conflicting = query(
            vec![
                linked(Some(ItemId::WandFrost), Some(1)),
                linked(Some(ItemId::WandLightning), Some(1)),
                linked(Some(ItemId::WandCorruption), None),
            ],
            24,
        );
        assert_eq!(
            conflicting.validate(),
            Err(QueryError::InconsistentIdentityGroup)
        );
    }

    #[test]
    fn one_of_effect_sets_match_any_member_and_nothing_else() {
        let wanted = Requirement {
            item: Some(ItemId::Greatshield),
            upgrade: UpgradeRequirement::Exact(2),
            effect: EffectRequirement::OneOf(
                EffectSet::from_effects([
                    Effect::Weapon(WeaponEffect::Blocking),
                    Effect::Weapon(WeaponEffect::Projecting),
                    Effect::Weapon(WeaponEffect::Vampiric),
                ])
                .unwrap(),
            ),
            ..requirement(ItemId::Greatshield)
        };
        assert_eq!(wanted.validate(), Ok(()));

        let mut candidate = world_item(ItemId::Greatshield, Accessibility::Independent);
        assert!(!wanted.matches(&candidate));
        candidate.effect = Some(Effect::Weapon(WeaponEffect::Projecting));
        assert!(wanted.matches(&candidate));
        candidate.effect = Some(Effect::Weapon(WeaponEffect::Blazing));
        assert!(!wanted.matches(&candidate));
    }

    #[test]
    fn any_enchantment_set_requires_a_non_curse_effect() {
        let enchanted = Requirement {
            item: None,
            kind: ItemKind::Weapon,
            upgrade: UpgradeRequirement::Any,
            effect: EffectRequirement::OneOf(EffectSet::enchantments(ItemKind::Weapon).unwrap()),
            ..requirement(ItemId::Sword)
        };
        assert_eq!(enchanted.validate(), Ok(()));

        let mut candidate = world_item(ItemId::Sword, Accessibility::Independent);
        assert!(!enchanted.matches(&candidate));
        candidate.effect = Some(Effect::Weapon(WeaponEffect::Lucky));
        assert!(enchanted.matches(&candidate));
        candidate.effect = Some(Effect::Weapon(WeaponEffect::Annoying));
        assert!(!enchanted.matches(&candidate));

        let glyphed = EffectSet::enchantments(ItemKind::Armor).unwrap();
        assert!(glyphed.contains(Effect::Armor(ArmorEffect::Thorns)));
        assert!(!glyphed.contains(Effect::Armor(ArmorEffect::Stench)));
        assert!(EffectSet::enchantments(ItemKind::Ring).is_none());
    }

    #[test]
    fn effect_sets_reject_foreign_families() {
        assert!(
            EffectSet::from_effects([
                Effect::Weapon(WeaponEffect::Blazing),
                Effect::Armor(ArmorEffect::Thorns),
            ])
            .is_none()
        );

        let mismatched = Requirement {
            effect: EffectRequirement::OneOf(EffectSet::single(Effect::Armor(ArmorEffect::Thorns))),
            ..requirement(ItemId::Sword)
        };
        assert_eq!(mismatched.validate(), Err(QueryError::EffectKindMismatch));

        let on_ring = Requirement {
            effect: EffectRequirement::OneOf(EffectSet::single(Effect::Weapon(
                WeaponEffect::Blazing,
            ))),
            upgrade: UpgradeRequirement::Any,
            ..requirement(ItemId::RingMight)
        };
        assert_eq!(on_ring.validate(), Err(QueryError::EffectKindMismatch));
    }

    #[test]
    fn alternatives_satisfy_the_group_with_any_single_member() {
        let alternative = |item, upgrade| Requirement {
            item: Some(item),
            kind: ItemKind::Weapon,
            upgrade: UpgradeRequirement::Exact(upgrade),
            alternative_group: Some(1),
            ..requirement(item)
        };
        let either = query(
            vec![
                alternative(ItemId::Spear, 3),
                alternative(ItemId::Shuriken, 2),
                alternative(ItemId::Sword, 1),
            ],
            24,
        );
        assert_eq!(either.validate(), Ok(()));

        let make = |item, upgrade| WorldItem {
            item,
            upgrade,
            effect: None,
            cursed: false,
            depth: 3,
            source: ItemSource::Heap,
            accessibility: Accessibility::Independent,
        };
        assert!(either.matches(&world(vec![make(ItemId::Shuriken, 2)])));
        assert!(either.matches(&world(vec![make(ItemId::Sword, 1)])));
        assert!(!either.matches(&world(vec![
            make(ItemId::Sword, 2),
            make(ItemId::Spear, 1),
            make(ItemId::Shuriken, 3),
        ])));
    }

    #[test]
    fn an_alternative_group_occupies_one_slot_alongside_other_requirements() {
        let alternative = |item| Requirement {
            item: Some(item),
            kind: ItemKind::Weapon,
            upgrade: UpgradeRequirement::Any,
            alternative_group: Some(3),
            ..requirement(item)
        };
        let sword_and_either = query(
            vec![
                alternative(ItemId::Spear),
                alternative(ItemId::Sword),
                Requirement {
                    upgrade: UpgradeRequirement::Any,
                    ..requirement(ItemId::Sword)
                },
            ],
            24,
        );
        let make = |item| WorldItem {
            item,
            upgrade: 0,
            effect: None,
            cursed: false,
            depth: 3,
            source: ItemSource::Heap,
            accessibility: Accessibility::Independent,
        };
        // One sword cannot satisfy both the group and the plain requirement.
        assert!(!sword_and_either.matches(&world(vec![make(ItemId::Sword)])));
        assert!(sword_and_either.matches(&world(vec![make(ItemId::Sword), make(ItemId::Spear)])));
        assert!(sword_and_either.matches(&world(vec![make(ItemId::Sword), make(ItemId::Sword)])));
    }

    #[test]
    fn upgrade_sum_totals_combine_linked_ring_upgrades() {
        let ring = || Requirement {
            kind: ItemKind::Ring,
            item: Some(ItemId::RingMight),
            upgrade: UpgradeRequirement::Any,
            identity_group: Some(1),
            upgrade_sum: Some(UpgradeSum {
                group: 1,
                minimum_total: 2,
            }),
            ..requirement(ItemId::RingMight)
        };
        let pair = query(vec![ring(), ring()], 4);
        assert_eq!(pair.validate(), Ok(()));

        let make = |item, upgrade| WorldItem {
            item,
            upgrade,
            effect: None,
            cursed: false,
            depth: 3,
            source: ItemSource::Heap,
            accessibility: Accessibility::Independent,
        };
        // +0 and +2 total the wanted +2, as do two +1 rings.
        assert!(pair.matches(&world(vec![
            make(ItemId::RingMight, 0),
            make(ItemId::RingMight, 2),
        ])));
        assert!(pair.matches(&world(vec![
            make(ItemId::RingMight, 1),
            make(ItemId::RingMight, 1),
        ])));
        // Two +0 rings fall short; a +2 alone is only one distinct copy.
        assert!(!pair.matches(&world(vec![
            make(ItemId::RingMight, 0),
            make(ItemId::RingMight, 0),
        ])));
        assert!(!pair.matches(&world(vec![make(ItemId::RingMight, 2)])));
        // The identity group still applies: different rings never pair up.
        assert!(!pair.matches(&world(vec![
            make(ItemId::RingMight, 1),
            make(ItemId::RingHaste, 1),
        ])));
    }

    #[test]
    fn upgrade_sum_members_keep_their_own_upgrade_predicates() {
        let member = |upgrade| Requirement {
            kind: ItemKind::Wand,
            item: Some(ItemId::WandFrost),
            upgrade,
            upgrade_sum: Some(UpgradeSum {
                group: 2,
                minimum_total: 3,
            }),
            ..requirement(ItemId::WandFrost)
        };
        let wands = query(
            vec![
                member(UpgradeRequirement::AtLeast(2)),
                member(UpgradeRequirement::Any),
            ],
            24,
        );
        let make = |upgrade| WorldItem {
            item: ItemId::WandFrost,
            upgrade,
            effect: None,
            cursed: false,
            depth: 3,
            source: ItemSource::Heap,
            accessibility: Accessibility::Independent,
        };
        // +1/+2 reaches the total but violates the first member's predicate on
        // one assignment order; the matcher must find the valid one.
        assert!(wands.matches(&world(vec![make(1), make(2)])));
        assert!(!wands.matches(&world(vec![make(1), make(1)])));
        assert!(wands.matches(&world(vec![make(3), make(0)])));
    }

    #[test]
    fn upgrade_sum_validation_rejects_inconsistent_or_unattainable_groups() {
        let member = |minimum_total| Requirement {
            kind: ItemKind::Ring,
            item: None,
            upgrade: UpgradeRequirement::Any,
            upgrade_sum: Some(UpgradeSum {
                group: 1,
                minimum_total,
            }),
            ..requirement(ItemId::RingMight)
        };
        let inconsistent = query(vec![member(2), member(3)], 24);
        assert_eq!(
            inconsistent.validate(),
            Err(QueryError::InconsistentUpgradeSum)
        );

        let unattainable = query(vec![member(9), member(9)], 24);
        assert_eq!(
            unattainable.validate(),
            Err(QueryError::UnattainableUpgradeSum)
        );

        let attainable = query(vec![member(8), member(8)], 24);
        assert_eq!(attainable.validate(), Ok(()));

        let zero_total = query(vec![member(0), member(0)], 24);
        assert_eq!(zero_total.validate(), Err(QueryError::InvalidUpgradeSum));

        let mut inside_alternative = member(2);
        inside_alternative.alternative_group = Some(1);
        assert_eq!(
            inside_alternative.validate(),
            Err(QueryError::UpgradeSumInsideAlternative)
        );
    }

    #[test]
    fn exact_upgrades_bound_what_a_sum_group_can_reach() {
        let member = |upgrade| Requirement {
            kind: ItemKind::Wand,
            item: None,
            upgrade,
            upgrade_sum: Some(UpgradeSum {
                group: 1,
                minimum_total: 5,
            }),
            ..requirement(ItemId::WandFrost)
        };
        let capped = query(
            vec![
                member(UpgradeRequirement::Exact(1)),
                member(UpgradeRequirement::Exact(2)),
            ],
            24,
        );
        assert_eq!(capped.validate(), Err(QueryError::UnattainableUpgradeSum));

        let reachable = query(
            vec![
                member(UpgradeRequirement::Exact(2)),
                member(UpgradeRequirement::Exact(3)),
            ],
            24,
        );
        assert_eq!(reachable.validate(), Ok(()));
    }

    #[test]
    fn best_match_indices_reports_partial_and_grouped_matches() {
        let make = |item, upgrade, depth| WorldItem {
            item,
            upgrade,
            effect: None,
            cursed: false,
            depth,
            source: ItemSource::Heap,
            accessibility: Accessibility::Independent,
        };
        let sample = world(vec![
            make(ItemId::RingMight, 0, 2),
            make(ItemId::Sword, 1, 3),
            make(ItemId::RingMight, 1, 5),
        ]);

        // A satisfied pair plus a missing wand: only the pair is highlighted.
        let ring = || Requirement {
            kind: ItemKind::Ring,
            item: Some(ItemId::RingMight),
            upgrade: UpgradeRequirement::Any,
            identity_group: Some(1),
            upgrade_sum: Some(UpgradeSum {
                group: 1,
                minimum_total: 1,
            }),
            ..requirement(ItemId::RingMight)
        };
        let mixed = query(
            vec![ring(), ring(), {
                Requirement {
                    kind: ItemKind::Wand,
                    item: Some(ItemId::WandFrost),
                    upgrade: UpgradeRequirement::Any,
                    ..requirement(ItemId::WandFrost)
                }
            }],
            24,
        );
        let mut matched = best_match_indices(&mixed, &sample);
        matched.sort_unstable();
        assert_eq!(matched, vec![0, 2]);

        // A pair that misses its total leaves both rings unhighlighted, while
        // an alternative group still claims the sword.
        let short_ring = || Requirement {
            upgrade_sum: Some(UpgradeSum {
                group: 1,
                minimum_total: 4,
            }),
            ..ring()
        };
        let alternative = |item| Requirement {
            item: Some(item),
            kind: ItemKind::Weapon,
            upgrade: UpgradeRequirement::Any,
            alternative_group: Some(1),
            ..requirement(item)
        };
        let grouped = query(
            vec![
                short_ring(),
                short_ring(),
                alternative(ItemId::Spear),
                alternative(ItemId::Sword),
            ],
            24,
        );
        assert_eq!(best_match_indices(&grouped, &sample), vec![1]);
    }

    #[test]
    fn best_match_indices_agrees_with_full_matching() {
        let make = |item, upgrade| WorldItem {
            item,
            upgrade,
            effect: None,
            cursed: false,
            depth: 3,
            source: ItemSource::Heap,
            accessibility: Accessibility::Independent,
        };
        let ring = || Requirement {
            kind: ItemKind::Ring,
            item: Some(ItemId::RingMight),
            upgrade: UpgradeRequirement::Any,
            identity_group: Some(1),
            upgrade_sum: Some(UpgradeSum {
                group: 1,
                minimum_total: 2,
            }),
            ..requirement(ItemId::RingMight)
        };
        let pair = query(vec![ring(), ring()], 24);
        let matching = world(vec![make(ItemId::RingMight, 0), make(ItemId::RingMight, 2)]);
        let short = world(vec![make(ItemId::RingMight, 0), make(ItemId::RingMight, 1)]);
        assert!(pair.matches(&matching));
        assert_eq!(best_match_indices(&pair, &matching).len(), 2);
        assert!(!pair.matches(&short));
        assert!(best_match_indices(&pair, &short).is_empty());
    }
}
