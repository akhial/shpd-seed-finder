//! Multi-item query validation and accessibility-aware matching.

use std::collections::BTreeMap;
use std::fmt;

use crate::catalog::{Effect, ItemId, ItemKind, WeaponCategory, item};
use crate::challenges::Challenges;
use crate::model::{GeneratedWorld, ItemSource, WorldItem};
use crate::quests::WandmakerQuestType;

type CandidateMatch = (usize, ItemId);
type RequirementCandidates = (Option<u8>, Vec<CandidateMatch>);

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

    /// Whether every tier this predicate accepts is also accepted by `base`.
    /// `Any` additionally accepts untiered items, so nothing but `Any`
    /// implies it from the untiered side; conservative `false` answers only
    /// cost a fresh scan, never soundness.
    const fn implies(self, base: Self) -> bool {
        match (self, base) {
            (_, Self::Any) => true,
            (Self::Exact(tier), Self::Exact(base_tier)) => tier == base_tier,
            (Self::Exact(tier) | Self::AtLeast(tier), Self::AtLeast(minimum)) => tier >= minimum,
            (Self::Exact(tier) | Self::AtMost(tier), Self::AtMost(maximum)) => tier <= maximum,
            _ => false,
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

    /// Whether every upgrade level this predicate accepts is also accepted
    /// by `base`.
    const fn implies(self, base: Self) -> bool {
        match (self, base) {
            (_, Self::Any) => true,
            (Self::Exact(upgrade), Self::Exact(base_upgrade)) => upgrade == base_upgrade,
            (Self::Exact(upgrade) | Self::AtLeast(upgrade), Self::AtLeast(minimum)) => {
                upgrade >= minimum
            }
            _ => false,
        }
    }
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
    pub effect: Option<Effect>,
    /// Whether cursed candidate items are ineligible for this requirement.
    pub require_uncursed: bool,
    pub source: Option<ItemSource>,
    /// Requirements in the same non-zero group must resolve to the same item ID.
    pub identity_group: Option<u8>,
    /// Optional inclusive floor limit for this item, independent of the query's
    /// overall generation limit.
    pub max_depth: Option<u8>,
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
            && self
                .effect
                .is_none_or(|wanted| candidate.effect == Some(wanted))
            && (!self.require_uncursed || !candidate.cursed)
            && self.source.is_none_or(|wanted| wanted == candidate.source))
        .then_some(identity)
    }

    /// Whether every item this requirement accepts is also accepted by
    /// `base`, assuming both live in queries of the same floor limit. This is the
    /// per-requirement half of the continuation rule: a requirement may be
    /// *strengthened* — an item named where `base` had only a kind, a bound
    /// tightened, uncursed demanded — and still cover `base`, because every
    /// world it admits was already admitted before.
    ///
    /// Identity groups compare by label: a base group constrains its members
    /// to one item, so a covering requirement must carry the same label (its
    /// group then imposes at least the same constraint), while a base with no
    /// group constrains nothing and the covering side may add one freely.
    /// A per-item floor limit of `None` means the query's own limit, which is
    /// identical on both sides under equal scope, so `None` on the base side
    /// is implied by everything and on the candidate side implies only
    /// `None`.
    fn implies(self, base: &Self) -> bool {
        self.kind == base.kind
            && base
                .weapon_category
                .is_none_or(|wanted| self.weapon_category == Some(wanted))
            && base.item.is_none_or(|wanted| self.item == Some(wanted))
            && self.tier.implies(base.tier)
            && self.upgrade.implies(base.upgrade)
            && base.effect.is_none_or(|wanted| self.effect == Some(wanted))
            && (self.require_uncursed || !base.require_uncursed)
            && base.source.is_none_or(|wanted| self.source == Some(wanted))
            && base
                .identity_group
                .is_none_or(|group| self.identity_group == Some(group))
            && match (self.max_depth, base.max_depth) {
                (_, None) => true,
                (Some(depth), Some(base_depth)) => depth <= base_depth,
                (None, Some(_)) => false,
            }
    }

    /// Checks that an item/effect/upgrade combination is meaningful.
    ///
    /// # Errors
    ///
    /// Returns a validation error for a category mismatch, an effect intended
    /// for another family, or an upgrade outside the UI's family-specific range.
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
        if self
            .max_depth
            .is_some_and(|depth| !(1..=24).contains(&depth))
        {
            return Err(QueryError::InvalidDepth);
        }
        match (self.kind, self.effect) {
            (ItemKind::Weapon, None | Some(Effect::Weapon(_)))
            | (ItemKind::Armor, None | Some(Effect::Armor(_)))
            | (ItemKind::Wand | ItemKind::Ring, None) => {}
            _ => return Err(QueryError::EffectKindMismatch),
        }
        if self.require_uncursed && self.effect.is_some_and(Effect::is_curse) {
            return Err(QueryError::UncursedWithCurse);
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
    /// Which Wandmaker quest the run must roll, or `None` for any. The quest
    /// item — corpse dust, an elemental ember, or a rotberry seed — is usable
    /// in the dungeon instead of being handed in, so which one a seed offers
    /// is worth searching for on its own; the other three givers' variants
    /// change nothing but the fight, and are reported rather than filtered.
    pub wandmaker_quest: Option<WandmakerQuestType>,
    /// Trades exhaustiveness for speed: +3 weapon/armor requirements are
    /// assumed to come from quest rewards, ignoring the far rarer Crypt and
    /// Sacrificial-fire prizes. Matches are still always genuine, but seeds
    /// whose only qualifying item comes from those rooms are skipped. See
    /// [`crate::feasibility`].
    pub fast_mode: bool,
}

