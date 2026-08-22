//! Brute-force oracle for the slot-based matcher: alternative groups,
//! combined-upgrade totals, effect sets, identity groups and accessibility
//! scenarios are all exercised on random small worlds and random queries,
//! and the engine's answers are compared with an exhaustive enumeration of
//! every assignment. The continuation predicate is checked for soundness the
//! same way: whenever `candidate.continues(base)` holds, every world the
//! candidate matches must be a world the base matches.

use std::collections::BTreeMap;

use shpd_seedfinder_core::catalog::{
    ALL_ARMOR_EFFECTS, ALL_WEAPON_EFFECTS, Effect, ItemId, ItemKind, item,
};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
use shpd_seedfinder_core::query::{
    EffectRequirement, EffectSet, Requirement, SearchQuery, TierRequirement, UpgradeRequirement,
    UpgradeSum, scout_matches,
};
use shpd_seedfinder_core::quests::QuestSummary;
use shpd_seedfinder_core::seed::DungeonSeed;

const POOL: [ItemId; 8] = [
    ItemId::Sword,
    ItemId::Mace,
    ItemId::Spear,
    ItemId::Shuriken,
    ItemId::MailArmor,
    ItemId::WandFrost,
    ItemId::RingMight,
    ItemId::RingHaste,
];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % bound as u64).unwrap()
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

fn family_effects(kind: ItemKind) -> Vec<Effect> {
    match kind {
        ItemKind::Weapon => ALL_WEAPON_EFFECTS
            .iter()
            .copied()
            .map(Effect::Weapon)
            .collect(),
        ItemKind::Armor => ALL_ARMOR_EFFECTS
            .iter()
            .copied()
            .map(Effect::Armor)
            .collect(),
        ItemKind::Wand | ItemKind::Ring => Vec::new(),
    }
}

fn random_world(rng: &mut Rng) -> GeneratedWorld {
    let count = 1 + rng.below(6);
    let items = (0..count)
        .map(|_| {
            let id = POOL[rng.below(POOL.len())];
            let kind = item(id).kind;
            let effects = family_effects(kind);
            let effect = (!effects.is_empty() && rng.chance(50))
                .then(|| effects[rng.below(effects.len().min(6))]);
            let cursed = effect.is_some_and(Effect::is_curse) || rng.chance(15);
            let accessibility = match rng.below(4) {
                0 => Accessibility::Choice {
                    group: 1,
                    option: u8::try_from(rng.below(2)).unwrap(),
                },
                1 => Accessibility::Scenarios {
                    group: 2,
                    mask: 1 + u64::try_from(rng.below(7)).unwrap(),
                },
                _ => Accessibility::Independent,
            };
            WorldItem {
                item: id,
                upgrade: u8::try_from(rng.below(usize::from(kind.maximum_search_upgrade()) + 1))
                    .unwrap(),
                effect,
                cursed,
                depth: 1 + u8::try_from(rng.below(10)).unwrap(),
                source: if rng.chance(10) {
                    ItemSource::BlacksmithReward
                } else {
                    ItemSource::Heap
                },
                accessibility,
                secret: false,
            }
        })
        .collect();
    GeneratedWorld {
        seed: DungeonSeed::MIN,
        items,
        quests: QuestSummary::default(),
    }
}

fn random_requirement(rng: &mut Rng) -> Requirement {
    let id = POOL[rng.below(POOL.len())];
    let kind = item(id).kind;
    let cap = usize::from(kind.maximum_search_upgrade());
    let upgrade = match rng.below(3) {
        0 => UpgradeRequirement::Any,
        1 => UpgradeRequirement::Exact(1 + u8::try_from(rng.below(cap)).unwrap()),
        _ => UpgradeRequirement::AtLeast(u8::try_from(rng.below(cap + 1)).unwrap()),
    };
    let effects = family_effects(kind);
    let effect = if effects.is_empty() || rng.chance(50) {
        EffectRequirement::Any
    } else {
        let wanted: Vec<Effect> = effects
            .iter()
            .take(6)
            .copied()
            .filter(|_| rng.chance(40))
            .collect();
        match EffectSet::from_effects(wanted) {
            Some(set) => EffectRequirement::OneOf(set),
            None => EffectRequirement::OneOf(EffectSet::enchantments(kind).unwrap()),
        }
    };
    Requirement {
        kind,
        weapon_category: None,
        item: rng.chance(50).then_some(id),
        tier: TierRequirement::Any,
        upgrade,
        effect,
        require_uncursed: rng.chance(20),
        source: None,
        identity_group: rng
            .chance(25)
            .then(|| 1 + u8::try_from(rng.below(2)).unwrap()),
        max_depth: rng
            .chance(30)
            .then(|| 1 + u8::try_from(rng.below(10)).unwrap()),
        alternative_group: None,
        upgrade_sum: None,
    }
}

