//! Regenerates [`shpd_seedfinder_core::probability_tables`].
//!
//! The probability estimator needs to know how much equipment the canonical
//! generator actually produces on each floor, and how it is distributed over
//! upgrades, curses, enchantments, and tiers. Those quantities are emergent
//! properties of room decks, quest placement, and chest budgets rather than
//! constants anyone can read off the upstream source, so they are measured
//! here and baked into a generated table.
//!
//! Usage:
//!
//! ```text
//! cargo run --release --example calibrate_probability -- [WORLDS] \
//!     > crates/seedfinder-core/src/probability_tables/measured.rs
//! ```
//!
//! `tests/probability_tables.rs` re-measures a smaller sample and fails when
//! the checked-in table drifts away from the generator.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Mutex;

use shpd_seedfinder_core::catalog::{ItemKind, item};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::main_world::CanonicalMainWorldGenerator;
use shpd_seedfinder_core::model::GeneratedWorld;
use shpd_seedfinder_core::probability_tables::{
    DEEPEST_FLOOR, DEPTHS, FLOOR_SETS, IDENTITY_REPEAT_LIMIT, KINDS, MAX_TABLED_UPGRADE, TIERS,
    bundle_size, is_missile, kind_index, source_index, sources,
};
use shpd_seedfinder_core::search::WorldGenerator;
use shpd_seedfinder_core::seed::{DungeonSeed, TOTAL_SEEDS};

const DEFAULT_WORLDS: u64 = 200_000;

#[derive(Clone)]
struct Tally {
    worlds: u64,
    /// [kind][source][depth]: items generated
    counts: Vec<u64>,
    /// [kind][source][depth]: mutually exclusive reward groups, counted once
    slots: Vec<u64>,
    /// [kind][depth]: scattered slots within the depth prefix, summed over
    /// worlds, with their squares. Together they give how widely the running
    /// total varies around its mean.
    prefix: Vec<u64>,
    prefix_squares: Vec<u64>,
    /// [kind][source][upgrade]
    upgrades: Vec<u64>,
    /// [kind][source]
    cursed: Vec<u64>,
    enchanted: Vec<u64>,
    totals: Vec<u64>,
    /// [kind][source][floor set][tier]
    tiers: Vec<u64>,
    /// [kind][repeats][depth]: worlds containing at least `repeats + 1` items
    /// of one identity within the depth prefix, summed over identities.
    repeats: Vec<u64>,
    /// [kind][identity][depth]
    identity_counts: Vec<u64>,
}

impl Tally {
    fn new() -> Self {
        Self {
            worlds: 0,
            counts: vec![0; KINDS * FAMILIES * MAX_SOURCES * DEPTHS],
            slots: vec![0; KINDS * FAMILIES * MAX_SOURCES * DEPTHS],
            prefix: vec![0; KINDS * FAMILIES * DEPTHS],
            prefix_squares: vec![0; KINDS * FAMILIES * DEPTHS],
            upgrades: vec![0; KINDS * FAMILIES * MAX_SOURCES * (MAX_TABLED_UPGRADE + 1)],
            cursed: vec![0; KINDS * FAMILIES * MAX_SOURCES],
            enchanted: vec![0; KINDS * FAMILIES * MAX_SOURCES],
            totals: vec![0; KINDS * FAMILIES * MAX_SOURCES],
            tiers: vec![0; KINDS * FAMILIES * MAX_SOURCES * FLOOR_SETS * TIERS],
            repeats: vec![0; KINDS * FAMILIES * IDENTITY_REPEAT_LIMIT * DEPTHS],
            identity_counts: vec![0; KINDS * FAMILIES * MAX_IDENTITIES * DEPTHS],
        }
    }

