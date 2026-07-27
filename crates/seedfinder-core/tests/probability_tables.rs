//! Guards the generated supply tables against generator drift.
//!
//! [`crate::probability_tables::measured`] is sampled from the canonical
//! generator, so a change to floor layout, room decks, or quest placement can
//! silently invalidate it. This re-measures a small sample and fails when a
//! source's output no longer resembles what the table claims, which is the cue
//! to rerun `cargo run --release --example calibrate_probability`.

use shpd_seedfinder_core::catalog::{ItemKind, item};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::main_world::CanonicalMainWorldGenerator;
use shpd_seedfinder_core::model::Accessibility;
use shpd_seedfinder_core::probability_tables::{DEEPEST_FLOOR, Line, SUPPLY, line_of};
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
                        let key = (
                            kind_slot(item(candidate.item).kind),
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
            kind: kind_of(kind),
            line,
            source,
            slots: count / worlds,
        })
        .collect()
}

const fn kind_slot(kind: ItemKind) -> usize {
    match kind {
        ItemKind::Weapon => 0,
        ItemKind::Armor => 1,
        ItemKind::Wand => 2,
        ItemKind::Ring => 3,
    }
}

const fn kind_of(slot: usize) -> ItemKind {
    match slot {
        0 => ItemKind::Weapon,
        1 => ItemKind::Armor,
        2 => ItemKind::Wand,
        _ => ItemKind::Ring,
    }
}