fn random_query(rng: &mut Rng) -> Option<SearchQuery> {
    let slots = 1 + rng.below(3);
    let mut requirements = Vec::new();
    for slot in 0..slots {
        let members = if rng.chance(40) { 1 + rng.below(3) } else { 1 };
        for _ in 0..members {
            let mut requirement = random_requirement(rng);
            if members > 1 {
                requirement.alternative_group = Some(u8::try_from(slot).unwrap() + 1);
            }
            requirements.push(requirement);
        }
    }
    if rng.chance(40) {
        // Put every single-member slot of one kind into a sum group.
        let singles: Vec<usize> = requirements
            .iter()
            .enumerate()
            .filter(|(_, requirement)| requirement.alternative_group.is_none())
            .map(|(index, _)| index)
            .collect();
        if singles.len() >= 2 {
            let capacity: u16 = singles
                .iter()
                .map(|&index| u16::from(requirements[index].maximum_upgrade()))
                .sum();
            let minimum_total = 1 + u8::try_from(rng.below(usize::from(capacity))).unwrap();
            for index in singles {
                requirements[index].upgrade_sum = Some(UpgradeSum {
                    group: 1,
                    minimum_total,
                });
            }
        }
    }
    let query = SearchQuery {
        requirements,
        max_depth: 10,
        challenges: Challenges::NONE,
        require_blacksmith: false,
        exclude_blacksmith_rewards: rng.chance(20),
        wandmaker_quest: None,
        fast_mode: false,
    };
    query.validate().ok().map(|()| query)
}

/// One slot's candidate members as `(requirement index, item index)` pairs
/// under the query's floor limits and blacksmith exclusion.
fn candidates(query: &SearchQuery, world: &GeneratedWorld) -> Vec<Vec<(usize, usize)>> {
    query
        .slots()
        .into_iter()
        .map(|slot| {
            let mut pairs = Vec::new();
            for member in slot {
                let requirement = &query.requirements[member];
                for (index, candidate) in world.items.iter().enumerate() {
                    if candidate.depth <= query.max_depth
                        && candidate.depth <= requirement.max_depth.unwrap_or(query.max_depth)
                        && (!query.exclude_blacksmith_rewards
                            || candidate.source != ItemSource::BlacksmithReward)
                        && requirement.matches(candidate)
                    {
                        pairs.push((member, index));
                    }
                }
            }
            pairs
        })
        .collect()
}

/// Whether a full or partial assignment respects every cross-item rule, and
/// how many slots it counts as satisfying (sum groups all-or-nothing).
fn score(
    query: &SearchQuery,
    world: &GeneratedWorld,
    chosen: &[Option<(usize, usize)>],
) -> Option<usize> {
    let mut used = vec![false; world.items.len()];
    let mut identities: BTreeMap<u8, ItemId> = BTreeMap::new();
    let mut scenarios: BTreeMap<u16, u64> = BTreeMap::new();
    let mut sums: BTreeMap<u8, (usize, u16)> = BTreeMap::new();
    for (member, index) in chosen.iter().flatten() {
        if std::mem::replace(&mut used[*index], true) {
            return None;
        }
        let requirement = &query.requirements[*member];
        let candidate = &world.items[*index];
        if let Some(group) = requirement.identity_group {
            if *identities.entry(group).or_insert(candidate.item) != candidate.item {
                return None;
            }
        }
        if let Some((group, mask)) = candidate.accessibility.scenario_constraint() {
            let compatible = scenarios.entry(group).or_insert(u64::MAX);
            *compatible &= mask;
            if *compatible == 0 {
                return None;
            }
        }
        if let Some(sum) = requirement.upgrade_sum {
            let entry = sums.entry(sum.group).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += u16::from(candidate.upgrade);
        }
    }
    let mut group_sizes: BTreeMap<u8, (usize, u16)> = BTreeMap::new();
    for requirement in &query.requirements {
        if let Some(sum) = requirement.upgrade_sum {
            let entry = group_sizes
                .entry(sum.group)
                .or_insert((0, u16::from(sum.minimum_total)));
            entry.0 += 1;
        }
    }
    let failed = |group: u8| {
        let (members, minimum_total) = group_sizes[&group];
        let (assigned, total) = sums.get(&group).copied().unwrap_or((0, 0));
        assigned < members || total < minimum_total
    };
    Some(
        chosen
            .iter()
            .flatten()
            .filter(|(member, _)| {
                query.requirements[*member]
                    .upgrade_sum
                    .is_none_or(|sum| !failed(sum.group))
            })
            .count(),
    )
}

fn best_partial(
    query: &SearchQuery,
    world: &GeneratedWorld,
    candidates: &[Vec<(usize, usize)>],
    slot: usize,
    chosen: &mut Vec<Option<(usize, usize)>>,
    full_only: bool,
) -> Option<usize> {
    if slot == candidates.len() {
        return score(query, world, chosen);
    }
    let mut best = None;
    for pair in &candidates[slot] {
        chosen.push(Some(*pair));
        if let Some(value) = best_partial(query, world, candidates, slot + 1, chosen, full_only) {
            best = Some(best.map_or(value, |current: usize| current.max(value)));
        }
        chosen.pop();
    }
    if !full_only {
        chosen.push(None);
        if let Some(value) = best_partial(query, world, candidates, slot + 1, chosen, full_only) {
            best = Some(best.map_or(value, |current: usize| current.max(value)));
        }
        chosen.pop();
    }
    best
}

