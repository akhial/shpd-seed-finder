//! Parity-gated composition of regular City floors (depths 16 through 19).
//!
//! A City world prefix advances one live run through Sewers, Prison, and
//! Caves, skipping the state-neutral boss floors at 5, 10, and 15. The
//! depth-16 shop therefore sees both earlier shops' persistent bag state.
//! `Imp.Quest.spawn()` remains in `initRooms()` and its generated ring mutates
//! the live Generator even though the eventual quest reward is not searchable.

#![allow(clippy::missing_panics_doc, clippy::too_many_lines)]

use std::cell::RefCell;
use std::fmt;

use crate::batch::seed_for_depth_batch4;
use crate::builder::{Builder, RegularLevelBuilder};
use crate::caves_floor::{CavesFloorError, generate_caves_floor};
use crate::city_mobs::{CityMobsResult, create_city_mobs};
use crate::city_rooms::{CityPainter, CityRoomDispatcher};
use crate::geometry::{Point, Rect, painter as draw, terrain};
use crate::level::Level;
use crate::level_flags::LevelFlags;
use crate::level_prelude::LimitedDrops;
use crate::model::{FloorFeeling, GeneratedWorld, WorldItem};
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
    RegularItemsError, RegularItemsResult, create_regular_items, level_create_queue,
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
use crate::shop::ShopRunState;
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
use crate::vault_floor::{VaultError, generate_vault};

/// Fully painted state immediately before `buildFlagMaps()`.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintedCityFloor {
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

/// One complete City `Level.create()` result after mobs and items.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedCityFloor {
    pub painted: PaintedCityFloor,
    pub flags: LevelFlags,
    pub mobs: CityMobsResult,
    pub regular_items: RegularItemsResult,
    pub world_items: Vec<WorldItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CityPaintError {
    Quest(QuestError),
    Painter(PaintError),
    UnsupportedRoom(RoomKind),
}

impl fmt::Display for CityPaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Quest(error) => error.fmt(formatter),
            Self::Painter(error) => error.fmt(formatter),
            Self::UnsupportedRoom(kind) => write!(
                formatter,
                "room painter is not parity-gated in the City composite: {kind:?}"
            ),
        }
    }
}

impl std::error::Error for CityPaintError {}

impl From<QuestError> for CityPaintError {
    fn from(error: QuestError) -> Self {
        Self::Quest(error)
    }
}

