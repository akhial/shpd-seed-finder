//! Prints, per regular main-dungeon floor, the generation-visible state that
//! the upstream Java floor probe prints: map size and `Arrays.hashCode`,
//! entrance/exit cells, every occupied mob cell, and the searchable items.
//!
//! Usage: `floor_probe <SEED-CODE> [MAX_DEPTH]`.
use shpd_seedfinder_core::caves_floor::generate_caves_floor;
use shpd_seedfinder_core::city_boss_shop::generate_city_boss_shop;
use shpd_seedfinder_core::city_floor::generate_city_floor;
use shpd_seedfinder_core::halls_floor::generate_halls_floor;
use shpd_seedfinder_core::level::Level;
use shpd_seedfinder_core::level_prelude::LimitedDrops;
use shpd_seedfinder_core::model::WorldItem;
use shpd_seedfinder_core::prison_floor::generate_prison_floor;
use shpd_seedfinder_core::quests::QuestState;
use shpd_seedfinder_core::rng::{RandomStack, seed_for_depth};
use shpd_seedfinder_core::run::RunState;
use shpd_seedfinder_core::seed::DungeonSeed;
use shpd_seedfinder_core::sewer_floor::generate_sewer_floor;
use shpd_seedfinder_core::shop::ShopRunState;

fn print_placements(records: &[shpd_seedfinder_core::regular_items::RegularItemPlacementRecord]) {
    if std::env::args().nth(3).as_deref() == Some("map") {
        for record in records {
            println!(
                "  reg {:05} {:?} : {:?}",
                record.cell, record.destination, record.items
            );
        }
    }
}

