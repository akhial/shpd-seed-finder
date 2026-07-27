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

use shpd_seedfinder_core::catalog::{ItemId, ItemKind, item};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::main_world::CanonicalMainWorldGenerator;
use shpd_seedfinder_core::model::GeneratedWorld;
use shpd_seedfinder_core::probability_tables::{
    DEEPEST_FLOOR, DEPTHS, FLOOR_SETS, IDENTITY_REPEAT_LIMIT, KINDS, LINES, Line,
    MAX_TABLED_UPGRADE, TIERS, TIPPED_DARTS, bundle_size, kind_index, line_index, line_of,
    source_index, sources,
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
    /// [kind][repeats][depth]: ways of choosing `repeats + 1` items of one
    /// identity within the depth prefix, summed over identities and worlds.
    repeats: Vec<u64>,
    /// [kind][identity][depth]
    identity_counts: Vec<u64>,
    /// [kind][source]: exclusive reward groups holding more than one item, and
    /// how many of those upgraded and cursed every member alike.
    grouped: Vec<u64>,
    agreeing: Vec<u64>,
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
            grouped: vec![0; KINDS * FAMILIES * MAX_SOURCES],
            agreeing: vec![0; KINDS * FAMILIES * MAX_SOURCES],
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
        for (target, value) in self.grouped.iter_mut().zip(&other.grouped) {
            *target += value;
        }
        for (target, value) in self.agreeing.iter_mut().zip(&other.agreeing) {
            *target += value;
        }
    }
}

/// One choice inside an exclusive reward group: the acquisition plans that
/// reach it, and the upgrade and curse it was rolled with.
type Alternative = (u64, u8, bool);

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