impl From<PaintError> for CityPaintError {
    fn from(error: PaintError) -> Self {
        Self::Painter(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CityFloorError {
    InvalidMaximumDepth(u8),
    Sewer(SewerFloorError),
    Prison(PrisonFloorError),
    Caves(CavesFloorError),
    Paint(CityPaintError),
    Items(RegularItemsError),
    Vault(VaultError),
}

impl fmt::Display for CityFloorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMaximumDepth(depth) => {
                write!(
                    formatter,
                    "City world depth must be in 16..=19, got {depth}"
                )
            }
            Self::Sewer(error) => error.fmt(formatter),
            Self::Prison(error) => error.fmt(formatter),
            Self::Caves(error) => error.fmt(formatter),
            Self::Paint(error) => error.fmt(formatter),
            Self::Items(error) => error.fmt(formatter),
            Self::Vault(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CityFloorError {}

impl From<VaultError> for CityFloorError {
    fn from(error: VaultError) -> Self {
        Self::Vault(error)
    }
}

impl From<SewerFloorError> for CityFloorError {
    fn from(error: SewerFloorError) -> Self {
        Self::Sewer(error)
    }
}

impl From<PrisonFloorError> for CityFloorError {
    fn from(error: PrisonFloorError) -> Self {
        Self::Prison(error)
    }
}

impl From<CavesFloorError> for CityFloorError {
    fn from(error: CavesFloorError) -> Self {
        Self::Caves(error)
    }
}

impl From<CityPaintError> for CityFloorError {
    fn from(error: CityPaintError) -> Self {
        Self::Paint(error)
    }
}

impl From<RegularItemsError> for CityFloorError {
    fn from(error: RegularItemsError) -> Self {
        Self::Items(error)
    }
}

/// Choice option of the first vault treasure item: the Imp's six reward
/// options occupy 0..=5 (the artifact slot keeps its index even though it is
/// not searchable).
const VAULT_FIRST_OPTION: u8 = 6;

/// Exact world prefix through a regular City depth.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanonicalCityWorldGenerator;

impl WorldGenerator for CanonicalCityWorldGenerator {
    fn generate(&self, seed: DungeonSeed, max_depth: u8) -> GeneratedWorld {
        generate_city_world(seed, max_depth)
            .expect("CanonicalCityWorldGenerator accepts only depths 16..=19")
    }

    fn generate_batch(&self, seeds: &[DungeonSeed], max_depth: u8) -> Vec<GeneratedWorld> {
        assert!(
            (16..=19).contains(&max_depth),
            "CanonicalCityWorldGenerator accepts only depths 16..=19"
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
                    generate_city_world_with_roots(chunk[lane], max_depth, &roots)
                        .expect("validated City batch satisfies canonical invariants"),
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
/// Returns an invalid-depth error outside 16..=19 or the exact failing floor
/// generation phase.
pub fn generate_city_world(
    seed: DungeonSeed,
    maximum_depth: u8,
) -> Result<GeneratedWorld, CityFloorError> {
    if !(16..=19).contains(&maximum_depth) {
        return Err(CityFloorError::InvalidMaximumDepth(maximum_depth));
    }
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let roots = (1..=4)
        .chain(6..=9)
        .chain(11..=14)
        .chain(16..=u32::from(maximum_depth))
        .map(|depth| seed_for_depth(dungeon_seed, depth, 0))
        .collect::<Vec<_>>();
    generate_city_world_with_roots(seed, maximum_depth, &roots)
}

fn generate_city_world_with_roots(
    seed: DungeonSeed,
    maximum_depth: u8,
    roots: &[i64],
) -> Result<GeneratedWorld, CityFloorError> {
    let expected_roots = usize::from(maximum_depth) - 3;
    if !(16..=19).contains(&maximum_depth) || roots.len() != expected_roots {
        return Err(CityFloorError::InvalidMaximumDepth(maximum_depth));
    }
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let mut run = RunState::new(dungeon_seed);
    let mut limited_drops = LimitedDrops::default();
    let mut quests = QuestState::new();
    let mut shop_run = ShopRunState::default();
    let mut random = RandomStack::with_base_seed(0);
    let mut items = Vec::new();
    let mut feelings = Vec::new();
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
        feelings.push(FloorFeeling {
            depth: u8::try_from(depth).expect("main-path depths fit u8"),
            feeling: floor.painted.level.feeling,
        });
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
        feelings.push(FloorFeeling {
            depth: u8::try_from(depth).expect("main-path depths fit u8"),
            feeling: floor.painted.level.feeling,
        });
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
        feelings.push(FloorFeeling {
            depth: u8::try_from(depth).expect("main-path depths fit u8"),
            feeling: floor.painted.level.feeling,
        });
        items.extend(floor.world_items);
    }

    for (index, &root) in roots[12..].iter().enumerate() {
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
        feelings.push(FloorFeeling {
            depth: u8::try_from(depth).expect("main-path depths fit u8"),
            feeling: floor.painted.level.feeling,
        });
        items.extend(floor.world_items);
    }
    Ok(GeneratedWorld {
        seed,
        items,
        feelings,
        quests: quests.summary(),
        ring_gems: run.appearances.ring_gems,
    })
}

/// Completes a regular City floor through flags, mobs, quests, and ordinary
/// item placement. An accessible Imp room contributes its deterministic ring
/// reward on the floor where the quest was generated.
///
/// # Errors
///
/// Returns a paint or ordinary-item generation invariant failure.
pub fn generate_city_floor(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    random: &mut RandomStack,
) -> Result<GeneratedCityFloor, CityFloorError> {
    let mut painted = paint_city_floor(run, limited_drops, quests, shop_run, depth, random)?;
    let mut flags = LevelFlags::build_for_generation(&painted.level.map);
    let entrance_cell = painted
        .level
        .entrance()
        .expect("painted regular floor has an entrance");
    let exit_cell = painted
        .level
        .exit()
        .expect("painted regular floor has an exit");
    let spatial_rules = CitySpatialRules {
        quest_state: painted.quest_paint_state.clone(),
    };
    let mobs = create_city_mobs(
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
    let imp_group = painted.remaining_prizes.next_choice_group;
    if quests.imp.depth == Some(u8::try_from(depth).expect("City depth fits u8")) {
        quests.imp.append_world_items(imp_group, &mut world_items);
        // The Imp's Vault (branch 1 of this depth) has its own depth seed and
        // mutates no run state, so it can be generated right here. The Escape
        // Crystal lets exactly one item leave, so its treasure joins the six
        // reward options' choice group, numbered after them.
        if run.generate_vault && quests.imp.room_accessible {
            let depth_u8 = u8::try_from(depth).expect("City depth fits u8");
            let vault = generate_vault(run.dungeon_seed, depth_u8, run.challenges)?;
            world_items.extend(vault.world_items(depth_u8, imp_group, VAULT_FIRST_OPTION));
        }
    }
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
            u8::try_from(depth).expect("City depth fits u8"),
            painted.prepared.feeling,
            run.challenges,
            queue,
            &mut placement,
        )?
    };
    world_items.extend(regular_items.world_items.iter().cloned());

    Ok(GeneratedCityFloor {
        painted,
        flags,
        mobs,
        regular_items,
        world_items,
    })
}

/// Runs the exact City build and painter phases. The caller owns the active
/// per-depth RNG generator.
///
/// # Errors
///
/// Returns a quest scheduling, unsupported-room, or structural painter
/// failure.
pub fn paint_city_floor(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    random: &mut RandomStack,
) -> Result<PaintedCityFloor, CityPaintError> {
    assert!((16..=19).contains(&depth), "City depths are 16..=19");
    let prepared = prepare_regular_floor(run, limited_drops, depth, random)
        .map_err(QuestError::from)
        .map_err(CityPaintError::from)?;
    let (built, shop_room) = build_city_room_graph(
        run,
        limited_drops,
        quests,
        shop_run,
        depth,
        prepared.feeling,
        random,
    )?;
    if let Err(error) = reject_unwired_rooms(&built) {
        quests.imp.finish_build_attempt(false);
        return Err(error);
    }

    let trap_count = draw_regular_trap_count(depth, random);
    let initial_queue =
        level_create_queue(&prepared.queued_generated_items, prepared.mandatory_drops);
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
        depth: i32::try_from(depth).expect("City depth fits Java int"),
        generator: shared_generator,
        prizes: shared_prizes,
    };
    let regular = CityRoomDispatcher::new(content).set_revealed_trap_chance(0.0);
    let mut dispatch = CityCompositeDispatcher {
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
    if let Err(error) = CityPainter::new(prepared.feeling, trap_count).paint(
        &mut level,
        &mut rooms,
        &mut dispatch,
        random,
    ) {
        quests.imp.finish_build_attempt(false);
        return Err(error.into());
    }

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
    quests.imp.finish_build_attempt(true);
    Ok(PaintedCityFloor {
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

fn build_city_room_graph(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    feeling: crate::level_prelude::Feeling,
    random: &mut RandomStack,
) -> Result<(BuiltRegularGraph, ForcedShopRoomState), CityPaintError> {
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
                    i32::try_from(depth).expect("City depth fits Java int"),
                    shop_run,
                )
                .expect("canonical City shop generation is valid");
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

fn reject_unwired_rooms(graph: &BuiltRegularGraph) -> Result<(), CityPaintError> {
    for room in &graph.rooms {
        match room.kind {
            RoomKind::Entrance(_)
            | RoomKind::Exit(_)
            | RoomKind::Standard(_)
            | RoomKind::Connection(_)
            | RoomKind::Secret(_)
            | RoomKind::Quest(_) => {}
            RoomKind::Special(kind)
                if is_equipment_special(kind)
                    || is_consumable_special(kind)
                    || is_forced_special(kind) => {}
            kind @ RoomKind::Special(_) => return Err(CityPaintError::UnsupportedRoom(kind)),
        }
    }
    Ok(())
}

struct CityCompositeDispatcher<'a, 's> {
    regular: CityRoomDispatcher<SharedSewerContent<'a>>,
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

impl RoomPaintDispatch for CityCompositeDispatcher<'_, '_> {
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
            RoomKind::Quest(_) => {
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
            RoomKind::Quest(_) => quest_can_merge(level, rooms, room, point),
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
            RoomKind::Quest(_) => quest_can_place_water(rooms, room, point),
            RoomKind::Special(SpecialRoomKind::CrystalPath) => false,
            RoomKind::Special(_) => true,
            _ => self.regular.can_place_water(level, rooms, room, point),
        }
    }

    fn can_place_grass(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        match rooms[room].kind {
            kind @ RoomKind::Secret(_) => secret_can_place_grass(kind),
            RoomKind::Quest(_) => quest_can_place_grass(rooms, room, point),
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
            kind @ RoomKind::Quest(_) => quest_can_place_trap(kind),
            RoomKind::Special(
                SpecialRoomKind::ToxicGas | SpecialRoomKind::CrystalPath | SpecialRoomKind::Pit,
            ) => false,
            RoomKind::Special(_) => true,
            _ => self.regular.can_place_trap(level, rooms, room, point),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CitySpatialRules {
    quest_state: QuestPaintState,
}

impl RoomCharacterRules for CitySpatialRules {
    fn can_place_character(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        point: Point,
    ) -> bool {
        let selected = &rooms[room];
        if matches!(selected.kind, RoomKind::Quest(_)) {
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
    use crate::city_mobs::CityMobKind;
    use crate::level_prelude::Feeling;
    use crate::model::{Accessibility, ItemSource};

    fn generate_city_prefix(seed: DungeonSeed, maximum_depth: u32) -> Vec<GeneratedCityFloor> {
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

        (16..=maximum_depth)
            .map(|depth| {
                random.push(seed_for_depth(dungeon_seed, depth, 0));
                let floor = generate_city_floor(
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

    #[test]
    fn aaa_sequential_city_maps_mobs_and_painted_actors_match_official_oracle() {
        let floors = generate_city_prefix(DungeonSeed::MIN, 19);
        let expected = [
            (
                16,
                Feeling::None,
                (39, 37),
                -640_772_373,
                1_186,
                383,
                vec![
                    (CityMobKind::FireElemental, 308),
                    (CityMobKind::Warlock, 539),
                    (CityMobKind::Ghoul, 687),
                    (CityMobKind::FireElemental, 799),
                    (CityMobKind::Ghoul, 1_045),
                    (CityMobKind::Ghoul, 1_214),
                ],
                vec![308, 539, 687, 799, 1_045, 1_214, 1_231],
            ),
            (
                17,
                Feeling::None,
                (57, 36),
                -1_251_089_393,
                679,
                1_601,
                vec![
                    (CityMobKind::Warlock, 257),
                    (CityMobKind::FrostElemental, 480),
                    (CityMobKind::Ghoul, 941),
                    (CityMobKind::Ghoul, 983),
                    (CityMobKind::ShockElemental, 988),
                    (CityMobKind::ShockElemental, 997),
                    (CityMobKind::Monk, 1_725),
                ],
                vec![144, 257, 480, 941, 983, 988, 997, 1_725],
            ),
            (
                18,
                Feeling::None,
                (43, 37),
                435_526_208,
                338,
                1_080,
                vec![
                    (CityMobKind::Ghoul, 501),
                    (CityMobKind::Warlock, 547),
                    (CityMobKind::Warlock, 594),
                    (CityMobKind::Monk, 749),
                    (CityMobKind::FireElemental, 1_012),
                    (CityMobKind::Golem, 1_039),
                ],
                vec![408, 501, 547, 594, 749, 1_012, 1_039],
            ),
            (
                19,
                Feeling::None,
                (37, 46),
                -538_847_853,
                461,
                397,
                vec![
                    (CityMobKind::ShockElemental, 279),
                    (CityMobKind::Golem, 325),
                    (CityMobKind::Warlock, 471),
                    (CityMobKind::Golem, 834),
                    (CityMobKind::Monk, 1_135),
                    (CityMobKind::Golem, 1_248),
                    (CityMobKind::Monk, 1_271),
                    (CityMobKind::Warlock, 1_346),
                ],
                vec![279, 325, 396, 471, 834, 1_135, 1_248, 1_271, 1_346, 1_437],
            ),
        ];

        for (floor, expected) in floors.iter().zip(expected) {
            let (depth, feeling, size, hash, entrance, exit, expected_mobs, occupied) = expected;
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
            let mut mobs = floor
                .mobs
                .mobs
                .iter()
                .map(|mob| (mob.mob.kind, mob.cell))
                .collect::<Vec<_>>();
            mobs.sort_unstable_by_key(|(_, cell)| *cell);
            assert_eq!(mobs, expected_mobs, "depth {depth}");
            let actual_occupied = floor
                .painted
                .level
                .mob_cells
                .iter()
                .enumerate()
                .filter_map(|(cell, &is_occupied)| is_occupied.then_some(cell))
                .collect::<Vec<_>>();
            assert_eq!(actual_occupied, occupied, "depth {depth}");
        }
    }

    #[test]
    fn three_nonzero_depth_sixteen_maps_match_official_oracle() {
        for (code, feeling, size, hash, entrance, exit) in [
            (
                "AAA-AAA-AAB",
                Feeling::None,
                (36, 56),
                129_683_275,
                1_420,
                194,
            ),
            (
                "ABC-DEF-GHI",
                Feeling::Traps,
                (52, 32),
                1_928_380_213,
                997,
                667,
            ),
            (
                "ZZZ-ZZZ-ZZZ",
                Feeling::None,
                (33, 49),
                -1_234_450_086,
                350,
                1_176,
            ),
        ] {
            let seed = DungeonSeed::from_code(code).unwrap();
            let floor = generate_city_prefix(seed, 16).pop().unwrap();
            assert_eq!(floor.painted.prepared.feeling, feeling, "{code}");
            assert_eq!(
                (floor.painted.level.width(), floor.painted.level.height()),
                size,
                "{code}"
            );
            assert_eq!(floor.painted.level.java_map_hash(), hash, "{code}");
            assert_eq!(floor.painted.level.entrance(), Some(entrance), "{code}");
            assert_eq!(floor.painted.level.exit(), Some(exit), "{code}");
        }
    }

    #[test]
    fn aaa_sequential_city_searchable_items_match_official_oracle() {
        let floors = generate_city_prefix(DungeonSeed::MIN, 19);
        let expected = [
            vec![
                (
                    ItemId::Glaive,
                    1,
                    None,
                    false,
                    ItemSource::Chest,
                    Accessibility::Independent,
                ),
                (
                    ItemId::WandMagicMissile,
                    0,
                    None,
                    false,
                    ItemSource::LockedChest,
                    Accessibility::Independent,
                ),
                (
                    ItemId::Javelin,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
                (
                    ItemId::Katana,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
                (
                    ItemId::IncendiaryDart,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
                (
                    ItemId::ScaleArmor,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
            ],
            vec![(
                ItemId::ThrowingHammer,
                0,
                None,
                false,
                ItemSource::Mimic,
                Accessibility::Independent,
            )],
            vec![
                (
                    ItemId::PlateArmor,
                    0,
                    None,
                    false,
                    ItemSource::Chest,
                    Accessibility::Independent,
                ),
                (
                    ItemId::MailArmor,
                    1,
                    Some(Effect::Armor(ArmorEffect::Multiplicity)),
                    true,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
                (
                    ItemId::Javelin,
                    1,
                    Some(Effect::Weapon(WeaponEffect::Sacrificial)),
                    true,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
                (
                    ItemId::Trident,
                    1,
                    None,
                    false,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
                (
                    ItemId::RingSharpshooting,
                    0,
                    None,
                    false,
                    ItemSource::Skeleton,
                    Accessibility::Independent,
                ),
            ],
            vec![
                (
                    ItemId::RingHaste,
                    2,
                    None,
                    false,
                    ItemSource::ImpReward,
                    Accessibility::Choice {
                        group: 0,
                        option: 1,
                    },
                ),
                (
                    ItemId::Greatshield,
                    2,
                    Some(Effect::Weapon(WeaponEffect::Lucky)),
                    false,
                    ItemSource::ImpReward,
                    Accessibility::Choice {
                        group: 0,
                        option: 2,
                    },
                ),
                (
                    ItemId::Javelin,
                    4,
                    Some(Effect::Weapon(WeaponEffect::Shocking)),
                    false,
                    ItemSource::ImpReward,
                    Accessibility::Choice {
                        group: 0,
                        option: 3,
                    },
                ),
                (
                    ItemId::PlateArmor,
                    3,
                    Some(Effect::Armor(ArmorEffect::Stone)),
                    false,
                    ItemSource::ImpReward,
                    Accessibility::Choice {
                        group: 0,
                        option: 4,
                    },
                ),
                (
                    ItemId::WandCorrosion,
                    2,
                    None,
                    false,
                    ItemSource::ImpReward,
                    Accessibility::Choice {
                        group: 0,
                        option: 5,
                    },
                ),
                (
                    ItemId::RingEvasion,
                    0,
                    None,
                    false,
                    ItemSource::Mimic,
                    Accessibility::Independent,
                ),
                (
                    ItemId::HeavyBoomerang,
                    2,
                    None,
                    false,
                    ItemSource::Skeleton,
                    Accessibility::Independent,
                ),
                (
                    ItemId::Katana,
                    2,
                    None,
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 6,
                    },
                ),
                (
                    ItemId::BattleAxe,
                    4,
                    Some(Effect::Weapon(WeaponEffect::Blooming)),
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 7,
                    },
                ),
                (
                    ItemId::Javelin,
                    2,
                    None,
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 8,
                    },
                ),
                (
                    ItemId::WandLivingEarth,
                    3,
                    None,
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 9,
                    },
                ),
                (
                    ItemId::Whip,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Kinetic)),
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 10,
                    },
                ),
                (
                    ItemId::RingEvasion,
                    1,
                    None,
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 11,
                    },
                ),
                (
                    ItemId::RingArcana,
                    2,
                    None,
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 12,
                    },
                ),
                (
                    ItemId::LeatherArmor,
                    0,
                    None,
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 13,
                    },
                ),
                (
                    ItemId::Sickle,
                    0,
                    None,
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 14,
                    },
                ),
                (
                    ItemId::Greatsword,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Grim)),
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 15,
                    },
                ),
                (
                    ItemId::WandFireblast,
                    1,
                    None,
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 16,
                    },
                ),
                (
                    ItemId::PlateArmor,
                    3,
                    Some(Effect::Armor(ArmorEffect::Entanglement)),
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 17,
                    },
                ),
                (
                    ItemId::Spear,
                    2,
                    Some(Effect::Weapon(WeaponEffect::Corrupting)),
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 18,
                    },
                ),
                (
                    ItemId::FishingSpear,
                    0,
                    None,
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 19,
                    },
                ),
                (
                    ItemId::HandAxe,
                    0,
                    None,
                    false,
                    ItemSource::VaultTreasure,
                    Accessibility::Choice {
                        group: 0,
                        option: 20,
                    },
                ),
            ],
        ];
        for (floor, expected) in floors.iter().zip(expected) {
            assert_eq!(
                floor
                    .world_items
                    .iter()
                    .filter(|entry| crate::catalog::item(entry.item).kind
                        != crate::catalog::ItemKind::Artifact)
                    .count(),
                expected.len(),
                "depth {} actual {:?}",
                floor.painted.level.depth,
                floor.world_items
            );
            for (item, upgrade, effect, cursed, source, accessibility) in expected {
                assert!(
                    floor.world_items.iter().any(|actual| {
                        actual.item == item
                            && actual.upgrade == upgrade
                            && actual.effect == effect
                            && actual.cursed == cursed
                            && actual.source == source
                            && actual.accessibility == accessibility
                    }),
                    "depth {} missing {item:?} {source:?}",
                    floor.painted.level.depth
                );
            }
        }
    }

    #[test]
    fn city_world_prefix_reports_all_regions_and_batch_roots_match_scalar() {
        let world = generate_city_world(DungeonSeed::MIN, 19).unwrap();
        assert_eq!(world.seed, DungeonSeed::MIN);
        assert!(!world.items.is_empty());

        let seeds = [
            DungeonSeed::MIN,
            DungeonSeed::new(1).unwrap(),
            DungeonSeed::from_code("ABC-DEF-GHI").unwrap(),
            DungeonSeed::MAX,
        ];
        let scalar = seeds.map(|seed| generate_city_world(seed, 19).unwrap());
        let batched = CanonicalCityWorldGenerator.generate_batch(&seeds, 19);
        assert_eq!(batched, scalar);
    }
}
