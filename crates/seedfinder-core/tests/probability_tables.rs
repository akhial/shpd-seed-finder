//! Guards the generated supply tables against generator drift.
//!
//! [`crate::probability_tables::measured`] is sampled from the canonical
//! generator, so a change to floor layout, room decks, or quest placement can
//! silently invalidate it. This re-measures a small sample and fails when a
//! source's output no longer resembles what the table claims, which is the cue
//! to rerun `cargo run --release --example calibrate_probability`.

use shpd_seedfinder_core::catalog::{EXTRA_UPGRADE_MAXIMUM, ItemKind, item};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::main_world::CanonicalMainWorldGenerator;
use shpd_seedfinder_core::model::Accessibility;
use std::collections::BTreeMap;

use shpd_seedfinder_core::probability_tables::{
    DEEPEST_FLOOR, KINDS_ORDER, Line, MAX_TABLED_UPGRADE, SUPPLY, kind_index, line_of,
};
use shpd_seedfinder_core::search::WorldGenerator;
use shpd_seedfinder_core::seed::{DungeonSeed, TOTAL_SEEDS};

/// Enough worlds to catch a source appearing, vanishing, or changing size,
/// without slowing an unoptimised build to a crawl.
const WORLDS: u64 = 384;

/// Rows contributing less than this per world are too rare to measure here.
const MEASURABLE_SLOTS: f64 = 0.4;

/// Allowed drift beyond sampling noise.
const TOLERANCE: f64 = 0.2;

#[test]
fn tabled_supply_matches_the_generator() {
    let sampled = measure();
    let mut checked = 0;
    for supply in SUPPLY {
        let tabled: f64 = supply
            .depth_slots
            .iter()
            .map(|slots| f64::from(*slots))
            .sum();
        if tabled < MEASURABLE_SLOTS {
            continue;
        }
        checked += 1;
        let observed = sampled
            .iter()
            .find(|row| {
                row.kind == supply.kind
                    && row.line == supply.line
                    && row.source == format!("{:?}", supply.source)
            })
            .map_or(0.0, |row| row.slots);
        // Slot counts vary between worlds, so allow three standard errors of a
        // Poisson count on top of the flat tolerance.
        #[allow(clippy::cast_precision_loss)]
        let noise = 3.0 * (tabled / WORLDS as f64).sqrt();
        assert!(
            (observed - tabled).abs() <= tabled * TOLERANCE + noise,
            "{:?} {:?}{} supplies {observed:.3} slots per world, table says {tabled:.3}; \
             rerun the calibrate_probability example",
            supply.kind,
            supply.source,
            match supply.line {
                Line::Plain => "",
                Line::Thrown => " (thrown)",
                Line::Tipped => " (tipped)",
            }
        );
    }
    assert!(checked > 20, "only {checked} rows were measurable");
}

#[test]
fn every_tabled_distribution_is_normalised() {
    for supply in SUPPLY {
        let upgrades: f64 = supply
            .upgrades
            .iter()
            .map(|share| f64::from(*share))
            .sum::<f64>();
        assert!(
            (upgrades - 1.0).abs() < 1e-3,
            "{:?} {:?} upgrade shares sum to {upgrades}",
            supply.kind,
            supply.source
        );
        assert!(
            supply.options >= 1.0,
            "{:?} {:?} claims fewer than one option per slot",
            supply.kind,
            supply.source
        );
        if let Some(levels) = supply.levels {
            for (tier, carried) in (1..).zip(levels) {
                let total: f64 = carried.iter().map(|share| f64::from(*share)).sum();
                assert!(
                    total < 1e-3 || (total - 1.0).abs() < 1e-3,
                    "{:?} {:?} tier-{tier} upgrade shares sum to {total}",
                    supply.kind,
                    supply.source
                );
            }
        }
        for (floor_set, tiers) in supply.tiers.iter().enumerate() {
            let total: f64 = tiers.iter().map(|share| f64::from(*share)).sum();
            assert!(
                total < 1e-3 || (total - 1.0).abs() < 1e-3,
                "{:?} {:?} tier shares for floor set {floor_set} sum to {total}",
                supply.kind,
                supply.source
            );
        }
    }
}

struct Row {
    kind: ItemKind,
    line: Line,
    source: String,
    slots: f64,
}

