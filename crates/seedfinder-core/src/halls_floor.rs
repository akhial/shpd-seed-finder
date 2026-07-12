//! Parity-gated composition of regular Halls floors (depths 21 through 24).
//!
//! A Halls world prefix advances one live run through Sewers, Prison, Caves,
//! City, and the depth-20 City boss shop while skipping the state-neutral boss
//! floors at 5, 10, and 15. The Last Shop at depth 21 therefore observes all
//! earlier shop bag state and Generator mutations.

#![allow(clippy::missing_panics_doc, clippy::too_many_lines)]

use std::cell::RefCell;
use std::fmt;

use crate::batch::seed_for_depth_batch4;
use crate::builder::{Builder, RegularLevelBuilder};
use crate::caves_floor::{CavesFloorError, generate_caves_floor};
use crate::city_boss_shop::generate_city_boss_shop;
use crate::city_floor::{CityFloorError, generate_city_floor};
use crate::geometry::{Point, Rect, painter as draw, terrain};
use crate::halls_mobs::{HallsMobsResult, create_halls_mobs};
use crate::halls_rooms::{HallsPainter, HallsRoomDispatcher};
use crate::level::Level;
use crate::level_flags::LevelFlags;
use crate::level_prelude::LimitedDrops;
use crate::model::{GeneratedWorld, WorldItem};
use crate::painter::{PaintError, RoomPaintDispatch, draw_regular_trap_count};
use crate::prison_floor::{PrisonFloorError, generate_prison_floor};
use crate::quest_rooms::{
    QuestLevelPaintContext, QuestPaintEvent, QuestPaintOutcome, QuestPaintState,
    can_merge as quest_can_merge, can_place_character as quest_can_place_character,
    can_place_grass as quest_can_place_grass, can_place_trap as quest_can_place_trap,
    can_place_water as quest_can_place_water, paint_quest_room,
};
use crate::quests::{QuestError, QuestState};
use crate::regular_items::{
    QueuedItemKind, RegularItem, RegularItemsError, RegularItemsResult, create_regular_items,
    level_create_queue,
};
use crate::regular_level::{
    BuiltRegularGraph, PreparedRegularFloor, init_regular_rooms, prepare_regular_floor,
};
use crate::regular_placement::{CanonicalRoomPlacementRules, PaintedRegularPlacement};
use crate::rng::{RandomStack, seed_for_depth};
use crate::room::{Room, RoomId, RoomKind, SpecialRoomKind, StandardRoomKind};
use crate::run::RunState;
use crate::search::WorldGenerator;
use crate::secret_rooms::{
    SecretLevelPaintContext, SecretPaintEvent, SecretPaintOutcome,
    can_place_grass as secret_can_place_grass, can_place_trap as secret_can_place_trap,
    can_place_water as secret_can_place_water, paint_secret_room,
};
use crate::seed::DungeonSeed;
use crate::sewer_floor::{
    SewerFloorError, SharedGenerator, SharedPrizes, SharedSewerContent, append_painted_room_items,
    generate_sewer_floor, regular_to_special, remap_floor_choice_groups, special_to_regular,
};
use crate::sewer_mob_placement::RoomCharacterRules;
use crate::shop::{ShopError, ShopRunState};
use crate::special_consumable::{
    ConsumableLevelPaintContext, ConsumablePaintEvent, ConsumablePaintOutcome,
    is_consumable_special, paint_consumable_special,
};
use crate::special_equipment::{
    LevelPaintContext, PrizePool, SpecialPaintEvent, SpecialPaintOutcome, is_equipment_special,
    paint_equipment_special,
};
use crate::special_forced::{
    ForcedLevelPaintContext, ForcedPaintEvent, ForcedPaintOutcome, ForcedShopRoomState,
    is_forced_special, paint_forced_special,
};

/// Fully painted state immediately before `buildFlagMaps()`.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintedHallsFloor {
    pub prepared: PreparedRegularFloor,
    pub graph_attempts: u32,
    pub level: Level,
    pub rooms: Vec<Room>,
    pub entrance_room: RoomId,
    pub exit_room: RoomId,
    pub remaining_prizes: PrizePool,
    pub equipment_events: Vec<SpecialPaintEvent>,
    pub consumable_events: Vec<ConsumablePaintEvent>,
    pub forced_events: Vec<ForcedPaintEvent>,
    pub secret_events: Vec<SecretPaintEvent>,
    pub quest_events: Vec<QuestPaintEvent>,
    pub quest_paint_state: QuestPaintState,
    pub world_items: Vec<WorldItem>,
}

/// One complete Halls `Level.create()` result after mobs and items.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedHallsFloor {
    pub painted: PaintedHallsFloor,
    pub flags: LevelFlags,
    pub mobs: HallsMobsResult,
    pub regular_items: RegularItemsResult,
    pub world_items: Vec<WorldItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HallsPaintError {
    Quest(QuestError),
    Painter(PaintError),
    UnsupportedRoom(RoomKind),
}

impl fmt::Display for HallsPaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Quest(error) => error.fmt(formatter),
            Self::Painter(error) => error.fmt(formatter),
            Self::UnsupportedRoom(kind) => write!(
                formatter,
                "room painter is not parity-gated in the Halls composite: {kind:?}"
            ),
        }
    }
}

impl std::error::Error for HallsPaintError {}

impl From<QuestError> for HallsPaintError {
    fn from(error: QuestError) -> Self {
        Self::Quest(error)
    }
}

