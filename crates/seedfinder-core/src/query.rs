//! Multi-item query validation and accessibility-aware matching.

use std::collections::BTreeMap;
use std::fmt;

use crate::catalog::{
    ALL_ARMOR_EFFECTS, ALL_WEAPON_EFFECTS, EXTRA_UPGRADE_TIER, Effect, ItemId, ItemKind,
    MAX_GENERATED_UPGRADE, MAX_STANDARD_RING_UPGRADE, WeaponCategory, item,
};
use crate::challenges::Challenges;
use crate::model::{GeneratedWorld, ItemSource, WorldItem};
use crate::quests::WandmakerQuestType;

/// Deepest floor a query may be limited to: the main dungeon ends at 24.
pub const MAX_SEARCH_DEPTH: u8 = 24;

/// Inclusive tier range an exact tier filter accepts. Tier 1 equipment is
/// starting gear the generator never places.
pub const EXACT_TIER_MIN: u8 = 2;
/// Upper end of [`EXACT_TIER_MIN`]'s range.
pub const EXACT_TIER_MAX: u8 = 5;

/// Inclusive tier range an at-least/at-most filter accepts; the bounds
/// outside it are redundant with no filter at all.
pub const BOUNDED_TIER_MIN: u8 = 3;
/// Upper end of [`BOUNDED_TIER_MIN`]'s range.
pub const BOUNDED_TIER_MAX: u8 = 4;

/// Identity group label reserved for "no group", which is why groups start
/// at 1. [`Requirement::validate`] rejects exactly this label.
pub const RESERVED_IDENTITY_GROUP: u8 = 0;

/// Highest identity group label the portable formats and every app's editor
/// can express (groups A..D). The matcher itself accepts any non-reserved
/// label, but a query that travels — as a share link or a results file —
/// must stay inside this range.
pub const MAX_IDENTITY_GROUP: u8 = 4;

/// Group label reserved for "no group" in alternative and combined-level
/// groups, mirroring [`RESERVED_IDENTITY_GROUP`].
pub const RESERVED_GROUP: u8 = 0;

/// Highest combined-level group label the portable formats and every
/// app's editor can express (groups A..D), the counterpart of
/// [`MAX_IDENTITY_GROUP`]. Alternative groups carry no such cap: the
/// portable formats write them structurally, as one `any_of` entry, and
/// renumber them on read.
pub const MAX_LEVEL_SUM_GROUP: u8 = 4;

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

/// Non-empty set of same-family effects, stored as a bitmask over the
/// family's upstream catalog ordering ([`ALL_WEAPON_EFFECTS`] and
/// [`ALL_ARMOR_EFFECTS`]). Only weapons and armor carry effects, so a set
/// always belongs to one of those two families.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EffectSet {
    family: ItemKind,
    mask: u32,
}

impl EffectSet {
    /// Builds a set holding one effect.
    #[must_use]
    pub const fn single(effect: Effect) -> Self {
        Self {
            family: effect_family(effect),
            mask: 1 << effect_index(effect),
        }
    }

    /// Builds a set from effects that must all belong to one family, or
    /// `None` for an empty iterator or a mix of families.
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

    /// Every non-curse enchantment or glyph of the family — the "any
    /// enchantment" predicate — or `None` for families that never carry
    /// effects.
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
    pub const fn contains(self, effect: Effect) -> bool {
        effect_family(effect) as u8 == self.family as u8
            && self.mask & (1 << effect_index(effect)) != 0
    }

    /// The member effects in upstream catalog order.
    pub fn effects(self) -> impl Iterator<Item = Effect> {
        family_effects(self.family)
            .into_iter()
            .flatten()
            .filter(move |effect| self.contains(*effect))
    }

    /// Whether every member of this set is also in `other`.
    #[must_use]
    pub const fn is_subset_of(self, other: Self) -> bool {
        self.family as u8 == other.family as u8 && self.mask & !other.mask == 0
    }

    /// The members shared with `other`, or `None` when nothing overlaps.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Option<Self> {
        if self.family as u8 != other.family as u8 {
            return None;
        }
        let mask = self.mask & other.mask;
        if mask == 0 {
            None
        } else {
            Some(Self {
                family: self.family,
                mask,
            })
        }
    }

    /// The set without its curse-type effects, or `None` when only curses
    /// were in it.
    #[must_use]
    pub fn without_curses(self) -> Option<Self> {
        Self::from_effects(self.effects().filter(|effect| !effect.is_curse()))
    }

    /// Whether every member is a curse-type effect.
    #[must_use]
    pub fn is_curses_only(self) -> bool {
        self.without_curses().is_none()
    }

    /// How many effects the set holds; never zero, at most 32.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // a u32 mask has at most 32 bits set
    pub const fn count(self) -> u8 {
        self.mask.count_ones() as u8
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
    /// full non-curse family set from [`EffectSet::enchantments`].
    OneOf(EffectSet),
}

impl EffectRequirement {
    /// The predicate accepting exactly one effect.
    #[must_use]
    pub const fn exactly(effect: Effect) -> Self {
        Self::OneOf(EffectSet::single(effect))
    }

    #[must_use]
    pub const fn matches(self, effect: Option<Effect>) -> bool {
        match self {
            Self::Any => true,
            Self::OneOf(set) => match effect {
                Some(effect) => set.contains(effect),
                None => false,
            },
        }
    }

    /// Whether every effect (or lack of one) this predicate accepts is also
    /// accepted by `base`.
    const fn implies(self, base: Self) -> bool {
        match (self, base) {
            (_, Self::Any) => true,
            (Self::Any, Self::OneOf(_)) => false,
            (Self::OneOf(set), Self::OneOf(base_set)) => set.is_subset_of(base_set),
        }
    }
}

