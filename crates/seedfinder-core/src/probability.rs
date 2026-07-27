//! Query probability estimates derived from the measured v3.3.8 item supply.
//!
//! The estimate answers "what fraction of seeds satisfies this query", not "how
//! likely is one item to match", so it has to know how much equipment a run
//! actually offers. That supply lives in [`crate::probability_tables`]: expected
//! reward slots per floor and source, with the upgrade, curse, enchantment, and
//! tier distributions each source produces.
//!
//! Every requirement becomes a filter over those slots, evaluated only down to
//! its own floor limit. Scattered drops arrive as a steady stream rather than a
//! Poisson one, because the generator deals item categories from a decrementing
//! deck; quests and shops place a fixed number of slots on a single floor. Slots
//! holding mutually exclusive prizes count once, since a run can only carry one
//! of them out.
//!
//! Requirements are then matched one-to-one onto slots, so three wands are not
//! scored as one wand three times and the Wandmaker's single prize is not spent
//! twice. Requirements linked to one identity are summed over the identities
//! they could share, discounted by the deck-driven scarcity of duplicates.
//!
//! Known simplifications, all of which make the estimate slightly optimistic:
//! challenges shift item placement but are ignored; rewards that exclude one
//! another across families, like the Ghost's weapon-or-armor choice, are counted
//! as independently obtainable; and a family carrying more requirements than
//! one matching resolves keeps only its scarcest ones.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::catalog::{Effect, ItemId, ItemKind, item};
use crate::generator::{
    ARMOR_ITEMS, RING_ITEMS, WAND_ITEMS, WEAPON_TIER_1_ITEMS, WEAPON_TIER_2_ITEMS,
    WEAPON_TIER_3_ITEMS, WEAPON_TIER_4_ITEMS, WEAPON_TIER_5_ITEMS,
};
use crate::model::ItemSource;
use crate::probability_tables::{
    DEEPEST_FLOOR, DEPTHS, FLOOR_SETS, HIGHEST_TABLED_UPGRADE, HIGHEST_TIER, IDENTITY_REPEAT_LIMIT,
    IDENTITY_REPEATS, SLOT_SPREAD, Supply, TIERS, appears_once, kind_index, missile_tier,
    missile_tier_items, spread_index, supply_for,
};
use crate::query::{Requirement, SearchQuery, UpgradeRequirement};

/// Estimates the fraction of seeds satisfying a query.
///
/// The result is fixed for a search: observed results never feed back into it.
#[must_use]
pub fn estimate_match_probability(query: &SearchQuery) -> f64 {
    let mut linked: BTreeMap<u8, Vec<Requirement>> = BTreeMap::new();
    let mut independent: Vec<Requirement> = Vec::new();
    for requirement in &query.requirements {
        match requirement.identity_group {
            Some(group) => linked.entry(group).or_default().push(*requirement),
            None => independent.push(*requirement),
        }
    }

    let mut probability = blacksmith_probability(query);
    for members in linked.into_values() {
        // A linked group that names its item constrains nothing extra: every
        // member already matches that one identity.
        if let Some(pinned) = members.iter().find_map(|member| member.item) {
            independent.extend(members.into_iter().map(|member| Requirement {
                item: Some(pinned),
                ..member
            }));
        } else {
            probability *= linked_probability(query, &members);
        }
    }
    probability *= competing_probability(query, &independent);
    if probability <= 0.0 {
        0.0
    } else {
        probability.min(1.0)
    }
}