/// Whether `candidate`'s Wandmaker filter is at least as strict as `base`'s.
///
/// Demanding a variant only ever removes seeds — the world generates the same
/// either way — so adding one to an unfiltered base narrows the match set just
/// like naming an item, and the base's covered region still contains every
/// match of the narrowed query. Dropping a filter, or swapping it for another
/// variant, admits seeds the base never accepted and must rescan.
const fn quest_at_least_as_strict(
    candidate: Option<WandmakerQuestType>,
    base: Option<WandmakerQuestType>,
) -> bool {
    match (candidate, base) {
        (_, None) => true,
        (Some(candidate), Some(wanted)) => candidate as u8 == wanted as u8,
        (None, Some(_)) => false,
    }
}

/// Whether a narrowing flag is at least as strict in `candidate` as in `base`.
///
/// The blacksmith flags are conditions on an unchanged world, exactly like the
/// quest filter: requiring a reachable Blacksmith, or barring the Smith
/// rewards from satisfying requirements, can only drop seeds the base already
/// matched. Switching one on therefore continues; switching it off widens the
/// query and has to rescan.
const fn flag_at_least_as_strict(candidate: bool, base: bool) -> bool {
    candidate || !base
}

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
        let mut identity_groups: BTreeMap<u8, (ItemKind, Option<ItemId>)> = BTreeMap::new();
        for requirement in &self.requirements {
            requirement.validate()?;
            if let Some(group) = requirement.identity_group {
                let current = (requirement.kind, requirement.item);
                if let Some(previous) = identity_groups.get(&group).copied() {
                    if previous.0 != current.0
                        || previous
                            .1
                            .zip(current.1)
                            .is_some_and(|(left, right)| left != right)
                    {
                        return Err(QueryError::InconsistentIdentityGroup);
                    }
                    if previous.1.is_none() && current.1.is_some() {
                        identity_groups.insert(group, current);
                    }
                } else {
                    identity_groups.insert(group, current);
                }
            }
        }
        Ok(())
    }

    /// Whether this query *continues* `base`: identical floor limit,
    /// challenges and fast mode, world conditions at least as strict as
    /// `base`'s (the blacksmith flags and the Wandmaker filter — see
    /// [`flag_at_least_as_strict`]), and, for every requirement
    /// of `base`, a *distinct* requirement of this query at least as strict
    /// ([`Requirement::implies`] — equality included, but so is naming a
    /// specific item where `base` wanted any of its kind, or tightening an
    /// upgrade bound). Only then is every match of this query within
    /// `base`'s covered region already among `base`'s matches, which is the
    /// soundness precondition for refining a search — filtering the
    /// delivered results and resuming the uncovered remainder (see
    /// `docs/search-semantics.md`). Frontends must consult this single
    /// predicate rather than re-deriving it.
    #[must_use]
    pub fn continues(&self, base: &SearchQuery) -> bool {
        if self.max_depth != base.max_depth
            || self.challenges != base.challenges
            || !flag_at_least_as_strict(self.require_blacksmith, base.require_blacksmith)
            || !flag_at_least_as_strict(
                self.exclude_blacksmith_rewards,
                base.exclude_blacksmith_rewards,
            )
            || !quest_at_least_as_strict(self.wandmaker_quest, base.wandmaker_quest)
            || self.fast_mode != base.fast_mode
            || self.requirements.len() < base.requirements.len()
        {
            return false;
        }
        // Implication is many-to-many (a named ring covers both "that ring"
        // and "any ring"), so covering every base requirement with a distinct
        // candidate is a bipartite matching problem; claiming greedily could
        // give "any ring" the lone Arcana and then fail "Arcana" against the
        // remaining "any ring". Augmenting paths keep the answer exact.
        let mut owner: Vec<Option<usize>> = vec![None; self.requirements.len()];
        (0..base.requirements.len()).all(|base_index| {
            let mut visited = vec![false; self.requirements.len()];
            self.cover_requirement(base, base_index, &mut owner, &mut visited)
        })
    }

    /// Finds an augmenting path assigning `base`'s requirement `base_index`
    /// to some candidate requirement of `self`, displacing earlier
    /// assignments when they can re-settle elsewhere.
    fn cover_requirement(
        &self,
        base: &SearchQuery,
        base_index: usize,
        owner: &mut [Option<usize>],
        visited: &mut [bool],
    ) -> bool {
        for (candidate_index, candidate) in self.requirements.iter().enumerate() {
            if visited[candidate_index] || !candidate.implies(&base.requirements[base_index]) {
                continue;
            }
            visited[candidate_index] = true;
            let free = match owner[candidate_index] {
                None => true,
                Some(displaced) => self.cover_requirement(base, displaced, owner, visited),
            };
            if free {
                owner[candidate_index] = Some(base_index);
                return true;
            }
        }
        false
    }

    /// Matches requirements as an AND query while respecting distinct item
    /// instances and mutually exclusive quest/chest reward branches.
    #[must_use]
    pub fn matches(&self, world: &GeneratedWorld) -> bool {
        if self.requirements.len() > world.items.len() {
            return false;
        }
        // A quest is reported only once its giver's floor is generated, so a
        // world whose prefix stops short of the Wandmaker simply has none and
        // cannot satisfy a variant filter.
        if let Some(wanted) = self.wandmaker_quest
            && !world
                .quests
                .wandmaker
                .is_some_and(|quest| quest.variant == wanted && quest.depth <= self.max_depth)
        {
            return false;
        }
        if self.require_blacksmith
            && !world.items.iter().any(|candidate| {
                candidate.depth <= self.max_depth
                    && candidate.source == ItemSource::BlacksmithReward
            })
        {
            return false;
        }

        let mut candidates: Vec<RequirementCandidates> = self
            .requirements
            .iter()
            .map(|requirement| {
                (
                    requirement.identity_group,
                    world
                        .items
                        .iter()
                        .enumerate()
                        .filter_map(|(index, candidate)| {
                            (candidate.depth <= self.max_depth
                                && candidate.depth
                                    <= requirement.max_depth.unwrap_or(self.max_depth)
                                && (!self.exclude_blacksmith_rewards
                                    || candidate.source != ItemSource::BlacksmithReward))
                                .then(|| {
                                    requirement
                                        .matching_identity(candidate)
                                        .map(|identity| (index, identity))
                                })
                                .flatten()
                        })
                        .collect(),
                )
            })
            .collect();
        if candidates.iter().any(|(_, values)| values.is_empty()) {
            return false;
        }

        // Fail early by assigning the most constrained requirement first.
        candidates.sort_by_key(|(_, values)| values.len());
        let mut used = vec![false; world.items.len()];
        let mut scenarios = BTreeMap::new();
        let mut identities = BTreeMap::new();
        match_recursive(
            &candidates,
            0,
            &world.items,
            &mut used,
            &mut scenarios,
            &mut identities,
        )
    }
}