/// Minimum combined item level shared by every requirement in one group.
///
/// An item's level counts as its upgrade plus one — a +0 Ring of Might still
/// grants one strength — and the group is satisfied when some subset of its
/// members, filled by distinct items, reaches `minimum_total` combined
/// levels. Members are *optional*: one +2 ring alone satisfies a two-member
/// group asking for three levels. Combine with [`Requirement::identity_group`]
/// to demand the contributing items be copies of one kind.
///
/// Only ring requirements may carry one: a ring's effect scales with its
/// level, so levels on separate rings add up the way no other family's do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelSum {
    /// Non-zero group label shared by the participating requirements.
    pub group: u8,
    /// Inclusive lower bound on the assigned members' combined levels,
    /// counting each item as `upgrade + 1`.
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
    /// Requirements in the same non-zero group are alternatives: together
    /// they form one *slot*, and a single item matching any member fills it.
    pub alternative_group: Option<u8>,
    /// Optional combined-level constraint shared with other requirements.
    /// Never set on a member of an alternative group.
    pub level_sum: Option<LevelSum>,
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

    /// The highest upgrade an item of the kind, identity and tier this
    /// requirement asks for can carry, whatever its upgrade filter says.
    ///
    /// Only a tier-[`EXTRA_UPGRADE_TIER`] weapon is levelled past
    /// [`MAX_GENERATED_UPGRADE`], so a requirement that rules that tier out —
    /// by naming an item of another tier, or by filtering the tier away —
    /// stops there.
    #[must_use]
    pub fn upgrade_ceiling(self) -> u8 {
        let reaches_the_extra_tier = match self.item {
            Some(item_id) => item(item_id).tier == Some(EXTRA_UPGRADE_TIER),
            None => self.tier.matches(Some(EXTRA_UPGRADE_TIER)),
        };
        if reaches_the_extra_tier {
            self.kind
                .maximum_search_upgrade_for_tier(EXTRA_UPGRADE_TIER)
        } else {
            MAX_GENERATED_UPGRADE
        }
    }

    /// The highest upgrade level an item satisfying this requirement can
    /// carry.
    #[must_use]
    pub fn maximum_upgrade(self) -> u8 {
        match self.upgrade {
            UpgradeRequirement::Exact(wanted) => wanted,
            UpgradeRequirement::Any | UpgradeRequirement::AtLeast(_) => self.upgrade_ceiling(),
        }
    }

    /// The most *levels* — upgrade plus one — an item satisfying this
    /// requirement can contribute to a combined-level group.
    #[must_use]
    pub fn maximum_level(self) -> u8 {
        self.maximum_upgrade() + 1
    }

    /// Whether this requirement constrains anything beyond its kind: a named
    /// item, a tier or upgrade bound, an effect, uncursedness, or a source.
    /// A stack's extra copies are exactly the unconstrained requirements; a
    /// per-item floor limit is a placement bound, not an item property, and
    /// does not count.
    #[must_use]
    pub const fn is_bare(self) -> bool {
        self.item.is_none()
            && self.weapon_category.is_none()
            && matches!(self.tier, TierRequirement::Any)
            && matches!(self.upgrade, UpgradeRequirement::Any)
            && matches!(self.effect, EffectRequirement::Any)
            && !self.require_uncursed
            && self.source.is_none()
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
    /// `None`. Alternative and combined-level groups are slot-level
    /// structure and are compared by [`SearchQuery::continues`] instead.
    fn implies(self, base: &Self) -> bool {
        self.kind == base.kind
            && base
                .weapon_category
                .is_none_or(|wanted| self.weapon_category == Some(wanted))
            && base.item.is_none_or(|wanted| self.item == Some(wanted))
            && self.tier.implies(base.tier)
            && self.upgrade.implies(base.upgrade)
            && self.effect.implies(base.effect)
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
    /// Returns a validation error for a category mismatch, an effect set of
    /// another family, an upgrade outside the UI's family-specific range, or
    /// an inconsistent group label.
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
            TierRequirement::Exact(tier) => {
                tierable && (EXACT_TIER_MIN..=EXACT_TIER_MAX).contains(&tier)
            }
            TierRequirement::AtLeast(tier) | TierRequirement::AtMost(tier) => {
                tierable && (BOUNDED_TIER_MIN..=BOUNDED_TIER_MAX).contains(&tier)
            }
        };
        if !valid_tier {
            return Err(QueryError::InvalidTier);
        }
        let maximum = self.upgrade_ceiling();
        let valid_upgrade = match self.upgrade {
            UpgradeRequirement::Any => true,
            UpgradeRequirement::Exact(upgrade) => (1..=maximum).contains(&upgrade),
            UpgradeRequirement::AtLeast(upgrade) => upgrade <= maximum,
        };
        if !valid_upgrade {
            return Err(QueryError::InvalidUpgrade);
        }
        if self.identity_group == Some(RESERVED_IDENTITY_GROUP) {
            return Err(QueryError::InvalidIdentityGroup);
        }
        if self.alternative_group == Some(RESERVED_GROUP) {
            return Err(QueryError::InvalidAlternativeGroup);
        }
        if self
            .level_sum
            .is_some_and(|sum| sum.group == RESERVED_GROUP || sum.minimum_total == 0)
        {
            return Err(QueryError::InvalidLevelSum);
        }
        // Levels only combine meaningfully across rings — a ring's effect
        // scales with its level, so a +0 and a +1 together grant what one
        // +2 does. No other family adds up that way.
        if self.level_sum.is_some() && self.kind != ItemKind::Ring {
            return Err(QueryError::LevelSumOutsideRings);
        }
        if self.alternative_group.is_some() && self.level_sum.is_some() {
            return Err(QueryError::LevelSumInsideAlternative);
        }
        if self
            .max_depth
            .is_some_and(|depth| !(1..=MAX_SEARCH_DEPTH).contains(&depth))
        {
            return Err(QueryError::InvalidDepth);
        }
        match self.effect {
            EffectRequirement::Any => {}
            EffectRequirement::OneOf(set) if set.family() == self.kind => {}
            EffectRequirement::OneOf(_) => return Err(QueryError::EffectKindMismatch),
        }
        if self.require_uncursed
            && let EffectRequirement::OneOf(set) = self.effect
            && set.is_curses_only()
        {
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

/// One identity-group member seen during validation: its index, alternative
/// group, category, and whether it is a bare copy ([`Requirement::is_bare`]).
type IdentityMember = (usize, Option<u8>, ItemKind, bool);

impl SearchQuery {
    /// Validates bounds and every requirement.
    ///
    /// # Errors
    ///
    /// Returns a [`QueryError`] when no requirements are present, the selected
    /// depth is outside the main dungeon, a requirement is inconsistent, or a
    /// cross-requirement group disagrees with itself.
    pub fn validate(&self) -> Result<(), QueryError> {
        if self.requirements.is_empty() {
            return Err(QueryError::Empty);
        }
        if !(1..=MAX_SEARCH_DEPTH).contains(&self.max_depth) {
            return Err(QueryError::InvalidDepth);
        }
        let mut identity_groups: BTreeMap<u8, Vec<IdentityMember>> = BTreeMap::new();
        let mut level_sums: BTreeMap<u8, u8> = BTreeMap::new();
        for (index, requirement) in self.requirements.iter().enumerate() {
            requirement.validate()?;
            if let Some(group) = requirement.identity_group {
                identity_groups.entry(group).or_default().push((
                    index,
                    requirement.alternative_group,
                    requirement.kind,
                    requirement.is_bare(),
                ));
            }
            if let Some(sum) = requirement.level_sum {
                let agreed = level_sums.entry(sum.group).or_insert(sum.minimum_total);
                if *agreed != sum.minimum_total {
                    return Err(QueryError::InconsistentLevelSum { group: sum.group });
                }
            }
        }
        // An identity group is a stack: one *anchor unit* — a lone
        // requirement, or the members of one alternative group — may
        // constrain which item the stack binds to; every other member is a
        // bare copy of the anchor's kind. Constraining a second unit would
        // describe two different items forced to be the same, which the
        // stack model deliberately cannot say.
        for members in identity_groups.values() {
            let (_, _, first_kind, _) = members[0];
            if members.iter().any(|&(_, _, kind, _)| kind != first_kind) {
                return Err(QueryError::InconsistentIdentityGroup);
            }
            let mut anchor: Option<(Option<u8>, usize)> = None;
            for &(index, alternative, _, bare) in members {
                if bare {
                    continue;
                }
                // Members of one alternative group form a single unit.
                let unit = alternative.map_or((None, index), |group| (Some(group), 0));
                if *anchor.get_or_insert(unit) != unit {
                    return Err(QueryError::OverconstrainedIdentityGroup);
                }
            }
        }
        for (group, summary) in self.level_sum_groups() {
            let attainable = summary.attainable_capacity();
            if summary.minimum_total > attainable {
                return Err(QueryError::UnattainableLevelSum {
                    group,
                    minimum_total: summary.minimum_total,
                    capacity: attainable,
                });
            }
        }
        Ok(())
    }

    /// Every combined-level group of the query, keyed by label.
    #[must_use]
    pub fn level_sum_groups(&self) -> BTreeMap<u8, SumGroup> {
        let mut groups: BTreeMap<u8, SumGroup> = BTreeMap::new();
        for requirement in &self.requirements {
            if let Some(sum) = requirement.level_sum {
                let group = groups.entry(sum.group).or_default();
                group.members += 1;
                group.minimum_total = u16::from(sum.minimum_total);
                group.capacity += u16::from(requirement.maximum_level());
            }
        }
        groups
    }

    /// The query's slots: requirement indices grouped so that the members of
    /// one alternative group share a slot, in first-appearance order. Every
    /// other requirement is a slot of its own. A world matches when every
    /// slot is filled by a distinct item matching one of its members.
    #[must_use]
    pub fn slots(&self) -> Vec<Vec<usize>> {
        let mut slot_of_group: BTreeMap<u8, usize> = BTreeMap::new();
        let mut slots: Vec<Vec<usize>> = Vec::new();
        for (index, requirement) in self.requirements.iter().enumerate() {
            match requirement.alternative_group {
                Some(group) => {
                    let slot = *slot_of_group.entry(group).or_insert_with(|| {
                        slots.push(Vec::new());
                        slots.len() - 1
                    });
                    slots[slot].push(index);
                }
                None => slots.push(vec![index]),
            }
        }
        slots
    }

    /// How many slots the query has — what a frontend counts as "requirements"
    /// once alternatives collapse.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots().len()
    }

    /// How many conditions the scout reports: one per slot, except that all
    /// the slots of one combined-level group collapse into a single
    /// condition, satisfied together or not at all.
    #[must_use]
    pub fn scout_condition_count(&self) -> usize {
        let slots = self.slots();
        let mut groups: Vec<u8> = Vec::new();
        let mut sum_slots = 0;
        for slot in &slots {
            // Combined-level members never sit in alternative groups.
            if let Some(sum) = self.requirements[slot[0]].level_sum {
                sum_slots += 1;
                if !groups.contains(&sum.group) {
                    groups.push(sum.group);
                }
            }
        }
        slots.len() - sum_slots + groups.len()
    }

    /// Whether this query *continues* `base`: identical floor limit and
    /// challenges, world conditions at least as strict as
    /// `base`'s (the blacksmith flags and the Wandmaker filter — see
    /// [`flag_at_least_as_strict`]), and, for every slot of `base`, a
    /// *distinct* slot of this query at least as strict: each of its members
    /// implies some member of the base slot ([`Requirement::implies`] —
    /// equality included, but so is naming a specific item where `base`
    /// wanted any of its kind, dropping an alternative, or tightening an
    /// upgrade bound). Combined-upgrade groups of `base` must be carried
    /// over intact: the slots covering a base group must form exactly one
    /// candidate group with at least the base total. Only then is every
    /// match of this query within `base`'s covered region already among
    /// `base`'s matches, which is the soundness precondition for refining a
    /// search — filtering the delivered results and resuming the uncovered
    /// remainder (see `docs/search-semantics.md`). Frontends must consult
    /// this single predicate rather than re-deriving it.
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
        {
            return false;
        }
        let candidate_slots = self.slots();
        let base_slots = base.slots();
        if candidate_slots.len() < base_slots.len() {
            return false;
        }
        // Implication is many-to-many (a named ring covers both "that ring"
        // and "any ring"), so covering every base slot with a distinct
        // candidate slot is a bipartite matching problem; claiming greedily
        // could give "any ring" the lone Arcana and then fail "Arcana" against
        // the remaining "any ring". Augmenting paths keep the answer exact.
        let implies = |candidate_slot: &[usize], base_slot: &[usize]| {
            // A combined-level slot is optional — a match may leave it empty —
            // so it can only stand in for another combined-level slot; a
            // mandatory base slot must be covered by a mandatory one.
            if candidate_slot
                .iter()
                .any(|&candidate| self.requirements[candidate].level_sum.is_some())
                && base.requirements[base_slot[0]].level_sum.is_none()
            {
                return false;
            }
            candidate_slot.iter().all(|&candidate| {
                base_slot
                    .iter()
                    .any(|&wanted| self.requirements[candidate].implies(&base.requirements[wanted]))
            })
        };
        let mut owner: Vec<Option<usize>> = vec![None; candidate_slots.len()];
        let covered = (0..base_slots.len()).all(|base_index| {
            let mut visited = vec![false; candidate_slots.len()];
            cover_slot(
                &candidate_slots,
                &base_slots,
                &implies,
                base_index,
                &mut owner,
                &mut visited,
            )
        });
        if !covered {
            return false;
        }
        // The matching found is one of possibly many; a base sum group it
        // happens to split across candidate groups reads as "not continued",
        // which only costs a rescan.
        let mut cover: Vec<usize> = vec![0; base_slots.len()];
        for (candidate_index, base_index) in owner.iter().enumerate() {
            if let Some(base_index) = base_index {
                cover[*base_index] = candidate_index;
            }
        }
        let candidate_groups = self.level_sum_groups();
        let mut base_groups: BTreeMap<u8, (u8, Vec<usize>)> = BTreeMap::new();
        for (base_index, base_slot) in base_slots.iter().enumerate() {
            // Sum members are never alternatives, so the slot is one member.
            if let Some(sum) = base.requirements[base_slot[0]].level_sum {
                base_groups
                    .entry(sum.group)
                    .or_insert((sum.minimum_total, Vec::new()))
                    .1
                    .push(base_index);
            }
        }
        base_groups.values().all(|(minimum_total, members)| {
            let mut carried: Option<u8> = None;
            members.iter().all(|&base_index| {
                let candidate_slot = &candidate_slots[cover[base_index]];
                let [candidate] = candidate_slot[..] else {
                    return false;
                };
                let Some(sum) = self.requirements[candidate].level_sum else {
                    return false;
                };
                sum.minimum_total >= *minimum_total
                    && candidate_groups
                        .get(&sum.group)
                        .is_some_and(|group| usize::from(group.members) == members.len())
                    && carried
                        .replace(sum.group)
                        .is_none_or(|group| group == sum.group)
            })
        })
    }

    /// Whether this query and `base` name a common item: some requirement of
    /// each has the same kind, and either both name the same item or at least
    /// one names none (a kind-level requirement subsumes every item of its
    /// kind). Scope and challenge differences are irrelevant — a filter
    /// re-verifies seeds from scratch — so this deliberately checks nothing
    /// else: it only estimates whether a previous search's results are
    /// enriched for this query's matches, which is what makes filtering them
    /// worthwhile. The relation is symmetric.
    ///
    /// This is the weaker sibling of [`SearchQuery::continues`] used by the
    /// start decision in `docs/search-semantics.md`; frontends must consult it
    /// rather than re-deriving it.
    #[must_use]
    pub fn shares_item(&self, base: &SearchQuery) -> bool {
        self.requirements.iter().any(|left| {
            base.requirements.iter().any(|right| {
                left.kind == right.kind
                    && (left.item.is_none() || right.item.is_none() || left.item == right.item)
            })
        })
    }

    /// Matches requirements as an AND query over slots while respecting
    /// distinct item instances, alternative groups, combined-level totals,
    /// and mutually exclusive quest/chest reward branches.
    #[must_use]
    pub fn matches(&self, world: &GeneratedWorld) -> bool {
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

        let mut assignment = Assignment::prepare(self, world);
        let mandatory = assignment
            .slots
            .iter()
            .filter(|slot| !slot.optional)
            .count();
        if mandatory > world.items.len()
            || assignment
                .slots
                .iter()
                .any(|slot| !slot.optional && slot.candidates.is_empty())
        {
            return false;
        }
        assignment.fills_every_slot(0)
    }
}

