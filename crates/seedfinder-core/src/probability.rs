//! Query probability estimates derived from the measured v3.3.8 item supply.
//!
//! The estimate answers "what fraction of seeds satisfies this query", not "how
//! likely is one item to match", so it has to know how much equipment a run
//! actually offers. That supply lives in [`crate::probability_tables`]: expected
//! reward slots per floor and source, with the upgrade, curse, enchantment, and
//! tier distributions each source produces.
//!
//! Every requirement becomes a filter over those slots. Floor limits carve the
//! dungeon into stretches, each holding its own supply, so two items wanted by
//! floor four compete over what those four floors offer rather than over the
//! whole run. Within a stretch a line's slots arrive as a run of independent
//! chances rather than a Poisson process, because the generator deals item
//! categories from a decrementing deck, and they all come out of that one run —
//! which is what stops two requirements from each being handed an item when only
//! one was ever produced. A shop places a fixed number of slots on a single
//! floor instead, and a shelf holding mutually exclusive stock counts once,
//! since a run can only carry one of them out.
//!
//! Requirements are then matched one-to-one onto slots, so three wands are not
//! scored as one wand three times and one shelf is not spent twice. Requirements
//! linked to one identity are summed over the identities they could share and
//! resolved alongside the rest of their family, discounted by the deck-driven
//! scarcity of duplicates.
//!
//! A slot's tier and its upgrade level are tabled apart and multiplied, which
//! holds wherever the generator rolls them apart. The Imp's two hoards do not:
//! its vault stocks four fixed shelves and its reward flips a coin over which
//! of two weapons is levelled furthest, so at both a tier names the levels it
//! can carry. Those sources carry a measured upgrade-per-tier table instead,
//! and are scored against it.
//!
//! Quest prizes are not supply like that, and they are the whole reason
//! families cannot be resolved apart. A giver lays out a pool spanning every
//! family — the Imp's five reward options beside its vault's treasure rooms —
//! and the player carries exactly one item away, so the Imp's haul can answer
//! the `+4` ring a query wants or its `+3` armor, never both. Each pool is
//! therefore lifted out of the per-family matching and spent once across the
//! whole query, on whichever requirement it reaches that helps most.
//!
//! Known simplifications: challenges shift item placement but are ignored, and
//! a query carrying more requirements than one matching resolves keeps only its
//! scarcest ones, which read high. Against them, a pool is spent on its single
//! best use rather than on whichever of them the seed left open; a pool that
//! reaches two unrelated requirements is read as reaching the easier whenever
//! it reaches the harder, which understates how often it ends up spent on the
//! lesser; and duplicate scarcity is measured over a whole line at once, so a
//! linked group whose members want very different items — one `+3` alongside
//! two plain ones — is discounted as heavily as one wanting three alike. Those
//! read low.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::catalog::{
    EXTRA_UPGRADE_MAXIMUM, EXTRA_UPGRADE_TIER, Effect, ItemId, ItemKind, WeaponCategory, item,
};
use crate::equipment::{
    ARMOR_COMMON, ARMOR_CURSES, ARMOR_RARE, ARMOR_UNCOMMON, WEAPON_COMMON, WEAPON_CURSES,
    WEAPON_RARE, WEAPON_UNCOMMON,
};
use crate::generator::{
    ARMOR_ITEMS, RING_ITEMS, WAND_ITEMS, WEAPON_TIER_1_ITEMS, WEAPON_TIER_2_ITEMS,
    WEAPON_TIER_3_ITEMS, WEAPON_TIER_4_ITEMS, WEAPON_TIER_5_ITEMS,
};
use crate::model::ItemSource;
use crate::probability_tables::{
    DEEPEST_FLOOR, DEPTHS, FLOOR_SETS, HIGHEST_TABLED_UPGRADE, HIGHEST_TIER, IDENTITY_REPEAT_LIMIT,
    IDENTITY_REPEATS, LINES_ORDER, Line, PRIZE_GROUPS, PrizeGroup, SLOT_SPREAD, Supply, TIERS,
    TIPPED_DARTS, TIPPED_SHARES, kind_index, line_of, missile_tier, missile_tier_items,
    prize_group, source_index, spread_index, supply_for, tipped_index,
};
use crate::query::{EffectRequirement, Requirement, SearchQuery, UpgradeRequirement};
use crate::quests::WandmakerQuestType;

/// Estimates the fraction of seeds satisfying a query.
///
/// The result is fixed for a search: observed results never feed back into it.
///
/// Alternative groups are approximated by their most plentiful member — a
/// pessimistic simplification, since any member can satisfy the group.
/// Combined-level groups collapse to their cheapest sufficient subset: the
/// fewest members that can reach the total, each carrying an equal share of
/// it — pessimistic, since lopsided splits and larger subsets also satisfy
/// the group.
#[must_use]
pub fn estimate_match_probability(query: &SearchQuery) -> f64 {
    let mut linked: BTreeMap<u8, Vec<Requirement>> = BTreeMap::new();
    let mut independent: Vec<Requirement> = Vec::new();
    for requirement in effective_requirements(query) {
        match requirement.identity_group {
            Some(group) => linked.entry(group).or_default().push(requirement),
            None => independent.push(requirement),
        }
    }

    let mut probability = blacksmith_probability(query) * wandmaker_quest_probability(query);
    let mut groups: Vec<Vec<Requirement>> = Vec::new();
    for members in linked.into_values() {
        // A linked group that names its item constrains nothing extra: every
        // member already matches that one identity. Neither does a group of
        // one, which has nothing to agree with.
        if let Some(pinned) = members.iter().find_map(|member| member.item) {
            independent.extend(members.into_iter().map(|member| Requirement {
                item: Some(pinned),
                ..member
            }));
        } else if members.len() < 2 {
            independent.extend(members);
        } else {
            groups.push(members);
        }
    }
    for members in groups {
        // Requirements draw on the same items whether or not they are linked,
        // and a quest prize is one pick across every family, so the group is
        // resolved alongside everything left rather than as though they never
        // met. Later groups are then resolved on their own: nesting the sums
        // over the identities each could take would cost their product.
        let others = std::mem::take(&mut independent);
        probability *= linked_probability(query, &members, &others);
    }
    probability *= competing_probability(query, &independent);
    if probability <= 0.0 {
        0.0
    } else {
        probability.min(1.0)
    }
}