    fn merge(&mut self, other: &Self) {
        self.worlds += other.worlds;
        for (target, value) in self.counts.iter_mut().zip(&other.counts) {
            *target += value;
        }
        for (target, value) in self.slots.iter_mut().zip(&other.slots) {
            *target += value;
        }
        for (target, value) in self.prefix.iter_mut().zip(&other.prefix) {
            *target += value;
        }
        for (target, value) in self.prefix_squares.iter_mut().zip(&other.prefix_squares) {
            *target += value;
        }
        for (target, value) in self.upgrades.iter_mut().zip(&other.upgrades) {
            *target += value;
        }
        for (target, value) in self.cursed.iter_mut().zip(&other.cursed) {
            *target += value;
        }
        for (target, value) in self.enchanted.iter_mut().zip(&other.enchanted) {
            *target += value;
        }
        for (target, value) in self.totals.iter_mut().zip(&other.totals) {
            *target += value;
        }
        for (target, value) in self.tiers.iter_mut().zip(&other.tiers) {
            *target += value;
        }
        for (target, value) in self.repeats.iter_mut().zip(&other.repeats) {
            *target += value;
        }
        for (target, value) in self.identity_counts.iter_mut().zip(&other.identity_counts) {
            *target += value;
        }
    }
}

/// How many items of one group can be carried out of a world together.
///
/// Simple either/or rewards leave exactly one; rooms whose reachability depends
/// on keys and doors enumerate their feasible plans as a bit set, and the best
/// plan is the one covering the most rewards.
fn co_obtainable(masks: &[u64]) -> u64 {
    (0..u64::BITS)
        .map(|plan| masks.iter().filter(|mask| *mask & (1 << plan) != 0).count() as u64)
        .max()
        .unwrap_or(0)
        .max(1)
}

const MAX_SOURCES: usize = 17;

/// Melee and thrown weapons are tallied into separate halves of every table.
const FAMILIES: usize = 2;
const MAX_IDENTITIES: usize = 96;

fn main() {
    let worlds: u64 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_WORLDS);
    let tally = measure(worlds);
    print!("{}", render(&tally));
}

fn measure(worlds: u64) -> Tally {
    let generator = CanonicalMainWorldGenerator::with_challenges(Challenges::NONE);
    let workers =
        std::thread::available_parallelism().map_or(8, std::num::NonZeroUsize::get) as u64;
    let merged = Mutex::new(Tally::new());
    let stride = (TOTAL_SEEDS / worlds.max(1)).max(1);
    std::thread::scope(|scope| {
        for worker in 0..workers {
            let generator = &generator;
            let merged = &merged;
            scope.spawn(move || {
                let mut local = Tally::new();
                let mut index = worker;
                while index < worlds {
                    let value = index.wrapping_mul(stride) % TOTAL_SEEDS;
                    let world = generator.generate(
                        DungeonSeed::new(value).expect("stride stays inside the seed space"),
                        DEEPEST_FLOOR,
                    );
                    local.record(&world);
                    index += workers;
                }
                merged.lock().expect("no worker panics").merge(&local);
            });
        }
    });
    merged.into_inner().expect("no worker panics")
}