/// Finds an augmenting path assigning base slot `base_index` to some
/// candidate slot, displacing earlier assignments when they can re-settle
/// elsewhere.
fn cover_slot(
    candidate_slots: &[Vec<usize>],
    base_slots: &[Vec<usize>],
    implies: &impl Fn(&[usize], &[usize]) -> bool,
    base_index: usize,
    owner: &mut [Option<usize>],
    visited: &mut [bool],
) -> bool {
    for (candidate_index, candidate_slot) in candidate_slots.iter().enumerate() {
        if visited[candidate_index] || !implies(candidate_slot, &base_slots[base_index]) {
            continue;
        }
        visited[candidate_index] = true;
        let free = match owner[candidate_index] {
            None => true,
            Some(displaced) => cover_slot(
                candidate_slots,
                base_slots,
                implies,
                displaced,
                owner,
                visited,
            ),
        };
        if free {
            owner[candidate_index] = Some(base_index);
            return true;
        }
    }
    false
}

/// One candidate match for a slot: the world item, the identity the member
/// matched on, and the member itself.
type SlotCandidate<'query> = (usize, ItemId, &'query Requirement);

/// Size, required total, and upgrade capacity of one combined-level group.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SumGroup {
    /// How many requirements carry the group.
    pub members: u16,
    /// The total the members agreed on (the last member's, before
    /// validation checks they agree).
    pub minimum_total: u16,
    /// The most combined levels the members could contribute together:
    /// each one's [`Requirement::maximum_level`].
    pub capacity: u16,
}

impl SumGroup {
    /// The most combined levels a generated world can actually put on the
    /// group: [`SumGroup::capacity`] bounded by generation, which levels at
    /// most one ring — the Imp vault's prize — beyond
    /// [`MAX_STANDARD_RING_UPGRADE`]. The matcher keeps pruning against the
    /// per-member `capacity`, which stays sound on any world it is handed;
    /// this tighter bound is what validation holds a total to.
    #[must_use]
    pub fn attainable_capacity(&self) -> u16 {
        let generated = u16::from(MAX_GENERATED_UPGRADE + 1)
            + self
                .members
                .saturating_sub(1)
                .saturating_mul(u16::from(MAX_STANDARD_RING_UPGRADE + 1));
        self.capacity.min(generated)
    }
}