/// Requirements reduced to the flat, independent form the supply tables can
/// answer: each alternative group collapses to its most plentiful member, and
/// each combined-level group collapses to its cheapest sufficient subset —
/// the fewest members whose level capacity reaches the total, each tightened
/// to carry an equal share. Members the subset does not need are optional in
/// the matcher and are dropped here.
fn effective_requirements(query: &SearchQuery) -> Vec<Requirement> {
    // For each group: how many members a satisfying subset needs at least,
    // given the members' level capacities, and the upgrade each of those
    // members then has to carry.
    let mut group_members: BTreeMap<u8, Vec<u8>> = BTreeMap::new();
    let mut group_totals: BTreeMap<u8, u8> = BTreeMap::new();
    for requirement in &query.requirements {
        if let Some(sum) = requirement.level_sum {
            group_members
                .entry(sum.group)
                .or_default()
                .push(requirement.maximum_level());
            group_totals.entry(sum.group).or_insert(sum.minimum_total);
        }
    }
    let mut group_plan: BTreeMap<u8, (usize, u8)> = BTreeMap::new();
    for (group, mut capacities) in group_members {
        capacities.sort_unstable_by(|a, b| b.cmp(a));
        let total = u16::from(group_totals.get(&group).copied().unwrap_or(0));
        let mut reached = 0u16;
        let mut needed = 0usize;
        for capacity in &capacities {
            if reached >= total {
                break;
            }
            reached += u16::from(*capacity);
            needed += 1;
        }
        let needed = needed.max(1);
        // Levels each taken member must average; its upgrade is one less.
        let share = total.div_ceil(u16::try_from(needed).unwrap_or(1));
        let implied_upgrade = u8::try_from(share.saturating_sub(1)).unwrap_or(u8::MAX);
        group_plan.insert(group, (needed, implied_upgrade));
    }
    let mut taken: BTreeMap<u8, usize> = BTreeMap::new();
    let mut alternatives: BTreeMap<u8, Requirement> = BTreeMap::new();
    let mut flattened: Vec<Requirement> = Vec::new();
    for requirement in &query.requirements {
        let mut requirement = *requirement;
        if let Some(sum) = requirement.level_sum.take() {
            let (needed, implied) = group_plan.get(&sum.group).copied().unwrap_or((1, 0));
            let already = taken.entry(sum.group).or_insert(0);
            if *already >= needed {
                // An optional member the cheapest subset does not use.
                continue;
            }
            *already += 1;
            requirement.upgrade = match requirement.upgrade {
                UpgradeRequirement::Any => UpgradeRequirement::AtLeast(implied),
                UpgradeRequirement::AtLeast(minimum) => {
                    UpgradeRequirement::AtLeast(minimum.max(implied))
                }
                exact @ UpgradeRequirement::Exact(_) => exact,
            };
        }
        match requirement.alternative_group.take() {
            None => flattened.push(requirement),
            Some(group) => {
                let slots = |candidate: &Requirement| {
                    expected_slots(&Predicate::of(*candidate, None).within(query, candidate))
                };
                let replace = alternatives
                    .get(&group)
                    .is_none_or(|kept| slots(&requirement) > slots(kept));
                if replace {
                    alternatives.insert(group, requirement);
                }
            }
        }
    }
    flattened.extend(alternatives.into_values());
    flattened
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

/// Probability that the Wandmaker spawns within the search depth *and* rolls
/// the demanded quest.
///
/// This factor is exact rather than measured. `Wandmaker.Quest.spawnRoom`
/// draws `Int(10 - depth) == 0` on each Prison floor above six, so the giver
/// arrives on floor seven one time in three, on floor eight one of the
/// remaining two times, and on floor nine always. The variant is then a flat
/// `Int(3)` over the three quests, independent of the floor.
fn wandmaker_quest_probability(query: &SearchQuery) -> f64 {
    if query.wandmaker_quest.is_none() {
        return 1.0;
    }
    let mut missed = 1.0;
    for depth in WandmakerQuestType::WINDOW {
        if depth > query.max_depth {
            break;
        }
        missed *= 1.0 - 1.0 / f64::from(10 - u16::from(depth));
    }
    (1.0 - missed) / 3.0
}

/// Probability that requirements sharing an identity group are all satisfied.
///
/// Every member has to resolve to the same item, so the group is evaluated once
/// per candidate identity and the results combined. Same-identity duplicates are
/// rarer than independent draws suggest because the generator deals items from
/// decrementing decks, which [`IDENTITY_REPEATS`] corrects for.
fn linked_probability(query: &SearchQuery, members: &[Requirement], others: &[Requirement]) -> f64 {
    let Some(kind) = members.first().map(|member| member.kind) else {
        return 1.0;
    };
    let mut none = 1.0;
    for (identity, alike) in identities(kind) {
        let shared = together_probability(query, members, Some(identity), others);
        none *= (1.0 - shared.clamp(0.0, 1.0)).powi(alike);
    }
    1.0 - none
}

/// How much rarer it is to hold several items of one identity than independent
/// draws suggest.
///
/// The generator deals each family from a decrementing deck, so drawing a wand
/// makes the same wand less likely next time. Requirements that all name one
/// item — or that are linked to share one — feel that suppression.
///
/// [`IDENTITY_REPEATS`] is measured against independent draws, so a family
/// asking for copies of one item is resolved on that footing too: the run of
/// chances a line's slots normally arrive on already carries some of the same
/// scarcity, and counting it twice would make duplicates look far rarer than
/// they are.
///
/// The table counts how many sets of copies a world offers rather than how
/// often it offers any, since only the former survives the upgrade and curse
/// filters a query puts on top. [`thinned_by`] puts the matching's answer on
/// the same footing, applies the scarcity there, and reads it back.
fn repeat_correction(ordered: &[Predicate], holding: f64, copies: usize) -> f64 {
    let Some((repeated, _)) = repeated_identity(ordered) else {
        return holding;
    };
    // The filters can span families now, so the scarcity is read off the
    // family the repeated identity belongs to rather than whichever filter
    // sorted first.
    let kind = item(repeated).kind;
    let depth = ordered
        .iter()
        .map(|predicate| predicate.max_depth)
        .max()
        .unwrap_or(DEEPEST_FLOOR);
    let line = spread_index(kind, line_for(kind, repeated));
    let copies = copies.min(IDENTITY_REPEAT_LIMIT);
    let depth = usize::from(depth).clamp(1, DEPTHS) - 1;
    thinned_by(
        holding,
        copies,
        f64::from(IDENTITY_REPEATS[line][copies - 1][depth]),
    )
}

/// Applies a scarcity measured on sets of `copies` items to a chance of holding
/// that many.
///
/// A world that barely ever has the copies offers about one set when it does,
/// so the scarcity multiplies straight through. A world that usually has them
/// to spare offers several, and thinning those still leaves it some. Reading
/// the answer back as a stream of arrivals gives the count of sets to thin;
/// a run holding sets that scarce holds at least one about `1 - e^-sets` of
/// the time.
///
/// Nothing converts the thinned count back into a chance of holding `copies`
/// exactly, because a deck that suppresses duplicates rarely hands over more
/// than the copies asked for: once they are that scarce, offering a set and
/// holding one are close to the same event.
fn thinned_by(holding: f64, copies: usize, scarcity: f64) -> f64 {
    if holding <= 0.0 || copies == 0 {
        return holding.max(0.0);
    }
    let mut low = 0.0;
    let mut high = BUSIEST_RUN;
    for _ in 0..ARRIVAL_STEPS {
        let middle = f64::midpoint(low, high);
        if poisson_at_least(middle, copies) < holding {
            low = middle;
        } else {
            high = middle;
        }
    }
    let mean = f64::midpoint(low, high);
    let sets = (1..=copies).fold(scarcity, |sets, taken| sets * mean / tally(taken));
    let missing = (-sets.max(0.0)).exp();
    1.0 - missing
}

/// Chance of at least `count` arrivals from a stream of that average.
fn poisson_at_least(mean: f64, count: usize) -> f64 {
    let mut term = (-mean).exp();
    let mut below = term;
    for step in 1..count {
        term *= mean / tally(step);
        below += term;
    }
    (1.0 - below).clamp(0.0, 1.0)
}

/// Widest average the arrival count is read back as. Anything busier is already
/// certain to hand over the copies a query can ask for.
const BUSIEST_RUN: f64 = 64.0;

/// Bisection steps used to read an arrival count back from a probability.
const ARRIVAL_STEPS: usize = 48;

/// The identity a family wants more than one of, with how many it wants.
fn repeated_identity(ordered: &[Predicate]) -> Option<(ItemId, usize)> {
    ordered
        .iter()
        .filter_map(|predicate| predicate.item)
        .fold(
            BTreeMap::new(),
            |mut counts: BTreeMap<ItemId, usize>, item| {
                *counts.entry(item).or_default() += 1;
                counts
            },
        )
        .into_iter()
        .max_by_key(|(_, copies)| *copies)
        .filter(|(_, copies)| *copies > 1)
}

/// Whether one generator line produces weapons of one melee/thrown class.
/// The plain line rolls wielded weapons; missiles and tipped darts are thrown.
const fn line_matches_category(line: Line, category: WeaponCategory) -> bool {
    match category {
        WeaponCategory::Melee => matches!(line, Line::Plain),
        WeaponCategory::Thrown => matches!(line, Line::Thrown | Line::Tipped),
    }
}

/// The line an identity belongs to. Only weapons have more than one.
fn line_for(kind: ItemKind, item: ItemId) -> Line {
    if kind == ItemKind::Weapon {
        line_of(item)
    } else {
        Line::Plain
    }
}

/// Probability that every requirement outside a linked group is satisfied at
/// once.
///
/// Families are resolved together rather than one at a time: they draw on
/// separate supply, but a quest lays its prizes out across all of them and
/// lets exactly one leave, so a ring and an armor both wanting the Imp's haul
/// are competitors.
fn competing_probability(query: &SearchQuery, requirements: &[Requirement]) -> f64 {
    together_probability(query, requirements, None, &[])
}

/// Probability that every requirement is satisfied by a distinct item.
///
/// The quest prizes are spent across the whole query and the rest of the
/// supply is matched family by family; see [`matching_chance`].
fn together_probability(
    query: &SearchQuery,
    requirements: &[Requirement],
    identity: Option<ItemId>,
    others: &[Requirement],
) -> f64 {
    let group = filters(query, requirements, identity, &[]);
    let Some((_, copies)) = repeated_identity(&group) else {
        return matching_chance(&filters(query, requirements, identity, others));
    };
    // Copies of one identity are scored against independent draws, since that is
    // the footing [`repeat_correction`] was measured on. The scarcity is read off
    // the group on its own, because what it corrects is how often one identity
    // turns up that many times — not how the rest of the family fares alongside.
    let alone = matching_chance(&group);
    if alone <= 0.0 {
        return 0.0;
    }
    let scarcer = repeat_correction(&group, alone, copies) / alone;
    if others.is_empty() {
        return (alone * scarcer).clamp(0.0, 1.0);
    }
    let together = matching_chance(&filters(query, requirements, identity, others));
    (together * scarcer).clamp(0.0, 1.0)
}

/// The requirements reduced to filters, scarcest first.
///
/// Keeping the scarcest first makes truncation lose the least: a query carrying
/// more requirements than one matching resolves keeps the ones that decide it.
fn filters(
    query: &SearchQuery,
    requirements: &[Requirement],
    identity: Option<ItemId>,
    others: &[Requirement],
) -> Vec<Predicate> {
    let mut ordered: Vec<Predicate> = requirements
        .iter()
        .map(|requirement| Predicate::of(*requirement, identity).within(query, requirement))
        .chain(
            others
                .iter()
                .map(|requirement| Predicate::of(*requirement, None).within(query, requirement)),
        )
        .collect();
    ordered.sort_by(|left, right| {
        expected_slots(left)
            .partial_cmp(&expected_slots(right))
            .unwrap_or(Ordering::Equal)
    });
    ordered.truncate(MAX_REQUIREMENTS);
    ordered
}

/// Probability that the supply can serve every filter with a distinct item.
///
/// The dungeon offers two kinds of supply. Scattered drops, chests and shop
/// shelves belong to one family each, so families never compete over them and
/// their answers multiply. A quest prize does not: the giver lays out a pool
/// spanning every family and the player carries exactly one item away, so the
/// Imp's haul can answer the `+4` ring a query wants or its `+3` armor, never
/// both.
///
/// So the prize pools are lifted out and resolved across the whole query,
/// while the supply that stays inside a family is still matched family by
/// family. Conditioning on what each pool reaches turns the estimate into a
/// sum over those reaches of the best use the query can make of them.
fn matching_chance(ordered: &[Predicate]) -> f64 {
    if ordered.is_empty() {
        return 1.0;
    }
    let deepest = ordered
        .iter()
        .map(|predicate| usize::from(predicate.max_depth).clamp(1, DEPTHS))
        .max()
        .unwrap_or(DEPTHS);
    let pools: Vec<Vec<f64>> = PRIZE_GROUPS
        .into_iter()
        .filter_map(|group| prize_reach(group, ordered, deepest))
        .collect();
    let mut open = OpenSupply {
        ordered,
        answered: BTreeMap::new(),
    };
    prize_chance(&pools, 0, 0, &mut open).clamp(0.0, 1.0)
}

/// The chance every requirement is served once the prize pools from `group`
/// onwards have been spent, given the requirements `discharged` by the pools
/// already spent.
///
/// A pool leaves one item, so it answers at most one requirement. Which one is
/// the query's choice, so the pool is spent on the requirement whose removal
/// helps most and that it can actually reach, walking them best-first: the
/// pool takes the best it reaches, and only misses out entirely when it
/// reaches none of them.
///
/// Whether the pool reaches one requirement is read as nested with whether it
/// reaches another — a pool that can answer the harder of two can answer the
/// easier. That is exact for the case that matters, several requirements on
/// one item, where a pool holding the item answers all of them and still only
/// leaves with one. Where two requirements really are unrelated it understates
/// how often the pool ends up spent on the lesser of them, which reads low.
fn prize_chance(pools: &[Vec<f64>], group: usize, discharged: usize, open: &mut OpenSupply) -> f64 {
    let Some(reach) = pools.get(group) else {
        return open.chance(discharged);
    };
    // Best-first over the requirements this pool could still be spent on.
    let mut spending: Vec<(f64, f64)> = (0..reach.len())
        .filter(|requirement| discharged & (1 << requirement) == 0 && reach[*requirement] > 0.0)
        .map(|requirement| {
            (
                reach[requirement],
                prize_chance(pools, group + 1, discharged | (1 << requirement), open),
            )
        })
        .collect();
    spending.sort_by(|left, right| right.1.partial_cmp(&left.1).unwrap_or(Ordering::Equal));

    // Nesting the reaches makes the walk a partition: each requirement claims
    // only what the better ones it is nested inside left over, and whatever no
    // requirement reaches falls through to the next pool.
    let mut claimed = 0.0_f64;
    let mut total = 0.0;
    for (reached, served) in spending {
        total += (reached - claimed).max(0.0) * served;
        claimed = claimed.max(reached);
    }
    total + (1.0 - claimed) * prize_chance(pools, group + 1, discharged, open)
}

/// The family-by-family matching over everything a quest prize is not, with
/// the answers it has already worked out.
struct OpenSupply<'a> {
    ordered: &'a [Predicate],
    answered: BTreeMap<usize, f64>,
}