impl From<PaintError> for HallsPaintError {
    fn from(error: PaintError) -> Self {
        Self::Painter(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum HallsFloorError {
    InvalidMaximumDepth(u8),
    Sewer(SewerFloorError),
    Prison(PrisonFloorError),
    Caves(CavesFloorError),
    City(CityFloorError),
    BossShop(ShopError),
    Paint(HallsPaintError),
    Items(RegularItemsError),
}

impl fmt::Display for HallsFloorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMaximumDepth(depth) => {
                write!(
                    formatter,
                    "Halls world depth must be in 21..=24, got {depth}"
                )
            }
            Self::Sewer(error) => error.fmt(formatter),
            Self::Prison(error) => error.fmt(formatter),
            Self::Caves(error) => error.fmt(formatter),
            Self::City(error) => error.fmt(formatter),
            Self::BossShop(error) => error.fmt(formatter),
            Self::Paint(error) => error.fmt(formatter),
            Self::Items(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for HallsFloorError {}

impl From<SewerFloorError> for HallsFloorError {
    fn from(error: SewerFloorError) -> Self {
        Self::Sewer(error)
    }
}

impl From<PrisonFloorError> for HallsFloorError {
    fn from(error: PrisonFloorError) -> Self {
        Self::Prison(error)
    }
}

impl From<CavesFloorError> for HallsFloorError {
    fn from(error: CavesFloorError) -> Self {
        Self::Caves(error)
    }
}

impl From<CityFloorError> for HallsFloorError {
    fn from(error: CityFloorError) -> Self {
        Self::City(error)
    }
}

impl From<ShopError> for HallsFloorError {
    fn from(error: ShopError) -> Self {
        Self::BossShop(error)
    }
}

impl From<HallsPaintError> for HallsFloorError {
    fn from(error: HallsPaintError) -> Self {
        Self::Paint(error)
    }
}

impl From<RegularItemsError> for HallsFloorError {
    fn from(error: RegularItemsError) -> Self {
        Self::Items(error)
    }
}

/// Exact world prefix through a regular Halls depth.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanonicalHallsWorldGenerator;

impl WorldGenerator for CanonicalHallsWorldGenerator {
    fn generate(&self, seed: DungeonSeed, max_depth: u8) -> GeneratedWorld {
        generate_halls_world(seed, max_depth)
            .expect("CanonicalHallsWorldGenerator accepts only depths 21..=24")
    }

    fn generate_batch(&self, seeds: &[DungeonSeed], max_depth: u8) -> Vec<GeneratedWorld> {
        assert!(
            (21..=24).contains(&max_depth),
            "CanonicalHallsWorldGenerator accepts only depths 21..=24"
        );
        let depths = (1..=4)
            .chain(6..=9)
            .chain(11..=14)
            .chain(16..=u32::from(max_depth))
            .collect::<Vec<_>>();
        let mut output = Vec::with_capacity(seeds.len());
        let mut chunks = seeds.chunks_exact(4);
        for chunk in &mut chunks {
            let dungeon_seeds = std::array::from_fn(|index| {
                i64::try_from(chunk[index].value()).expect("base-26 seed range fits Java long")
            });
            let roots_by_depth = depths
                .iter()
                .map(|&depth| seed_for_depth_batch4(dungeon_seeds, depth, 0))
                .collect::<Vec<_>>();
            for lane in 0..4 {
                let roots = roots_by_depth
                    .iter()
                    .map(|depth_roots| depth_roots[lane])
                    .collect::<Vec<_>>();
                output.push(
                    generate_halls_world_with_roots(chunk[lane], max_depth, &roots)
                        .expect("validated Halls batch satisfies canonical invariants"),
                );
            }
        }
        output.extend(
            chunks
                .remainder()
                .iter()
                .copied()
                .map(|seed| self.generate(seed, max_depth)),
        );
        output
    }
}

/// Generates every regular main-path floor through `maximum_depth`, skipping
/// boss floors 5, 10, and 15 whose canonical fresh-run generation does not
/// mutate the run-global state represented by this engine.
///
/// # Errors
///
/// Returns an invalid-depth error outside 21..=24 or the exact failing floor
/// generation phase.
pub fn generate_halls_world(
    seed: DungeonSeed,
    maximum_depth: u8,
) -> Result<GeneratedWorld, HallsFloorError> {
    if !(21..=24).contains(&maximum_depth) {
        return Err(HallsFloorError::InvalidMaximumDepth(maximum_depth));
    }
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let roots = (1..=4)
        .chain(6..=9)
        .chain(11..=14)
        .chain(16..=u32::from(maximum_depth))
        .map(|depth| seed_for_depth(dungeon_seed, depth, 0))
        .collect::<Vec<_>>();
    generate_halls_world_with_roots(seed, maximum_depth, &roots)
}

fn generate_halls_world_with_roots(
    seed: DungeonSeed,
    maximum_depth: u8,
    roots: &[i64],
) -> Result<GeneratedWorld, HallsFloorError> {
    let expected_roots = usize::from(maximum_depth) - 3;
    if !(21..=24).contains(&maximum_depth) || roots.len() != expected_roots {
        return Err(HallsFloorError::InvalidMaximumDepth(maximum_depth));
    }
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let mut run = RunState::new(dungeon_seed);
    let mut limited_drops = LimitedDrops::default();
    let mut quests = QuestState::new();
    let mut shop_run = ShopRunState::default();
    let mut random = RandomStack::with_base_seed(0);
    let mut items = Vec::new();
    let mut next_choice_group = 0_u16;

    for (index, &root) in roots[..4].iter().enumerate() {
        let depth = u32::try_from(index + 1).expect("four Sewer floors fit u32");
        random.push(root);
        let mut floor = generate_sewer_floor(
            &mut run,
            &mut limited_drops,
            &mut quests,
            depth,
            &mut random,
        )?;
        random.pop();
        next_choice_group = remap_floor_choice_groups(&mut floor.world_items, next_choice_group);
        items.extend(floor.world_items);
    }

    for (index, &root) in roots[4..8].iter().enumerate() {
        let depth = u32::try_from(index + 6).expect("four Prison floors fit u32");
        random.push(root);
        let mut floor = generate_prison_floor(
            &mut run,
            &mut limited_drops,
            &mut quests,
            &mut shop_run,
            depth,
            &mut random,
        )?;
        random.pop();
        next_choice_group = remap_floor_choice_groups(&mut floor.world_items, next_choice_group);
        items.extend(floor.world_items);
    }

    for (index, &root) in roots[8..12].iter().enumerate() {
        let depth = u32::try_from(index + 11).expect("four Caves floors fit u32");
        random.push(root);
        let mut floor = generate_caves_floor(
            &mut run,
            &mut limited_drops,
            &mut quests,
            &mut shop_run,
            depth,
            &mut random,
        )?;
        random.pop();
        next_choice_group = remap_floor_choice_groups(&mut floor.world_items, next_choice_group);
        items.extend(floor.world_items);
    }

    for (index, &root) in roots[12..16].iter().enumerate() {
        let depth = u32::try_from(index + 16).expect("four City floors fit u32");
        random.push(root);
        let mut floor = generate_city_floor(
            &mut run,
            &mut limited_drops,
            &mut quests,
            &mut shop_run,
            depth,
            &mut random,
        )?;
        random.pop();
        next_choice_group = remap_floor_choice_groups(&mut floor.world_items, next_choice_group);
        items.extend(floor.world_items);
    }

    random.push(roots[16]);
    let mut boss_shop = generate_city_boss_shop(&mut run, &mut shop_run, &mut random)?;
    random.pop();
    next_choice_group = remap_floor_choice_groups(&mut boss_shop.world_items, next_choice_group);
    items.extend(boss_shop.world_items);

    for (index, &root) in roots[17..].iter().enumerate() {
        let depth = u32::try_from(index + 21).expect("four Halls floors fit u32");
        random.push(root);
        let mut floor = generate_halls_floor(
            &mut run,
            &mut limited_drops,
            &mut quests,
            &mut shop_run,
            depth,
            &mut random,
        )?;
        random.pop();
        next_choice_group = remap_floor_choice_groups(&mut floor.world_items, next_choice_group);
        items.extend(floor.world_items);
    }
    Ok(GeneratedWorld { seed, items })
}

/// Completes a regular Halls floor through flags, mobs, and ordinary item
/// placement. `DemonSpawnerRoom.paint()` runs before inherited ordinary mob
/// placement so its occupied cell participates in every spatial rejection.
///
/// # Errors
///
/// Returns a paint or ordinary-item generation invariant failure.
pub fn generate_halls_floor(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    random: &mut RandomStack,
) -> Result<GeneratedHallsFloor, HallsFloorError> {
    let mut painted = paint_halls_floor(run, limited_drops, quests, shop_run, depth, random)?;
    let mut flags = LevelFlags::build_for_generation(&painted.level.map);
    let entrance_cell = painted
        .level
        .entrance()
        .expect("painted regular floor has an entrance");
    let exit_cell = painted
        .level
        .exit()
        .expect("painted regular floor has an exit");
    let spatial_rules = HallsSpatialRules {
        quest_state: painted.quest_paint_state.clone(),
    };
    let mobs = create_halls_mobs(
        &mut painted.level,
        &mut flags,
        &painted.rooms,
        painted.entrance_room,
        painted.exit_room,
        entrance_cell,
        exit_cell,
        &spatial_rules,
        random,
    );

    let mut world_items = painted.world_items.clone();
    append_painted_room_items(&painted.level, depth, &mut world_items);
    let queue = painted
        .remaining_prizes
        .items_to_spawn
        .iter()
        .copied()
        .map(special_to_regular)
        .collect();
    let mut placement_rules = CanonicalRoomPlacementRules::default();
    let regular_items = {
        let mut placement = PaintedRegularPlacement::new(
            &mut painted.level,
            &mut flags,
            &painted.rooms,
            &mut placement_rules,
        );
        create_regular_items(
            random,
            &mut run.generator,
            u8::try_from(depth).expect("Halls depth fits u8"),
            painted.prepared.feeling,
            run.challenges,
            queue,
            &mut placement,
        )?
    };
    world_items.extend(regular_items.world_items.iter().cloned());

    Ok(GeneratedHallsFloor {
        painted,
        flags,
        mobs,
        regular_items,
        world_items,
    })
}

/// Runs the exact Halls build and painter phases. The caller owns the active
/// per-depth RNG generator.
///
/// # Errors
///
/// Returns a quest scheduling, unsupported-room, or structural painter
/// failure.
pub fn paint_halls_floor(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    random: &mut RandomStack,
) -> Result<PaintedHallsFloor, HallsPaintError> {
    assert!((21..=24).contains(&depth), "Halls depths are 21..=24");
    let prepared = prepare_regular_floor(run, limited_drops, depth, random)
        .map_err(QuestError::from)
        .map_err(HallsPaintError::from)?;
    let (built, shop_room) = build_halls_room_graph(
        run,
        limited_drops,
        quests,
        shop_run,
        depth,
        prepared.feeling,
        random,
    )?;
    reject_unwired_rooms(&built)?;

    let trap_count = draw_regular_trap_count(depth, random);
    let initial_queue = halls_level_create_queue(&prepared);
    let initial_prizes = initial_queue.into_iter().map(regular_to_special).collect();
    let generator = RefCell::new(run.generator.clone());
    let prizes = RefCell::new(PrizePool {
        items_to_spawn: initial_prizes,
        rat_skull_level: -1,
        mimic_tooth_level: -1,
        next_choice_group: 0,
    });
    let shared_generator = SharedGenerator(&generator);
    let shared_prizes = SharedPrizes(&prizes);
    let content = SharedSewerContent {
        depth: i32::try_from(depth).expect("Halls depth fits Java int"),
        generator: shared_generator,
        prizes: shared_prizes,
    };
    let regular = HallsRoomDispatcher::new(content).set_revealed_trap_chance(0.0);
    let mut dispatch = HallsCompositeDispatcher {
        regular,
        generator: shared_generator,
        prizes: shared_prizes,
        equipment_events: Vec::new(),
        consumable_events: Vec::new(),
        forced_events: Vec::new(),
        secret_events: Vec::new(),
        quest_events: Vec::new(),
        quest_state: QuestPaintState::default(),
        shop_room,
        shop_run,
        world_items: Vec::new(),
    };

    let mut rooms = built.rooms;
    let mut level = Level::new(depth, prepared.feeling);
    level.plants_enabled = !run
        .challenges
        .contains(crate::challenges::Challenges::NO_HERBALISM);
    HallsPainter::new(prepared.feeling, trap_count).paint(
        &mut level,
        &mut rooms,
        &mut dispatch,
        random,
    )?;

    let equipment_events = std::mem::take(&mut dispatch.equipment_events);
    let consumable_events = std::mem::take(&mut dispatch.consumable_events);
    let forced_events = std::mem::take(&mut dispatch.forced_events);
    let secret_events = std::mem::take(&mut dispatch.secret_events);
    let quest_events = std::mem::take(&mut dispatch.quest_events);
    let quest_paint_state = std::mem::take(&mut dispatch.quest_state);
    let world_items = std::mem::take(&mut dispatch.world_items);
    drop(dispatch);

    run.generator = generator.into_inner();
    let remaining_prizes = prizes.into_inner();
    Ok(PaintedHallsFloor {
        prepared,
        graph_attempts: built.builder_attempts,
        level,
        rooms,
        entrance_room: built.entrance,
        exit_room: built.exit,
        remaining_prizes,
        equipment_events,
        consumable_events,
        forced_events,
        secret_events,
        quest_events,
        quest_paint_state,
        world_items,
    })
}

fn halls_level_create_queue(prepared: &PreparedRegularFloor) -> Vec<RegularItem> {
    let mut queue = Vec::with_capacity(
        usize::from(prepared.prequeued_torches) + prepared.queued_generated_items.len() + 6,
    );
    queue.extend(std::iter::repeat_n(
        RegularItem::Queued(QueuedItemKind::Torch),
        usize::from(prepared.prequeued_torches),
    ));
    queue.extend(level_create_queue(
        &prepared.queued_generated_items,
        prepared.mandatory_drops,
    ));
    queue
}

fn build_halls_room_graph(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    feeling: crate::level_prelude::Feeling,
    random: &mut RandomStack,
) -> Result<(BuiltRegularGraph, ForcedShopRoomState), HallsPaintError> {
    let mut builder = RegularLevelBuilder::for_regular_level(random);
    let initialized = init_regular_rooms(run, limited_drops, quests, depth, feeling, random)?;
    let mut shop_room = ForcedShopRoomState::default();
    let mut attempts = 0_u32;
    loop {
        attempts = attempts.wrapping_add(1);
        let mut rooms = initialized.rooms.clone();
        for room in &mut rooms {
            room.neighbours.clear();
            room.connected.clear();
        }
        let mut shop_size_hook = |room: &mut Room, random: &mut RandomStack| {
            let dimension = shop_room
                .minimum_dimension(
                    random,
                    &mut run.generator,
                    i32::try_from(depth).expect("Halls depth fits Java int"),
                    shop_run,
                )
                .expect("canonical Halls shop generation is valid");
            room.set_shop_minimum_dimension(dimension);
        };
        if builder.build_with_shop_size_hook(&mut rooms, depth, random, &mut shop_size_hook) {
            let entrance = rooms
                .iter()
                .position(Room::is_entrance)
                .expect("regular graph retains entrance");
            let exit = rooms
                .iter()
                .position(Room::is_exit)
                .expect("regular graph retains exit");
            return Ok((
                BuiltRegularGraph {
                    rooms,
                    entrance,
                    exit,
                    floor_specials: initialized.floor_specials,
                    builder_attempts: attempts,
                },
                shop_room,
            ));
        }
    }
}

fn reject_unwired_rooms(graph: &BuiltRegularGraph) -> Result<(), HallsPaintError> {
    for room in &graph.rooms {
        match room.kind {
            RoomKind::Entrance(_)
            | RoomKind::Exit(_)
            | RoomKind::Standard(_)
            | RoomKind::Connection(_)
            | RoomKind::Secret(_)
            | RoomKind::Quest(_)
            | RoomKind::Special(SpecialRoomKind::DemonSpawner) => {}
            RoomKind::Special(kind)
                if is_equipment_special(kind)
                    || is_consumable_special(kind)
                    || is_forced_special(kind) => {}
            kind @ RoomKind::Special(_) => return Err(HallsPaintError::UnsupportedRoom(kind)),
        }
    }
    Ok(())
}

struct HallsCompositeDispatcher<'a, 's> {
    regular: HallsRoomDispatcher<SharedSewerContent<'a>>,
    generator: SharedGenerator<'a>,
    prizes: SharedPrizes<'a>,
    equipment_events: Vec<SpecialPaintEvent>,
    consumable_events: Vec<ConsumablePaintEvent>,
    forced_events: Vec<ForcedPaintEvent>,
    secret_events: Vec<SecretPaintEvent>,
    quest_events: Vec<QuestPaintEvent>,
    quest_state: QuestPaintState,
    shop_room: ForcedShopRoomState,
    shop_run: &'s mut ShopRunState,
    world_items: Vec<WorldItem>,
}

impl RoomPaintDispatch for HallsCompositeDispatcher<'_, '_> {
    fn paint_room(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        random: &mut RandomStack,
    ) {
        match rooms[room].kind {
            RoomKind::Entrance(_)
            | RoomKind::Exit(_)
            | RoomKind::Standard(_)
            | RoomKind::Connection(_) => {
                self.regular.paint_room(level, rooms, room, random);
                return;
            }
            RoomKind::Secret(_) => {
                let outcome = {
                    let mut context = SecretLevelPaintContext::new(level, &mut self.secret_events);
                    paint_secret_room(
                        &mut context,
                        rooms,
                        room,
                        &mut self.generator,
                        &mut self.prizes,
                        random,
                    )
                    .expect("builder-produced secret room has a valid entrance")
                };
                let SecretPaintOutcome::Painted(report) = outcome else {
                    unreachable!("secret dispatcher handles every secret room")
                };
                self.world_items.extend(report.searchable_items);
                return;
            }
            RoomKind::Quest(_) | RoomKind::Special(SpecialRoomKind::DemonSpawner) => {
                let outcome = {
                    let mut context = QuestLevelPaintContext::new(level, &mut self.quest_events);
                    paint_quest_room(
                        &mut context,
                        rooms,
                        room,
                        &mut self.generator,
                        &mut self.prizes,
                        &mut self.quest_state,
                        random,
                    )
                    .expect("builder-produced quest room has a valid entrance")
                };
                let QuestPaintOutcome::Painted(report) = outcome else {
                    unreachable!("quest dispatcher handles every quest room")
                };
                self.world_items.extend(report.searchable_items);
                return;
            }
            RoomKind::Special(_) => {}
        }

        let equipment = {
            let mut context = LevelPaintContext::new(level, &mut self.equipment_events);
            paint_equipment_special(
                &mut context,
                rooms,
                room,
                &mut self.generator,
                &mut self.prizes,
                random,
            )
            .expect("builder-produced equipment room has a valid entrance")
        };
        if let SpecialPaintOutcome::Painted(report) = equipment {
            self.world_items.extend(report.searchable_items);
            return;
        }

        let consumable = {
            let mut context = ConsumableLevelPaintContext::new(level, &mut self.consumable_events);
            paint_consumable_special(
                &mut context,
                rooms,
                room,
                &mut self.generator,
                &mut self.prizes,
                random,
            )
            .expect("builder-produced consumable room has a valid entrance")
        };
        if let ConsumablePaintOutcome::Painted(report) = consumable {
            self.world_items.extend(report.searchable_items);
            return;
        }

        let forced = {
            let mut generator = self.generator.0.borrow_mut();
            let mut context = ForcedLevelPaintContext::new(level, &mut self.forced_events);
            paint_forced_special(
                &mut context,
                rooms,
                room,
                &mut generator,
                &mut self.prizes,
                &mut self.shop_room,
                self.shop_run,
                random,
            )
            .expect("builder-produced forced room has a valid entrance")
        };
        let ForcedPaintOutcome::Painted(report) = forced else {
            unreachable!("unwired special rooms are rejected before painting")
        };
        self.world_items.extend(report.searchable_items);
    }

    fn can_merge(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        other: RoomId,
        point: Point,
        merge_terrain: i32,
    ) -> bool {
        match rooms[room].kind {
            RoomKind::Quest(_) | RoomKind::Special(SpecialRoomKind::DemonSpawner) => {
                quest_can_merge(level, rooms, room, point)
            }
            RoomKind::Special(_) | RoomKind::Secret(_) => false,
            _ => self
                .regular
                .can_merge(level, rooms, room, other, point, merge_terrain),
        }
    }

    fn merge(
        &mut self,
        level: &mut Level,
        rooms: &[Room],
        room: RoomId,
        other: RoomId,
        merge: Rect,
        merge_terrain: i32,
    ) {
        if matches!(rooms[room].kind, RoomKind::Quest(_) | RoomKind::Special(_)) {
            draw::fill_rect(&mut level.map, merge, merge_terrain);
        } else {
            self.regular
                .merge(level, rooms, room, other, merge, merge_terrain);
        }
    }

    fn can_place_water(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        match rooms[room].kind {
            kind @ RoomKind::Secret(_) => secret_can_place_water(kind),
            RoomKind::Quest(_) | RoomKind::Special(SpecialRoomKind::DemonSpawner) => {
                quest_can_place_water(rooms, room, point)
            }
            RoomKind::Special(SpecialRoomKind::CrystalPath) => false,
            RoomKind::Special(_) => true,
            _ => self.regular.can_place_water(level, rooms, room, point),
        }
    }

    fn can_place_grass(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        match rooms[room].kind {
            kind @ RoomKind::Secret(_) => secret_can_place_grass(kind),
            RoomKind::Quest(_) | RoomKind::Special(SpecialRoomKind::DemonSpawner) => {
                quest_can_place_grass(rooms, room, point)
            }
            RoomKind::Special(
                SpecialRoomKind::MagicalFire | SpecialRoomKind::CrystalPath | SpecialRoomKind::Pit,
            ) => false,
            RoomKind::Special(_) => true,
            _ => self.regular.can_place_grass(level, rooms, room, point),
        }
    }

    fn can_place_trap(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        match rooms[room].kind {
            kind @ RoomKind::Secret(_) => secret_can_place_trap(kind),
            kind @ (RoomKind::Quest(_) | RoomKind::Special(SpecialRoomKind::DemonSpawner)) => {
                quest_can_place_trap(kind)
            }
            RoomKind::Special(
                SpecialRoomKind::ToxicGas | SpecialRoomKind::CrystalPath | SpecialRoomKind::Pit,
            ) => false,
            RoomKind::Special(_) => true,
            _ => self.regular.can_place_trap(level, rooms, room, point),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct HallsSpatialRules {
    quest_state: QuestPaintState,
}

impl RoomCharacterRules for HallsSpatialRules {
    fn can_place_character(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        point: Point,
    ) -> bool {
        let selected = &rooms[room];
        if matches!(
            selected.kind,
            RoomKind::Quest(_) | RoomKind::Special(SpecialRoomKind::DemonSpawner)
        ) {
            return quest_can_place_character(level, rooms, room, point, &self.quest_state);
        }
        if !selected.inside(point) {
            return false;
        }
        let cell = level.point_to_cell(point);
        match selected.kind {
            RoomKind::Standard(StandardRoomKind::Plants)
                if level.plants.iter().any(|plant| plant.cell == cell) =>
            {
                return false;
            }
            RoomKind::Standard(StandardRoomKind::Aquarium)
                if level.map.cells[cell] == terrain::WATER =>
            {
                return false;
            }
            _ => {}
        }
        !selected.is_exit() || level.exit() != Some(cell)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{ArmorEffect, Effect, ItemId, WeaponEffect};
    use crate::halls_mobs::HallsMobKind;
    use crate::level_prelude::Feeling;
    use crate::model::{Accessibility, ItemSource};
    use crate::quest_rooms::{QuestMobKind, QuestPaintEvent};

    fn generate_halls_prefix(seed: DungeonSeed, maximum_depth: u32) -> Vec<GeneratedHallsFloor> {
        let dungeon_seed = i64::try_from(seed.value()).unwrap();
        let mut run = RunState::new(dungeon_seed);
        let mut limited = LimitedDrops::default();
        let mut quests = QuestState::new();
        let mut shop_run = ShopRunState::default();
        let mut random = RandomStack::with_base_seed(0);

        for depth in 1..=4 {
            random.push(seed_for_depth(dungeon_seed, depth, 0));
            generate_sewer_floor(&mut run, &mut limited, &mut quests, depth, &mut random).unwrap();
            random.pop();
        }
        for depth in 6..=9 {
            random.push(seed_for_depth(dungeon_seed, depth, 0));
            generate_prison_floor(
                &mut run,
                &mut limited,
                &mut quests,
                &mut shop_run,
                depth,
                &mut random,
            )
            .unwrap();
            random.pop();
        }
        for depth in 11..=14 {
            random.push(seed_for_depth(dungeon_seed, depth, 0));
            generate_caves_floor(
                &mut run,
                &mut limited,
                &mut quests,
                &mut shop_run,
                depth,
                &mut random,
            )
            .unwrap();
            random.pop();
        }
        for depth in 16..=19 {
            random.push(seed_for_depth(dungeon_seed, depth, 0));
            generate_city_floor(
                &mut run,
                &mut limited,
                &mut quests,
                &mut shop_run,
                depth,
                &mut random,
            )
            .unwrap();
            random.pop();
        }
        random.push(seed_for_depth(dungeon_seed, 20, 0));
        generate_city_boss_shop(&mut run, &mut shop_run, &mut random).unwrap();
        random.pop();

        (21..=maximum_depth)
            .map(|depth| {
                random.push(seed_for_depth(dungeon_seed, depth, 0));
                let floor = generate_halls_floor(
                    &mut run,
                    &mut limited,
                    &mut quests,
                    &mut shop_run,
                    depth,
                    &mut random,
                )
                .unwrap_or_else(|error| panic!("depth {depth}: {error}"));
                random.pop();
                floor
            })
            .collect()
    }

    fn assert_floor(
        floor: &GeneratedHallsFloor,
        expected: (u32, Feeling, (i32, i32), i32, usize, usize),
        ordinary_mobs: &[(HallsMobKind, usize)],
        occupied: &[usize],
        expected_items: Vec<WorldItem>,
    ) {
        let (depth, feeling, size, hash, entrance, exit) = expected;
        assert_eq!(floor.painted.level.depth, depth);
        assert_eq!(floor.painted.prepared.feeling, feeling, "depth {depth}");
        assert_eq!(
            (floor.painted.level.width(), floor.painted.level.height()),
            size,
            "depth {depth}"
        );
        assert_eq!(floor.painted.level.java_map_hash(), hash, "depth {depth}");
        assert_eq!(
            floor.painted.level.entrance(),
            Some(entrance),
            "depth {depth}"
        );
        assert_eq!(floor.painted.level.exit(), Some(exit), "depth {depth}");

        let mut actual_mobs = floor
            .mobs
            .mobs
            .iter()
            .map(|mob| (mob.mob.kind, mob.cell))
            .collect::<Vec<_>>();
        actual_mobs.sort_unstable_by_key(|(_, cell)| *cell);
        assert_eq!(actual_mobs, ordinary_mobs, "depth {depth}");

        let actual_occupied = floor
            .painted
            .level
            .mob_cells
            .iter()
            .enumerate()
            .filter_map(|(cell, &value)| value.then_some(cell))
            .collect::<Vec<_>>();
        assert_eq!(actual_occupied, occupied, "depth {depth}");
        assert_eq!(floor.painted.quest_paint_state.spawners_alive, 1);
        assert_eq!(
            floor
                .painted
                .quest_events
                .iter()
                .filter(|event| matches!(
                    event,
                    QuestPaintEvent::Mob {
                        kind: QuestMobKind::DemonSpawner {
                            spawn_recorded: true
                        },
                        ..
                    }
                ))
                .count(),
            1,
            "depth {depth}"
        );

        let mut actual_items = floor.world_items.clone();
        assert_eq!(actual_items.len(), expected_items.len(), "depth {depth}");
        for expected_item in expected_items {
            let index = actual_items
                .iter()
                .position(|actual| *actual == expected_item)
                .unwrap_or_else(|| {
                    panic!(
                        "depth {depth} missing {expected_item:?}; unmatched actual {actual_items:?}"
                    )
                });
            actual_items.remove(index);
        }
        assert!(actual_items.is_empty(), "depth {depth}: {actual_items:?}");
    }

    fn item(
        id: ItemId,
        upgrade: u8,
        effect: Option<Effect>,
        cursed: bool,
        depth: u8,
        source: ItemSource,
        accessibility: Accessibility,
    ) -> WorldItem {
        WorldItem {
            item: id,
            transmuted_item: None,
            upgrade,
            effect,
            cursed,
            depth,
            source,
            accessibility,
        }
    }

    #[test]
    fn aaa_sequential_halls_maps_mobs_and_items_match_official_oracle() {
        let floors = generate_halls_prefix(DungeonSeed::MIN, 24);
        assert_floor(
            &floors[0],
            (21, Feeling::None, (48, 52), 115_893_013, 244, 1_040),
            &[
                (HallsMobKind::Eye, 783),
                (HallsMobKind::Eye, 876),
                (HallsMobKind::Succubus, 969),
                (HallsMobKind::Succubus, 1_282),
                (HallsMobKind::Succubus, 2_109),
            ],
            &[783, 876, 969, 1_282, 1_313, 2_109],
            vec![
                item(
                    ItemId::WandCorrosion,
                    0,
                    None,
                    false,
                    21,
                    ItemSource::CrystalChest,
                    Accessibility::Choice {
                        group: 0,
                        option: 0,
                    },
                ),
                item(
                    ItemId::ThrowingHammer,
                    0,
                    None,
                    false,
                    21,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
                item(
                    ItemId::AssassinsBlade,
                    0,
                    None,
                    false,
                    21,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
            ],
        );
        assert_floor(
            &floors[1],
            (22, Feeling::Chasm, (36, 54), 1_283_341_899, 1_523, 310),
            &[
                (HallsMobKind::Eye, 309),
                (HallsMobKind::Eye, 847),
                (HallsMobKind::Succubus, 883),
                (HallsMobKind::Succubus, 956),
                (HallsMobKind::Eye, 1_264),
                (HallsMobKind::Succubus, 1_288),
            ],
            &[
                309, 374, 549, 623, 658, 698, 847, 883, 956, 1_264, 1_288, 1_575,
            ],
            vec![
                item(
                    ItemId::PlateArmor,
                    2,
                    Some(Effect::Armor(ArmorEffect::Swiftness)),
                    false,
                    22,
                    ItemSource::Chest,
                    Accessibility::Independent,
                ),
                item(
                    ItemId::ThrowingHammer,
                    0,
                    None,
                    false,
                    22,
                    ItemSource::LockedChest,
                    Accessibility::Independent,
                ),
                item(
                    ItemId::WandPrismaticLight,
                    0,
                    None,
                    true,
                    22,
                    ItemSource::LockedChest,
                    Accessibility::Independent,
                ),
                item(
                    ItemId::Glaive,
                    0,
                    Some(Effect::Weapon(WeaponEffect::Shocking)),
                    false,
                    22,
                    ItemSource::Statue,
                    Accessibility::Independent,
                ),
            ],
        );
        assert_floor(
            &floors[2],
            (23, Feeling::Dark, (44, 65), -1_159_174_921, 168, 2_000),
            &[
                (HallsMobKind::Succubus, 644),
                (HallsMobKind::Eye, 1_252),
                (HallsMobKind::Scorpio, 1_397),
                (HallsMobKind::Eye, 1_439),
                (HallsMobKind::Succubus, 1_824),
                (HallsMobKind::Eye, 2_039),
                (HallsMobKind::Eye, 2_131),
                (HallsMobKind::Scorpio, 2_343),
            ],
            &[644, 823, 1_252, 1_397, 1_439, 1_824, 2_039, 2_131, 2_343],
            vec![],
        );
        assert_floor(
            &floors[3],
            (24, Feeling::None, (39, 40), -192_497_479, 1_071, 445),
            &[
                (HallsMobKind::Scorpio, 426),
                (HallsMobKind::Eye, 529),
                (HallsMobKind::Scorpio, 570),
                (HallsMobKind::Scorpio, 578),
                (HallsMobKind::Succubus, 594),
                (HallsMobKind::Scorpio, 705),
                (HallsMobKind::Eye, 1_311),
            ],
            &[426, 529, 570, 578, 594, 687, 705, 1_311],
            vec![item(
                ItemId::RunicBlade,
                0,
                Some(Effect::Weapon(WeaponEffect::Displacing)),
                true,
                24,
                ItemSource::SacrificialFire,
                Accessibility::Independent,
            )],
        );
    }

    #[test]
    fn three_nonzero_depth_twenty_one_floors_match_official_oracle() {
        let fixtures = [
            (
                "AAA-AAA-AAB",
                (21, Feeling::Dark, (44, 44), 1_818_812_898, 193, 1_211),
                vec![
                    (HallsMobKind::Succubus, 761),
                    (HallsMobKind::Succubus, 1_031),
                    (HallsMobKind::Succubus, 1_064),
                    (HallsMobKind::Eye, 1_298),
                    (HallsMobKind::Succubus, 1_345),
                    (HallsMobKind::Eye, 1_677),
                ],
                vec![761, 1_031, 1_064, 1_163, 1_298, 1_345, 1_677],
                vec![
                    item(
                        ItemId::HeavyBoomerang,
                        0,
                        None,
                        false,
                        21,
                        ItemSource::Heap,
                        Accessibility::Independent,
                    ),
                    item(
                        ItemId::WandFireblast,
                        1,
                        None,
                        false,
                        21,
                        ItemSource::Tomb,
                        Accessibility::Independent,
                    ),
                ],
            ),
            (
                "ABC-DEF-GHI",
                (21, Feeling::Water, (51, 49), -1_080_298_065, 1_856, 735),
                vec![
                    (HallsMobKind::Succubus, 685),
                    (HallsMobKind::Eye, 782),
                    (HallsMobKind::Succubus, 984),
                    (HallsMobKind::Succubus, 1_054),
                    (HallsMobKind::Succubus, 1_598),
                    (HallsMobKind::Eye, 1_645),
                ],
                vec![685, 782, 984, 1_054, 1_598, 1_645, 2_266],
                vec![item(
                    ItemId::RingForce,
                    0,
                    None,
                    true,
                    21,
                    ItemSource::Chest,
                    Accessibility::Choice {
                        group: 0,
                        option: 1,
                    },
                )],
            ),
            (
                "ZZZ-ZZZ-ZZZ",
                (21, Feeling::Dark, (47, 52), 1_615_833_505, 392, 1_436),
                vec![
                    (HallsMobKind::Succubus, 1_136),
                    (HallsMobKind::Eye, 1_760),
                    (HallsMobKind::Succubus, 1_856),
                    (HallsMobKind::Succubus, 2_011),
                ],
                vec![1_136, 1_430, 1_760, 1_856, 2_011, 2_190],
                vec![
                    item(
                        ItemId::PlateArmor,
                        0,
                        None,
                        false,
                        21,
                        ItemSource::Chest,
                        Accessibility::Independent,
                    ),
                    item(
                        ItemId::RingSharpshooting,
                        1,
                        None,
                        false,
                        21,
                        ItemSource::CrystalChest,
                        Accessibility::Choice {
                            group: 0,
                            option: 0,
                        },
                    ),
                ],
            ),
        ];

        for (code, expected, ordinary_mobs, occupied, expected_items) in fixtures {
            let seed = DungeonSeed::from_code(code).unwrap();
            let floor = generate_halls_prefix(seed, 21).pop().unwrap();
            assert_floor(&floor, expected, &ordinary_mobs, &occupied, expected_items);
        }
    }

    #[test]
    fn torches_precede_level_create_queue_and_depth_twenty_shop_is_reported() {
        let floor = generate_halls_prefix(DungeonSeed::MIN, 21).pop().unwrap();
        let queue = halls_level_create_queue(&floor.painted.prepared);
        assert_eq!(
            &queue[..2],
            &[
                RegularItem::Queued(QueuedItemKind::Torch),
                RegularItem::Queued(QueuedItemKind::Torch),
            ]
        );
        assert!(matches!(queue[2], RegularItem::Generated(_)));

        let world = generate_halls_world(DungeonSeed::MIN, 21).unwrap();
        let boss_shop = world
            .items
            .iter()
            .filter(|item| item.depth == 20 && item.source == ItemSource::Shop)
            .map(|item| item.item)
            .collect::<Vec<_>>();
        assert_eq!(
            boss_shop,
            vec![
                ItemId::PlateArmor,
                ItemId::ThrowingHammer,
                ItemId::Greatshield,
                ItemId::IncendiaryDart,
            ]
        );
    }

    #[test]
    fn seed_two_hundred_ten_unconnected_platform_merge_matches_depth_twenty_two_oracle() {
        let seed = DungeonSeed::from_code("AAA-AAA-AIC").unwrap();
        let floor = generate_halls_prefix(seed, 22).pop().unwrap();
        assert_floor(
            &floor,
            (22, Feeling::None, (34, 40), 486_780_263, 605, 1_038),
            &[
                (HallsMobKind::Eye, 187),
                (HallsMobKind::Succubus, 653),
                (HallsMobKind::Succubus, 659),
                (HallsMobKind::Succubus, 819),
                (HallsMobKind::Eye, 1_142),
            ],
            &[143, 187, 412, 653, 659, 732, 734, 819, 823, 1_142],
            vec![
                item(
                    ItemId::PlateArmor,
                    0,
                    None,
                    false,
                    22,
                    ItemSource::Chest,
                    Accessibility::Independent,
                ),
                item(
                    ItemId::StoneGauntlet,
                    0,
                    None,
                    false,
                    22,
                    ItemSource::Chest,
                    Accessibility::Independent,
                ),
                item(
                    ItemId::PlateArmor,
                    0,
                    Some(Effect::Armor(ArmorEffect::Viscosity)),
                    false,
                    22,
                    ItemSource::Skeleton,
                    Accessibility::Independent,
                ),
                item(
                    ItemId::RingWealth,
                    2,
                    None,
                    false,
                    22,
                    ItemSource::Mimic,
                    Accessibility::Independent,
                ),
            ],
        );
    }

    #[test]
    fn halls_world_batch_roots_match_scalar() {
        let seeds = [
            DungeonSeed::MIN,
            DungeonSeed::new(1).unwrap(),
            DungeonSeed::from_code("ABC-DEF-GHI").unwrap(),
            DungeonSeed::MAX,
        ];
        let scalar = seeds.map(|seed| generate_halls_world(seed, 21).unwrap());
        let batched = CanonicalHallsWorldGenerator.generate_batch(&seeds, 21);
        assert_eq!(batched, scalar);
    }
}