fn match_recursive(
    candidates: &[RequirementCandidates],
    requirement_index: usize,
    items: &[WorldItem],
    used: &mut [bool],
    scenarios: &mut BTreeMap<u16, u64>,
    identities: &mut BTreeMap<u8, ItemId>,
) -> bool {
    if requirement_index == candidates.len() {
        return true;
    }

    let (identity_group, requirement_candidates) = &candidates[requirement_index];
    for &(item_index, matched_identity) in requirement_candidates {
        if used[item_index] {
            continue;
        }
        let mut previous_identity = None;
        if let Some(group) = identity_group {
            if identities
                .get(group)
                .is_some_and(|wanted| *wanted != matched_identity)
            {
                continue;
            }
            previous_identity = Some((*group, identities.insert(*group, matched_identity)));
        }
        let mut previous_scenarios = None;
        if let Some((group, item_scenarios)) = items[item_index].accessibility.scenario_constraint()
        {
            let compatible = scenarios.get(&group).copied().unwrap_or(u64::MAX) & item_scenarios;
            if compatible == 0 {
                if let Some((identity_group, previous)) = previous_identity {
                    if let Some(previous) = previous {
                        identities.insert(identity_group, previous);
                    } else {
                        identities.remove(&identity_group);
                    }
                }
                continue;
            }
            previous_scenarios = Some((group, scenarios.insert(group, compatible)));
        }

        used[item_index] = true;
        if match_recursive(
            candidates,
            requirement_index + 1,
            items,
            used,
            scenarios,
            identities,
        ) {
            return true;
        }
        used[item_index] = false;
        if let Some((group, previous)) = previous_scenarios {
            if let Some(previous) = previous {
                scenarios.insert(group, previous);
            } else {
                scenarios.remove(&group);
            }
        }
        if let Some((group, previous)) = previous_identity {
            if let Some(previous) = previous {
                identities.insert(group, previous);
            } else {
                identities.remove(&group);
            }
        }
    }
    false
}

/// Which items of a scouted world satisfy which requirements of a query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoutMatches {
    /// One flag per world item, in the scouted world's own item order — the
    /// order [`crate::wire::encode_scout_world`] emits — set for every item
    /// the selection claimed.
    pub matched: Vec<bool>,
    /// How many requirements the selection satisfies. Every requirement claims
    /// a distinct item, so this is also the number of flags set.
    pub matched_requirements: usize,
    /// How many requirements the query has in total.
    pub total_requirements: usize,
}

impl ScoutMatches {
    /// Indices of the selected items, ascending.
    #[must_use]
    pub fn matched_indices(&self) -> Vec<usize> {
        self.matched
            .iter()
            .enumerate()
            .filter_map(|(index, matched)| matched.then_some(index))
            .collect()
    }
}