impl OpenSupply<'_> {
    /// Chance the supply outside the prize pools serves every requirement
    /// except the `discharged` ones, which the pools have already answered.
    fn chance(&mut self, discharged: usize) -> f64 {
        if let Some(answer) = self.answered.get(&discharged) {
            return *answer;
        }
        let mut families: BTreeMap<usize, Vec<Predicate>> = BTreeMap::new();
        for (requirement, predicate) in self.ordered.iter().enumerate() {
            if discharged & (1 << requirement) == 0 {
                families
                    .entry(kind_index(predicate.kind))
                    .or_default()
                    .push(*predicate);
            }
        }
        let answer: f64 = families
            .into_values()
            .map(|family| open_chance(&family))
            .product();
        self.answered.insert(discharged, answer);
        answer
    }
}

/// Probability that one family's own supply serves every one of its filters
/// with a distinct item.
///
/// Each reward slot in the dungeon covers some set of the requirements, and the
/// query succeeds exactly when the slots can be matched one-to-one onto the
/// requirements. By Hall's theorem that holds precisely when no set of
/// requirements outnumbers the slots covering it, which is what
/// [`covers_every_requirement`] checks.
///
/// Working in coverage sets rather than per requirement is what stops one slot
/// from being spent twice: a shop shelf can hold the `+0` wand a query asks for
/// or one of its plain wands, never both.
fn open_chance(ordered: &[Predicate]) -> f64 {
    let Some(kind) = ordered.first().map(|predicate| predicate.kind) else {
        return 1.0;
    };
    let wanted = ordered.len();
    let coverages = 1 << wanted;
    let shared: Vec<Option<Predicate>> = (0..coverages)
        .map(|coverage| narrow(ordered, coverage))
        .collect();

    // Floor limits carve the dungeon into stretches that different requirements
    // can reach. Each is its own supply: two items wanted by floor four compete
    // over what those four floors hold, not over the whole run. Nothing past the
    // deepest floor any requirement accepts can serve the query at all.
    let mut limits: Vec<usize> = ordered
        .iter()
        .map(|predicate| usize::from(predicate.max_depth).clamp(1, DEPTHS))
        .collect();
    limits.sort_unstable();
    limits.dedup();

    let steady = repeated_identity(ordered).is_none();
    let mut streams: Vec<Stream> = Vec::new();
    // A shop's shelf holds one item whichever line it comes from, so its lines
    // are pooled into a single slot rather than each being offered one of their
    // own. A shop restocks on every shop floor, so its floors are not
    // alternatives the way a quest's are — and a quest's prizes are not here at
    // all, having been lifted out to [`prize_reach`].
    let mut bundles: BTreeMap<(usize, usize), (u8, Vec<f64>)> = BTreeMap::new();
    for (line, (from, until)) in LINES_ORDER
        .into_iter()
        .flat_map(|line| stretches(&limits).map(move |stretch| (line, stretch)))
    {
        let mut placed = 0.0;
        let mut covered = vec![0.0; coverages];
        for supply in supply_for(kind).filter(|supply| supply.line == line) {
            if prize_group(supply.source).is_some() {
                continue;
            }
            for depth in from..=until {
                let available = f64::from(supply.depth_slots[depth - 1]);
                if available <= 0.0 {
                    continue;
                }
                if supply.bundle == 0 {
                    placed += available;
                }
                let covered_by = coverage_shares(&shared, supply, depth);
                if covered_by.iter().skip(1).all(|share| *share <= 0.0) {
                    continue;
                }
                if supply.bundle == 0 {
                    for (coverage, share) in covered_by.iter().enumerate().skip(1) {
                        covered[coverage] += available * share;
                    }
                    continue;
                }
                let appearances = available / f64::from(supply.bundle);
                let bundle = bundles
                    .entry((source_index(supply.source), depth))
                    .or_insert_with(|| (supply.bundle, vec![0.0; coverages]));
                for (coverage, share) in covered_by.iter().enumerate().skip(1) {
                    bundle.1[coverage] += appearances * share;
                }
            }
        }
        if covered.iter().skip(1).any(|mass| *mass > 0.0) {
            streams.push(Stream::of(
                spread_index(kind, line),
                until,
                placed,
                covered,
                steady,
            ));
        }
    }
    let mut slots: Vec<Slot> = Vec::new();
    for (bundle, covers) in bundles.into_values() {
        let claimed: f64 = covers.iter().skip(1).sum();
        for _ in 0..bundle {
            slots.push(Slot {
                // A slot covers one set of requirements at most, so pooling the
                // lines cannot leave it more than fully spoken for.
                covers: covers.iter().map(|mass| mass / claimed.max(1.0)).collect(),
            });
        }
    }
    matching_probability(wanted, &streams, &slots).clamp(0.0, 1.0)
}