fn measure() -> Vec<Row> {
    let generator = CanonicalMainWorldGenerator::with_challenges(Challenges::NONE);
    let stride = (TOTAL_SEEDS / WORLDS).max(1);
    let workers = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    let mut counted: std::collections::BTreeMap<(usize, Line, String), f64> =
        std::collections::BTreeMap::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for worker in 0..workers {
            let generator = &generator;
            handles.push(scope.spawn(move || {
                let mut local: std::collections::BTreeMap<(usize, Line, String), f64> =
                    std::collections::BTreeMap::new();
                let mut index = worker as u64;
                while index < WORLDS {
                    let value = index.wrapping_mul(stride) % TOTAL_SEEDS;
                    let seed = DungeonSeed::new(value).expect("stride stays in range");
                    let world = generator.generate(seed, DEEPEST_FLOOR);
                    let mut groups: std::collections::BTreeSet<(usize, Line, String, u16)> =
                        std::collections::BTreeSet::new();
                    for candidate in &world.items {
                        if matches!(
                            item(candidate.item).kind,
                            shpd_seedfinder_core::catalog::ItemKind::Trinket
                                | shpd_seedfinder_core::catalog::ItemKind::Artifact
                        ) {
                            continue;
                        }
                        let key = (
                            kind_index(item(candidate.item).kind),
                            line_of(candidate.item),
                            format!("{:?}", candidate.source),
                        );
                        // Rewards that exclude one another share one slot.
                        match candidate.accessibility {
                            Accessibility::Independent => *local.entry(key).or_default() += 1.0,
                            Accessibility::Choice { group, .. }
                            | Accessibility::Scenarios { group, .. } => {
                                if groups.insert((key.0, key.1, key.2.clone(), group)) {
                                    *local.entry(key).or_default() += 1.0;
                                }
                            }
                        }
                    }
                    index += workers as u64;
                }
                local
            }));
        }
        for handle in handles {
            for (key, value) in handle.join().expect("no worker panics") {
                *counted.entry(key).or_default() += value;
            }
        }
    });
    #[allow(clippy::cast_precision_loss)]
    let worlds = WORLDS as f64;
    counted
        .into_iter()
        .map(|((kind, line, source), count)| Row {
            kind: KINDS_ORDER[kind],
            line,
            source,
            slots: count / worlds,
        })
        .collect()
}

/// The estimator keeps no special case for a level the generator ties to a
/// single tier: it multiplies a row's two marginals unless the row carries a
/// levels table. That is only sound while every such level belongs to a locked
/// row — otherwise the level would be spread over every tier the row stocks.
///
/// `+5` is the one the generator ties this way, to a tier-4 weapon, and both
/// rows reaching it are the Imp's. This is the check that lets the estimator
/// treat every other row as a plain product.
#[test]
fn only_a_locked_row_reaches_the_level_tied_to_one_tier() {
    let mut reached = 0;
    for supply in SUPPLY {
        let extra = f64::from(supply.upgrades[usize::from(EXTRA_UPGRADE_MAXIMUM)]);
        if extra <= 0.0 {
            continue;
        }
        reached += 1;
        assert!(
            supply.levels.is_some(),
            "{:?} {:?} {:?} levels {extra} of its items to \
             +{EXTRA_UPGRADE_MAXIMUM} with no levels table to say which tier \
             carries it; rerun the calibrate_probability example",
            supply.kind,
            supply.line,
            supply.source
        );
    }
    assert!(reached >= 2, "only {reached} rows reached the level at all");
}

/// A tier and an upgrade level are tabled apart and multiplied, which is only
/// sound where the generator rolls them apart. This samples the pair and fails
/// both ways: when a locked row claims a combination the generator never
/// builds, and — the drift that hides — when an unlocked row's tier in fact
/// leans on its level, so multiplying the marginals invents items.
#[test]
fn tabled_levels_match_how_the_generator_pairs_tiers_and_upgrades() {
    let paired = measure_pairs();
    let mut locked = 0;
    let mut checked = 0;
    for supply in SUPPLY {
        let Some(observed) = paired
            .iter()
            .find(|row| {
                row.kind == supply.kind
                    && row.line == supply.line
                    && row.source == format!("{:?}", supply.source)
            })
            .map(|row| &row.levels)
        else {
            continue;
        };
        let name = format!("{:?} {:?} {:?}", supply.kind, supply.line, supply.source);
        if let Some(tabled) = supply.levels {
            locked += 1;
            for ((tier, upgrade), count) in observed {
                assert!(
                    tabled[tier - 1][*upgrade] > 0.0,
                    "{name} built {count} tier-{tier} items at +{upgrade}, which its \
                     levels table calls impossible; \
                     rerun the calibrate_probability example"
                );
            }
            continue;
        }
        let items: u64 = observed.values().sum();
        if items < MEASURABLE_ITEMS {
            continue;
        }
        checked += 1;
        let apart = independence_gap(observed, items);
        assert!(
            apart <= INDEPENDENT,
            "{name} pairs its tiers and upgrade levels {apart:.3} away from rolling \
             them apart, over {items} items; it needs a levels table"
        );
    }
    assert!(locked >= 3, "only {locked} locked rows were checked");
    assert!(checked > 15, "only {checked} unlocked rows were measurable");
}

