//! Materializes the canonical regional floors for parity-only terrain and
//! placement inspection. Production item matching is checked separately.
//! Map keys 101..=124 identify branch 1 at the corresponding main depth.
mod locations;
use std::fmt::Write as _;

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

// One match arm per region keeps the sequential driver readable as a whole.
#[allow(clippy::too_many_lines)]
pub fn dump(seed: DungeonSeed, max_depth: u8, challenges: Challenges) -> Result<String, String> {
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
                locations::collect(
                    &mut output,
                    &floor.painted.level,
                    &floor.regular_items,
                    &floor.painted.equipment_events,
                    &floor.painted.consumable_events,
                    &floor.painted.forced_events,
                    &floor.painted.secret_events,
                    &[],
                );

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
                locations::collect(
                    &mut output,
                    &floor.painted.level,
                    &floor.regular_items,
                    &floor.painted.equipment_events,
                    &floor.painted.consumable_events,
                    &floor.painted.forced_events,
                    &floor.painted.secret_events,
                    &floor.painted.quest_events,
                );

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
                locations::collect(
                    &mut output,
                    &floor.painted.level,
                    &floor.regular_items,
                    &floor.painted.equipment_events,
                    &floor.painted.consumable_events,
                    &floor.painted.forced_events,
                    &floor.painted.secret_events,
                    &floor.painted.quest_events,
                );

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
                locations::collect(
                    &mut output,
                    &floor.painted.level,
                    &floor.regular_items,
                    &floor.painted.equipment_events,
                    &floor.painted.consumable_events,
                    &floor.painted.forced_events,
                    &floor.painted.secret_events,
                    &floor.painted.quest_events,
                );

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
                locations::collect(
                    &mut output,
                    &floor.painted.level,
                    &floor.regular_items,
                    &floor.painted.equipment_events,
                    &floor.painted.consumable_events,
                    &floor.painted.forced_events,
                    &floor.painted.secret_events,
                    &floor.painted.quest_events,
                );

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
    if let Some(depth) = quests.imp.depth {
        let vault =
            shpd_seedfinder_core::vault_floor::generate_vault(dungeon_seed, depth, challenges)
                .map_err(|error| format!("vault: {error:?}"))?;
        for heap in &vault.heaps {
            if vault.room_at(heap.cell).is_some_and(|r| {
                vault.rooms[r].kind == shpd_seedfinder_core::vault_rooms::VaultRoomKind::Final
            }) {
                continue;
            }
            for value in &heap.items {
                if let Some(value) = value.equipment() {
                    writeln!(
                        output,
                        "loc {depth},1,{},{},{},0,{}",
                        heap.cell,
                        item(value.item).stable_id,
                        value.upgrade,
                        value.effect.map_or("-", |e| e.wire_name())
                    )
                    .unwrap();
                }
            }
        }
        Floor::new("VaultLevel", &vault.level, Vec::new(), Vec::new())
            .write(u32::from(depth) + 100, &mut output);
    }
    Ok(output)
}

struct Floor {
    class: &'static str,
    size: Option<(i32, i32)>,
    map_hash: Option<i32>,
    map: Vec<i32>,
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
            map: level.map.cells.clone(),
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
            map: Vec::new(),
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
        if !self.map.is_empty() {
            writeln!(
                output,
                "map {depth} {},{}:{}",
                self.size.unwrap().0,
                self.size.unwrap().1,
                self.map
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
            .unwrap();
        }
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
