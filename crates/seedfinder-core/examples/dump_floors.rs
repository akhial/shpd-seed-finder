//! Prints one seed's main-dungeon floors in a stable, diff-friendly text form.
//!
//! Each regular floor lists its size, Java `Arrays.hashCode` map fingerprint,
//! entrance, exit, feeling, every placed mob (kind and cell, sorted by cell)
//! and every searchable item the floor contributed. The format mirrors the
//! reduced parity fixtures of the Java oracles so that an oracle run and the
//! engine can be compared with an ordinary `diff`.
//!
//! Usage:
//!
//! ```text
//! cargo run --release --example dump_floors -- AAA-AAA-AAA [MAX_DEPTH] [CHALLENGE_MASK]
//! ```

use std::fmt::Write as _;
use std::process::ExitCode;

use shpd_seedfinder_core::catalog::item;
use shpd_seedfinder_core::caves_floor::generate_caves_floor;
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::city_boss_shop::generate_city_boss_shop;
use shpd_seedfinder_core::city_floor::generate_city_floor;
use shpd_seedfinder_core::halls_floor::generate_halls_floor;
use shpd_seedfinder_core::level::Level;
use shpd_seedfinder_core::level_prelude::LimitedDrops;
use shpd_seedfinder_core::model::{Accessibility, WorldItem};
use shpd_seedfinder_core::prison_floor::generate_prison_floor;
use shpd_seedfinder_core::quests::QuestState;
use shpd_seedfinder_core::rng::{RandomStack, seed_for_depth};
use shpd_seedfinder_core::run::RunState;
use shpd_seedfinder_core::seed::DungeonSeed;
use shpd_seedfinder_core::sewer_floor::generate_sewer_floor;
use shpd_seedfinder_core::shop::ShopRunState;

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(code) = arguments.first() else {
        eprintln!("usage: dump_floors SEED-CODE [MAX_DEPTH] [CHALLENGE_MASK]");
        return ExitCode::from(2);
    };
    let seed = match DungeonSeed::from_code(code) {
        Ok(seed) => seed,
        Err(error) => {
            eprintln!("invalid seed code {code:?}: {error}");
            return ExitCode::from(2);
        }
    };
    let max_depth = arguments
        .get(1)
        .map_or(Ok(24), |value| value.parse::<u8>())
        .unwrap_or(24)
        .clamp(1, 24);
    let challenges = arguments
        .get(2)
        .and_then(|value| value.parse::<u16>().ok())
        .and_then(|mask| Challenges::new(mask).ok())
        .unwrap_or(Challenges::NONE);

    match dump(seed, max_depth, challenges) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

// One match arm per region keeps the sequential driver readable as a whole.
#[allow(clippy::too_many_lines)]
fn dump(seed: DungeonSeed, max_depth: u8, challenges: Challenges) -> Result<String, String> {
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let mut run = RunState::with_challenges(dungeon_seed, challenges);
    let mut limited_drops = LimitedDrops::default();
    let mut quests = QuestState::new();
    let mut shop_run = ShopRunState::default();
    let mut random = RandomStack::with_base_seed(0);
    let mut output = String::new();
    writeln!(
        output,
        "seed {code} challenges {mask}",
        code = seed.to_code(),
        mask = challenges.bits()
    )
    .unwrap();

    for depth in 1..=u32::from(max_depth) {
        if matches!(depth, 5 | 10 | 15) {
            // State-neutral boss floors; the engine never simulates them.
            continue;
        }
        random.push(seed_for_depth(dungeon_seed, depth, 0));
        let floor = match depth {
            1..=4 => generate_sewer_floor(
                &mut run,
                &mut limited_drops,
                &mut quests,
                depth,
                &mut random,
            )
            .map(|floor| {
                let mobs = floor
                    .mobs
                    .mobs
                    .iter()
                    .map(|placed| (format!("{:?}", placed.mob.kind), placed.cell))
                    .collect();
                Floor::new("SewerLevel", &floor.painted.level, mobs, floor.world_items)
            })
            .map_err(|error| format!("depth {depth}: {error:?}"))?,
            6..=9 => generate_prison_floor(
                &mut run,
                &mut limited_drops,
                &mut quests,
                &mut shop_run,
                depth,
                &mut random,
            )
            .map(|floor| {
                let mobs = floor
                    .mobs
                    .mobs
                    .iter()
                    .map(|placed| (format!("{:?}", placed.mob.kind), placed.cell))
                    .collect();
                Floor::new("PrisonLevel", &floor.painted.level, mobs, floor.world_items)
            })
            .map_err(|error| format!("depth {depth}: {error:?}"))?,
            11..=14 => generate_caves_floor(
                &mut run,
                &mut limited_drops,
                &mut quests,
                &mut shop_run,
                depth,
                &mut random,
            )
            .map(|floor| {
                let mobs = floor
                    .mobs
                    .mobs
                    .iter()
                    .map(|placed| (format!("{:?}", placed.mob.kind), placed.cell))
                    .collect();
                Floor::new("CavesLevel", &floor.painted.level, mobs, floor.world_items)
            })
            .map_err(|error| format!("depth {depth}: {error:?}"))?,
            16..=19 => generate_city_floor(
                &mut run,
                &mut limited_drops,
                &mut quests,
                &mut shop_run,
                depth,
                &mut random,
            )
            .map(|floor| {
                let mobs = floor
                    .mobs
                    .mobs
                    .iter()
                    .map(|placed| (format!("{:?}", placed.mob.kind), placed.cell))
                    .collect();
                Floor::new("CityLevel", &floor.painted.level, mobs, floor.world_items)
            })
            .map_err(|error| format!("depth {depth}: {error:?}"))?,
            20 => generate_city_boss_shop(&mut run, &mut shop_run, &mut random)
                .map(|shop| Floor::shop_only("CityBossLevel", shop.world_items))
                .map_err(|error| format!("depth {depth}: {error:?}"))?,
            _ => generate_halls_floor(
                &mut run,
                &mut limited_drops,
                &mut quests,
                &mut shop_run,
                depth,
                &mut random,
            )
            .map(|floor| {
                let mobs = floor
                    .mobs
                    .mobs
                    .iter()
                    .map(|placed| (format!("{:?}", placed.mob.kind), placed.cell))
                    .collect();
                Floor::new("HallsLevel", &floor.painted.level, mobs, floor.world_items)
            })
            .map_err(|error| format!("depth {depth}: {error:?}"))?,
        };
        random.pop();
        floor.write(depth, &mut output);
    }
    writeln!(output, "quests {:?}", quests.summary()).unwrap();
    Ok(output)
}