/// Selects a largest set of distinct world items satisfying as many of
/// `query`'s requirements as possible, for explaining a scouted seed: the
/// partial-assignment variant of [`SearchQuery::matches`], which answers the
/// same question but only all-or-nothing.
///
/// The rules are the matcher's: the query's floor limit and each
/// requirement's own, the blacksmith-reward exclusion, one item per
/// requirement, identity groups bound to a single item ID, and accessibility
/// scenarios intersected per group. World-level conditions
/// (`require_blacksmith`, the Wandmaker filter) are *not* applied — they say
/// nothing about which item explains which requirement.
///
/// A full selection is therefore equivalent to
/// [`SearchQuery::matches`] on a query without those world conditions.
#[must_use]
pub fn scout_matches(world: &GeneratedWorld, query: &SearchQuery) -> ScoutMatches {
    let mut candidates: Vec<RequirementCandidates> = query
        .requirements
        .iter()
        .map(|requirement| {
            (
                requirement.identity_group,
                world
                    .items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, candidate)| {
                        (candidate.depth <= query.max_depth
                            && candidate.depth <= requirement.max_depth.unwrap_or(query.max_depth)
                            && (!query.exclude_blacksmith_rewards
                                || candidate.source != ItemSource::BlacksmithReward))
                            .then(|| {
                                // Identity groups bind to the identity the
                                // requirement matched on, exactly like
                                // `SearchQuery::matches`. That is the
                                // candidate's own item on every path today,
                                // but the requirement owns the notion of
                                // identity and must keep owning it here.
                                requirement
                                    .matching_identity(candidate)
                                    .map(|identity| (index, identity))
                            })
                            .flatten()
                    })
                    .collect(),
            )
        })
        .collect();
    // Try the most constrained requirement first, like the matcher does.
    candidates.sort_by_key(|(_, values)| values.len());
    let mut search = BestSubset {
        candidates: &candidates,
        items: &world.items,
        used: vec![false; world.items.len()],
        selected: Vec::new(),
        best: Vec::new(),
        scenarios: BTreeMap::new(),
        identities: BTreeMap::new(),
    };
    search.visit(0);
    let mut matched = vec![false; world.items.len()];
    for index in &search.best {
        matched[*index] = true;
    }
    ScoutMatches {
        matched,
        matched_requirements: search.best.len(),
        total_requirements: query.requirements.len(),
    }
}

/// Backtracking search for the largest assignment, keeping the best selection
/// seen so far and pruning branches which can no longer beat it.
struct BestSubset<'a> {
    candidates: &'a [RequirementCandidates],
    items: &'a [WorldItem],
    used: Vec<bool>,
    selected: Vec<usize>,
    best: Vec<usize>,
    scenarios: BTreeMap<u16, u64>,
    identities: BTreeMap<u8, ItemId>,
}