/// Letter every editor shows for a portable group label (A..D), falling
/// back to the number for labels beyond [`MAX_LEVEL_SUM_GROUP`].
#[must_use]
pub fn group_label(group: u8) -> String {
    if (1..=MAX_LEVEL_SUM_GROUP).contains(&group) {
        char::from(b'A' + group - 1).to_string()
    } else {
        group.to_string()
    }
}

/// Running state of one combined-level group inside an assignment.
#[derive(Clone, Copy, Debug, Default)]
struct SumProgress {
    /// Combined levels of the assigned members, each item counting
    /// `upgrade + 1`.
    total: u16,
    /// Level capacity of the members assigned so far.
    spent_capacity: u16,
}

/// One resolved slot: its candidate matches, and whether it may stay empty
/// (a combined-level member is optional — the rest of its group can carry
/// the total).
struct Slot<'query> {
    candidates: Vec<SlotCandidate<'query>>,
    optional: bool,
}

/// Query slots resolved against one world's items: alternatives collapse to
/// one slot, and every mandatory slot must be served by a distinct item.
struct Assignment<'query> {
    items: &'query [WorldItem],
    /// Resolved slots, most constrained slot first.
    slots: Vec<Slot<'query>>,
    sum_groups: BTreeMap<u8, SumGroup>,
    used: Vec<bool>,
    scenarios: BTreeMap<u16, u64>,
    identities: BTreeMap<u8, ItemId>,
    sums: BTreeMap<u8, SumProgress>,
}

/// What one placement changed, so it can be undone exactly.
#[derive(Clone, Copy)]
struct Undo {
    item_index: usize,
    identity: Option<(u8, Option<ItemId>)>,
    scenario: Option<(u16, Option<u64>)>,
    sum: Option<(u8, Option<SumProgress>)>,
}

impl<'query> Assignment<'query> {
    /// Builds per-slot candidate lists under the query's floor limits and the
    /// blacksmith-reward exclusion, sorted most constrained slot first.
    fn prepare(query: &'query SearchQuery, world: &'query GeneratedWorld) -> Self {
        let mut slots: Vec<Slot<'query>> = Vec::new();
        for slot in query.slots() {
            let mut candidates = Vec::new();
            // Combined-level members never sit in alternative groups, so a
            // slot is optional exactly when its members carry a level sum.
            let optional = slot
                .iter()
                .all(|&member| query.requirements[member].level_sum.is_some());
            for member in slot {
                let requirement = &query.requirements[member];
                for (index, candidate) in world.items.iter().enumerate() {
                    if candidate.depth <= query.max_depth
                        && candidate.depth <= requirement.max_depth.unwrap_or(query.max_depth)
                        && (!query.exclude_blacksmith_rewards
                            || candidate.source != ItemSource::BlacksmithReward)
                        && let Some(identity) = requirement.matching_identity(candidate)
                    {
                        candidates.push((index, identity, requirement));
                    }
                }
            }
            slots.push(Slot {
                candidates,
                optional,
            });
        }
        let sum_groups = query.level_sum_groups();
        // Fail early by assigning the most constrained slot first.
        slots.sort_by_key(|slot| slot.candidates.len());
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

    /// Depth-first assignment requiring every mandatory slot to hold a
    /// distinct item and every combined-level group to reach its total.
    fn fills_every_slot(&mut self, slot: usize) -> bool {
        if slot == self.slots.len() {
            return self.level_sums_satisfied();
        }
        for candidate in 0..self.slots[slot].candidates.len() {
            let (item_index, identity, requirement) = self.slots[slot].candidates[candidate];
            let Some(undo) = self.assign(item_index, identity, requirement) else {
                continue;
            };
            if self.fills_every_slot(slot + 1) {
                return true;
            }
            self.unassign(undo);
        }
        // A combined-level slot may stay empty: the rest of its group can
        // carry the total.
        self.slots[slot].optional && self.fills_every_slot(slot + 1)
    }

    /// Whether every combined-level group's assigned members reach its total.
    fn level_sums_satisfied(&self) -> bool {
        self.sum_groups.iter().all(|(label, group)| {
            self.sums.get(label).map_or(0, |progress| progress.total) >= group.minimum_total
        })
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
        if let Some(sum) = requirement.level_sum {
            let group = self.sum_groups.get(&sum.group).copied().unwrap_or_default();
            let previous = self.sums.get(&sum.group).copied().unwrap_or_default();
            let progress = SumProgress {
                total: previous.total + u16::from(self.items[item_index].upgrade) + 1,
                spent_capacity: previous.spent_capacity + u16::from(requirement.maximum_level()),
            };
            // Prune once even the unassigned members at their caps cannot
            // lift the total to the target.
            let reachable = group.capacity.saturating_sub(progress.spent_capacity);
            if progress.total + reachable < group.minimum_total {
                self.unassign(undo);
                return None;
            }
            undo.sum = Some((sum.group, self.sums.insert(sum.group, progress)));
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

    /// Combined-level groups short of their total: their assigned members
    /// do not count as satisfied.
    fn failed_sum_groups(&self) -> Vec<u8> {
        self.sum_groups
            .iter()
            .filter(|(label, group)| {
                self.sums.get(label).map_or(0, |progress| progress.total) < group.minimum_total
            })
            .map(|(label, _)| *label)
            .collect()
    }
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

/// Which items of a scouted world satisfy which slots of a query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoutMatches {
    /// One flag per world item, in the scouted world's own item order — the
    /// order [`crate::wire::encode_scout_world`] emits — set for every item
    /// the selection claimed for a satisfied condition.
    pub matched: Vec<bool>,
    /// How many conditions the selection satisfies: one per filled plain
    /// slot, plus one per combined-level group whose assigned items reach
    /// its total. A satisfied group flags every contributing item, so more
    /// flags than conditions may be set; items of a group short of its
    /// total are not flagged at all.
    pub matched_requirements: usize,
    /// How many conditions the query has in total — one per slot, with all
    /// the slots of a combined-level group counting once
    /// ([`SearchQuery::scout_condition_count`]).
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
/// `query`'s slots as possible, for explaining a scouted seed: the
/// partial-assignment variant of [`SearchQuery::matches`], which answers the
/// same question but only all-or-nothing.
///
/// The rules are the matcher's: the query's floor limit and each
/// requirement's own, the blacksmith-reward exclusion, one item per slot,
/// any member of an alternative group filling its slot, identity groups
/// bound to a single item ID, accessibility scenarios intersected per group,
/// and combined-level groups counting as one condition, satisfied when the
/// assigned members' levels reach the total — a lone +0 ring of a wanted
/// pair that falls short is not highlighted.
/// World-level conditions (`require_blacksmith`, the Wandmaker filter) are
/// *not* applied — they say nothing about which item explains which slot.
///
/// A full selection is therefore equivalent to
/// [`SearchQuery::matches`] on a query without those world conditions.
#[must_use]
pub fn scout_matches(world: &GeneratedWorld, query: &SearchQuery) -> ScoutMatches {
    let total_requirements = query.scout_condition_count();
    let mut search = BestSubset {
        assignment: Assignment::prepare(query, world),
        selected: Vec::new(),
        best: Vec::new(),
        best_conditions: 0,
    };
    search.visit(0);
    let mut matched = vec![false; world.items.len()];
    for index in &search.best {
        matched[*index] = true;
    }
    ScoutMatches {
        matched,
        matched_requirements: search.best_conditions,
        total_requirements,
    }
}

/// Backtracking search for the most satisfied conditions, keeping the best
/// selection seen so far and pruning branches which can no longer beat it.
struct BestSubset<'query> {
    assignment: Assignment<'query>,
    /// Assigned items with the combined-level group they serve, if any.
    selected: Vec<(usize, Option<u8>)>,
    /// The items of the best selection.
    best: Vec<usize>,
    /// The conditions the best selection satisfies.
    best_conditions: usize,
}

impl BestSubset<'_> {
    fn visit(&mut self, slot: usize) {
        if slot == self.assignment.slots.len() {
            // Items serving a group short of its total do not count and are
            // not highlighted; a satisfied group counts once, however many
            // items carried it.
            let failed = self.assignment.failed_sum_groups();
            let mut items: Vec<usize> = Vec::new();
            let mut satisfied_groups: Vec<u8> = Vec::new();
            let mut conditions = 0;
            for &(item_index, sum_group) in &self.selected {
                match sum_group {
                    None => {
                        conditions += 1;
                        items.push(item_index);
                    }
                    Some(group) if !failed.contains(&group) => {
                        if !satisfied_groups.contains(&group) {
                            satisfied_groups.push(group);
                            conditions += 1;
                        }
                        items.push(item_index);
                    }
                    Some(_) => {}
                }
            }
            if conditions > self.best_conditions {
                self.best_conditions = conditions;
                self.best = items;
            }
            return;
        }
        // The remaining slots bound what this branch can still add: each
        // selected item and each remaining slot satisfies at most one
        // condition.
        if self.selected.len() + (self.assignment.slots.len() - slot) <= self.best_conditions {
            return;
        }
        for candidate in 0..self.assignment.slots[slot].candidates.len() {
            let (item_index, identity, requirement) =
                self.assignment.slots[slot].candidates[candidate];
            let Some(undo) = self.assignment.assign(item_index, identity, requirement) else {
                continue;
            };
            self.selected
                .push((item_index, requirement.level_sum.map(|sum| sum.group)));
            self.visit(slot + 1);
            self.selected.pop();
            self.assignment.unassign(undo);
        }
        // Skipping this slot keeps the rest of the selection available.
        self.visit(slot + 1);
    }
}
/// What pressing Start Search does with a query, per `docs/search-semantics.md`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartDecision {
    /// Fresh full-range scan that establishes the Target on conclusion.
    Anchor,
    /// Filter the Target Set, then resume the target's uncovered remainder.
    TargetRefine,
    /// Filter the Target Set only; the set and its coverage stay untouched.
    TargetFilter,
    /// Continue the previous detached scan (filter its results, resume its
    /// remainder). The Target is untouched.
    ContinueDetached,
    /// Fresh full-range scan that leaves the Target untouched.
    Detached,
}

impl StartDecision {
    /// The lowercase name every boundary reports, matching the terminology of
    /// `docs/search-semantics.md`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anchor => "anchor",
            Self::TargetRefine => "target-refine",
            Self::TargetFilter => "target-filter",
            Self::ContinueDetached => "continue-detached",
            Self::Detached => "detached",
        }
    }
}

