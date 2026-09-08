//! Anonymous placements sampled from 4,096 BETA-4 worlds by
//! `cargo run --release --example calibrate_artifacts`. Identities are averaged
//! analytically over the eleven-card deck, not estimated from observed hits.
//! Each world is a count byte followed by 11-byte records: floor, source/curse,
//! local accessibility group, and a little-endian 64-bit scenario mask.
use super::{Predicate, artifact_identity_count, tally};
use crate::{
    model::ItemSource,
    probability_tables::{prize_group, sources},
    query::EffectRequirement,
};
use std::sync::OnceLock;

struct Placement {
    depth: u8,
    source: ItemSource,
    cursed: bool,
    group: u8,
    scenarios: u64,
}

fn worlds() -> &'static [Vec<Placement>] {
    static WORLDS: OnceLock<Vec<Vec<Placement>>> = OnceLock::new();
    WORLDS.get_or_init(|| {
        let mut bytes = &include_bytes!("../probability_tables/artifact_worlds.bin")[..];
        let mut worlds = Vec::new();
        while let Some((&count, tail)) = bytes.split_first() {
            let (records, tail) = tail.split_at(usize::from(count) * 11);
            worlds.push(
                records
                    .chunks_exact(11)
                    .map(|record| Placement {
                        depth: record[0],
                        source: sources()[usize::from(record[1] & 31)],
                        cursed: record[1] & 32 != 0,
                        group: record[2],
                        scenarios: u64::from_le_bytes(record[3..11].try_into().unwrap()),
                    })
                    .collect(),
            );
            bytes = tail;
        }
        worlds
    })
}

pub(super) fn probability(predicates: &[Predicate], open_only: bool) -> f64 {
    let mut ways = 0_u32;
    for world in worlds() {
        let mut eligible: Vec<Vec<usize>> = predicates
            .iter()
            .map(|predicate| {
                world
                    .iter()
                    .enumerate()
                    .filter_map(|(index, placement)| {
                        let upgrade = if placement.source == ItemSource::ImpReward {
                            5
                        } else {
                            0
                        };
                        (placement.depth <= predicate.max_depth
                            && predicate
                                .source
                                .is_none_or(|source| source == placement.source)
                            && (!predicate.require_uncursed || !placement.cursed)
                            && predicate.upgrades & (1 << upgrade) != 0
                            && predicate.effect == EffectRequirement::Any
                            && (!open_only || prize_group(placement.source).is_none()))
                        .then_some(index)
                    })
                    .collect()
            })
            .collect();
        eligible.sort_by_key(Vec::len);
        ways += assignments(world, &eligible, &mut Vec::new());
    }
    // Each injective placement of the named identities is a disjoint event.
    // The rest of the deck is unconstrained and cancels from the ratio.
    let denominator = (0..predicates.len()).fold(tally(worlds().len()), |denominator, used| {
        denominator * tally(artifact_identity_count() - used)
    });
    f64::from(ways) / denominator
}

fn assignments(world: &[Placement], eligible: &[Vec<usize>], chosen: &mut Vec<usize>) -> u32 {
    let Some((choices, tail)) = eligible.split_first() else {
        return 1;
    };
    let mut ways = 0;
    for &index in choices {
        if chosen.contains(&index) {
            continue;
        }
        let candidate = &world[index];
        if candidate.group != 0 {
            let scenarios = chosen
                .iter()
                .filter(|&&taken| world[taken].group == candidate.group)
                .fold(candidate.scenarios, |mask, &taken| {
                    mask & world[taken].scenarios
                });
            if scenarios == 0 {
                continue;
            }
        }
        chosen.push(index);
        ways += assignments(world, tail, chosen);
        chosen.pop();
    }
    ways
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(group: u8, scenarios: u64) -> Placement {
        Placement {
            depth: 1,
            source: ItemSource::Chest,
            cursed: false,
            group,
            scenarios,
        }
    }

    #[test]
    fn assignment_counts_use_distinct_positions_and_respect_shared_choices() {
        let independent: Vec<_> = (0..5).map(|_| placement(0, 0)).collect();
        assert_eq!(
            assignments(&independent, &vec![vec![0, 1, 2, 3, 4]; 3], &mut Vec::new()),
            60
        );
        let exclusive = [placement(1, 1), placement(1, 2)];
        assert_eq!(
            assignments(&exclusive, &vec![vec![0, 1]; 2], &mut Vec::new()),
            0
        );
        // Pairwise overlap alone is insufficient: no scenario admits all three.
        let scenarios = [placement(1, 3), placement(1, 6), placement(1, 5)];
        assert_eq!(
            assignments(&scenarios, &[vec![0], vec![1], vec![2]], &mut Vec::new()),
            0
        );
        assert_eq!(worlds().len(), 4096);
    }
}