impl BestSubset<'_> {
    fn visit(&mut self, position: usize) {
        if position == self.candidates.len() {
            if self.selected.len() > self.best.len() {
                self.best.clone_from(&self.selected);
            }
            return;
        }
        if self.selected.len() + (self.candidates.len() - position) <= self.best.len() {
            return;
        }

        let (identity_group, candidates) = &self.candidates[position];
        for &(index, identity) in candidates {
            if self.used[index] {
                continue;
            }
            let mut previous_identity = None;
            if let Some(group) = identity_group {
                if self
                    .identities
                    .get(group)
                    .is_some_and(|wanted| *wanted != identity)
                {
                    continue;
                }
                previous_identity = Some((*group, self.identities.insert(*group, identity)));
            }
            let mut previous_scenarios = None;
            if let Some((group, mask)) = self.items[index].accessibility.scenario_constraint() {
                let compatible = self.scenarios.get(&group).copied().unwrap_or(u64::MAX) & mask;
                if compatible == 0 {
                    Self::rewind(&mut self.identities, previous_identity);
                    continue;
                }
                previous_scenarios = Some((group, self.scenarios.insert(group, compatible)));
            }

            self.used[index] = true;
            self.selected.push(index);
            self.visit(position + 1);
            self.selected.pop();
            self.used[index] = false;
            Self::rewind(&mut self.scenarios, previous_scenarios);
            Self::rewind(&mut self.identities, previous_identity);
        }
        // Skipping this requirement keeps the rest of the selection available.
        self.visit(position + 1);
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
            Self::UncursedWithCurse => "an uncursed item cannot have a curse",
            Self::InvalidIdentityGroup => "identity group zero is reserved for no group",
            Self::InconsistentIdentityGroup => {
                "linked item requirements must use the same category and item"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for QueryError {}

#[cfg(test)]
mod tests {
    use crate::catalog::{Effect, ItemId, ItemKind, WeaponEffect};
    use crate::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
    use crate::seed::DungeonSeed;

    use super::{
        QueryError, Requirement, SearchQuery, TierRequirement, UpgradeRequirement, scout_matches,
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
            secret: false,
        }
    }

    fn requirement(item: ItemId) -> Requirement {
        Requirement {
            kind: crate::catalog::item(item).kind,
            weapon_category: None,
            item: Some(item),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(2),
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
        }
    }

    #[test]
    fn continuation_needs_a_compatible_scope_and_a_requirement_superset() {
        let base = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::Sword)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };

        // Equality and supersets continue, in any requirement order.
        assert!(base.continues(&base));
        let mut narrowed = base.clone();
        narrowed
            .requirements
            .insert(0, requirement(ItemId::WandFrost));
        assert!(narrowed.continues(&base));
        assert!(!base.continues(&narrowed));

        // The multiset counts duplicates: one Sword does not cover two.
        let mut single = base.clone();
        single.requirements.pop();
        assert!(base.continues(&single));
        assert!(!single.continues(&base));

        // A different world — floor limit, challenges, or the lossy fast
        // mode — breaks continuation outright.
        let mut deeper = base.clone();
        deeper.max_depth = 5;
        assert!(!deeper.continues(&base));
        let mut challenged = base.clone();
        challenged.challenges = crate::challenges::Challenges::DARKNESS;
        assert!(!challenged.continues(&base));
        let mut fast = base.clone();
        fast.fast_mode = true;
        assert!(!fast.continues(&base));

        // The world conditions only ever remove seeds, so switching one on
        // strengthens the query rather than ending the continuation. Turning
        // it back off — or swapping the quest for another variant — widens it
        // and must rescan.
        let mut smith = base.clone();
        smith.require_blacksmith = true;
        assert!(smith.continues(&base));
        assert!(smith.continues(&smith));
        assert!(!base.continues(&smith));
        let mut excluded = base.clone();
        excluded.exclude_blacksmith_rewards = true;
        assert!(excluded.continues(&base));
        assert!(!base.continues(&excluded));
        let mut quested = base.clone();
        quested.wandmaker_quest = Some(crate::quests::WandmakerQuestType::CorpseDust);
        assert!(quested.continues(&base));
        assert!(quested.continues(&quested));
        assert!(!base.continues(&quested));
        let mut other = base.clone();
        other.wandmaker_quest = Some(crate::quests::WandmakerQuestType::Rotberry);
        assert!(!other.continues(&quested));

        // Tightening several at once still continues; a single loosened one
        // among them does not.
        let mut all = smith.clone();
        all.exclude_blacksmith_rewards = true;
        all.wandmaker_quest = Some(crate::quests::WandmakerQuestType::CorpseDust);
        assert!(all.continues(&base));
        assert!(all.continues(&smith));
        let mut relaxed = all.clone();
        relaxed.require_blacksmith = false;
        assert!(!relaxed.continues(&all));
    }

    #[test]
    fn continuation_accepts_strengthened_requirements() {
        let any_ring = Requirement {
            kind: ItemKind::Ring,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::AtLeast(3),
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
        };
        let arcana = Requirement {
            item: Some(ItemId::RingArcana),
            ..any_ring
        };
        let query = |requirements: Vec<Requirement>| SearchQuery {
            requirements,
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        let base = query(vec![any_ring]);

        // Naming the item strengthens "any ring": every Arcana +3 world is
        // an any-ring +3 world, so filter-and-resume stays sound. This is
        // the narrowing that must refine, not merely filter (the 274-seed
        // stall): the reverse widening must rescan.
        assert!(query(vec![arcana]).continues(&base));
        assert!(!base.continues(&query(vec![arcana])));

        // Tightening bounds strengthens; loosening them does not.
        let stricter = Requirement {
            upgrade: UpgradeRequirement::AtLeast(4),
            require_uncursed: true,
            max_depth: Some(10),
            ..arcana
        };
        assert!(query(vec![stricter]).continues(&base));
        assert!(
            !query(vec![Requirement {
                upgrade: UpgradeRequirement::AtLeast(2),
                ..any_ring
            }])
            .continues(&base)
        );
        assert!(
            !query(vec![Requirement {
                upgrade: UpgradeRequirement::Any,
                ..any_ring
            }])
            .continues(&base)
        );

        // Distinct requirements must cover distinct base requirements: one
        // Arcana cannot stand in for both rings, and greedy assignment must
        // not strand "Arcana against any-ring" when the candidate lists the
        // named ring first.
        let two_rings = query(vec![any_ring, any_ring]);
        assert!(query(vec![arcana, any_ring]).continues(&two_rings));
        assert!(!query(vec![arcana]).continues(&two_rings));
        let mixed_base = query(vec![any_ring, arcana]);
        assert!(query(vec![arcana, any_ring]).continues(&mixed_base));
        assert!(!query(vec![any_ring, any_ring]).continues(&mixed_base));

        // A base identity group must be carried; adding one is fine.
        let grouped = Requirement {
            identity_group: Some(1),
            ..any_ring
        };
        assert!(query(vec![grouped]).continues(&query(vec![grouped])));
        assert!(query(vec![grouped]).continues(&base));
        assert!(!base.continues(&query(vec![grouped])));
    }

    #[test]
    fn and_query_requires_distinct_item_occurrences() {
        let query = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::Sword)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        let one = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![world_item(ItemId::Sword, Accessibility::Independent)],
        };
        assert!(!query.matches(&one));
        let two = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![
                world_item(ItemId::Sword, Accessibility::Independent),
                world_item(ItemId::Sword, Accessibility::Independent),
            ],
        };
        assert!(query.matches(&two));
    }

    #[test]
    fn wandmaker_filter_needs_the_quest_itself_inside_the_floor_limit() {
        use crate::quests::{QuestSummary, ScheduledQuest, WandmakerQuestType};

        let mut query = SearchQuery {
            requirements: vec![requirement(ItemId::Sword)],
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: Some(WandmakerQuestType::Rotberry),
            fast_mode: false,
        };
        let world = |wandmaker| GeneratedWorld {
            quests: QuestSummary {
                wandmaker,
                ..QuestSummary::default()
            },
            seed: DungeonSeed::MIN,
            items: vec![world_item(ItemId::Sword, Accessibility::Independent)],
        };
        let rotberry = ScheduledQuest {
            variant: WandmakerQuestType::Rotberry,
            depth: 8,
        };

        assert!(query.matches(&world(Some(rotberry))));
        assert!(!query.matches(&world(Some(ScheduledQuest {
            variant: WandmakerQuestType::CorpseDust,
            depth: 8,
        }))));
        // A prefix that never reached the Prison has no Wandmaker at all.
        assert!(!query.matches(&world(None)));

        // The item requirement is unaffected either way.
        query.wandmaker_quest = None;
        assert!(query.matches(&world(None)));

        // A quest below the floor limit still counts; one above cannot.
        query.wandmaker_quest = Some(WandmakerQuestType::Rotberry);
        query.max_depth = 8;
        assert!(query.matches(&world(Some(rotberry))));
        query.max_depth = 7;
        assert!(!query.matches(&world(Some(rotberry))));
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
        let world = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![world_item(ItemId::Sword, Accessibility::Independent)],
        };
        let mut limited = requirement(ItemId::Sword);
        limited.max_depth = Some(2);
        let mut query = SearchQuery {
            requirements: vec![limited],
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        assert!(!query.matches(&world));
        query.requirements[0].max_depth = Some(3);
        assert!(query.matches(&world));
    }

    #[test]
    fn mutually_exclusive_rewards_cannot_satisfy_and_query() {
        let query = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::MailArmor)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        let world = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![
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
            ],
        };
        assert!(!query.matches(&world));
    }

    #[test]
    fn same_choice_option_and_independent_rewards_can_match() {
        let query = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::MailArmor)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        let world = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![
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
            ],
        };
        assert!(query.matches(&world));
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
        let world = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![sword, armor, wand],
        };

        let compatible = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::MailArmor)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        assert!(compatible.matches(&world));

        let incompatible = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::WandFrost)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        assert!(!incompatible.matches(&world));
    }

    #[test]
    fn validation_rejects_wrong_category() {
        let invalid = Requirement {
            kind: ItemKind::Wand,
            weapon_category: None,
            item: Some(ItemId::Sword),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(2),
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn weapon_category_narrows_wildcard_weapon_requirements() {
        use crate::catalog::WeaponCategory;

        let any_weapon = Requirement {
            kind: ItemKind::Weapon,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
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
    fn validation_rejects_uncursed_items_with_a_curse() {
        let invalid = Requirement {
            effect: Some(Effect::Weapon(WeaponEffect::Displacing)),
            require_uncursed: true,
            ..requirement(ItemId::Sword)
        };
        assert_eq!(invalid.validate(), Err(QueryError::UncursedWithCurse));
    }

    #[test]
    fn plus_four_is_valid_only_for_rings() {
        let ring = Requirement {
            kind: ItemKind::Ring,
            weapon_category: None,
            item: Some(ItemId::RingSharpshooting),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(4),
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
        };
        assert_eq!(ring.validate(), Ok(()));

        let wand = Requirement {
            kind: ItemKind::Wand,
            weapon_category: None,
            item: Some(ItemId::WandFrost),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(4),
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
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
            upgrade: UpgradeRequirement::Exact(2),
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
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
            ..tier_five
        };
        assert_eq!(invalid.validate(), Err(QueryError::InvalidTier));

        let tier_one = Requirement {
            tier: TierRequirement::Exact(1),
            ..tier_five
        };
        assert_eq!(tier_one.validate(), Err(QueryError::InvalidTier));

        let redundant_maximum = Requirement {
            tier: TierRequirement::AtMost(5),
            ..tier_five
        };
        assert_eq!(redundant_maximum.validate(), Err(QueryError::InvalidTier));

        for redundant in [
            TierRequirement::AtLeast(2),
            TierRequirement::AtLeast(5),
            TierRequirement::AtMost(2),
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
            tier: TierRequirement::Any,
            upgrade,
            effect: None,
            require_uncursed: false,
            source,
            identity_group: Some(1),
            max_depth: None,
        };
        let mut query = SearchQuery {
            requirements: vec![
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
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::Exact(1),
                    effect: None,
                    require_uncursed: false,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                },
            ],
            max_depth: 14,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: true,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        let make = |item, upgrade, depth, source| WorldItem {
            item,
            upgrade,
            effect: None,
            cursed: false,
            depth,
            source,
            accessibility: Accessibility::Independent,
            secret: false,
        };
        let world = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![
                make(ItemId::WandFrost, 3, 7, ItemSource::WandmakerReward),
                make(ItemId::WandFrost, 0, 2, ItemSource::Heap),
                make(ItemId::WandFrost, 1, 4, ItemSource::Chest),
                make(ItemId::WandLightning, 1, 5, ItemSource::Heap),
                make(ItemId::Sword, 2, 13, ItemSource::BlacksmithReward),
            ],
        };

        assert_eq!(query.validate(), Ok(()));
        assert!(query.matches(&world));

        let mut wrong_type = world.clone();
        wrong_type.items[2].item = ItemId::WandLightning;
        assert!(!query.matches(&wrong_type));

        query.max_depth = 12;
        assert!(!query.matches(&world));
    }

    #[test]
    fn smith_rewards_can_be_excluded_without_hiding_the_blacksmith() {
        let mut query = SearchQuery {
            requirements: vec![requirement(ItemId::Sword)],
            max_depth: 14,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: true,
            exclude_blacksmith_rewards: true,
            wandmaker_quest: None,
            fast_mode: false,
        };
        let make = |source| WorldItem {
            item: ItemId::Sword,
            upgrade: 2,
            effect: None,
            cursed: false,
            depth: 13,
            source,
            accessibility: Accessibility::Independent,
            secret: false,
        };
        let smith_only = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![make(ItemSource::BlacksmithReward)],
        };

        assert!(!query.matches(&smith_only));

        let mut reforging_setup = smith_only.clone();
        reforging_setup.items.push(make(ItemSource::Heap));
        assert!(query.matches(&reforging_setup));

        query.require_blacksmith = false;
        let no_blacksmith = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![make(ItemSource::Heap)],
        };
        assert!(query.matches(&no_blacksmith));
    }

    #[test]
    fn wildcard_does_not_hide_conflicting_concrete_identity_group_members() {
        let linked = |item| Requirement {
            kind: ItemKind::Wand,
            weapon_category: None,
            item,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: Some(1),
            max_depth: None,
        };
        let query = SearchQuery {
            requirements: vec![
                linked(Some(ItemId::WandFrost)),
                linked(None),
                linked(Some(ItemId::WandLightning)),
            ],
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };

        assert_eq!(query.validate(), Err(QueryError::InconsistentIdentityGroup));
    }

    fn scout_query(requirements: Vec<Requirement>) -> SearchQuery {
        SearchQuery {
            requirements,
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        }
    }

    fn scout_world(items: Vec<WorldItem>) -> GeneratedWorld {
        GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items,
        }
    }

    fn any_requirement(kind: ItemKind) -> Requirement {
        Requirement {
            kind,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
        }
    }

    #[test]
    fn scout_marks_the_largest_satisfiable_selection() {
        let world = scout_world(vec![
            world_item(ItemId::Sword, Accessibility::Independent),
            world_item(ItemId::WandFrost, Accessibility::Independent),
        ]);

        // Two swords wanted, one present: the marks explain the requirement
        // that can be satisfied instead of reporting nothing at all.
        let query = scout_query(vec![requirement(ItemId::Sword), requirement(ItemId::Sword)]);
        assert!(!query.matches(&world));
        let marks = scout_matches(&world, &query);
        assert_eq!(marks.matched, vec![true, false]);
        assert_eq!(marks.matched_indices(), vec![0]);
        assert_eq!(marks.matched_requirements, 1);
        assert_eq!(marks.total_requirements, 2);

        // Every requirement satisfied marks every item it claimed.
        let query = scout_query(vec![
            requirement(ItemId::Sword),
            requirement(ItemId::WandFrost),
        ]);
        let marks = scout_matches(&world, &query);
        assert_eq!(marks.matched, vec![true, true]);
        assert_eq!(marks.matched_requirements, 2);
        assert_eq!(marks.total_requirements, 2);

        // Nothing matching marks nothing.
        let marks = scout_matches(
            &world,
            &scout_query(vec![requirement(ItemId::WandLightning)]),
        );
        assert_eq!(marks.matched, vec![false, false]);
        assert_eq!(marks.matched_requirements, 0);
        assert_eq!(marks.total_requirements, 1);
    }

    #[test]
    fn scout_marks_bind_identity_groups_to_one_item() {
        let linked = Requirement {
            identity_group: Some(1),
            ..any_requirement(ItemKind::Wand)
        };
        let query = scout_query(vec![linked, linked]);

        // Two different wands cannot both answer a linked pair: the group
        // binds the second requirement to the first's item.
        let mixed = scout_world(vec![
            world_item(ItemId::WandFrost, Accessibility::Independent),
            world_item(ItemId::WandLightning, Accessibility::Independent),
        ]);
        assert!(!query.matches(&mixed));
        let marks = scout_matches(&mixed, &query);
        assert_eq!(marks.matched_requirements, 1);
        assert_eq!(marks.matched_indices().len(), 1);

        // Two copies of one wand satisfy both.
        let paired = scout_world(vec![
            world_item(ItemId::WandFrost, Accessibility::Independent),
            world_item(ItemId::WandFrost, Accessibility::Independent),
        ]);
        assert!(query.matches(&paired));
        assert_eq!(scout_matches(&paired, &query).matched, vec![true, true]);
    }

    #[test]
    fn scout_marks_respect_accessibility_scenarios() {
        let query = scout_query(vec![requirement(ItemId::Sword), requirement(ItemId::Sword)]);

        // Two swords on mutually exclusive acquisition plans of one group:
        // only one of them is ever obtainable, so only one is marked.
        let exclusive = scout_world(vec![
            world_item(
                ItemId::Sword,
                Accessibility::Scenarios {
                    group: 1,
                    mask: 0b01,
                },
            ),
            world_item(
                ItemId::Sword,
                Accessibility::Scenarios {
                    group: 1,
                    mask: 0b10,
                },
            ),
        ]);
        assert!(!query.matches(&exclusive));
        assert_eq!(scout_matches(&exclusive, &query).matched_requirements, 1);

        // A shared plan lets both count.
        let compatible = scout_world(vec![
            world_item(
                ItemId::Sword,
                Accessibility::Scenarios {
                    group: 1,
                    mask: 0b11,
                },
            ),
            world_item(
                ItemId::Sword,
                Accessibility::Scenarios {
                    group: 1,
                    mask: 0b10,
                },
            ),
        ]);
        assert!(query.matches(&compatible));
        assert_eq!(scout_matches(&compatible, &query).matched, vec![true, true]);
    }

    #[test]
    fn scout_marks_honour_floor_limits_and_the_blacksmith_exclusion() {
        let world = scout_world(vec![
            WorldItem {
                depth: 5,
                source: ItemSource::BlacksmithReward,
                ..world_item(ItemId::Sword, Accessibility::Independent)
            },
            WorldItem {
                depth: 9,
                ..world_item(ItemId::Sword, Accessibility::Independent)
            },
        ]);
        let mut query = scout_query(vec![requirement(ItemId::Sword)]);
        assert_eq!(scout_matches(&world, &query).matched_indices(), vec![0]);

        // The query's own floor limit hides the deeper copy, then both.
        query.max_depth = 5;
        assert_eq!(scout_matches(&world, &query).matched_indices(), vec![0]);
        query.max_depth = 4;
        assert_eq!(scout_matches(&world, &query).matched_requirements, 0);

        // A per-requirement limit narrows the same way on its own.
        query.max_depth = 24;
        query.requirements[0].max_depth = Some(8);
        assert_eq!(scout_matches(&world, &query).matched_indices(), vec![0]);
        query.requirements[0].max_depth = Some(4);
        assert_eq!(scout_matches(&world, &query).matched_requirements, 0);

        // Excluding Smith rewards drops the shallow copy for the deep one.
        query.requirements[0].max_depth = None;
        query.exclude_blacksmith_rewards = true;
        assert_eq!(scout_matches(&world, &query).matched_indices(), vec![1]);
    }

    #[test]
    fn scout_marks_agree_with_the_matcher_on_scouted_seeds() {
        let linked = |kind| Requirement {
            identity_group: Some(1),
            ..any_requirement(kind)
        };
        let mut shallow = scout_query(vec![any_requirement(ItemKind::Ring)]);
        shallow.max_depth = 6;
        let queries = [
            scout_query(vec![any_requirement(ItemKind::Ring)]),
            scout_query(vec![
                any_requirement(ItemKind::Wand),
                any_requirement(ItemKind::Wand),
                any_requirement(ItemKind::Wand),
            ]),
            scout_query(vec![
                requirement(ItemId::Sword),
                any_requirement(ItemKind::Armor),
            ]),
            scout_query(vec![linked(ItemKind::Ring), linked(ItemKind::Ring)]),
            scout_query(vec![Requirement {
                upgrade: UpgradeRequirement::AtLeast(3),
                ..any_requirement(ItemKind::Weapon)
            }]),
            shallow,
        ];

        let (mut satisfied, mut unsatisfied) = (0, 0);
        for value in [0_u64, 1, 7, 99] {
            let seed = DungeonSeed::new(value).unwrap();
            let world = crate::main_world::generate_main_world(seed, 12).unwrap();
            for query in &queries {
                let marks = scout_matches(&world, query);
                assert_eq!(marks.total_requirements, query.requirements.len());
                assert_eq!(marks.matched_indices().len(), marks.matched_requirements);
                assert_eq!(marks.matched.len(), world.items.len());
                // A full selection is exactly what the search matcher accepts.
                let complete = marks.matched_requirements == marks.total_requirements;
                assert_eq!(complete, query.matches(&world), "seed {value}");
                if complete {
                    satisfied += 1;
                } else {
                    unsatisfied += 1;
                }
            }
        }
        // Both outcomes must occur, or the agreement above proves nothing.
        assert!(satisfied > 0, "no query was fully satisfied");
        assert!(unsatisfied > 0, "every query was fully satisfied");
    }
}