#[test]
fn matcher_and_scout_agree_with_exhaustive_enumeration() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut checked = 0;
    let mut matched = 0;
    while checked < 3_000 {
        let Some(query) = random_query(&mut rng) else {
            continue;
        };
        let world = random_world(&mut rng);
        let candidates = candidates(&query, &world);
        // A full assignment whose sum group falls short scores below the
        // slot count, so only a perfect score is a match.
        let full = best_partial(&query, &world, &candidates, 0, &mut Vec::new(), true);
        let expected = full == Some(query.slot_count());
        assert_eq!(
            query.matches(&world),
            expected,
            "matcher disagrees with brute force for {query:?} on {world:?}"
        );
        let best = best_partial(&query, &world, &candidates, 0, &mut Vec::new(), false)
            .expect("skipping every slot is always a valid selection");
        let marks = scout_matches(&world, &query);
        assert_eq!(marks.total_requirements, query.slot_count());
        assert_eq!(
            marks.matched_requirements, best,
            "scout disagrees with brute force for {query:?} on {world:?}"
        );
        assert_eq!(marks.matched_indices().len(), marks.matched_requirements);
        if expected {
            assert_eq!(marks.matched_requirements, query.slot_count());
        }
        checked += 1;
        matched += usize::from(expected);
    }
    // The generator must produce a healthy mix of matches and misses.
    assert!(
        matched > 200 && matched < 2_800,
        "{matched} of {checked} matched"
    );
}

/// Random narrowing or widening edits, so that continuation is sometimes
/// true and its soundness can be checked on real worlds.
fn mutate(rng: &mut Rng, base: &SearchQuery) -> Option<SearchQuery> {
    let mut query = base.clone();
    for _ in 0..=rng.below(3) {
        let index = rng.below(query.requirements.len());
        let requirement = &mut query.requirements[index];
        match rng.below(9) {
            0 => {
                requirement.item = Some(POOL[rng.below(POOL.len())]);
                requirement.kind = item(requirement.item.unwrap()).kind;
            }
            1 => {
                requirement.upgrade =
                    UpgradeRequirement::AtLeast(1 + u8::try_from(rng.below(3)).unwrap());
            }
            2 => requirement.upgrade = UpgradeRequirement::Any,
            3 => requirement.require_uncursed = !requirement.require_uncursed,
            4 => {
                requirement.max_depth = rng
                    .chance(50)
                    .then(|| 1 + u8::try_from(rng.below(10)).unwrap());
            }
            5 => {
                // Drop an alternative member.
                if requirement.alternative_group.is_some() {
                    query.requirements.remove(index);
                }
            }
            6 => {
                // Raise or lower a sum total for a whole group.
                if let Some(sum) = requirement.upgrade_sum {
                    let delta = if rng.chance(50) { 1 } else { -1 };
                    let total = i16::from(sum.minimum_total) + delta;
                    let total = u8::try_from(total.max(1)).unwrap();
                    for other in &mut query.requirements {
                        if let Some(other_sum) = &mut other.upgrade_sum
                            && other_sum.group == sum.group
                        {
                            other_sum.minimum_total = total;
                        }
                    }
                }
            }
            7 => {
                // Add a requirement, possibly as a new alternative of a slot.
                let mut added = random_requirement(rng);
                if rng.chance(50) {
                    added.alternative_group = requirement.alternative_group;
                }
                query.requirements.push(added);
            }
            _ => {
                // Drop a sum group from everyone.
                for other in &mut query.requirements {
                    other.upgrade_sum = None;
                }
            }
        }
        if query.requirements.is_empty() {
            return None;
        }
    }
    query.validate().ok().map(|()| query)
}

#[test]
fn continuation_never_admits_a_world_the_base_rejects() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let worlds: Vec<GeneratedWorld> = (0..400).map(|_| random_world(&mut rng)).collect();
    let mut continued = 0;
    let mut checked = 0;
    while checked < 2_000 {
        let Some(base) = random_query(&mut rng) else {
            continue;
        };
        let Some(candidate) = mutate(&mut rng, &base) else {
            continue;
        };
        checked += 1;
        if !candidate.continues(&base) {
            continue;
        }
        continued += 1;
        for world in &worlds {
            if candidate.matches(world) {
                assert!(
                    base.matches(world),
                    "{candidate:?} continues {base:?} but matches a world the base rejects: {world:?}"
                );
            }
        }
    }
    assert!(
        continued > 100,
        "only {continued} of {checked} pairs continued"
    );
}