struct Floor {
    class: &'static str,
    size: Option<(i32, i32)>,
    map_hash: Option<i32>,
    entrance: Option<usize>,
    exit: Option<usize>,
    feeling: Option<String>,
    mobs: Vec<(String, usize)>,
    items: Vec<WorldItem>,
}

impl Floor {
    fn new(
        class: &'static str,
        level: &Level,
        mut mobs: Vec<(String, usize)>,
        items: Vec<WorldItem>,
    ) -> Self {
        mobs.sort_by_key(|(_, cell)| *cell);
        Self {
            class,
            size: Some((level.map.width, level.map.height)),
            map_hash: Some(level.java_map_hash()),
            entrance: level.entrance(),
            exit: level.exit(),
            feeling: Some(format!("{:?}", level.feeling)),
            mobs,
            items,
        }
    }

    fn shop_only(class: &'static str, items: Vec<WorldItem>) -> Self {
        Self {
            class,
            size: None,
            map_hash: None,
            entrance: None,
            exit: None,
            feeling: None,
            mobs: Vec::new(),
            items,
        }
    }

    fn write(&self, depth: u32, output: &mut String) {
        write!(output, "depth {depth} {}", self.class).unwrap();
        if let (Some((width, height)), Some(hash)) = (self.size, self.map_hash) {
            write!(output, " size {width}x{height} map_hash {hash}").unwrap();
        }
        if let Some(entrance) = self.entrance {
            write!(output, " entrance {entrance}").unwrap();
        }
        if let Some(exit) = self.exit {
            write!(output, " exit {exit}").unwrap();
        }
        if let Some(feeling) = &self.feeling {
            write!(output, " feeling {feeling}").unwrap();
        }
        writeln!(output).unwrap();
        for (kind, cell) in &self.mobs {
            writeln!(output, "  mob {kind} {cell}").unwrap();
        }
        let mut items = self.items.iter().map(describe_item).collect::<Vec<_>>();
        items.sort();
        for line in items {
            writeln!(output, "  item {line}").unwrap();
        }
    }
}

fn describe_item(world_item: &WorldItem) -> String {
    let definition = item(world_item.item);
    let effect = world_item
        .effect
        .map_or_else(|| "-".to_owned(), |effect| effect.wire_name().to_owned());
    let accessibility = match world_item.accessibility {
        Accessibility::Independent => "independent".to_owned(),
        Accessibility::Choice { group, option } => format!("choice:{group}:{option}"),
        Accessibility::Scenarios { group, mask } => format!("scenarios:{group}:{mask:#x}"),
    };
    format!(
        "{source:?} {id} +{upgrade} cursed={cursed} effect={effect} secret={secret} {accessibility}",
        source = world_item.source,
        id = definition.stable_id,
        upgrade = world_item.upgrade,
        cursed = world_item.cursed,
        secret = world_item.secret,
    )
}