impl Tally {
    fn record(&mut self, world: &GeneratedWorld) {
        self.worlds += 1;
        let mut identity_depths: BTreeMap<(usize, u8), Vec<u8>> = BTreeMap::new();
        let mut reward_groups: BTreeMap<(usize, usize, usize, u16), Vec<u64>> = BTreeMap::new();
        let mut scattered: BTreeMap<usize, u64> = BTreeMap::new();
        for candidate in &world.items {
            let definition = item(candidate.item);
            let kind =
                kind_index(definition.kind) + if is_missile(candidate.item) { KINDS } else { 0 };
            let source = source_index(candidate.source);
            let depth = usize::from(candidate.depth) - 1;
            let row = kind * MAX_SOURCES + source;
            self.counts[row * DEPTHS + depth] += 1;
            self.totals[row] += 1;
            let upgrade = usize::from(candidate.upgrade).min(MAX_TABLED_UPGRADE);
            self.upgrades[row * (MAX_TABLED_UPGRADE + 1) + upgrade] += 1;
            if candidate.cursed {
                self.cursed[row] += 1;
            }
            if candidate.effect.is_some_and(|effect| !effect.is_curse()) {
                self.enchanted[row] += 1;
            }
            if let Some(tier) = definition.tier {
                let floor_set = (depth / 5).min(FLOOR_SETS - 1);
                self.tiers[(row * FLOOR_SETS + floor_set) * TIERS + usize::from(tier) - 1] += 1;
            }
            // Rewards that exclude one another share a slot: a query can only
            // ever claim one of them.
            let fresh = if let Some((group, mask)) = candidate.accessibility.scenario_constraint() {
                let members = reward_groups
                    .entry((kind, source, depth, group))
                    .or_default();
                let fresh = members.is_empty();
                members.push(mask);
                fresh
            } else {
                self.slots[row * DEPTHS + depth] += 1;
                true
            };
            // Scattered supply is what the estimator treats as random arrivals,
            // so only it feeds the spread of the running total.
            if fresh && bundle_size(candidate.source, definition.kind) == 0 {
                *scattered.entry(kind * DEPTHS + depth).or_default() += 1;
            }
            identity_depths
                .entry((kind, candidate.item as u8))
                .or_default()
                .push(candidate.depth);
        }
        for ((kind, source, depth, _), masks) in &reward_groups {
            self.slots[(kind * MAX_SOURCES + source) * DEPTHS + depth] += co_obtainable(masks);
        }
        self.record_prefixes(&scattered);
        self.record_identities(&identity_depths);
    }

    /// Running totals of scattered slots, floor by floor.
    fn record_prefixes(&mut self, scattered: &BTreeMap<usize, u64>) {
        for kind in 0..KINDS * FAMILIES {
            let mut running = 0;
            for depth in 0..DEPTHS {
                running += scattered
                    .get(&(kind * DEPTHS + depth))
                    .copied()
                    .unwrap_or(0);
                self.prefix[kind * DEPTHS + depth] += running;
                self.prefix_squares[kind * DEPTHS + depth] += running * running;
            }
        }
    }