/// How likely one quest's prize pool is to be able to serve each requirement.
///
/// The giver lays out a whole pool — the Imp's five reward options beside its
/// vault's treasure rooms — and the player leaves with one item, so what
/// matters per requirement is whether anything in the pool would answer it.
/// Working that union out per floor and weighting it by the chance the quest
/// landed there keeps a floor limit that cuts the quest's window from claiming
/// the prize.
///
/// Returns `None` when nothing the pool holds can serve the query.
fn prize_reach(group: PrizeGroup, ordered: &[Predicate], deepest: usize) -> Option<Vec<f64>> {
    let mut kinds: Vec<ItemKind> = ordered.iter().map(|predicate| predicate.kind).collect();
    kinds.sort_unstable_by_key(|kind| kind_index(*kind));
    kinds.dedup();
    let mut reach = vec![0.0; ordered.len()];
    for depth in 1..=deepest {
        // Every row of one quest shares its giver's floor distribution, so any
        // of them reports the chance the prize is waiting on this floor.
        let mut appeared = 0.0_f64;
        let mut missing = vec![1.0; ordered.len()];
        for supply in kinds
            .iter()
            .flat_map(|kind| supply_for(*kind))
            .filter(|supply| prize_group(supply.source) == Some(group))
        {
            let available = f64::from(supply.depth_slots[depth - 1]);
            if available <= 0.0 {
                continue;
            }
            appeared = appeared.max(available);
            for (requirement, predicate) in ordered.iter().enumerate() {
                missing[requirement] *= 1.0 - predicate.slot_probability(supply, depth);
            }
        }
        if appeared <= 0.0 {
            continue;
        }
        for (requirement, missed) in missing.into_iter().enumerate() {
            reach[requirement] += appeared * (1.0 - missed);
        }
    }
    if reach.iter().all(|chance| *chance <= 0.0) {
        return None;
    }
    for chance in &mut reach {
        *chance = chance.clamp(0.0, 1.0);
    }
    Some(reach)
}

/// The stretches of floors the query's limits carve out, as inclusive ranges.
fn stretches(limits: &[usize]) -> impl Iterator<Item = (usize, usize)> + '_ {
    limits
        .iter()
        .scan(1, |from, until| {
            let stretch = (*from, *until);
            *from = until + 1;
            Some(stretch)
        })
        .filter(|(from, until)| from <= until)
}

/// One reward slot, with the chance it covers each set of requirements.
struct Slot {
    covers: Vec<f64>,
}

/// The scattered supply of one generator line.
///
/// A line deals its items from a decrementing deck, so its slots arrive as a run
/// of independent chances rather than a Poisson process: the same average, but
/// far less likely to hand over three items where one was expected. All of the
/// line's slots come out of that one run, which is what stops two requirements
/// from each being handed their own item as though the other had not taken one.
struct Stream {
    /// Chances the line takes, or `None` when its count is spread widely enough
    /// that random arrivals describe it just as well.
    trials: Option<f64>,
    /// Expected slots covering each set of requirements.
    covered: Vec<f64>,
}

impl Stream {
    fn of(line: usize, reach: usize, placed: f64, covered: Vec<f64>, steady: bool) -> Self {
        let steadiness = f64::from(SLOT_SPREAD[line][reach - 1]).clamp(0.0, 1.0);
        let chance = 1.0 - steadiness;
        let trials = placed / chance;
        let runs = steady && chance > 0.0 && trials <= MAX_CHANCES && placed > 0.0;
        Self {
            trials: runs.then_some(trials),
            covered,
        }
    }