/// Probability that an accessible Blacksmith exists within the search depth.
fn blacksmith_probability(query: &SearchQuery) -> f64 {
    if !query.require_blacksmith {
        return 1.0;
    }
    supply_for(ItemKind::Armor)
        .filter(|supply| supply.source == ItemSource::BlacksmithReward)
        .map(|supply| {
            supply.depth_slots[..usize::from(query.max_depth).min(DEPTHS)]
                .iter()
                .map(|slots| f64::from(*slots))
                .sum::<f64>()
                / f64::from(supply.bundle.max(1))
        })
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

/// Probability that requirements sharing an identity group are all satisfied.
///
/// Every member has to resolve to the same item, so the group is evaluated once
/// per candidate identity and the results combined. Same-identity duplicates are
/// rarer than independent draws suggest because the generator deals items from
/// decrementing decks, which [`IDENTITY_REPEATS`] corrects for.
fn linked_probability(query: &SearchQuery, members: &[Requirement]) -> f64 {
    let Some(kind) = members.first().map(|member| member.kind) else {
        return 1.0;
    };
    let mut none = 1.0;
    for identity in identities(kind) {
        let shared = family_probability(query, members, Some(identity));
        none *= 1.0 - shared.clamp(0.0, 1.0);
    }
    1.0 - none
}

/// How much rarer it is to hold several items of one identity than independent
/// draws suggest.
///
/// The generator deals each family from a decrementing deck, so drawing a wand
/// makes the same wand less likely next time. Requirements that all name one
/// item — or that are linked to share one — feel that suppression.
fn repeat_correction(ordered: &[Predicate]) -> f64 {
    let Some(kind) = ordered.first().map(|predicate| predicate.kind) else {
        return 1.0;
    };
    let copies = ordered
        .iter()
        .filter_map(|predicate| predicate.item)
        .fold(
            BTreeMap::new(),
            |mut counts: BTreeMap<ItemId, usize>, item| {
                *counts.entry(item).or_default() += 1;
                counts
            },
        )
        .into_values()
        .max()
        .unwrap_or(1);
    if copies < 2 {
        return 1.0;
    }
    let depth = ordered
        .iter()
        .map(|predicate| predicate.max_depth)
        .max()
        .unwrap_or(DEEPEST_FLOOR);
    let copies = copies.min(IDENTITY_REPEAT_LIMIT) - 1;
    let depth = usize::from(depth).clamp(1, DEPTHS) - 1;
    f64::from(IDENTITY_REPEATS[kind_index(kind)][copies][depth])
}

/// Probability that every requirement outside a linked group is satisfied at
/// once.
///
/// Items of different families never compete, so each family is resolved on its
/// own and the answers multiply.
fn competing_probability(query: &SearchQuery, requirements: &[Requirement]) -> f64 {
    let mut families: BTreeMap<usize, Vec<Requirement>> = BTreeMap::new();
    for requirement in requirements {
        families
            .entry(kind_index(requirement.kind))
            .or_default()
            .push(*requirement);
    }
    families
        .into_values()
        .map(|family| family_probability(query, &family, None))
        .product()
}

/// Probability that every requirement on one equipment family is satisfied by a
/// distinct item.
///
/// Each reward slot in the dungeon covers some set of the requirements, and the
/// query succeeds exactly when the slots can be matched one-to-one onto the
/// requirements. By Hall's theorem that holds precisely when no set of
/// requirements outnumbers the slots covering it, which is what
/// [`covers_every_requirement`] checks.
///
/// Working in coverage sets rather than per requirement is what stops one slot
/// from being spent twice: the Wandmaker's single prize can be the `+3` wand a
/// query asks for or one of its plain wands, never both.
fn family_probability(
    query: &SearchQuery,
    requirements: &[Requirement],
    identity: Option<ItemId>,
) -> f64 {
    let mut ordered: Vec<Predicate> = requirements
        .iter()
        .map(|requirement| Predicate::of(*requirement, identity).within(query, requirement))
        .collect();
    // Keeping the scarcest requirements first makes truncation lose the least.
    ordered.sort_by(|left, right| {
        expected_slots(left)
            .partial_cmp(&expected_slots(right))
            .unwrap_or(Ordering::Equal)
    });
    ordered.truncate(MAX_REQUIREMENTS);
    let Some(kind) = ordered.first().map(|predicate| predicate.kind) else {
        return 1.0;
    };
    let wanted = ordered.len();
    let coverages = 1 << wanted;
    // Every set of requirements narrows to one filter, matched by the items
    // that could serve all of them at once.
    let shared: Vec<Option<Predicate>> = (0..coverages)
        .map(|coverage| narrow(&ordered, coverage))
        .collect();

    let mut scattered = vec![Spread::default(); coverages];
    let mut slots: Vec<Slot> = Vec::new();
    for supply in supply_for(kind) {
        // A quest runs once per dungeon, so its floors are alternatives: the
        // requirements it could cover are pooled across them, not repeated.
        let mut once = vec![0.0; coverages];
        for depth in 1..=DEPTHS {
            let available = f64::from(supply.depth_slots[depth - 1]);
            if available <= 0.0 {
                continue;
            }
            let covered = coverage_shares(&shared, supply, depth);
            if covered.iter().skip(1).all(|share| *share <= 0.0) {
                continue;
            }
            if supply.bundle == 0 {
                for (coverage, share) in covered.iter().enumerate() {
                    let steadiness = f64::from(
                        SLOT_SPREAD[spread_index(kind, supply.missile)]
                            [window(shared[coverage].as_ref())],
                    );
                    scattered[coverage].add(available * share, steadiness, *share);
                }
                continue;
            }
            let appearances = available / f64::from(supply.bundle);
            if appears_once(supply.source) {
                for (coverage, share) in covered.iter().enumerate() {
                    once[coverage] += appearances * share;
                }
                continue;
            }
            // A shop restocks on every shop floor, so each is its own chance.
            for _ in 0..supply.bundle {
                slots.push(Slot {
                    covers: covered.iter().map(|share| appearances * share).collect(),
                });
            }
        }
        if once.iter().skip(1).any(|share| *share > 0.0) {
            for _ in 0..supply.bundle {
                slots.push(Slot {
                    covers: once.clone(),
                });
            }
        }
    }
    let matched = matching_probability(wanted, &scattered, &slots);
    (matched * repeat_correction(&ordered)).clamp(0.0, 1.0)
}

/// One reward slot, with the chance it covers each set of requirements.
struct Slot {
    covers: Vec<f64>,
}

/// How many scattered slots cover one set of requirements, and how widely that
/// count varies.
#[derive(Clone, Copy, Default)]
struct Spread {
    mean: f64,
    variance: f64,
}

impl Spread {
    /// Folds in `mean` slots drawn from a supply whose counts have the given
    /// spread, of which a `share` covers this set.
    ///
    /// Picking a share out of a stream only carries over that stream's
    /// steadiness in proportion to the share, which is why a narrow filter over
    /// a steady supply still looks like an ordinary random arrival.
    fn add(&mut self, mean: f64, spread: f64, share: f64) {
        self.mean += mean;
        self.variance += mean * (1.0 + share * (spread - 1.0)).max(0.0);
    }
}

/// Deepest floor a coverage set can draw on, as an index into [`SLOT_SPREAD`].
fn window(narrowed: Option<&Predicate>) -> usize {
    narrowed
        .map_or(DEPTHS, |narrowed| usize::from(narrowed.max_depth))
        .clamp(1, DEPTHS)
        - 1
}

/// The filter matching items that satisfy every requirement in `coverage`.
fn narrow(ordered: &[Predicate], coverage: usize) -> Option<Predicate> {
    let mut narrowed: Option<Predicate> = None;
    for (index, predicate) in ordered.iter().enumerate() {
        if coverage & (1 << index) == 0 {
            continue;
        }
        narrowed = Some(match narrowed {
            None => *predicate,
            Some(narrowed) => narrowed.intersect(*predicate)?,
        });
    }
    narrowed
}

/// Chance that one slot of `supply` at `depth` covers exactly each set of
/// requirements.
///
/// The filters overlap, so the chance of satisfying a given set is not the
/// chance of satisfying that set and nothing more. Inverting over the subset
/// lattice turns the first into the second.
fn coverage_shares(shared: &[Option<Predicate>], supply: &Supply, depth: usize) -> Vec<f64> {
    let mut exact: Vec<f64> = shared
        .iter()
        .map(|narrowed| narrowed.map_or(0.0, |narrowed| narrowed.slot_probability(supply, depth)))
        .collect();
    exact[0] = 1.0;
    for requirement in 0..exact.len().trailing_zeros() {
        let bit = 1_usize << requirement;
        for coverage in 0..exact.len() {
            if coverage & bit == 0 {
                exact[coverage] -= exact[coverage | bit];
            }
        }
    }
    for share in &mut exact {
        *share = share.max(0.0);
    }
    exact
}

/// Probability that the slots can be matched one-to-one onto the requirements.
///
/// Scattered drops arrive as independent Poisson counts per coverage set; quest
/// and shop slots are then folded in one at a time, each covering one set or
/// nothing. The surviving states are the ones Hall's theorem admits.
fn matching_probability(wanted: usize, scattered: &[Spread], slots: &[Slot]) -> f64 {
    let cap = wanted.min(MAX_COUNT);
    let mut states = BTreeMap::from([(0_u128, 1.0)]);
    for (coverage, spread) in scattered.iter().enumerate().skip(1) {
        if spread.mean <= 0.0 {
            continue;
        }
        let arrivals = arrival_counts(*spread, cap);
        let mut next = BTreeMap::new();
        for (state, reached) in &states {
            for (count, share) in arrivals.iter().enumerate() {
                if *share > 0.0 {
                    accumulate(
                        &mut next,
                        add_count(*state, coverage, count, cap),
                        reached * share,
                    );
                }
            }
        }
        states = prune(next);
    }
    for slot in slots {
        let missed = (1.0 - slot.covers.iter().skip(1).sum::<f64>()).max(0.0);
        let mut next = BTreeMap::new();
        for (state, reached) in &states {
            for (coverage, landed) in slot.covers.iter().enumerate().skip(1) {
                if *landed > 0.0 {
                    accumulate(
                        &mut next,
                        add_count(*state, coverage, 1, cap),
                        reached * landed,
                    );
                }
            }
            accumulate(&mut next, *state, reached * missed);
        }
        states = prune(next);
    }
    states
        .iter()
        .filter(|(state, _)| covers_every_requirement(**state, wanted))
        .map(|(_, reached)| reached)
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

/// Hall's condition: no set of requirements may outnumber the slots covering it.
fn covers_every_requirement(state: u128, wanted: usize) -> bool {
    (1..1_usize << wanted).all(|group| {
        let held: u32 = (1..1_usize << wanted)
            .filter(|coverage| coverage & group != 0)
            .map(|coverage| slot_count(state, coverage))
            .sum();
        held >= group.count_ones()
    })
}

/// Requirements on one family resolved together. Longer lists keep their
/// scarcest members, which dominate the estimate, and coverage sets stay
/// packable into a single state.
const MAX_REQUIREMENTS: usize = 5;

/// Slots per coverage set are packed four bits each.
const MAX_COUNT: usize = 15;

/// Bits each coverage set's slot count occupies in a packed state.
const BITS_PER_COVERAGE: usize = 4;

/// Mask covering one coverage set's packed slot count.
const COVERAGE_MASK: u128 = 0xF;

/// States below this carry no weight worth the work of tracking them.
const STATE_FLOOR: f64 = 1e-15;

/// Largest number of packed states kept between steps.
const STATE_LIMIT: usize = 4096;

fn coverage_shift(coverage: usize) -> u32 {
    u32::try_from((coverage - 1) * BITS_PER_COVERAGE).unwrap_or(0)
}

fn slot_count(state: u128, coverage: usize) -> u32 {
    u32::try_from((state >> coverage_shift(coverage)) & COVERAGE_MASK).unwrap_or(0)
}

fn add_count(state: u128, coverage: usize, count: usize, cap: usize) -> u128 {
    let shift = coverage_shift(coverage);
    let current = usize::try_from((state >> shift) & COVERAGE_MASK).unwrap_or(0);
    let raised = (current + count).min(cap);
    (state & !(COVERAGE_MASK << shift)) | (u128::try_from(raised).unwrap_or(0) << shift)
}

fn accumulate(states: &mut BTreeMap<u128, f64>, state: u128, reached: f64) {
    if reached > 0.0 {
        *states.entry(state).or_insert(0.0) += reached;
    }
}

fn prune(states: BTreeMap<u128, f64>) -> BTreeMap<u128, f64> {
    let mut kept: BTreeMap<u128, f64> = states
        .into_iter()
        .filter(|(_, reached)| *reached > STATE_FLOOR)
        .collect();
    if kept.len() > STATE_LIMIT {
        let mut weights: Vec<f64> = kept.values().copied().collect();
        weights.sort_by(|left, right| right.partial_cmp(left).unwrap_or(Ordering::Equal));
        let floor = weights[STATE_LIMIT];
        kept.retain(|_, reached| *reached > floor);
    }
    kept
}

/// Expected number of slots one requirement can draw on.
fn expected_slots(predicate: &Predicate) -> f64 {
    supply_for(predicate.kind)
        .map(|supply| {
            (1..=usize::from(predicate.max_depth).min(DEPTHS))
                .map(|depth| {
                    f64::from(supply.depth_slots[depth - 1])
                        * predicate.slot_probability(supply, depth)
                })
                .sum::<f64>()
        })
        .sum()
}

/// One requirement reduced to the filters the supply tables can answer.
///
/// Tiers and upgrades become bit sets so that requirements can be intersected:
/// the matching needs to know which of them one item could serve at once.
#[derive(Clone, Copy, Debug)]
struct Predicate {
    kind: ItemKind,
    item: Option<ItemId>,
    tiers: u8,
    upgrades: u8,
    effect: Option<Effect>,
    require_uncursed: bool,
    source: Option<ItemSource>,
    max_depth: u8,
    exclude_blacksmith: bool,
    fast_mode: bool,
}

impl Predicate {
    fn of(requirement: Requirement, identity: Option<ItemId>) -> Self {
        let mut tiers = 0;
        for tier in 1..=HIGHEST_TIER {
            if requirement.tier.matches(Some(tier)) {
                tiers |= 1 << (tier - 1);
            }
        }
        let mut upgrades = 0;
        for upgrade in 0..=HIGHEST_TABLED_UPGRADE {
            let matches = match requirement.upgrade {
                UpgradeRequirement::Any => true,
                UpgradeRequirement::Exact(wanted) => upgrade == wanted,
                UpgradeRequirement::AtLeast(minimum) => upgrade >= minimum,
            };
            if matches {
                upgrades |= 1 << upgrade;
            }
        }
        Self {
            kind: requirement.kind,
            item: identity.or(requirement.item),
            tiers,
            upgrades,
            effect: requirement.effect,
            require_uncursed: requirement.require_uncursed,
            source: requirement.source,
            max_depth: requirement.max_depth.unwrap_or(DEEPEST_FLOOR),
            exclude_blacksmith: false,
            fast_mode: false,
        }
    }

    /// Narrows the filter with the query-wide settings.
    fn within(mut self, query: &SearchQuery, requirement: &Requirement) -> Self {
        self.max_depth = effective_depth(query, requirement);
        self.exclude_blacksmith = query.exclude_blacksmith_rewards;
        self.fast_mode = query.fast_mode;
        self
    }

    /// The filter matching exactly the items both accept, or `None` when no item
    /// can satisfy both.
    fn intersect(self, other: Self) -> Option<Self> {
        if self.kind != other.kind {
            return None;
        }
        let item = match (self.item, other.item) {
            (Some(left), Some(right)) if left != right => return None,
            (left, right) => left.or(right),
        };
        let source = match (self.source, other.source) {
            (Some(left), Some(right)) if left != right => return None,
            (left, right) => left.or(right),
        };
        let effect = match (self.effect, other.effect) {
            (Some(left), Some(right)) if left != right => return None,
            (left, right) => left.or(right),
        };
        let tiers = self.tiers & other.tiers;
        let upgrades = self.upgrades & other.upgrades;
        let require_uncursed = self.require_uncursed || other.require_uncursed;
        if tiers == 0 || upgrades == 0 || (require_uncursed && effect.is_some_and(Effect::is_curse))
        {
            return None;
        }
        Some(Self {
            kind: self.kind,
            item,
            tiers,
            upgrades,
            effect,
            require_uncursed,
            source,
            max_depth: self.max_depth.min(other.max_depth),
            exclude_blacksmith: self.exclude_blacksmith || other.exclude_blacksmith,
            fast_mode: self.fast_mode || other.fast_mode,
        })
    }

    /// Probability that one reward slot of `supply` on `depth` satisfies this
    /// filter.
    ///
    /// A slot holding mutually exclusive alternatives matches when any one of
    /// them does, since the query is free to claim whichever qualifies.
    fn slot_probability(self, supply: &Supply, depth: usize) -> f64 {
        if usize::from(self.max_depth) < depth
            || self.source.is_some_and(|wanted| wanted != supply.source)
            || (self.exclude_blacksmith && supply.source == ItemSource::BlacksmithReward)
        {
            return 0.0;
        }
        let matching = self.identity_probability(supply, depth)
            * self.upgrade_probability(supply)
            * self.effect_probability(supply)
            * self.uncursed_probability(supply);
        1.0 - (1.0 - matching).powf(f64::from(supply.options))
    }

    fn identity_probability(self, supply: &Supply, depth: usize) -> f64 {
        let tiers = &supply.tiers[((depth - 1) / 5).min(FLOOR_SETS - 1)];
        match (self.kind, self.item) {
            (ItemKind::Weapon, Some(wanted)) => {
                let Some((tier, siblings, missile)) = weapon_family(wanted) else {
                    return 0.0;
                };
                if missile != supply.missile || self.tiers & (1 << (tier - 1)) == 0 {
                    return 0.0;
                }
                f64::from(tiers[usize::from(tier) - 1]) / tally(siblings)
            }
            // One generic armor exists per tier, so identity and tier coincide.
            (ItemKind::Armor, Some(wanted)) => item(wanted)
                .tier
                .filter(|tier| ARMOR_ITEMS.contains(&wanted) && self.tiers & (1 << (tier - 1)) != 0)
                .map_or(0.0, |tier| f64::from(tiers[usize::from(tier) - 1])),
            (ItemKind::Weapon | ItemKind::Armor, None) => self.tier_probability(tiers),
            (ItemKind::Wand, Some(wanted)) => {
                if WAND_ITEMS.contains(&wanted) {
                    1.0 / tally(WAND_ITEMS.len())
                } else {
                    0.0
                }
            }
            (ItemKind::Ring, Some(wanted)) => {
                if RING_ITEMS.iter().any(|ring| ring.item_id() == wanted) {
                    1.0 / tally(RING_ITEMS.len())
                } else {
                    0.0
                }
            }
            (ItemKind::Wand | ItemKind::Ring, None) => 1.0,
        }
    }

    fn tier_probability(self, tiers: &[f32; TIERS]) -> f64 {
        tiers
            .iter()
            .enumerate()
            .filter(|(index, _)| self.tiers & (1 << index) != 0)
            .map(|(_, share)| f64::from(*share))
            .sum()
    }

    fn upgrade_probability(self, supply: &Supply) -> f64 {
        let mut allowed = self.upgrades;
        if self.fast_mode && fast_mode_skips(supply.source, self.kind) {
            allowed &= (1 << (FAST_MODE_UPGRADE_CAP + 1)) - 1;
        }
        supply
            .upgrades
            .iter()
            .enumerate()
            .filter(|(upgrade, _)| allowed & (1 << upgrade) != 0)
            .map(|(_, share)| f64::from(*share))
            .sum()
    }

    fn effect_probability(self, supply: &Supply) -> f64 {
        match self.effect {
            None => 1.0,
            Some(effect) if effect.is_curse() => f64::from(supply.cursed) / f64::from(CURSE_COUNT),
            Some(Effect::Weapon(effect)) => {
                f64::from(supply.enchanted) * rarity_probability(effect as u8)
            }
            Some(Effect::Armor(effect)) => {
                f64::from(supply.enchanted) * rarity_probability(effect as u8)
            }
        }
    }

    fn uncursed_probability(self, supply: &Supply) -> f64 {
        if !self.require_uncursed {
            return 1.0;
        }
        match self.effect {
            Some(effect) if effect.is_curse() => 0.0,
            // Positive enchantments and glyphs are generated only on clean items.
            Some(_) => 1.0,
            None => 1.0 - f64::from(supply.cursed),
        }
    }
}

fn effective_depth(query: &SearchQuery, requirement: &Requirement) -> u8 {
    requirement
        .max_depth
        .map_or(query.max_depth, |limit| limit.min(query.max_depth))
}

/// Tier, number of identities sharing that tier, and whether the weapon is
/// thrown. `None` for anything the generator never produces.
fn weapon_family(wanted: ItemId) -> Option<(u8, usize, bool)> {
    if let Some(tier) = melee_tier(wanted) {
        return Some((tier, melee_tier_items(tier).iter().flatten().count(), false));
    }
    missile_tier(wanted).map(|tier| {
        let siblings = missile_tier_items(tier)
            .iter()
            .filter(|kind| kind.item_id().is_some())
            .count();
        (tier, siblings, true)
    })
}

fn melee_tier(wanted: ItemId) -> Option<u8> {
    (1..=5).find(|tier| melee_tier_items(*tier).contains(&Some(wanted)))
}

fn melee_tier_items(tier: u8) -> &'static [Option<ItemId>] {
    match tier {
        1 => &WEAPON_TIER_1_ITEMS,
        2 => &WEAPON_TIER_2_ITEMS,
        3 => &WEAPON_TIER_3_ITEMS,
        4 => &WEAPON_TIER_4_ITEMS,
        _ => &WEAPON_TIER_5_ITEMS,
    }
}

/// Every identity a family can actually generate.
fn identities(kind: ItemKind) -> Vec<ItemId> {
    match kind {
        ItemKind::Weapon => (1..=5)
            .flat_map(|tier| melee_tier_items(tier).iter().flatten().copied())
            .chain((1..=5).flat_map(|tier| {
                missile_tier_items(tier)
                    .iter()
                    .filter_map(|kind| kind.item_id())
            }))
            .collect(),
        ItemKind::Armor => ARMOR_ITEMS.to_vec(),
        ItemKind::Wand => WAND_ITEMS.to_vec(),
        ItemKind::Ring => RING_ITEMS.iter().map(|ring| ring.item_id()).collect(),
    }
}

/// Fast mode drops the Crypt and Sacrificial-fire +3 prizes, making +3 weapon
/// and armor requirements quest-only. See [`crate::feasibility`].
const FAST_MODE_UPGRADE_CAP: u8 = 2;

const fn fast_mode_skips(source: ItemSource, kind: ItemKind) -> bool {
    matches!(
        (source, kind),
        (ItemSource::Tomb, ItemKind::Armor) | (ItemSource::SacrificialFire, ItemKind::Weapon)
    )
}

/// Curses are drawn uniformly; both families define exactly eight.
const CURSE_COUNT: u32 = 8;

/// Enchantments and glyphs share one rarity split: four common, six uncommon,
/// three rare.
fn rarity_probability(effect: u8) -> f64 {
    match effect {
        0..=3 => 0.50 / 4.0,
        4..=9 => 0.40 / 6.0,
        10..=12 => 0.10 / 3.0,
        _ => 0.0,
    }
}

/// Table sizes and item counts are small enough to be exact in `f64`.
#[allow(clippy::cast_precision_loss)]
fn tally(count: usize) -> f64 {
    count as f64
}

/// Chances of zero through `cap` slots arriving, with everything past the cap
/// folded into the last bucket.
///
/// A steadier-than-random stream is a run of independent chances rather than a
/// Poisson process: the same average, but far less likely to hand over three
/// items where one was expected. Matching the spread picks how many chances
/// that run holds.
fn arrival_counts(spread: Spread, cap: usize) -> Vec<f64> {
    let dispersion = (spread.variance / spread.mean).clamp(0.0, 1.0);
    let chance = 1.0 - dispersion;
    let chances = spread.mean / chance;
    if chance <= 0.0 || chances > MAX_CHANCES {
        return poisson_counts(spread.mean, cap);
    }
    let whole = chances.floor();
    let mut counts = binomial_counts(whole, chance, cap);
    let remainder = (chances - whole) * chance;
    if remainder > 0.0 {
        let mut shifted = vec![0.0; cap + 1];
        for (count, share) in counts.iter().enumerate() {
            shifted[count] += share * (1.0 - remainder);
            shifted[(count + 1).min(cap)] += share * remainder;
        }
        counts = shifted;
    }
    counts
}

/// Past this many chances a run is indistinguishable from a Poisson process.
const MAX_CHANCES: f64 = 64.0;

fn poisson_counts(mean: f64, cap: usize) -> Vec<f64> {
    let mut counts = vec![0.0; cap + 1];
    if mean <= 0.0 {
        counts[0] = 1.0;
        return counts;
    }
    let mut term = (-mean).exp();
    counts[0] = term;
    for (index, count) in counts.iter_mut().enumerate().skip(1) {
        term *= mean / tally(index);
        *count = term;
    }
    let overflow = 1.0 - counts.iter().sum::<f64>();
    counts[cap] += overflow.max(0.0);
    counts
}

fn binomial_counts(chances: f64, chance: f64, cap: usize) -> Vec<f64> {
    let mut counts = vec![0.0; cap + 1];
    let mut term = (1.0 - chance).powf(chances);
    counts[0] = term;
    for (index, count) in counts.iter_mut().enumerate().skip(1) {
        let remaining = chances - tally(index) + 1.0;
        if remaining <= 0.0 {
            break;
        }
        term *= chance / (1.0 - chance) * remaining / tally(index);
        *count = term;
    }
    let overflow = 1.0 - counts.iter().sum::<f64>();
    counts[cap] += overflow.max(0.0);
    counts
}

#[cfg(test)]
mod tests {
    use crate::catalog::{ArmorEffect, Effect, ItemId, ItemKind};
    use crate::challenges::Challenges;
    use crate::model::ItemSource;
    use crate::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};

    use super::estimate_match_probability;

    fn requirement(kind: ItemKind) -> Requirement {
        Requirement {
            kind,
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

    fn query(requirements: Vec<Requirement>, max_depth: u8) -> SearchQuery {
        SearchQuery {
            requirements,
            max_depth,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        }
    }

    fn staff(max_depth: u8) -> SearchQuery {
        let mut requirements = vec![Requirement {
            upgrade: UpgradeRequirement::Exact(3),
            identity_group: Some(1),
            ..requirement(ItemKind::Wand)
        }];
        requirements.extend([1, 2].map(|_| Requirement {
            identity_group: Some(1),
            ..requirement(ItemKind::Wand)
        }));
        requirements.push(Requirement {
            upgrade: UpgradeRequirement::AtLeast(1),
            ..requirement(ItemKind::Wand)
        });
        query(requirements, max_depth)
    }

    #[test]
    fn searching_deeper_finds_more_seeds() {
        let shallow = estimate_match_probability(&staff(7));
        let middle = estimate_match_probability(&staff(9));
        let deep = estimate_match_probability(&staff(24));
        assert!(
            shallow < middle && middle < deep,
            "{shallow:e} {middle:e} {deep:e}"
        );
    }

    #[test]
    fn a_per_item_floor_limit_binds_independently_of_the_search_depth() {
        let anywhere = query(
            vec![Requirement {
                upgrade: UpgradeRequirement::Exact(2),
                ..requirement(ItemKind::Wand)
            }],
            24,
        );
        let early = query(
            vec![Requirement {
                upgrade: UpgradeRequirement::Exact(2),
                max_depth: Some(4),
                ..requirement(ItemKind::Wand)
            }],
            24,
        );
        let shallow_search = query(anywhere.requirements.clone(), 4);
        let limited = estimate_match_probability(&early);
        assert!(limited < estimate_match_probability(&anywhere));
        // Capping one item at floor four is the same as searching four floors.
        let difference = (limited - estimate_match_probability(&shallow_search)).abs();
        assert!(difference < 1e-12, "{difference:e}");
    }

    #[test]
    fn a_guaranteed_reward_is_certain() {
        let ghost = query(
            vec![Requirement {
                source: Some(ItemSource::GhostReward),
                ..requirement(ItemKind::Armor)
            }],
            24,
        );
        assert!(estimate_match_probability(&ghost) > 0.99);
    }

    #[test]
    fn each_extra_copy_costs_something() {
        let one = query(
            vec![Requirement {
                upgrade: UpgradeRequirement::Exact(2),
                ..requirement(ItemKind::Wand)
            }],
            24,
        );
        let two = query(
            vec![
                Requirement {
                    upgrade: UpgradeRequirement::Exact(2),
                    ..requirement(ItemKind::Wand)
                };
                2
            ],
            24,
        );
        assert!(estimate_match_probability(&two) < estimate_match_probability(&one));
    }

    #[test]
    fn unreachable_requirements_are_impossible() {
        // The Wandmaker never hands out armor, and no source stocks a wand
        // beyond the depth its quest occupies.
        let armor_from_wandmaker = query(
            vec![Requirement {
                source: Some(ItemSource::WandmakerReward),
                ..requirement(ItemKind::Armor)
            }],
            24,
        );
        assert!(estimate_match_probability(&armor_from_wandmaker) <= 0.0);

        let wand_before_the_wandmaker = query(
            vec![Requirement {
                source: Some(ItemSource::WandmakerReward),
                max_depth: Some(3),
                ..requirement(ItemKind::Wand)
            }],
            24,
        );
        assert!(estimate_match_probability(&wand_before_the_wandmaker) <= 0.0);
    }

    #[test]
    fn thrown_weapons_are_not_confused_with_melee_ones() {
        let thrown = query(
            vec![Requirement {
                item: Some(ItemId::ThrowingClub),
                ..requirement(ItemKind::Weapon)
            }],
            24,
        );
        let melee = query(
            vec![Requirement {
                item: Some(ItemId::Sword),
                ..requirement(ItemKind::Weapon)
            }],
            24,
        );
        assert!(estimate_match_probability(&thrown) > 0.0);
        assert!(estimate_match_probability(&melee) > 0.0);
    }

    #[test]
    fn rarer_modifiers_are_rarer() {
        let common = query(
            vec![Requirement {
                effect: Some(Effect::Armor(ArmorEffect::Viscosity)),
                ..requirement(ItemKind::Armor)
            }],
            24,
        );
        let rare = query(
            vec![Requirement {
                effect: Some(Effect::Armor(ArmorEffect::Thorns)),
                ..requirement(ItemKind::Armor)
            }],
            24,
        );
        assert!(estimate_match_probability(&rare) < estimate_match_probability(&common));
    }

    #[test]
    fn requiring_a_blacksmith_needs_the_floors_it_lives_on() {
        let mut early = query(vec![requirement(ItemKind::Armor)], 8);
        early.require_blacksmith = true;
        assert!(estimate_match_probability(&early) <= 0.0);

        let mut late = query(vec![requirement(ItemKind::Armor)], 14);
        late.require_blacksmith = true;
        assert!(estimate_match_probability(&late) > 0.9);
    }
}