/// The single gate for what Start Search does with `candidate`, per
/// `docs/search-semantics.md`. The Target Set is the anchor: a continuation of
/// the Target Query refines it, a query sharing an item filters it (always
/// from the full set, so loosening a requirement brings seeds back), and
/// anything else scans the full range without touching it — continuing the
/// last detached run when `detached_base` says that is sound.
///
/// `target` is the Target Query, or `None` when there is no Target at all
/// (boot, after Clear, or after a failed first run), which always anchors.
/// `target_set_empty` says the Target Set holds no seeds: it holds nothing
/// worth preserving, so only a continuation with range left to scan
/// (`target_has_uncovered_seeds`) resumes it and every other query re-anchors.
/// `detached_base` is the last concluded run's query when — and only when —
/// that run was itself detached; a failed run is never a continuation base.
///
/// Continuation itself is [`SearchQuery::continues`] and sharing is
/// [`SearchQuery::shares_item`], both consulted here: callers get the whole
/// decision from this one call and must not re-derive either half.
#[must_use]
pub fn decide_start(
    candidate: &SearchQuery,
    target: Option<&SearchQuery>,
    target_set_empty: bool,
    target_has_uncovered_seeds: bool,
    detached_base: Option<&SearchQuery>,
) -> StartDecision {
    let Some(target) = target else {
        return StartDecision::Anchor;
    };
    let continues_target = candidate.continues(target);
    if target_set_empty {
        return if continues_target && target_has_uncovered_seeds {
            StartDecision::TargetRefine
        } else {
            StartDecision::Anchor
        };
    }
    if continues_target {
        return StartDecision::TargetRefine;
    }
    if candidate.shares_item(target) {
        return StartDecision::TargetFilter;
    }
    match detached_base {
        Some(base) if candidate.continues(base) => StartDecision::ContinueDetached,
        _ => StartDecision::Detached,
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
    /// Two members of an identity group outside one alternative group carry
    /// their own constraints; a stack has one anchor and bare copies.
    OverconstrainedIdentityGroup,
    InvalidAlternativeGroup,
    InvalidLevelSum,
    /// A combined-level group member of a family other than rings.
    LevelSumOutsideRings,
    /// The members of the group disagree on the total.
    InconsistentLevelSum {
        group: u8,
    },
    /// The group's total exceeds what its members can carry together.
    UnattainableLevelSum {
        group: u8,
        minimum_total: u16,
        capacity: u16,
    },
    LevelSumInsideAlternative,
}

impl fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InconsistentLevelSum { group } => {
                return write!(
                    formatter,
                    "the items in combined level group {} must agree on the total",
                    group_label(*group)
                );
            }
            Self::UnattainableLevelSum {
                group,
                minimum_total,
                capacity,
            } => {
                return write!(
                    formatter,
                    "combined level group {} needs {minimum_total} levels but its items can \
                     reach at most {capacity}",
                    group_label(*group)
                );
            }
            _ => {}
        }
        let message = match self {
            Self::Empty => "at least one item requirement is needed",
            Self::InvalidDepth => "maximum depth must be between 1 and 24",
            Self::InvalidUpgrade => {
                "upgrade must be between +1 and +4; only a tier-4 weapon, melee or thrown, reaches +5"
            }
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
                "linked item requirements must use the same category"
            }
            Self::OverconstrainedIdentityGroup => {
                "only one linked requirement (or one alternative group) may carry item \
                 constraints; the extra copies must be plain"
            }
            Self::InvalidAlternativeGroup => "alternative group zero is reserved for no group",
            Self::InvalidLevelSum => "combined level groups need a non-zero group and total",
            Self::LevelSumOutsideRings => "levels are only counted together across rings",
            Self::InconsistentLevelSum { .. } | Self::UnattainableLevelSum { .. } => {
                unreachable!("written above")
            }
            Self::LevelSumInsideAlternative => {
                "a combined level group cannot include alternative requirements"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for QueryError {}

#[cfg(test)]
mod tests {
    use crate::catalog::{Effect, ItemId, ItemKind, WeaponCategory, WeaponEffect};
    use crate::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
    use crate::run::RingGems;
    use crate::seed::DungeonSeed;

    use super::{
        EffectRequirement, EffectSet, LevelSum, QueryError, Requirement, SearchQuery,
        TierRequirement, UpgradeRequirement, scout_matches,
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
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
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

        // A different world — floor limit or challenges — breaks continuation
        // outright.
        let mut deeper = base.clone();
        deeper.max_depth = 5;
        assert!(!deeper.continues(&base));
        let mut challenged = base.clone();
        challenged.challenges = crate::challenges::Challenges::DARKNESS;
        assert!(!challenged.continues(&base));

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
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
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
    fn sharing_compares_kinds_and_named_items_only() {
        let query = |kind: ItemKind, item: Option<ItemId>| SearchQuery {
            requirements: vec![Requirement {
                kind,
                weapon_category: None,
                item,
                tier: TierRequirement::Any,
                upgrade: UpgradeRequirement::Any,
                effect: EffectRequirement::Any,
                require_uncursed: false,
                source: None,
                identity_group: None,
                max_depth: None,
                alternative_group: None,
                level_sum: None,
            }],
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
        };
        let any_ring = query(ItemKind::Ring, None);
        let tenacity = query(ItemKind::Ring, Some(ItemId::RingTenacity));
        let greatsword = query(ItemKind::Weapon, Some(ItemId::Greatsword));
        let sword = query(ItemKind::Weapon, Some(ItemId::Sword));

        // A kind-level requirement subsumes every item of its kind, in either
        // direction: sharing is symmetric.
        assert!(any_ring.shares_item(&tenacity));
        assert!(tenacity.shares_item(&any_ring));
        assert!(tenacity.shares_item(&tenacity));

        // Different kinds never share, and neither do two distinct named
        // items of the same kind.
        assert!(!greatsword.shares_item(&any_ring));
        assert!(!greatsword.shares_item(&sword));

        // One shared pair is enough, however many requirements surround it.
        let mut mixed = greatsword.clone();
        mixed.requirements.push(any_ring.requirements[0]);
        assert!(mixed.shares_item(&tenacity));
        assert!(tenacity.shares_item(&mixed));

        // Scope differences are irrelevant: a filter re-verifies from scratch.
        let mut deep_ring = any_ring.clone();
        deep_ring.max_depth = 5;
        assert!(deep_ring.shares_item(&tenacity));
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
        };
        let one = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![world_item(ItemId::Sword, Accessibility::Independent)],
            ring_gems: RingGems::UNSHUFFLED,
        };
        assert!(!query.matches(&one));
        let two = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![
                world_item(ItemId::Sword, Accessibility::Independent),
                world_item(ItemId::Sword, Accessibility::Independent),
            ],
            ring_gems: RingGems::UNSHUFFLED,
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
        };
        let world = |wandmaker| GeneratedWorld {
            quests: QuestSummary {
                wandmaker,
                ..QuestSummary::default()
            },
            seed: DungeonSeed::MIN,
            items: vec![world_item(ItemId::Sword, Accessibility::Independent)],
            ring_gems: RingGems::UNSHUFFLED,
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
            ring_gems: RingGems::UNSHUFFLED,
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
            ring_gems: RingGems::UNSHUFFLED,
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
            ring_gems: RingGems::UNSHUFFLED,
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
            ring_gems: RingGems::UNSHUFFLED,
        };

        let compatible = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::MailArmor)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
        };
        assert!(compatible.matches(&world));

        let incompatible = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::WandFrost)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
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
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
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
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
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
            effect: EffectRequirement::exactly(Effect::Weapon(WeaponEffect::Displacing)),
            require_uncursed: true,
            ..requirement(ItemId::Sword)
        };
        assert_eq!(invalid.validate(), Err(QueryError::UncursedWithCurse));
    }

    #[test]
    fn upgrade_ceilings_follow_the_item_kind_and_tier() {
        let ring = Requirement {
            kind: ItemKind::Ring,
            weapon_category: None,
            item: Some(ItemId::RingSharpshooting),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(4),
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
        };
        assert_eq!(ring.validate(), Ok(()));

        let wand = Requirement {
            kind: ItemKind::Wand,
            weapon_category: None,
            item: Some(ItemId::WandFrost),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(4),
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
        };
        assert_eq!(wand.validate(), Ok(()));
        let five_wand = Requirement {
            upgrade: UpgradeRequirement::Exact(5),
            ..wand
        };
        assert_eq!(five_wand.validate(), Err(QueryError::InvalidUpgrade));

        let armor = Requirement {
            kind: ItemKind::Armor,
            item: Some(ItemId::PlateArmor),
            upgrade: UpgradeRequirement::Exact(5),
            ..wand
        };
        assert_eq!(armor.validate(), Err(QueryError::InvalidUpgrade));
    }

    /// Only the tier-4 weapons are levelled past `+4`, melee and thrown
    /// alike; every other tier stops one short.
    #[test]
    fn only_a_tier_four_weapon_reaches_the_top_upgrade() {
        let wand = Requirement {
            kind: ItemKind::Wand,
            weapon_category: None,
            item: Some(ItemId::WandFrost),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(4),
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
        };
        let battle_axe = Requirement {
            kind: ItemKind::Weapon,
            item: Some(ItemId::BattleAxe),
            upgrade: UpgradeRequirement::AtLeast(5),
            ..wand
        };
        assert_eq!(battle_axe.validate(), Ok(()));
        let six_battle_axe = Requirement {
            upgrade: UpgradeRequirement::Exact(6),
            ..battle_axe
        };
        assert_eq!(six_battle_axe.validate(), Err(QueryError::InvalidUpgrade));
        let javelin = Requirement {
            item: Some(ItemId::Javelin),
            weapon_category: Some(WeaponCategory::Thrown),
            upgrade: UpgradeRequirement::Exact(5),
            ..battle_axe
        };
        assert_eq!(javelin.validate(), Ok(()));

        let sword = Requirement {
            item: Some(ItemId::Sword),
            upgrade: UpgradeRequirement::Exact(5),
            ..battle_axe
        };
        assert_eq!(sword.validate(), Err(QueryError::InvalidUpgrade));
        assert_eq!(
            Requirement {
                upgrade: UpgradeRequirement::Exact(4),
                ..sword
            }
            .validate(),
            Ok(())
        );
        let trident = Requirement {
            item: Some(ItemId::Trident),
            ..sword
        };
        assert_eq!(trident.validate(), Err(QueryError::InvalidUpgrade));

        // A wildcard weapon reaches +5 unless its tier filter rules tier 4 out.
        let any_weapon = Requirement {
            item: None,
            upgrade: UpgradeRequirement::Exact(5),
            ..battle_axe
        };
        assert_eq!(any_weapon.validate(), Ok(()));
        for tier in [
            TierRequirement::Exact(4),
            TierRequirement::AtLeast(4),
            TierRequirement::AtMost(4),
        ] {
            assert_eq!(
                Requirement { tier, ..any_weapon }.validate(),
                Ok(()),
                "{tier:?}"
            );
        }
        for tier in [
            TierRequirement::Exact(5),
            TierRequirement::Exact(2),
            TierRequirement::AtMost(3),
        ] {
            assert_eq!(
                Requirement { tier, ..any_weapon }.validate(),
                Err(QueryError::InvalidUpgrade),
                "{tier:?}"
            );
        }
    }

    #[test]
    fn tier_predicates_match_exact_minimum_and_maximum_tiers() {
        let tier_five = Requirement {
            kind: ItemKind::Weapon,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Exact(5),
            upgrade: UpgradeRequirement::Exact(2),
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
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
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source,
            identity_group: Some(1),
            max_depth: None,
            alternative_group: None,
            level_sum: None,
        };
        let mut query = SearchQuery {
            requirements: vec![
                linked(
                    UpgradeRequirement::Exact(3),
                    Some(ItemSource::WandmakerReward),
                ),
                linked(UpgradeRequirement::Any, None),
                linked(UpgradeRequirement::Any, None),
                Requirement {
                    kind: ItemKind::Wand,
                    weapon_category: None,
                    item: None,
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::Exact(1),
                    effect: EffectRequirement::Any,
                    require_uncursed: false,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                    alternative_group: None,
                    level_sum: None,
                },
            ],
            max_depth: 14,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: true,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
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
            ring_gems: RingGems::UNSHUFFLED,
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
            ring_gems: RingGems::UNSHUFFLED,
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
            ring_gems: RingGems::UNSHUFFLED,
        };
        assert!(query.matches(&no_blacksmith));
    }

    #[test]
    fn a_stack_carries_constraints_on_one_member_only() {
        let linked = |item| Requirement {
            kind: ItemKind::Wand,
            weapon_category: None,
            item,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: Some(1),
            max_depth: None,
            alternative_group: None,
            level_sum: None,
        };
        let query = |members: Vec<Requirement>| SearchQuery {
            requirements: members,
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
        };

        // Two members naming items would describe two different wands forced
        // to be the same wand; the stack model refuses it.
        assert_eq!(
            query(vec![
                linked(Some(ItemId::WandFrost)),
                linked(None),
                linked(Some(ItemId::WandLightning)),
            ])
            .validate(),
            Err(QueryError::OverconstrainedIdentityGroup)
        );
        // One anchor with bare copies is the intended shape.
        assert_eq!(
            query(vec![
                linked(Some(ItemId::WandFrost)),
                linked(None),
                linked(None),
            ])
            .validate(),
            Ok(())
        );
        // Members of different kinds never describe one item.
        assert_eq!(
            query(vec![
                linked(None),
                Requirement {
                    kind: ItemKind::Ring,
                    ..linked(None)
                },
            ])
            .validate(),
            Err(QueryError::InconsistentIdentityGroup)
        );
    }

    #[test]
    fn a_stack_can_anchor_on_a_whole_alternative_group() {
        // "Runic Blade OR War Hammer, plus two more of whichever matched":
        // the anchor unit is the alternative group, the copies are bare.
        let anchor = |item| Requirement {
            item: Some(item),
            upgrade: UpgradeRequirement::AtLeast(1),
            alternative_group: Some(1),
            identity_group: Some(1),
            ..plain(ItemKind::Weapon)
        };
        let copy = Requirement {
            identity_group: Some(1),
            ..plain(ItemKind::Weapon)
        };
        let query = SearchQuery {
            requirements: vec![
                anchor(ItemId::RunicBlade),
                anchor(ItemId::WarHammer),
                copy,
                copy,
            ],
            ..scout_query(Vec::new())
        };
        assert_eq!(query.validate(), Ok(()));
        assert_eq!(query.slot_count(), 3);

        // Three hammers: the group binds to the hammer and the copies follow.
        assert!(query.matches(&scout_world(vec![
            upgraded(ItemId::WarHammer, 1),
            upgraded(ItemId::WarHammer, 0),
            upgraded(ItemId::WarHammer, 0),
        ])));
        // Copies of the wrong identity do not count, whichever member won.
        assert!(!query.matches(&scout_world(vec![
            upgraded(ItemId::WarHammer, 1),
            upgraded(ItemId::WarHammer, 0),
            upgraded(ItemId::RunicBlade, 0),
        ])));
        // An upgraded blade with two more blades matches through the other
        // alternative.
        assert!(query.matches(&scout_world(vec![
            upgraded(ItemId::RunicBlade, 2),
            upgraded(ItemId::RunicBlade, 0),
            upgraded(ItemId::RunicBlade, 0),
        ])));
    }

    fn plain(kind: ItemKind) -> Requirement {
        Requirement {
            kind,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
        }
    }

    fn upgraded(item: ItemId, upgrade: u8) -> WorldItem {
        WorldItem {
            upgrade,
            ..world_item(item, Accessibility::Independent)
        }
    }

    #[test]
    fn effect_sets_hold_one_family_and_match_any_member() {
        let blocking = Effect::Weapon(WeaponEffect::Blocking);
        let grim = Effect::Weapon(WeaponEffect::Grim);
        let thorns = Effect::Armor(crate::catalog::ArmorEffect::Thorns);
        let set = EffectSet::from_effects([blocking, grim]).unwrap();
        assert_eq!(set.count(), 2);
        assert!(set.contains(blocking) && set.contains(grim));
        assert!(!set.contains(thorns));
        assert_eq!(set.effects().collect::<Vec<_>>(), vec![blocking, grim]);
        assert!(EffectSet::from_effects([blocking, thorns]).is_none());
        assert!(EffectSet::from_effects([]).is_none());

        // Every enchantment but no curse; wands and rings carry none.
        let enchantments = EffectSet::enchantments(ItemKind::Weapon).unwrap();
        assert!(enchantments.contains(blocking));
        assert!(!enchantments.contains(Effect::Weapon(WeaponEffect::Annoying)));
        assert!(EffectSet::enchantments(ItemKind::Wand).is_none());
        assert!(EffectSet::single(Effect::Weapon(WeaponEffect::Annoying)).is_curses_only());
        assert_eq!(set.without_curses(), Some(set));
        assert_eq!(
            EffectSet::single(blocking).intersection(set),
            Some(EffectSet::single(blocking))
        );
        assert!(EffectSet::single(thorns).intersection(set).is_none());
        assert!(EffectSet::single(blocking).is_subset_of(set));
        assert!(!set.is_subset_of(EffectSet::single(blocking)));

        let wanted = Requirement {
            effect: EffectRequirement::OneOf(set),
            ..plain(ItemKind::Weapon)
        };
        let mut sword = world_item(ItemId::Sword, Accessibility::Independent);
        assert!(!wanted.matches(&sword));
        sword.effect = Some(grim);
        assert!(wanted.matches(&sword));
        sword.effect = Some(Effect::Weapon(WeaponEffect::Blazing));
        assert!(!wanted.matches(&sword));

        // Validation: the set's family must be the requirement's, and an
        // uncursed item cannot be limited to curses.
        assert_eq!(
            Requirement {
                effect: EffectRequirement::OneOf(set),
                ..plain(ItemKind::Armor)
            }
            .validate(),
            Err(QueryError::EffectKindMismatch)
        );
        assert_eq!(
            Requirement {
                effect: EffectRequirement::OneOf(set),
                ..plain(ItemKind::Ring)
            }
            .validate(),
            Err(QueryError::EffectKindMismatch)
        );
        assert_eq!(
            Requirement {
                effect: EffectRequirement::OneOf(
                    EffectSet::from_effects([
                        Effect::Weapon(WeaponEffect::Annoying),
                        Effect::Weapon(WeaponEffect::Sacrificial),
                    ])
                    .unwrap()
                ),
                require_uncursed: true,
                ..plain(ItemKind::Weapon)
            }
            .validate(),
            Err(QueryError::UncursedWithCurse)
        );
        assert_eq!(
            Requirement {
                effect: EffectRequirement::OneOf(
                    EffectSet::from_effects([Effect::Weapon(WeaponEffect::Annoying), blocking])
                        .unwrap()
                ),
                require_uncursed: true,
                ..plain(ItemKind::Weapon)
            }
            .validate(),
            Ok(())
        );
    }

    #[test]
    fn alternatives_form_one_slot_any_member_can_fill() {
        let spear = Requirement {
            item: Some(ItemId::Spear),
            upgrade: UpgradeRequirement::Exact(3),
            alternative_group: Some(1),
            ..plain(ItemKind::Weapon)
        };
        let shuriken = Requirement {
            item: Some(ItemId::Shuriken),
            upgrade: UpgradeRequirement::Exact(2),
            alternative_group: Some(1),
            ..plain(ItemKind::Weapon)
        };
        let sword = Requirement {
            item: Some(ItemId::Sword),
            upgrade: UpgradeRequirement::Exact(1),
            alternative_group: Some(1),
            ..plain(ItemKind::Weapon)
        };
        let query = SearchQuery {
            requirements: vec![spear, shuriken, sword, plain(ItemKind::Wand)],
            ..scout_query(Vec::new())
        };
        assert_eq!(query.validate(), Ok(()));
        assert_eq!(query.slots(), vec![vec![0, 1, 2], vec![3]]);
        assert_eq!(query.slot_count(), 2);

        // Three members but one slot: two items suffice.
        let wand = upgraded(ItemId::WandFrost, 0);
        assert!(query.matches(&scout_world(vec![upgraded(ItemId::Sword, 1), wand.clone()])));
        assert!(query.matches(&scout_world(vec![
            upgraded(ItemId::Shuriken, 2),
            wand.clone()
        ])));
        assert!(!query.matches(&scout_world(vec![upgraded(ItemId::Sword, 2), wand])));
        assert!(!query.matches(&scout_world(vec![upgraded(ItemId::Sword, 1)])));
        // One item cannot serve the slot and another requirement at once.
        let two_swords = SearchQuery {
            requirements: vec![
                spear,
                sword,
                Requirement {
                    item: Some(ItemId::Sword),
                    ..plain(ItemKind::Weapon)
                },
            ],
            ..scout_query(Vec::new())
        };
        assert!(!two_swords.matches(&scout_world(vec![upgraded(ItemId::Sword, 1)])));
        assert!(two_swords.matches(&scout_world(vec![
            upgraded(ItemId::Sword, 1),
            upgraded(ItemId::Sword, 0)
        ])));

        // The scout counts the group as one requirement.
        let marks = scout_matches(&scout_world(vec![upgraded(ItemId::Sword, 1)]), &query);
        assert_eq!(marks.total_requirements, 2);
        assert_eq!(marks.matched_requirements, 1);
        assert_eq!(marks.matched_indices(), vec![0]);

        // Group zero is reserved, like identity group zero.
        assert_eq!(
            Requirement {
                alternative_group: Some(0),
                ..plain(ItemKind::Wand)
            }
            .validate(),
            Err(QueryError::InvalidAlternativeGroup)
        );
        // Alternatives of one slot may disagree inside an identity group —
        // only one of them is ever assigned — but members of different slots
        // must agree.
        let linked = |item, alternative_group| Requirement {
            item: Some(item),
            identity_group: Some(1),
            alternative_group,
            ..plain(ItemKind::Ring)
        };
        assert_eq!(
            SearchQuery {
                requirements: vec![
                    linked(ItemId::RingMight, Some(1)),
                    linked(ItemId::RingHaste, Some(1)),
                ],
                ..scout_query(Vec::new())
            }
            .validate(),
            Ok(())
        );
        assert_eq!(
            SearchQuery {
                requirements: vec![
                    linked(ItemId::RingMight, Some(1)),
                    linked(ItemId::RingHaste, None),
                ],
                ..scout_query(Vec::new())
            }
            .validate(),
            Err(QueryError::OverconstrainedIdentityGroup)
        );
    }

    #[test]
    fn combined_level_groups_sum_distinct_items_and_members_are_optional() {
        let might = |level_sum| Requirement {
            item: Some(ItemId::RingMight),
            level_sum: Some(level_sum),
            ..plain(ItemKind::Ring)
        };
        let pair = |minimum_total| SearchQuery {
            requirements: vec![
                might(LevelSum {
                    group: 1,
                    minimum_total,
                });
                2
            ],
            ..scout_query(Vec::new())
        };
        let rings = |upgrades: &[u8]| {
            scout_world(
                upgrades
                    .iter()
                    .map(|upgrade| upgraded(ItemId::RingMight, *upgrade))
                    .collect(),
            )
        };
        // +3 strength from Rings of Might: one +2 ring, or a +0 and a +1.
        assert!(pair(3).matches(&rings(&[2])));
        assert!(pair(3).matches(&rings(&[0, 1])));
        assert!(!pair(3).matches(&rings(&[1])));
        assert!(!pair(3).matches(&rings(&[0])));
        // Distinct items only: a pair cannot count one ring twice.
        assert!(pair(8).matches(&rings(&[3, 3])));
        assert!(!pair(8).matches(&rings(&[3, 2])));
        assert!(!pair(8).matches(&rings(&[4])));
        // Backtracking over assignments: the +0 and +1 pair falls short,
        // the +1 and +3 pair carries it.
        assert!(pair(6).matches(&rings(&[0, 1, 3])));
        assert!(!pair(7).matches(&rings(&[0, 1, 3])));

        // The scout counts the whole group as one condition and flags every
        // contributing item once the total is met.
        let met = scout_matches(&rings(&[1, 3]), &pair(6));
        assert_eq!(met.total_requirements, 1);
        assert_eq!(met.matched_requirements, 1);
        assert_eq!(met.matched_indices(), vec![0, 1]);
        let lone = scout_matches(&rings(&[3]), &pair(4));
        assert_eq!(lone.matched_requirements, 1);
        assert_eq!(lone.matched_indices(), vec![0]);
        let short = scout_matches(&rings(&[1]), &pair(4));
        assert_eq!(short.matched_requirements, 0);
        assert!(short.matched_indices().is_empty());
    }

    #[test]
    fn combined_level_validation_caps_totals_and_admits_rings_only() {
        let might = |level_sum| Requirement {
            item: Some(ItemId::RingMight),
            level_sum: Some(level_sum),
            ..plain(ItemKind::Ring)
        };
        let pair = |minimum_total| SearchQuery {
            requirements: vec![
                might(LevelSum {
                    group: 1,
                    minimum_total,
                });
                2
            ],
            ..scout_query(Vec::new())
        };
        // A ring reaches +4 (five levels), but only one per world — the Imp
        // vault's prize; every other ring stops at +2 (three levels). Two
        // rings therefore reach eight levels together, not ten.
        assert_eq!(pair(3).validate(), Ok(()));
        assert_eq!(pair(8).validate(), Ok(()));
        assert_eq!(
            pair(9).validate(),
            Err(QueryError::UnattainableLevelSum {
                group: 1,
                minimum_total: 9,
                capacity: 8,
            })
        );
        assert_eq!(
            pair(9).validate().unwrap_err().to_string(),
            "combined level group A needs 9 levels but its items can reach at most 8"
        );
        // Only rings count levels together.
        assert_eq!(
            Requirement {
                item: Some(ItemId::Sword),
                level_sum: Some(LevelSum {
                    group: 1,
                    minimum_total: 3,
                }),
                ..plain(ItemKind::Weapon)
            }
            .validate(),
            Err(QueryError::LevelSumOutsideRings)
        );

        // Members agree on the total, sums need a group and a total, and a
        // sum cannot live inside an alternative group.
        assert_eq!(
            SearchQuery {
                requirements: vec![
                    might(LevelSum {
                        group: 1,
                        minimum_total: 2
                    }),
                    might(LevelSum {
                        group: 1,
                        minimum_total: 3
                    }),
                ],
                ..scout_query(Vec::new())
            }
            .validate(),
            Err(QueryError::InconsistentLevelSum { group: 1 })
        );
        assert_eq!(
            might(LevelSum {
                group: 0,
                minimum_total: 2
            })
            .validate(),
            Err(QueryError::InvalidLevelSum)
        );
        assert_eq!(
            might(LevelSum {
                group: 1,
                minimum_total: 0
            })
            .validate(),
            Err(QueryError::InvalidLevelSum)
        );
        assert_eq!(
            Requirement {
                alternative_group: Some(1),
                ..might(LevelSum {
                    group: 1,
                    minimum_total: 1
                })
            }
            .validate(),
            Err(QueryError::LevelSumInsideAlternative)
        );
    }

    #[test]
    fn continuation_compares_slots_and_carries_sum_groups() {
        let spear = Requirement {
            item: Some(ItemId::Spear),
            alternative_group: Some(1),
            ..plain(ItemKind::Weapon)
        };
        let sword = Requirement {
            item: Some(ItemId::Sword),
            alternative_group: Some(1),
            ..plain(ItemKind::Weapon)
        };
        let mace = Requirement {
            item: Some(ItemId::Mace),
            alternative_group: Some(1),
            ..plain(ItemKind::Weapon)
        };
        let query = |requirements| SearchQuery {
            requirements,
            ..scout_query(Vec::new())
        };
        let either = query(vec![spear, sword]);
        // Dropping an alternative narrows; adding one widens.
        assert!(either.continues(&either));
        assert!(
            query(vec![Requirement {
                alternative_group: None,
                ..spear
            }])
            .continues(&either)
        );
        assert!(query(vec![spear, sword]).continues(&query(vec![spear, sword, mace])));
        assert!(!query(vec![spear, sword, mace]).continues(&either));
        assert!(!either.continues(&query(vec![Requirement {
            alternative_group: None,
            ..spear
        }])));
        // A group covers a wildcard of its kind, and each member must imply
        // some base member.
        assert!(either.continues(&query(vec![plain(ItemKind::Weapon)])));
        assert!(!either.continues(&query(vec![plain(ItemKind::Wand)])));
        assert!(
            !query(vec![
                spear,
                Requirement {
                    item: Some(ItemId::WandFrost),
                    alternative_group: Some(1),
                    ..plain(ItemKind::Wand)
                }
            ])
            .continues(&query(vec![plain(ItemKind::Weapon)]))
        );

        let might = |sum: Option<LevelSum>| Requirement {
            item: Some(ItemId::RingMight),
            level_sum: sum,
            ..plain(ItemKind::Ring)
        };
        let sum = |group, minimum_total| {
            Some(LevelSum {
                group,
                minimum_total,
            })
        };
        let pair = |total| query(vec![might(sum(1, total)); 2]);
        let plain_pair = query(vec![might(None); 2]);
        // Raising a total narrows; lowering it widens. Adding a total to a
        // plain pair also *widens* — its members become optional, so one
        // strong ring now suffices where two were demanded — and dropping
        // one widens the totals away; neither direction continues.
        assert!(pair(4).continues(&pair(4)));
        assert!(pair(5).continues(&pair(4)));
        assert!(!pair(4).continues(&plain_pair));
        assert!(!pair(3).continues(&pair(4)));
        assert!(!plain_pair.continues(&pair(4)));
        // The base group must map onto exactly one candidate group of the
        // same size: a third member would let the total spread thinner.
        assert!(query(vec![might(sum(2, 4)); 2]).continues(&pair(4)));
        assert!(!query(vec![might(sum(1, 4)); 3]).continues(&pair(4)));
        assert!(!query(vec![might(sum(1, 4)), might(sum(2, 4))]).continues(&pair(4)));
        // Extra requirements alongside the carried group are fine.
        let mut extended = pair(4);
        extended.requirements.push(plain(ItemKind::Wand));
        assert!(extended.continues(&pair(4)));
    }

    fn scout_query(requirements: Vec<Requirement>) -> SearchQuery {
        SearchQuery {
            requirements,
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
        }
    }

    fn scout_world(items: Vec<WorldItem>) -> GeneratedWorld {
        GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items,
            ring_gems: RingGems::UNSHUFFLED,
        }
    }

    fn any_requirement(kind: ItemKind) -> Requirement {
        Requirement {
            kind,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
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