    /// Folds this line's slots into the states reached so far.
    ///
    /// The sets are taken one at a time out of the same run of chances, each
    /// drawing on what the earlier ones left. That is what keeps two
    /// requirements from both being handed an item when the line only ever
    /// produced one, and it fades out on its own as the run grows longer.
    fn fold(&self, states: BTreeMap<u128, f64>, cap: usize) -> BTreeMap<u128, f64> {
        let mut states = states;
        // How much of the run earlier sets have taken: the share of its chances
        // they claimed, and the slots they took that a state cannot record.
        let mut claimed = 0.0_f64;
        let mut hidden = 0.0_f64;
        for (coverage, mean) in self.covered.iter().enumerate().skip(1) {
            if *mean <= 0.0 {
                continue;
            }
            let chance = self
                .trials
                .map(|trials| (mean / trials / (1.0 - claimed).max(f64::EPSILON)).clamp(0.0, 1.0));
            let mut arrivals: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
            let mut next = BTreeMap::new();
            for (state, reached) in &states {
                let spent = taken(*state, self.covered.len());
                let counts = arrivals
                    .entry(spent)
                    .or_insert_with(|| self.counts(f64::from(spent) + hidden, *mean, chance, cap));
                for (count, share) in counts.iter().enumerate() {
                    if *share > 0.0 {
                        accumulate(
                            &mut next,
                            add_count(*state, coverage, count, cap),
                            reached * share,
                        );
                    }
                }
            }
            if let Some(trials) = self.trials {
                let typical = self.counts(hidden, *mean, chance, cap);
                let recorded: f64 = typical
                    .iter()
                    .enumerate()
                    .map(|(count, share)| tally(count) * share)
                    .sum();
                hidden += (mean - recorded).max(0.0);
                claimed += mean / trials;
            }
            states = prune(next);
        }
        states
    }

    /// Chances of each number of slots covering one set, out of what is left of
    /// the run once `spent` of its chances have gone.
    fn counts(&self, spent: f64, mean: f64, chance: Option<f64>, cap: usize) -> Vec<f64> {
        match (self.trials, chance) {
            (Some(trials), Some(chance)) => binomial_counts((trials - spent).max(0.0), chance, cap),
            _ => poisson_counts(mean, cap),
        }
    }
}