fn print_floor(depth: u32, level: &Level, mobs: &[(usize, String)], items: &[WorldItem]) {
    println!(
        "depth {depth} {}x{} mapHash={} entrance={} exit={}",
        level.width(),
        level.height(),
        level.java_map_hash(),
        level.entrance().map_or(-1, |cell| cell as i64),
        level.exit().map_or(-1, |cell| cell as i64),
    );
    if std::env::args().nth(3).as_deref() == Some("map") {
        let width = level.width() as usize;
        for (y, row) in level.map.cells.chunks(width).enumerate() {
            let cells: Vec<String> = row.iter().map(ToString::to_string).collect();
            println!("  map {y}: {}", cells.join(" "));
        }
        for heap in &level.heaps {
            println!("  heap {:05} {:?} : {:?}", heap.cell, heap.kind, heap.items);
        }
        for mob in &level.mobs {
            println!("  paintmob {mob:?}");
        }
    }
    let mut all_mobs: Vec<(usize, String)> = level
        .mob_cells
        .iter()
        .enumerate()
        .filter(|&(cell, _)| !mobs.iter().any(|(mob_cell, _)| *mob_cell == cell))
        .filter_map(|(cell, &occupied)| occupied.then(|| (cell, "painted".to_string())))
        .collect();
    all_mobs.extend(mobs.iter().cloned());
    all_mobs.sort();
    for (cell, kind) in all_mobs {
        println!("  mob {cell:05} {kind}");
    }
    for item in items {
        println!(
            "  item {:?} +{}{}{} source={:?} access={:?}{}",
            item.item,
            item.upgrade,
            if item.cursed { " cursed" } else { "" },
            item.effect
                .map(|effect| format!(" {effect:?}"))
                .unwrap_or_default(),
            item.source,
            item.accessibility,
            if item.secret { " secret" } else { "" },
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = args.get(1).map_or("AAA-AAA-AAA", String::as_str);
    let max_depth: u32 = args.get(2).map_or(24, |value| value.parse().unwrap());
    let seed = DungeonSeed::from_code(code).unwrap();
    let dungeon_seed = i64::try_from(seed.value()).unwrap();
    println!("seed={dungeon_seed}");

    let mut run = RunState::new(dungeon_seed);
    let mut limited_drops = LimitedDrops::default();
    let mut quests = QuestState::new();
    let mut shop_run = ShopRunState::default();
    let mut random = RandomStack::with_base_seed(0);

    for depth in 1..=max_depth {
        match depth {
            5 | 10 | 15 => {
                println!("depth {depth} boss (skipped)");
                continue;
            }
            _ => {}
        }
        random.push(seed_for_depth(dungeon_seed, depth, 0));
        match depth {
            1..=4 => {
                let floor = generate_sewer_floor(
                    &mut run,
                    &mut limited_drops,
                    &mut quests,
                    depth,
                    &mut random,
                )
                .unwrap();
                let mut mobs: Vec<(usize, String)> = floor
                    .mobs
                    .mobs
                    .iter()
                    .map(|mob| (mob.cell, format!("{:?}", mob.mob.kind)))
                    .collect();
                if let Some(cell) = floor.mobs.ghost_cell {
                    mobs.push((cell, "Ghost".to_string()));
                }
                print_floor(depth, &floor.painted.level, &mobs, &floor.world_items);
                println!("  feeling {:?}", floor.painted.prepared.feeling);
                print_placements(&floor.regular_items.placements);
            }
            6..=9 => {
                let floor = generate_prison_floor(
                    &mut run,
                    &mut limited_drops,
                    &mut quests,
                    &mut shop_run,
                    depth,
                    &mut random,
                )
                .unwrap();
                let mut mobs: Vec<(usize, String)> = floor
                    .mobs
                    .mobs
                    .iter()
                    .map(|mob| (mob.cell, format!("{:?}", mob.mob.kind)))
                    .collect();
                if let Some(cell) = floor.mobs.wandmaker_cell {
                    mobs.push((cell, "Wandmaker".to_string()));
                }
                print_floor(depth, &floor.painted.level, &mobs, &floor.world_items);
                println!("  feeling {:?}", floor.painted.prepared.feeling);
                print_placements(&floor.regular_items.placements);
            }
            11..=14 => {
                let floor = generate_caves_floor(
                    &mut run,
                    &mut limited_drops,
                    &mut quests,
                    &mut shop_run,
                    depth,
                    &mut random,
                )
                .unwrap();
                let mobs: Vec<(usize, String)> = floor
                    .mobs
                    .mobs
                    .iter()
                    .map(|mob| (mob.cell, format!("{:?}", mob.mob.kind)))
                    .collect();
                print_floor(depth, &floor.painted.level, &mobs, &floor.world_items);
                println!("  feeling {:?}", floor.painted.prepared.feeling);
                print_placements(&floor.regular_items.placements);
            }
            16..=19 => {
                let floor = generate_city_floor(
                    &mut run,
                    &mut limited_drops,
                    &mut quests,
                    &mut shop_run,
                    depth,
                    &mut random,
                )
                .unwrap();
                let mobs: Vec<(usize, String)> = floor
                    .mobs
                    .mobs
                    .iter()
                    .map(|mob| (mob.cell, format!("{:?}", mob.mob.kind)))
                    .collect();
                print_floor(depth, &floor.painted.level, &mobs, &floor.world_items);
                println!("  feeling {:?}", floor.painted.prepared.feeling);
                print_placements(&floor.regular_items.placements);
            }
            20 => {
                let shop = generate_city_boss_shop(&mut run, &mut shop_run, &mut random).unwrap();
                println!("depth 20 boss shop");
                if std::env::args().nth(3).as_deref() == Some("map") {
                    for item in &shop.inventory.items {
                        println!("  shopstock : {item:?}");
                    }
                }
                for item in &shop.world_items {
                    println!(
                        "  shopitem {:?} +{}{}{} source={:?}",
                        item.item,
                        item.upgrade,
                        if item.cursed { " cursed" } else { "" },
                        item.effect
                            .map(|effect| format!(" {effect:?}"))
                            .unwrap_or_default(),
                        item.source,
                    );
                }
            }
            21..=24 => {
                let floor = generate_halls_floor(
                    &mut run,
                    &mut limited_drops,
                    &mut quests,
                    &mut shop_run,
                    depth,
                    &mut random,
                )
                .unwrap();
                let mobs: Vec<(usize, String)> = floor
                    .mobs
                    .mobs
                    .iter()
                    .map(|mob| (mob.cell, format!("{:?}", mob.mob.kind)))
                    .collect();
                print_floor(depth, &floor.painted.level, &mobs, &floor.world_items);
                println!("  feeling {:?}", floor.painted.prepared.feeling);
                print_placements(&floor.regular_items.placements);
            }
            _ => unreachable!(),
        }
        random.pop();
    }
    println!("quests={:?}", quests.summary());
}