/// Melee weapons, thrown weapons, and tipped darts are tallied into separate
/// bands of every table.
const FAMILIES: usize = LINES;
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
        let mut reward_groups: BTreeMap<(usize, usize, usize, u16), Vec<Alternative>> =
            BTreeMap::new();
        let mut scattered: BTreeMap<usize, u64> = BTreeMap::new();
        for candidate in &world.items {
            let definition = item(candidate.item);
            let kind = kind_index(definition.kind) + KINDS * line_index(line_of(candidate.item));
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
                members.push((mask, candidate.upgrade, candidate.cursed));
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
        for ((kind, source, depth, _), members) in &reward_groups {
            let row = kind * MAX_SOURCES + source;
            let masks: Vec<u64> = members.iter().map(|(mask, _, _)| *mask).collect();
            self.slots[row * DEPTHS + depth] += co_obtainable(&masks);
            // Alternatives that always carry the same upgrade and curse were
            // rolled once between them, so asking for one is a single chance.
            if let Some((_, upgrade, cursed)) = members.first().filter(|_| members.len() > 1) {
                self.grouped[row] += 1;
                if members
                    .iter()
                    .all(|(_, other, curse)| other == upgrade && curse == cursed)
                {
                    self.agreeing[row] += 1;
                }
            }
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

    /// How many sets of same-identity copies a world offers within each depth
    /// prefix.
    ///
    /// Counting sets rather than worlds — a factorial moment rather than a tail
    /// probability — is what makes the answer survive the upgrade and curse
    /// filters a query puts on top of the identity: those thin every set alike.
    fn record_identities(&mut self, identity_depths: &BTreeMap<(usize, u8), Vec<u8>>) {
        for ((kind, identity), depths) in identity_depths {
            let mut sorted = depths.clone();
            sorted.sort_unstable();
            for depth in 0..DEPTHS {
                let limit = u8::try_from(depth + 1).unwrap_or(u8::MAX);
                let seen = sorted.iter().take_while(|value| **value <= limit).count();
                if seen == 0 {
                    continue;
                }
                self.identity_counts
                    [(kind * MAX_IDENTITIES + usize::from(*identity)) * DEPTHS + depth] +=
                    seen as u64;
                for repeats in 0..IDENTITY_REPEAT_LIMIT.min(seen) {
                    self.repeats[(kind * IDENTITY_REPEAT_LIMIT + repeats) * DEPTHS + depth] +=
                        choose(seen, repeats + 1);
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
         use super::{{DEPTHS, IDENTITY_REPEAT_LIMIT, KINDS, LINES, Line, Supply, TIPPED_DARTS}};",
        tally.worlds
    );
    render_supply(tally, &mut output);
    render_spread(tally, &mut output);
    render_repeats(tally, &mut output);
    render_tipped(tally, &mut output);
    output
}

#[allow(clippy::cast_precision_loss)]
fn render_supply(tally: &Tally, output: &mut String) {
    let worlds = tally.worlds as f64;
    let _ = writeln!(
        output,
        "\n/// Every measured source/family combination.\npub static SUPPLY: &[Supply] = &["
    );
    for (line, kind) in KINDS_ORDER
        .into_iter()
        .flat_map(|kind| LINES_ORDER.map(|line| (line, kind)))
    {
        for &source in sources() {
            let kind_slot = kind_index(kind) + line_index(line) * KINDS;
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
            let _ = writeln!(output, "        line: Line::{line:?},");
            let _ = writeln!(output, "        source: ItemSource::{source:?},");
            let _ = writeln!(output, "        bundle: {},", bundle_size(source, kind));
            let _ = writeln!(output, "        options: {},", format_number(options));
            let grouped = tally.grouped[kind_slot * MAX_SOURCES + source_slot];
            let agreeing = tally.agreeing[kind_slot * MAX_SOURCES + source_slot];
            let shared = grouped > 0 && agreeing as f64 / grouped as f64 > AGREEMENT;
            let _ = writeln!(output, "        shared_roll: {shared},");

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
         /// process would and these sit below one. Entry `[line][depth - 1]`\n\
         /// covers floors one through `depth`, with the rows running weapon,\n\
         /// armor, wand, ring for each line in turn.\n\
         pub static SLOT_SPREAD: [[f32; DEPTHS]; KINDS * LINES] = ["
    );
    for family in 0..KINDS * LINES {
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
         /// Every line is dealt from a decrementing deck, so a world offers\n\
         /// fewer sets of one identity than independent draws would predict.\n\
         /// Entry `[line][copies - 1][depth - 1]` scales the independent\n\
         /// estimate of holding `copies` items of a single identity within the\n\
         /// depth prefix. It compares how many such sets a world holds against\n\
         /// how many independent draws of the same average would offer.\n\
         pub static IDENTITY_REPEATS: [[[f32; DEPTHS]; IDENTITY_REPEAT_LIMIT]; KINDS * LINES] = ["
    );
    // Rows run in table order: every family for one line, then the next line.
    for (line, kind) in LINES_ORDER
        .into_iter()
        .flat_map(|line| KINDS_ORDER.map(|kind| (line, kind)))
    {
        let kind_slot = kind_index(kind) + line_index(line) * KINDS;
        let _ = writeln!(output, "    // {kind:?} {line:?}");
        let _ = writeln!(output, "    [");
        for repeats in 0..IDENTITY_REPEAT_LIMIT {
            let row: Vec<String> = (0..DEPTHS)
                .map(|depth| {
                    let observed = tally.repeats
                        [(kind_slot * IDENTITY_REPEAT_LIMIT + repeats) * DEPTHS + depth]
                        as f64
                        / worlds;
                    // The same count if every draw picked its identity afresh.
                    let power = i32::try_from(repeats + 1).unwrap_or(1);
                    let independent: f64 = (0..MAX_IDENTITIES)
                        .map(|identity| {
                            let mean = tally.identity_counts
                                [(kind_slot * MAX_IDENTITIES + identity) * DEPTHS + depth]
                                as f64
                                / worlds;
                            mean.powi(power) / factorial(repeats + 1)
                        })
                        .sum();
                    format_number(if independent <= 0.0 {
                        1.0
                    } else {
                        (observed / independent).clamp(0.0, MOST_REPEATS)
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

const LINES_ORDER: [Line; LINES] = [Line::Plain, Line::Thrown, Line::Tipped];

/// Widest the identity correction may run. A line whose repeats are too rare to
/// measure would otherwise hand the estimator a wild multiplier.
const MOST_REPEATS: f64 = 4.0;

/// How often a source's alternatives must agree before their upgrade and curse
/// are taken to be one roll. The generator either shares the roll or draws it
/// independently, so anything in between is sampling noise.
const AGREEMENT: f64 = 0.99;

#[allow(clippy::cast_precision_loss)]
fn factorial(count: usize) -> f64 {
    (1..=count).map(|step| step as f64).product()
}

/// Sets of `chosen` items that can be picked out of `count`.
fn choose(count: usize, chosen: usize) -> u64 {
    if chosen > count {
        return 0;
    }
    (0..chosen).fold(1_u64, |total, step| {
        total * (count - step) as u64 / (step as u64 + 1)
    })
}

/// Share of each tipped dart among the darts a run produces.
#[allow(clippy::cast_precision_loss)]
fn render_tipped(tally: &Tally, output: &mut String) {
    let line = kind_index(ItemKind::Weapon) + line_index(Line::Tipped) * KINDS;
    let counts: Vec<f64> = (0..TIPPED_DARTS)
        .map(|dart| {
            let identity = ItemId::RotDart as usize + dart;
            tally.identity_counts[(line * MAX_IDENTITIES + identity) * DEPTHS + DEPTHS - 1] as f64
        })
        .collect();
    let total: f64 = counts.iter().sum();
    let _ = writeln!(
        output,
        "\n/// Share of each tipped dart among the darts a run offers, in catalog\n\
         /// order from `RotDart`. The generator tips them from the plant seeds it\n\
         /// has on hand rather than dealing them evenly.\n\
         pub static TIPPED_SHARES: [f32; TIPPED_DARTS] = [{}];",
        counts
            .iter()
            .map(|count| format_number(if total <= 0.0 { 0.0 } else { count / total }))
            .collect::<Vec<_>>()
            .join(", ")
    );
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