/// Slots a packed state already holds, across every coverage set.
fn taken(state: u128, coverages: usize) -> u32 {
    (1..coverages)
        .map(|coverage| slot_count(state, coverage))
        .sum()
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
fn coverage_shares(shared: &[Option<Predicate>], supply: &Supply, depth: usize) -> Vec<f64> {
    exact_shares(
        shared
            .iter()
            .map(|narrowed| {
                narrowed.map_or(0.0, |narrowed| narrowed.slot_probability(supply, depth))
            })
            .collect(),
    )
}

/// Turns "satisfies at least this set" into "satisfies exactly this set".
///
/// The filters overlap, so one slot's chance of covering a set is not its
/// chance of covering that set and nothing more. Inverting over the subset
/// lattice turns the first into the second.
fn exact_shares(mut exact: Vec<f64>) -> Vec<f64> {
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
/// Each scattered line contributes its whole run of chances at once; quest and
/// shop slots are then folded in one at a time, each covering one set or
/// nothing. The surviving states are the ones Hall's theorem admits.
fn matching_probability(wanted: usize, streams: &[Stream], slots: &[Slot]) -> f64 {
    let cap = wanted.min(MAX_COUNT);
    let mut states = BTreeMap::from([(0_u128, 1.0)]);
    for stream in streams {
        states = stream.fold(states, cap);
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
    weapon_category: Option<WeaponCategory>,
    item: Option<ItemId>,
    tiers: u8,
    upgrades: u8,
    effect: EffectRequirement,
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
            weapon_category: requirement.weapon_category,
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
        let weapon_category = match (self.weapon_category, other.weapon_category) {
            (Some(left), Some(right)) if left != right => return None,
            (left, right) => left.or(right),
        };
        let item = match (self.item, other.item) {
            (Some(left), Some(right)) if left != right => return None,
            (left, right) => left.or(right),
        };
        let source = match (self.source, other.source) {
            (Some(left), Some(right)) if left != right => return None,
            (left, right) => left.or(right),
        };
        let effect = match (self.effect, other.effect) {
            (EffectRequirement::OneOf(left), EffectRequirement::OneOf(right)) => {
                EffectRequirement::OneOf(left.intersection(right)?)
            }
            (EffectRequirement::Any, other) | (other, EffectRequirement::Any) => other,
        };
        let tiers = self.tiers & other.tiers;
        let upgrades = self.upgrades & other.upgrades;
        let require_uncursed = self.require_uncursed || other.require_uncursed;
        let curses_only = match effect {
            EffectRequirement::OneOf(set) => set.is_curses_only(),
            EffectRequirement::Any => false,
        };
        if tiers == 0 || upgrades == 0 || (require_uncursed && curses_only) {
            return None;
        }
        Some(Self {
            kind: self.kind,
            weapon_category,
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
    /// them does, since the query is free to claim whichever qualifies. Whether
    /// that is several chances or one depends on how the source rolls them: the
    /// Blacksmith upgrades its whole weapon rack together, so a `+3` there is a
    /// single chance however many weapons it lays out.
    fn slot_probability(self, supply: &Supply, depth: usize) -> f64 {
        if self.kind != supply.kind
            || usize::from(self.max_depth) < depth
            || self.source.is_some_and(|wanted| wanted != supply.source)
            || (self.exclude_blacksmith && supply.source == ItemSource::BlacksmithReward)
        {
            return 0.0;
        }
        if self.weapon_category.is_some_and(|category| {
            self.kind == ItemKind::Weapon && !line_matches_category(supply.line, category)
        }) {
            return 0.0;
        }
        let tiers = &supply.tiers[((depth - 1) / 5).min(FLOOR_SETS - 1)];
        let identity = self.identity_probability(supply, tiers);
        let modifiers = self.effect_probability(supply) * self.uncursed_probability(supply);
        let options = f64::from(supply.options);
        if supply.shared_roll {
            // No source that rolls its alternatives as one reaches the extra
            // upgrade, so the identity keeps the tabled tier shares here.
            let rolled = self.upgrade_probability(supply) * modifiers;
            rolled * (1.0 - (1.0 - identity).powf(options))
        } else {
            let matched = self.identity_and_upgrade_probability(supply, tiers) * modifiers;
            1.0 - (1.0 - matched).powf(options)
        }
    }

    /// Probability that one alternative of `supply` is an item this filter
    /// accepts, identity and upgrade together.
    ///
    /// Tiers and upgrades are tabled apart and roll independently, save in two
    /// places. A source that locks the two together carries its own
    /// upgrade-per-tier table and is scored against that; see
    /// [`locks_levels_to_tiers`](crate::probability_tables::locks_levels_to_tiers).
    /// Everywhere else, [`EXTRA_UPGRADE_MAXIMUM`] lands only on a tier-4
    /// weapon, so that level is scored against tier 4 alone and the levels
    /// below it against the tier shares that remain — which leaves every
    /// source that never reaches it scored exactly as before.
    fn identity_and_upgrade_probability(self, supply: &Supply, tiers: &[f32; TIERS]) -> f64 {
        let tiered = matches!(supply.kind, ItemKind::Weapon | ItemKind::Armor);
        if let Some(levels) = supply.levels.filter(|_| tiered) {
            // Each tier is worth only its own share of the levels this filter
            // accepts, not the whole source's: asking for a tier and a level
            // that never came off the same shelf has to score zero.
            let allowed = self.allowed_upgrades(supply);
            let mut reachable = [0.0_f32; TIERS];
            for ((share, tabled), carried) in reachable.iter_mut().zip(tiers).zip(levels) {
                let held: f64 = carried
                    .iter()
                    .enumerate()
                    .filter(|(upgrade, _)| allowed & (1 << upgrade) != 0)
                    .map(|(_, chance)| f64::from(*chance))
                    .sum();
                #[allow(clippy::cast_possible_truncation)]
                {
                    *share = (f64::from(*tabled) * held) as f32;
                }
            }
            return self.identity_probability(supply, &reachable);
        }
        let (extra, ordinary) = self.upgrade_probabilities(supply);
        let tabled_extra = f64::from(supply.upgrades[usize::from(EXTRA_UPGRADE_MAXIMUM)]);
        if tabled_extra <= 0.0 {
            return ordinary * self.identity_probability(supply, tiers);
        }
        // The tier shares an item carries once its level is known not to be
        // the extra one: tier 4 gives up that whole share, and the rest are
        // renormalised over what is left.
        let mut without_extra = [0.0_f32; TIERS];
        for (tier, share) in (1..).zip(tiers) {
            let held_back = if tier == EXTRA_UPGRADE_TIER {
                tabled_extra
            } else {
                0.0
            };
            #[allow(clippy::cast_possible_truncation)]
            {
                without_extra[usize::from(tier) - 1] =
                    ((f64::from(*share) - held_back) / (1.0 - tabled_extra)).max(0.0) as f32;
            }
        }
        let mut only_the_extra_tier = [0.0; TIERS];
        only_the_extra_tier[usize::from(EXTRA_UPGRADE_TIER) - 1] = 1.0;
        extra * self.identity_probability(supply, &only_the_extra_tier)
            + ordinary * self.identity_probability(supply, &without_extra)
    }

    fn identity_probability(self, supply: &Supply, tiers: &[f32; TIERS]) -> f64 {
        match (self.kind, self.item) {
            (ItemKind::Weapon, Some(wanted)) => {
                if line_of(wanted) != supply.line {
                    return 0.0;
                }
                // A tipped dart's identity is the plant seed it was tipped with,
                // which the generator does not hand out evenly.
                if let Some(dart) = tipped_index(wanted) {
                    return self.tier_probability(tiers) * f64::from(TIPPED_SHARES[dart]);
                }
                let Some((tier, siblings)) = weapon_family(wanted) else {
                    return 0.0;
                };
                if self.tiers & (1 << (tier - 1)) == 0 {
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
        let (extra, ordinary) = self.upgrade_probabilities(supply);
        extra + ordinary
    }

    /// The tabled share of the upgrade levels this filter accepts, split into
    /// [`EXTRA_UPGRADE_MAXIMUM`] — the one level tied to a single tier — and
    /// everything below it.
    fn upgrade_probabilities(self, supply: &Supply) -> (f64, f64) {
        let allowed = self.allowed_upgrades(supply);
        let extra_level = usize::from(EXTRA_UPGRADE_MAXIMUM);
        let share = |upgrade: usize| {
            if allowed & (1 << upgrade) == 0 {
                0.0
            } else {
                f64::from(supply.upgrades[upgrade])
            }
        };
        let ordinary = (0..supply.upgrades.len())
            .filter(|upgrade| *upgrade != extra_level)
            .map(share)
            .sum();
        (share(extra_level), ordinary)
    }

    /// The upgrade levels this filter accepts, as a bit set, once fast mode
    /// has given up the ones it skips.
    fn allowed_upgrades(self, supply: &Supply) -> u8 {
        let mut allowed = self.upgrades;
        if self.fast_mode && fast_mode_skips(supply.source, self.kind) {
            allowed &= (1 << (FAST_MODE_UPGRADE_CAP + 1)) - 1;
        }
        allowed
    }

    fn effect_probability(self, supply: &Supply) -> f64 {
        let EffectRequirement::OneOf(set) = self.effect else {
            return 1.0;
        };
        // Uncursed items never carry curse effects, so those members of the
        // set can never be the match.
        let set = if self.require_uncursed {
            match set.without_curses() {
                Some(set) => set,
                None => return 0.0,
            }
        } else {
            set
        };
        // Each member is a disjoint outcome for one item, so their chances add.
        set.effects()
            .map(|effect| {
                if effect.is_curse() {
                    f64::from(supply.cursed) / curse_count(effect)
                } else {
                    f64::from(supply.enchanted) * rarity_probability(effect)
                }
            })
            .sum()
    }

    fn uncursed_probability(self, supply: &Supply) -> f64 {
        if !self.require_uncursed {
            return 1.0;
        }
        match self.effect {
            // Positive enchantments and glyphs are generated only on clean
            // items, and `effect_probability` already dropped the curses.
            EffectRequirement::OneOf(_) => 1.0,
            EffectRequirement::Any => 1.0 - f64::from(supply.cursed),
        }
    }
}

fn effective_depth(query: &SearchQuery, requirement: &Requirement) -> u8 {
    requirement
        .max_depth
        .map_or(query.max_depth, |limit| limit.min(query.max_depth))
}

/// Tier of a melee or thrown weapon and how many identities share that tier.
/// `None` for anything the generator never produces.
fn weapon_family(wanted: ItemId) -> Option<(u8, usize)> {
    if let Some(tier) = melee_tier(wanted) {
        return Some((tier, melee_tier_items(tier).iter().flatten().count()));
    }
    missile_tier(wanted).map(|tier| {
        let siblings = missile_tier_items(tier)
            .iter()
            .filter(|kind| kind.item_id().is_some())
            .count();
        (tier, siblings)
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

/// Every identity a family can generate, collapsed onto one standing for each
/// group the supply tables cannot tell apart, with how many it stands for.
///
/// Weapons of one tier are drawn equally often, and so are wands and rings, so
/// resolving one of them and raising the answer to the size of its group is
/// exact — and much cheaper than resolving all forty-odd weapon identities.
fn identities(kind: ItemKind) -> Vec<(ItemId, i32)> {
    match kind {
        ItemKind::Weapon => (1..=HIGHEST_TIER)
            .filter_map(|tier| {
                let items = melee_tier_items(tier);
                Some((*items.iter().flatten().next()?, alike(items.len())))
            })
            .chain((1..=HIGHEST_TIER).filter_map(|tier| {
                let items = missile_tier_items(tier);
                let first = items.iter().find_map(|kind| kind.item_id())?;
                let generated = items.iter().filter_map(|kind| kind.item_id()).count();
                Some((first, alike(generated)))
            }))
            // Tipped darts follow the plant seeds a run happens to grow, which
            // do not come up equally often.
            .chain(TIPPED_DART_IDS.map(|dart| (dart, 1)))
            .collect(),
        ItemKind::Armor => ARMOR_ITEMS.iter().map(|armor| (*armor, 1)).collect(),
        ItemKind::Wand => WAND_ITEMS
            .first()
            .map(|wand| vec![(*wand, alike(WAND_ITEMS.len()))])
            .unwrap_or_default(),
        ItemKind::Ring => RING_ITEMS
            .first()
            .map(|ring| vec![(ring.item_id(), alike(RING_ITEMS.len()))])
            .unwrap_or_default(),
    }
}

/// Identities one representative stands for, as a power.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn alike(count: usize) -> i32 {
    count.min(i32::MAX as usize) as i32
}

/// Every tipped dart the generator can produce, in catalog order.
const TIPPED_DART_IDS: [ItemId; TIPPED_DARTS] = [
    ItemId::RotDart,
    ItemId::IncendiaryDart,
    ItemId::AdrenalineDart,
    ItemId::HealingDart,
    ItemId::ChillingDart,
    ItemId::ShockingDart,
    ItemId::PoisonDart,
    ItemId::CleansingDart,
    ItemId::ParalyticDart,
    ItemId::HolyDart,
    ItemId::DisplacingDart,
    ItemId::BlindingDart,
];

/// Fast mode drops the Crypt and Sacrificial-fire +3 prizes, making +3 weapon
/// and armor requirements quest-only. See [`crate::feasibility`].
const FAST_MODE_UPGRADE_CAP: u8 = 2;

const fn fast_mode_skips(source: ItemSource, kind: ItemKind) -> bool {
    matches!(
        (source, kind),
        (ItemSource::Tomb, ItemKind::Armor) | (ItemSource::SacrificialFire, ItemKind::Weapon)
    )
}

/// Curses are drawn uniformly from the family's list.
fn curse_count(effect: Effect) -> f64 {
    match effect {
        Effect::Weapon(_) => tally(WEAPON_CURSES.len()),
        Effect::Armor(_) => tally(ARMOR_CURSES.len()),
    }
}

/// Chance that one positive-effect draw lands on `effect`: rarities come up
/// 50/40/10 and each rarity picks uniformly from its list.
fn rarity_probability(effect: Effect) -> f64 {
    fn bucket_share<T: Copy + PartialEq>(effect: T, buckets: [&[T]; 3]) -> f64 {
        const RARITY_SHARES: [f64; 3] = [0.50, 0.40, 0.10];
        buckets
            .iter()
            .zip(RARITY_SHARES)
            .find(|(bucket, _)| bucket.contains(&effect))
            .map_or(0.0, |(bucket, share)| share / tally(bucket.len()))
    }
    match effect {
        Effect::Weapon(effect) => {
            bucket_share(effect, [&WEAPON_COMMON, &WEAPON_UNCOMMON, &WEAPON_RARE])
        }
        Effect::Armor(effect) => {
            bucket_share(effect, [&ARMOR_COMMON, &ARMOR_UNCOMMON, &ARMOR_RARE])
        }
    }
}

/// Table sizes and item counts are small enough to be exact in `f64`.
#[allow(clippy::cast_precision_loss)]
fn tally(count: usize) -> f64 {
    count as f64
}

/// Past this many chances a run is indistinguishable from a Poisson process.
const MAX_CHANCES: f64 = 64.0;

/// Chances of zero through `cap` arrivals, with everything past the cap folded
/// into the last bucket.
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
    use crate::query::{
        EffectRequirement, EffectSet, LevelSum, Requirement, SearchQuery, TierRequirement,
        UpgradeRequirement,
    };

    use super::{estimate_match_probability, rarity_probability};

    fn requirement(kind: ItemKind) -> Requirement {
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

    fn query(requirements: Vec<Requirement>, max_depth: u8) -> SearchQuery {
        SearchQuery {
            requirements,
            max_depth,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        }
    }

    /// A `+4` armor and a `+4` ring are each common enough — the Imp hands one
    /// of each out about a third of the time — but only the Imp reaches `+4`
    /// in those families, and the Escape Crystal lets exactly one item leave.
    /// Wanting both is therefore impossible, however plentiful each looks
    /// alone. Scoring the families apart put it at one seed in nine.
    #[test]
    fn one_quest_prize_cannot_answer_two_families_at_once() {
        let plus_four = |kind| Requirement {
            upgrade: UpgradeRequirement::AtLeast(4),
            ..requirement(kind)
        };
        let armor = estimate_match_probability(&query(vec![plus_four(ItemKind::Armor)], 24));
        let ring = estimate_match_probability(&query(vec![plus_four(ItemKind::Ring)], 24));
        assert!(armor > 0.2, "{armor:e}");
        assert!(ring > 0.2, "{ring:e}");
        let both = estimate_match_probability(&query(
            vec![plus_four(ItemKind::Armor), plus_four(ItemKind::Ring)],
            24,
        ));
        assert!(both <= 1e-9, "{both:e} for a pair no seed can satisfy");
    }

    /// The same pick cannot be spent twice within one family either: the Imp's
    /// reward armor and its vault's treasure armors are one choice, not two.
    #[test]
    fn one_quest_prize_cannot_answer_two_requirements_of_a_family() {
        let plus_three = || Requirement {
            upgrade: UpgradeRequirement::AtLeast(3),
            ..requirement(ItemKind::Armor)
        };
        let one = estimate_match_probability(&query(vec![plus_three()], 24));
        let two = estimate_match_probability(&query(vec![plus_three(), plus_three()], 24));
        // Sampled over 200000 seeds: 0.909 and 0.173, so the pair is far below
        // the 0.83 that scoring the two independently would give. The bands are
        // the sweep's own factor-of-two tolerance around those rates.
        assert!((0.45..=1.0).contains(&one), "{one:e}");
        assert!((0.086..=0.346).contains(&two), "{two:e}");
        assert!(two < one / 2.0, "{two:e} against {one:e}");
    }

    /// The Imp's hoards hand a tier and a level out together, so a source
    /// filter pinned to one of them has to score the pairs they never build at
    /// nothing, however plentiful each half looks on its own.
    #[test]
    fn a_locked_source_supplies_no_tier_and_level_it_never_pairs() {
        let from = |source, requirement| {
            estimate_match_probability(&query(
                vec![Requirement {
                    source: Some(source),
                    ..requirement
                }],
                24,
            ))
        };
        // The vault's armor is +0 at tier 2 and climbs one level a tier, so it
        // stocks tier-5 armor at +3 and nothing below tier 5 at +3.
        let armor = |tier| Requirement {
            tier,
            upgrade: UpgradeRequirement::Exact(3),
            ..requirement(ItemKind::Armor)
        };
        let stocked = from(ItemSource::VaultTreasure, armor(TierRequirement::Exact(5)));
        let never = from(ItemSource::VaultTreasure, armor(TierRequirement::AtMost(3)));
        assert!(stocked > 0.05, "{stocked:e}");
        assert!(never <= 1e-9, "{never:e} for armor the vault never shelves");
        // Whichever of the Imp's two weapons is tier 5 is the one levelled
        // +2..=+4, so its tier-4 weapon is never the +2 one.
        let weapon = |upgrade| Requirement {
            tier: TierRequirement::Exact(4),
            upgrade,
            ..requirement(ItemKind::Weapon)
        };
        let rolled = from(ItemSource::ImpReward, weapon(UpgradeRequirement::Exact(3)));
        let skipped = from(ItemSource::ImpReward, weapon(UpgradeRequirement::Exact(2)));
        assert!(rolled > 0.05, "{rolled:e}");
        assert!(
            skipped <= 1e-9,
            "{skipped:e} for a level the Imp keeps for tier 5"
        );
    }

    #[test]
    fn weapon_category_narrows_the_estimate() {
        use crate::catalog::WeaponCategory;

        let exact_two = Requirement {
            upgrade: UpgradeRequirement::Exact(2),
            ..requirement(ItemKind::Weapon)
        };
        let melee = Requirement {
            weapon_category: Some(WeaponCategory::Melee),
            ..exact_two
        };
        let thrown = Requirement {
            weapon_category: Some(WeaponCategory::Thrown),
            ..exact_two
        };
        let any = estimate_match_probability(&query(vec![exact_two], 6));
        let melee = estimate_match_probability(&query(vec![melee], 6));
        let thrown = estimate_match_probability(&query(vec![thrown], 6));

        assert!(melee > 0.0, "{melee}");
        assert!(thrown > 0.0, "{thrown}");
        assert!(melee < any, "{melee} vs {any}");
        assert!(thrown < any, "{thrown} vs {any}");
        // One weapon of either class is at least as likely as one of a fixed
        // class, and no likelier than having one of each class to pick from.
        assert!(any <= melee + thrown + 1e-9, "{any} vs {melee} + {thrown}");
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
    fn tipped_darts_are_obtainable() {
        // Darts are weapons to the catalog but come from plant seeds sold in
        // shops, so they are not in the weapon deck at all.
        let dart = query(
            vec![Requirement {
                item: Some(ItemId::BlindingDart),
                ..requirement(ItemKind::Weapon)
            }],
            24,
        );
        assert!(estimate_match_probability(&dart) > 0.1);
        // The one dart the generator never tips is still impossible.
        let never = query(
            vec![Requirement {
                item: Some(ItemId::RotDart),
                ..requirement(ItemKind::Weapon)
            }],
            24,
        );
        assert!(estimate_match_probability(&never) <= 0.0);
    }

    #[test]
    fn a_per_item_floor_limit_makes_its_own_supply_compete() {
        // Two wands wanted by floor four draw on those four floors alone, so the
        // second costs far more than it would with the run to draw on.
        let shallow = |copies: usize| {
            query(
                vec![
                    Requirement {
                        upgrade: UpgradeRequirement::AtLeast(1),
                        max_depth: Some(4),
                        ..requirement(ItemKind::Wand)
                    };
                    copies
                ],
                24,
            )
        };
        let one = estimate_match_probability(&shallow(1));
        let two = estimate_match_probability(&shallow(2));
        assert!(
            two < one * one,
            "{two:e} is not scarcer than {:e}",
            one * one
        );
    }

    #[test]
    fn a_linked_group_competes_with_the_rest_of_its_family() {
        let mut linked: Vec<Requirement> = (0..2)
            .map(|_| Requirement {
                identity_group: Some(1),
                ..requirement(ItemKind::Wand)
            })
            .collect();
        let alone = estimate_match_probability(&query(linked.clone(), 6));
        linked.push(Requirement {
            upgrade: UpgradeRequirement::AtLeast(2),
            ..requirement(ItemKind::Wand)
        });
        let alongside = estimate_match_probability(&query(linked, 6));
        assert!(alongside < alone);
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
                effect: EffectRequirement::exactly(Effect::Armor(ArmorEffect::Viscosity)),
                ..requirement(ItemKind::Armor)
            }],
            24,
        );
        let rare = query(
            vec![Requirement {
                effect: EffectRequirement::exactly(Effect::Armor(ArmorEffect::Thorns)),
                ..requirement(ItemKind::Armor)
            }],
            24,
        );
        assert!(estimate_match_probability(&rare) < estimate_match_probability(&common));
    }

    #[test]
    fn broader_effect_sets_are_likelier_than_their_members() {
        // Shallow on purpose: by depth 24 the broadest of these queries rounds
        // to exactly 1.0 in `f64` and the strict orderings collapse.
        let depth = 6;
        let single = |effect| {
            query(
                vec![Requirement {
                    effect: EffectRequirement::exactly(Effect::Armor(effect)),
                    ..requirement(ItemKind::Armor)
                }],
                depth,
            )
        };
        let both = query(
            vec![Requirement {
                effect: EffectRequirement::OneOf(
                    EffectSet::from_effects([
                        Effect::Armor(ArmorEffect::Viscosity),
                        Effect::Armor(ArmorEffect::Thorns),
                    ])
                    .unwrap(),
                ),
                ..requirement(ItemKind::Armor)
            }],
            depth,
        );
        let any_glyph = query(
            vec![Requirement {
                effect: EffectRequirement::OneOf(EffectSet::enchantments(ItemKind::Armor).unwrap()),
                ..requirement(ItemKind::Armor)
            }],
            depth,
        );
        let viscosity = estimate_match_probability(&single(ArmorEffect::Viscosity));
        let thorns = estimate_match_probability(&single(ArmorEffect::Thorns));
        let pair = estimate_match_probability(&both);
        let any = estimate_match_probability(&any_glyph);
        assert!(pair > viscosity && pair > thorns, "{pair:e}");
        assert!(any > pair, "{any:e} vs {pair:e}");
        assert!(
            any < estimate_match_probability(&query(vec![requirement(ItemKind::Armor)], depth))
        );
    }

    #[test]
    fn every_enchantment_is_reachable_and_each_family_sums_to_one() {
        for effects in [
            EffectSet::enchantments(ItemKind::Weapon).unwrap(),
            EffectSet::enchantments(ItemKind::Armor).unwrap(),
        ] {
            let total: f64 = effects
                .effects()
                .map(|effect| {
                    let chance = rarity_probability(effect);
                    assert!(chance > 0.0, "{effect:?} can never be drawn");
                    chance
                })
                .sum();
            assert!((total - 1.0).abs() < 1e-12, "{total:e}");
        }
    }

    #[test]
    fn alternatives_score_at_least_their_best_member() {
        let spear_only = query(
            vec![Requirement {
                item: Some(ItemId::Spear),
                upgrade: UpgradeRequirement::Exact(3),
                ..requirement(ItemKind::Weapon)
            }],
            24,
        );
        let either = query(
            vec![
                Requirement {
                    item: Some(ItemId::Spear),
                    upgrade: UpgradeRequirement::Exact(3),
                    alternative_group: Some(1),
                    ..requirement(ItemKind::Weapon)
                },
                Requirement {
                    item: Some(ItemId::Sword),
                    upgrade: UpgradeRequirement::Exact(1),
                    alternative_group: Some(1),
                    ..requirement(ItemKind::Weapon)
                },
            ],
            24,
        );
        let alone = estimate_match_probability(&spear_only);
        let grouped = estimate_match_probability(&either);
        assert!(grouped >= alone, "{grouped:e} vs {alone:e}");
    }

    #[test]
    fn combined_upgrade_totals_cost_something() {
        let pair = |level_sum| {
            query(
                vec![
                    Requirement {
                        item: Some(ItemId::RingMight),
                        identity_group: Some(1),
                        level_sum,
                        ..requirement(ItemKind::Ring)
                    };
                    2
                ],
                24,
            )
        };
        let plain = estimate_match_probability(&pair(None));
        let modest = estimate_match_probability(&pair(Some(LevelSum {
            group: 1,
            minimum_total: 4,
        })));
        let steep = estimate_match_probability(&pair(Some(LevelSum {
            group: 1,
            minimum_total: 7,
        })));
        assert!(modest <= plain, "{modest:e} vs {plain:e}");
        assert!(steep < plain, "{steep:e} vs {plain:e}");
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

    #[test]
    fn a_wandmaker_quest_costs_exactly_its_spawn_and_variant_odds() {
        use crate::quests::WandmakerQuestType;

        let quested = |max_depth| SearchQuery {
            wandmaker_quest: Some(WandmakerQuestType::Rotberry),
            ..query(vec![requirement(ItemKind::Armor)], max_depth)
        };
        let open = |max_depth| query(vec![requirement(ItemKind::Armor)], max_depth);
        let ratio = |max_depth| {
            estimate_match_probability(&quested(max_depth))
                / estimate_match_probability(&open(max_depth))
        };

        // Below the Prison the Wandmaker never spawns at all.
        assert!(estimate_match_probability(&quested(6)) <= 0.0);
        // Floor seven spawns one run in three, floor eight two in three, and
        // floor nine always; one of three quests then has to be the wanted one.
        assert!((ratio(7) - 1.0 / 9.0).abs() < 1e-9);
        assert!((ratio(8) - 2.0 / 9.0).abs() < 1e-9);
        assert!((ratio(24) - 1.0 / 3.0).abs() < 1e-9);
    }
}