    /// How often one identity turns up more than once within a depth prefix.
    fn record_identities(&mut self, identity_depths: &BTreeMap<(usize, u8), Vec<u8>>) {
        for ((kind, identity), depths) in identity_depths {
            let mut sorted = depths.clone();
            sorted.sort_unstable();
            for depth in 0..DEPTHS {
                let limit = u8::try_from(depth + 1).unwrap_or(u8::MAX);
                let seen = sorted.iter().take_while(|value| **value <= limit).count();
                if seen > 0 {
                    self.identity_counts
                        [(kind * MAX_IDENTITIES + usize::from(*identity)) * DEPTHS + depth] +=
                        seen as u64;
                }
                for repeats in 0..IDENTITY_REPEAT_LIMIT.min(seen) {
                    self.repeats[(kind * IDENTITY_REPEAT_LIMIT + repeats) * DEPTHS + depth] += 1;
                }
            }
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn render(tally: &Tally) -> String {
    let mut output = String::new();
    let _ = writeln!(
        output,
        "//! Equipment supply measured from the canonical v3.3.8 generator.\n\
         //!\n\
         //! Generated over {} sampled worlds by\n\
         //! `cargo run --release --example calibrate_probability`. Rerun that\n\
         //! example and replace this file rather than editing it by hand.\n\n\
         use crate::catalog::ItemKind;\n\
         use crate::model::ItemSource;\n\n\
         use super::{{DEPTHS, IDENTITY_REPEAT_LIMIT, KINDS, Supply}};",
        tally.worlds
    );
    render_supply(tally, &mut output);
    render_spread(tally, &mut output);
    render_repeats(tally, &mut output);
    output
}

#[allow(clippy::cast_precision_loss)]
fn render_supply(tally: &Tally, output: &mut String) {
    let worlds = tally.worlds as f64;
    let _ = writeln!(
        output,
        "\n/// Every measured source/family combination.\npub static SUPPLY: &[Supply] = &["
    );
    for (family, kind) in KINDS_ORDER
        .into_iter()
        .flat_map(|kind| [(false, kind), (true, kind)])
    {
        for &source in sources() {
            let kind_slot = kind_index(kind) + usize::from(family) * KINDS;
            let source_slot = source_index(source);
            let total = tally.totals[kind_slot * MAX_SOURCES + source_slot];
            if total == 0 {
                continue;
            }
            let counts: Vec<f64> = (0..DEPTHS)
                .map(|depth| {
                    tally.slots[(kind_slot * MAX_SOURCES + source_slot) * DEPTHS + depth] as f64
                        / worlds
                })
                .collect();
            let slots: u64 = (0..DEPTHS)
                .map(|depth| tally.slots[(kind_slot * MAX_SOURCES + source_slot) * DEPTHS + depth])
                .sum();
            let options = if slots == 0 {
                1.0
            } else {
                total as f64 / slots as f64
            };

            let upgrades: Vec<f64> = (0..=MAX_TABLED_UPGRADE)
                .map(|upgrade| {
                    tally.upgrades[(kind_slot * MAX_SOURCES + source_slot)
                        * (MAX_TABLED_UPGRADE + 1)
                        + upgrade] as f64
                        / total as f64
                })
                .collect();
            let cursed = tally.cursed[kind_slot * MAX_SOURCES + source_slot] as f64 / total as f64;
            let enchanted =
                tally.enchanted[kind_slot * MAX_SOURCES + source_slot] as f64 / total as f64;
            let _ = writeln!(output, "    Supply {{");
            let _ = writeln!(output, "        kind: ItemKind::{kind:?},");
            let _ = writeln!(output, "        missile: {family},");
            let _ = writeln!(output, "        source: ItemSource::{source:?},");
            let _ = writeln!(output, "        bundle: {},", bundle_size(source, kind));
            let _ = writeln!(output, "        options: {},", format_number(options));

            let _ = writeln!(
                output,
                "        depth_slots: [{}],",
                counts
                    .iter()
                    .map(|value| format_number(*value))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let _ = writeln!(
                output,
                "        upgrades: [{}],",
                upgrades
                    .iter()
                    .map(|value| format_number(*value))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let _ = writeln!(output, "        cursed: {},", format_number(cursed));
            let _ = writeln!(output, "        enchanted: {},", format_number(enchanted));
            let _ = writeln!(output, "        tiers: [");
            for floor_set in 0..FLOOR_SETS {
                let observed: Vec<u64> = (0..TIERS)
                    .map(|tier| {
                        tally.tiers[((kind_slot * MAX_SOURCES + source_slot) * FLOOR_SETS
                            + floor_set)
                            * TIERS
                            + tier]
                    })
                    .collect();
                let sum: u64 = observed.iter().sum();
                let row: Vec<String> = observed
                    .iter()
                    .map(|value| {
                        format_number(if sum == 0 {
                            0.0
                        } else {
                            *value as f64 / sum as f64
                        })
                    })
                    .collect();
                let _ = writeln!(output, "            [{}],", row.join(", "));
            }
            let _ = writeln!(output, "        ],");
            let _ = writeln!(output, "    }},");
        }
    }
    let _ = writeln!(output, "];");
}

#[allow(clippy::cast_precision_loss)]
fn render_spread(tally: &Tally, output: &mut String) {
    let worlds = tally.worlds as f64;
    let _ = writeln!(
        output,
        "\n/// How widely the number of scattered slots varies within a depth\n\
         /// prefix, as variance over mean.\n\
         ///\n\
         /// Items are dealt from a decrementing category deck rather than drawn\n\
         /// independently, so a run produces a steadier stream than a Poisson\n\
         /// process would and these sit below one. Entry `[family][depth - 1]`\n\
         /// covers floors one through `depth`; families run melee, armor, wand,\n\
         /// ring, then the thrown weapons.\n\
         pub static SLOT_SPREAD: [[f32; DEPTHS]; KINDS * 2] = ["
    );
    for family in 0..KINDS * 2 {
        let row: Vec<String> = (0..DEPTHS)
            .map(|depth| {
                let mean = tally.prefix[family * DEPTHS + depth] as f64 / worlds;
                let square = tally.prefix_squares[family * DEPTHS + depth] as f64 / worlds;
                let variance = square - mean * mean;
                format_number(if mean <= 0.0 {
                    1.0
                } else {
                    (variance / mean).clamp(0.05, 1.0)
                })
            })
            .collect();
        let _ = writeln!(output, "    [{}],", row.join(", "));
    }
    let _ = writeln!(output, "];");
}

#[allow(clippy::cast_precision_loss)]
fn render_repeats(tally: &Tally, output: &mut String) {
    let worlds = tally.worlds as f64;
    let _ = writeln!(
        output,
        "\n/// Correction for same-identity duplicates.\n\
         ///\n\
         /// Wands, rings, weapons, and armor are drawn from decrementing decks,\n\
         /// so a world holds fewer copies of one identity than independent draws\n\
         /// would predict. Entry `[kind][copies - 1][depth - 1]` scales the\n\
         /// independent estimate of holding `copies` items of a single identity\n\
         /// within the depth prefix.\n\
         pub static IDENTITY_REPEATS: [[[f32; DEPTHS]; IDENTITY_REPEAT_LIMIT]; KINDS] = ["
    );
    for kind in KINDS_ORDER {
        let kind_slot = kind_index(kind);
        let _ = writeln!(output, "    // {kind:?}");
        let _ = writeln!(output, "    [");
        for repeats in 0..IDENTITY_REPEAT_LIMIT {
            let row: Vec<String> = (0..DEPTHS)
                .map(|depth| {
                    let observed = tally.repeats
                        [(kind_slot * IDENTITY_REPEAT_LIMIT + repeats) * DEPTHS + depth]
                        as f64
                        / worlds;
                    let independent: f64 = (0..MAX_IDENTITIES)
                        .map(|identity| {
                            let mean = tally.identity_counts
                                [(kind_slot * MAX_IDENTITIES + identity) * DEPTHS + depth]
                                as f64
                                / worlds;
                            poisson_at_least(mean, repeats + 1)
                        })
                        .sum();
                    format_number(if independent <= 0.0 {
                        1.0
                    } else {
                        observed / independent
                    })
                })
                .collect();
            let _ = writeln!(output, "        [{}],", row.join(", "));
        }
        let _ = writeln!(output, "    ],");
    }
    let _ = writeln!(output, "];");
}

const KINDS_ORDER: [ItemKind; KINDS] = [
    ItemKind::Weapon,
    ItemKind::Armor,
    ItemKind::Wand,
    ItemKind::Ring,
];

#[allow(clippy::cast_precision_loss)]
fn poisson_at_least(mean: f64, count: usize) -> f64 {
    if mean <= 0.0 {
        return 0.0;
    }
    let mut term = (-mean).exp();
    let mut below = term;
    for step in 1..count {
        term *= mean / step as f64;
        below += term;
    }
    (1.0 - below).clamp(0.0, 1.0)
}

/// Formats a probability with the digit separators clippy's pedantic lints
/// expect from long literals.
#[allow(clippy::cast_possible_truncation)] // The table stores `f32`.
fn format_number(value: f64) -> String {
    if value == 0.0 {
        return "0.0".to_owned();
    }
    // The shortest representation that round-trips through `f32` keeps clippy's
    // excessive-precision lint quiet.
    let text = format!("{:?}", value as f32);
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let grouped: Vec<String> = fraction
        .as_bytes()
        .chunks(3)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect();
    format!("{whole}.{}", grouped.join("_"))
}