/// Items a row needs before its pairing is worth measuring here.
const MEASURABLE_ITEMS: u64 = 250;

/// How far a row's joint may sit from the product of its marginals and still
/// count as rolling the two apart.
///
/// At [`WORLDS`] the sources that do roll them apart reach 0.06, and the
/// weakest lock the generator has — the Imp's reward, whose two weapons swap
/// which tier is levelled furthest — reaches 0.17.
const INDEPENDENT: f64 = 0.12;

/// Total-variation distance between a row's `(tier, upgrade)` counts and the
/// product of its own marginals: zero when the two roll apart, and one when a
/// tier names its level outright.
fn independence_gap(observed: &Pairing, items: u64) -> f64 {
    let mut tiers: BTreeMap<usize, u64> = BTreeMap::new();
    let mut upgrades: BTreeMap<usize, u64> = BTreeMap::new();
    for ((tier, upgrade), count) in observed {
        *tiers.entry(*tier).or_default() += count;
        *upgrades.entry(*upgrade).or_default() += count;
    }
    #[allow(clippy::cast_precision_loss)]
    let total = items as f64;
    let mut apart = 0.0;
    for (tier, in_tier) in &tiers {
        for (upgrade, at_level) in &upgrades {
            let together = observed.get(&(*tier, *upgrade)).copied().unwrap_or(0);
            #[allow(clippy::cast_precision_loss)]
            let seen = together as f64 / total;
            #[allow(clippy::cast_precision_loss)]
            let rolled_apart = (*in_tier as f64 / total) * (*at_level as f64 / total);
            apart += (seen - rolled_apart).abs();
        }
    }
    apart / 2.0
}

struct PairedRow {
    kind: ItemKind,
    line: Line,
    source: String,
    /// `(tier, upgrade)` counts, tiers numbered from one.
    levels: Pairing,
}

/// `(tier, upgrade)` counts for one row, tiers numbered from one.
type Pairing = BTreeMap<(usize, usize), u64>;

/// One row's key while counting: family, line, and source name.
type RowKey = (usize, Line, String);

fn measure_pairs() -> Vec<PairedRow> {
    let generator = CanonicalMainWorldGenerator::with_challenges(Challenges::NONE);
    let stride = (TOTAL_SEEDS / WORLDS).max(1);
    let workers = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    let mut counted: BTreeMap<RowKey, Pairing> = BTreeMap::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for worker in 0..workers {
            let generator = &generator;
            handles.push(scope.spawn(move || {
                let mut local: BTreeMap<RowKey, Pairing> = BTreeMap::new();
                let mut index = worker as u64;
                while index < WORLDS {
                    let value = index.wrapping_mul(stride) % TOTAL_SEEDS;
                    let seed = DungeonSeed::new(value).expect("stride stays in range");
                    for candidate in &generator.generate(seed, DEEPEST_FLOOR).items {
                        let definition = item(candidate.item);
                        let Some(tier) = definition.tier else {
                            continue;
                        };
                        let key = (
                            kind_index(definition.kind),
                            line_of(candidate.item),
                            format!("{:?}", candidate.source),
                        );
                        let upgrade = usize::from(candidate.upgrade).min(MAX_TABLED_UPGRADE);
                        *local
                            .entry(key)
                            .or_default()
                            .entry((usize::from(tier), upgrade))
                            .or_default() += 1;
                    }
                    index += workers as u64;
                }
                local
            }));
        }
        for handle in handles {
            for (key, pairs) in handle.join().expect("no worker panics") {
                let row = counted.entry(key).or_default();
                for (pair, count) in pairs {
                    *row.entry(pair).or_default() += count;
                }
            }
        }
    });
    counted
        .into_iter()
        .map(|((kind, line, source), levels)| PairedRow {
            kind: KINDS_ORDER[kind],
            line,
            source,
            levels,
        })
        .collect()
}
