//! Parity-gated composition of regular Caves floors (depths 11 through 14).
//!
//! A Caves world prefix advances the same run through Sewers and Prison,
//! skipping the generator-state-neutral boss floors at depths 5 and 10. The
//! depth-11 shop therefore observes the bag and category state left by the
//! depth-6 shop, while Blacksmith scheduling and rewards mutate the live
//! Generator during `initRooms()` before room sizing and painting.

#![allow(clippy::missing_panics_doc, clippy::too_many_lines)]

use std::cell::RefCell;
use std::fmt;

use crate::batch::seed_for_depth_batch4;
use crate::builder::{Builder, RegularLevelBuilder};
use crate::caves_mobs::{CavesMobsResult, create_caves_mobs};
use crate::caves_rooms::{CavesPainter, CavesRoomDispatcher};
use crate::geometry::{Point, Rect, painter as draw, terrain};
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

/// Fully painted state immediately before `buildFlagMaps()`.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintedCavesFloor {
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
    /// Both common `WaterBridge` and Caves `RegionDecoBridge` exclusion spaces.
    pub bridge_spaces: Vec<(RoomId, Rect)>,
}

/// One complete Caves `Level.create()` result after mobs and items.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedCavesFloor {
    pub painted: PaintedCavesFloor,
    pub flags: LevelFlags,
    pub mobs: CavesMobsResult,
    pub regular_items: RegularItemsResult,
    pub world_items: Vec<WorldItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CavesPaintError {
    Quest(QuestError),
    Painter(PaintError),
    UnsupportedRoom(RoomKind),
}

impl fmt::Display for CavesPaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Quest(error) => error.fmt(formatter),
            Self::Painter(error) => error.fmt(formatter),
            Self::UnsupportedRoom(kind) => write!(
                formatter,
                "room painter is not parity-gated in the Caves composite: {kind:?}"
            ),
        }
    }
}

impl std::error::Error for CavesPaintError {}

impl From<QuestError> for CavesPaintError {
    fn from(error: QuestError) -> Self {
        Self::Quest(error)
    }
}

impl From<PaintError> for CavesPaintError {
    fn from(error: PaintError) -> Self {
        Self::Painter(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CavesFloorError {
    InvalidMaximumDepth(u8),
    Sewer(SewerFloorError),
    Prison(PrisonFloorError),
    Paint(CavesPaintError),
    Items(RegularItemsError),
}

impl fmt::Display for CavesFloorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMaximumDepth(depth) => {
                write!(
                    formatter,
                    "Caves world depth must be in 11..=14, got {depth}"
                )
            }
            Self::Sewer(error) => error.fmt(formatter),
            Self::Prison(error) => error.fmt(formatter),
            Self::Paint(error) => error.fmt(formatter),
            Self::Items(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CavesFloorError {}

impl From<SewerFloorError> for CavesFloorError {
    fn from(error: SewerFloorError) -> Self {
        Self::Sewer(error)
    }
}

impl From<PrisonFloorError> for CavesFloorError {
    fn from(error: PrisonFloorError) -> Self {
        Self::Prison(error)
    }
}

impl From<CavesPaintError> for CavesFloorError {
    fn from(error: CavesPaintError) -> Self {
        Self::Paint(error)
    }
}

impl From<RegularItemsError> for CavesFloorError {
    fn from(error: RegularItemsError) -> Self {
        Self::Items(error)
    }
}

/// Exact world prefix through a regular Caves depth.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanonicalCavesWorldGenerator;

impl WorldGenerator for CanonicalCavesWorldGenerator {
    fn generate(&self, seed: DungeonSeed, max_depth: u8) -> GeneratedWorld {
        generate_caves_world(seed, max_depth)
            .expect("CanonicalCavesWorldGenerator accepts only depths 11..=14")
    }

    fn generate_batch(&self, seeds: &[DungeonSeed], max_depth: u8) -> Vec<GeneratedWorld> {
        assert!(
            (11..=14).contains(&max_depth),
            "CanonicalCavesWorldGenerator accepts only depths 11..=14"
        );
        let depths = (1..=4)
            .chain(6..=9)
            .chain(11..=u32::from(max_depth))
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
                    generate_caves_world_with_roots(chunk[lane], max_depth, &roots)
                        .expect("validated Caves batch satisfies canonical invariants"),
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

/// Generates the full main-path prefix through `maximum_depth`.
///
/// Boss floors 5 and 10 consume only their independent depth RNG and leave
/// Generator, quest, limited-drop, room-deck, and shop state unchanged in the
/// canonical fresh-run profile. Official checkpoints pin the state hashes on
/// both sides of each skip.
///
/// # Errors
///
/// Returns an invalid-depth error outside 11..=14 or the exact failing floor
/// generation phase.
pub fn generate_caves_world(
    seed: DungeonSeed,
    maximum_depth: u8,
) -> Result<GeneratedWorld, CavesFloorError> {
    if !(11..=14).contains(&maximum_depth) {
        return Err(CavesFloorError::InvalidMaximumDepth(maximum_depth));
    }
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let roots = (1..=4)
        .chain(6..=9)
        .chain(11..=u32::from(maximum_depth))
        .map(|depth| seed_for_depth(dungeon_seed, depth, 0))
        .collect::<Vec<_>>();
    generate_caves_world_with_roots(seed, maximum_depth, &roots)
}

fn generate_caves_world_with_roots(
    seed: DungeonSeed,
    maximum_depth: u8,
    roots: &[i64],
) -> Result<GeneratedWorld, CavesFloorError> {
    let expected_roots = usize::from(maximum_depth) - 2;
    if !(11..=14).contains(&maximum_depth) || roots.len() != expected_roots {
        return Err(CavesFloorError::InvalidMaximumDepth(maximum_depth));
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

    for (index, &root) in roots[8..].iter().enumerate() {
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
    Ok(GeneratedWorld { seed, items })
}

/// Completes a regular Caves floor through flags, mobs, Blacksmith rewards,
/// and ordinary item placement.
///
/// # Errors
///
/// Returns a paint or ordinary-item generation invariant failure.
pub fn generate_caves_floor(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    random: &mut RandomStack,
) -> Result<GeneratedCavesFloor, CavesFloorError> {
    let mut painted = paint_caves_floor(run, limited_drops, quests, shop_run, depth, random)?;
    let mut flags = LevelFlags::build_for_generation(&painted.level.map);
    let entrance_cell = painted
        .level
        .entrance()
        .expect("painted regular floor has an entrance");
    let exit_cell = painted
        .level
        .exit()
        .expect("painted regular floor has an exit");
    let spatial_rules = CavesSpatialRules {
        bridge_spaces: painted.bridge_spaces.clone(),
        quest_state: painted.quest_paint_state.clone(),
    };
    let mobs = create_caves_mobs(
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
    let blacksmith_group = painted.remaining_prizes.next_choice_group;
    if quests.blacksmith.depth == Some(u8::try_from(depth).expect("Caves depth fits u8")) {
        quests
            .blacksmith
            .append_world_items(blacksmith_group, &mut world_items);
    }

    let queue = painted
        .remaining_prizes
        .items_to_spawn
        .iter()
        .copied()
        .map(special_to_regular)
        .collect();
    let mut placement_rules = CanonicalRoomPlacementRules::default();
    for &(room, rect) in &painted.bridge_spaces {
        placement_rules.set_bridge_space_rect(room, rect);
    }
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
            u8::try_from(depth).expect("Caves depth fits u8"),
            painted.prepared.feeling,
            run.challenges,
            queue,
            &mut placement,
        )?
    };
    world_items.extend(regular_items.world_items.iter().cloned());

    Ok(GeneratedCavesFloor {
        painted,
        flags,
        mobs,
        regular_items,
        world_items,
    })
}

/// Runs the exact Caves build and painter phases. The caller owns the active
/// per-depth RNG generator.
///
/// # Errors
///
/// Returns a quest scheduling, unsupported-room, or structural painter
/// failure.
pub fn paint_caves_floor(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    random: &mut RandomStack,
) -> Result<PaintedCavesFloor, CavesPaintError> {
    assert!((11..=14).contains(&depth), "Caves depths are 11..=14");
    let prepared = prepare_regular_floor(run, limited_drops, depth, random)
        .map_err(QuestError::from)
        .map_err(CavesPaintError::from)?;
    let (built, shop_room) = build_caves_room_graph(
        run,
        limited_drops,
        quests,
        shop_run,
        depth,
        prepared.feeling,
        random,
    )?;
    if let Err(error) = reject_unwired_rooms(&built) {
        quests.blacksmith.finish_build_attempt(false);
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
        depth: i32::try_from(depth).expect("Caves depth fits Java int"),
        generator: shared_generator,
        prizes: shared_prizes,
    };
    let regular = CavesRoomDispatcher::new(content).set_revealed_trap_chance(0.0);
    let mut dispatch = CavesCompositeDispatcher {
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
    if let Err(error) = CavesPainter::new(prepared.feeling, trap_count).paint(
        &mut level,
        &mut rooms,
        &mut dispatch,
        random,
    ) {
        quests.blacksmith.finish_build_attempt(false);
        return Err(error.into());
    }

    let bridge_spaces = (0..rooms.len())
        .filter_map(|room| {
            dispatch
                .regular
                .bridge_space(room)
                .or_else(|| dispatch.regular.common.bridge_space(room))
                .map(|rect| (room, rect))
        })
        .collect();
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
    quests.blacksmith.finish_build_attempt(true);
    Ok(PaintedCavesFloor {
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
        bridge_spaces,
    })
}

fn build_caves_room_graph(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    feeling: crate::level_prelude::Feeling,
    random: &mut RandomStack,
) -> Result<(BuiltRegularGraph, ForcedShopRoomState), CavesPaintError> {
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
                    i32::try_from(depth).expect("Caves depth fits Java int"),
                    shop_run,
                )
                .expect("canonical Caves shop generation is valid");
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

fn reject_unwired_rooms(graph: &BuiltRegularGraph) -> Result<(), CavesPaintError> {
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
            kind @ RoomKind::Special(_) => return Err(CavesPaintError::UnsupportedRoom(kind)),
        }
    }
    Ok(())
}

struct CavesCompositeDispatcher<'a, 's> {
    regular: CavesRoomDispatcher<SharedSewerContent<'a>>,
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

impl RoomPaintDispatch for CavesCompositeDispatcher<'_, '_> {
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
struct CavesSpatialRules {
    bridge_spaces: Vec<(RoomId, Rect)>,
    quest_state: QuestPaintState,
}

impl RoomCharacterRules for CavesSpatialRules {
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
            RoomKind::Entrance(
                StandardRoomKind::WaterBridge | StandardRoomKind::RegionDecoBridge,
            )
            | RoomKind::Exit(StandardRoomKind::WaterBridge | StandardRoomKind::RegionDecoBridge)
            | RoomKind::Standard(
                StandardRoomKind::WaterBridge | StandardRoomKind::RegionDecoBridge,
            ) => {
                if self
                    .bridge_spaces
                    .iter()
                    .find_map(|(id, rect)| (*id == room).then_some(*rect))
                    .is_some_and(|rect| rect.inside(point))
                {
                    return false;
                }
            }
            RoomKind::Entrance(StandardRoomKind::CavesFissure)
            | RoomKind::Exit(StandardRoomKind::CavesFissure)
            | RoomKind::Standard(StandardRoomKind::CavesFissure)
                if level.map.cells[cell] == terrain::EMPTY_SP =>
            {
                return false;
            }
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
    use crate::caves_mobs::CavesMobKind;
    use crate::level_prelude::Feeling;
    use crate::model::{Accessibility, ItemSource};

    type ExpectedItem = (ItemId, u8, Option<Effect>, bool, ItemSource, Accessibility);
    type DepthElevenFixture = (
        &'static str,
        Feeling,
        (i32, i32),
        i32,
        usize,
        usize,
        &'static [usize],
        &'static [ExpectedItem],
    );

    fn generate_caves_prefix(seed: DungeonSeed, maximum_depth: u32) -> Vec<GeneratedCavesFloor> {
        generate_caves_prefix_with_challenges(
            seed,
            maximum_depth,
            crate::challenges::Challenges::NONE,
        )
    }

    fn generate_caves_prefix_with_challenges(
        seed: DungeonSeed,
        maximum_depth: u32,
        challenges: crate::challenges::Challenges,
    ) -> Vec<GeneratedCavesFloor> {
        let dungeon_seed = i64::try_from(seed.value()).unwrap();
        let mut run = RunState::with_challenges(dungeon_seed, challenges);
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

        (11..=maximum_depth)
            .map(|depth| {
                random.push(seed_for_depth(dungeon_seed, depth, 0));
                let floor = generate_caves_floor(
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
    fn aaf_challenge_caves_match_official_oracle_reduced_fixtures() {
        use crate::challenges::Challenges;

        let fixtures = [
            (
                Challenges::NONE,
                [1_019_694_273, 1_480_294_126, 1_728_692_213, -2_037_398_484],
            ),
            (
                Challenges::NO_HERBALISM,
                [-1_886_738_028, 1_480_294_126, 222_561_544, 1_382_767_999],
            ),
            (
                Challenges::DARKNESS,
                [-60_845_842, -234_339_807, 1_728_692_213, -2_037_398_484],
            ),
            (
                Challenges::NO_SCROLLS,
                [1_019_694_273, 1_480_294_126, 1_728_692_213, -2_037_398_484],
            ),
            (
                Challenges::NO_HERBALISM | Challenges::DARKNESS | Challenges::NO_SCROLLS,
                [-1_820_831_865, 1_480_294_126, 222_561_544, -1_734_856_910],
            ),
        ];
        let seed = DungeonSeed::new(5).unwrap();
        for (challenges, hashes) in fixtures {
            let floors = generate_caves_prefix_with_challenges(seed, 14, challenges);
            assert_eq!(
                std::array::from_fn(|index| floors[index].painted.level.java_map_hash()),
                hashes,
                "mask {}",
                challenges.bits(),
            );
        }
    }

    fn occupied_cells(floor: &GeneratedCavesFloor) -> Vec<usize> {
        floor
            .painted
            .level
            .mob_cells
            .iter()
            .enumerate()
            .filter_map(|(cell, &occupied)| occupied.then_some(cell))
            .collect()
    }

    fn assert_items(floor: &GeneratedCavesFloor, expected: &[ExpectedItem]) {
        assert_eq!(
            floor.world_items.len(),
            expected.len(),
            "depth {} item count",
            floor.painted.level.depth
        );
        for &(item, upgrade, effect, cursed, source, accessibility) in expected {
            assert!(
                floor.world_items.iter().any(|actual| {
                    actual.item == item
                        && actual.upgrade == upgrade
                        && actual.effect == effect
                        && actual.cursed == cursed
                        && actual.source == source
                        && actual.accessibility == accessibility
                }),
                "depth {} missing {item:?} {source:?} +{upgrade} {effect:?}",
                floor.painted.level.depth
            );
        }
    }

    #[test]
    fn aaa_sequential_caves_maps_mobs_and_npc_cells_match_official_oracle() {
        let floors = generate_caves_prefix(DungeonSeed::MIN, 14);
        let expected = [
            (
                11,
                Feeling::Water,
                (45, 46),
                -839_635_038,
                346,
                1_630,
                vec![
                    (CavesMobKind::Bat, 952),
                    (CavesMobKind::Bat, 1_190),
                    (CavesMobKind::Brute, 1_310),
                    (CavesMobKind::BlueShaman, 1_681),
                ],
                vec![263, 904, 952, 1_190, 1_310, 1_681],
            ),
            (
                12,
                Feeling::Water,
                (43, 48),
                -1_548_060_147,
                1_692,
                414,
                vec![
                    (CavesMobKind::Spinner, 161),
                    (CavesMobKind::RedShaman, 329),
                    (CavesMobKind::Brute, 865),
                    (CavesMobKind::Brute, 968),
                    (CavesMobKind::Bat, 1_085),
                    (CavesMobKind::Bat, 1_192),
                    (CavesMobKind::Bat, 1_615),
                ],
                vec![161, 329, 865, 968, 1_085, 1_192, 1_615, 1_913, 1_914, 1_957],
            ),
            (
                13,
                Feeling::Traps,
                (45, 32),
                1_671_650_609,
                624,
                769,
                vec![
                    (CavesMobKind::Brute, 389),
                    (CavesMobKind::Bat, 724),
                    (CavesMobKind::Spinner, 963),
                    (CavesMobKind::Dm200, 1_055),
                    (CavesMobKind::Brute, 1_058),
                    (CavesMobKind::RedShaman, 1_163),
                    (CavesMobKind::Spinner, 1_209),
                ],
                vec![389, 724, 963, 1_055, 1_058, 1_163, 1_164, 1_209],
            ),
            (
                14,
                Feeling::Water,
                (49, 41),
                -434_007_164,
                655,
                1_413,
                vec![
                    (CavesMobKind::Dm200, 526),
                    (CavesMobKind::RedShaman, 877),
                    (CavesMobKind::Spinner, 924),
                    (CavesMobKind::Bat, 1_021),
                    (CavesMobKind::Brute, 1_096),
                    (CavesMobKind::RedShaman, 1_414),
                    (CavesMobKind::Dm200, 1_676),
                    (CavesMobKind::BlueShaman, 1_684),
                    (CavesMobKind::Spinner, 1_744),
                ],
                vec![526, 877, 924, 973, 1_021, 1_096, 1_414, 1_676, 1_684, 1_744],
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
            assert_eq!(occupied_cells(floor), occupied, "depth {depth}");
        }
    }

    #[test]
    fn aaa_sequential_caves_searchable_items_match_official_oracle() {
        let floors = generate_caves_prefix(DungeonSeed::MIN, 14);
        let expected: [Vec<ExpectedItem>; 4] = [
            vec![
                (
                    ItemId::ScaleArmor,
                    0,
                    Some(Effect::Armor(ArmorEffect::Stone)),
                    false,
                    ItemSource::Mimic,
                    Accessibility::Independent,
                ),
                (
                    ItemId::ChillingDart,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
                (
                    ItemId::Whip,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
                (
                    ItemId::MailArmor,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
                (
                    ItemId::ThrowingSpear,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
                (
                    ItemId::WandMagicMissile,
                    0,
                    None,
                    true,
                    ItemSource::Tomb,
                    Accessibility::Independent,
                ),
            ],
            vec![(
                ItemId::Crossbow,
                1,
                Some(Effect::Weapon(WeaponEffect::Explosive)),
                true,
                ItemSource::SacrificialFire,
                Accessibility::Independent,
            )],
            vec![
                (
                    ItemId::BattleAxe,
                    2,
                    None,
                    false,
                    ItemSource::BlacksmithReward,
                    Accessibility::Choice {
                        group: 0,
                        option: 0,
                    },
                ),
                (
                    ItemId::Greatshield,
                    2,
                    None,
                    false,
                    ItemSource::BlacksmithReward,
                    Accessibility::Choice {
                        group: 0,
                        option: 1,
                    },
                ),
                (
                    ItemId::Tomahawk,
                    2,
                    None,
                    false,
                    ItemSource::BlacksmithReward,
                    Accessibility::Choice {
                        group: 0,
                        option: 2,
                    },
                ),
                (
                    ItemId::ScaleArmor,
                    2,
                    None,
                    false,
                    ItemSource::BlacksmithReward,
                    Accessibility::Choice {
                        group: 0,
                        option: 3,
                    },
                ),
                (
                    ItemId::Scimitar,
                    1,
                    Some(Effect::Weapon(WeaponEffect::Polarized)),
                    true,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
                (
                    ItemId::MailArmor,
                    0,
                    None,
                    false,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
            ],
            vec![
                (
                    ItemId::AssassinsBlade,
                    1,
                    None,
                    false,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
                (
                    ItemId::Katana,
                    0,
                    None,
                    false,
                    ItemSource::LockedChest,
                    Accessibility::Independent,
                ),
                (
                    ItemId::Katana,
                    0,
                    Some(Effect::Weapon(WeaponEffect::Annoying)),
                    true,
                    ItemSource::Mimic,
                    Accessibility::Independent,
                ),
                (
                    ItemId::PlateArmor,
                    0,
                    Some(Effect::Armor(ArmorEffect::AntiEntropy)),
                    true,
                    ItemSource::Tomb,
                    Accessibility::Independent,
                ),
            ],
        ];
        for (floor, expected) in floors.iter().zip(&expected) {
            assert_items(floor, expected);
        }
    }

    #[test]
    fn three_nonzero_depth_eleven_fixtures_match_full_official_oracle() {
        let fixtures: [DepthElevenFixture; 3] = [
            (
                "AAA-AAA-AAB",
                Feeling::Large,
                (37, 64),
                1_241_369_111,
                686,
                1_377,
                &[
                    156, 193, 252, 326, 339, 352, 660, 1_026, 1_269, 1_978, 2_086,
                ],
                &[
                    (
                        ItemId::ScaleArmor,
                        0,
                        Some(Effect::Armor(ArmorEffect::Swiftness)),
                        false,
                        ItemSource::Chest,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::RoundShield,
                        1,
                        None,
                        false,
                        ItemSource::LockedChest,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::CleansingDart,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::ThrowingSpear,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::RoundShield,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::MailArmor,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                ],
            ),
            (
                "ABC-DEF-GHI",
                Feeling::None,
                (43, 41),
                -835_444_084,
                955,
                894,
                &[534, 779, 877, 918, 922, 1_371, 1_510, 1_519],
                &[
                    (
                        ItemId::DisplacingDart,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::Kunai,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::Mace,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::MailArmor,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                ],
            ),
            (
                "ZZZ-ZZZ-ZZZ",
                Feeling::None,
                (42, 48),
                -97_339_910,
                858,
                1_729,
                &[283, 306, 658, 1_044, 1_366, 1_731, 1_855],
                &[
                    (
                        ItemId::RingForce,
                        1,
                        None,
                        false,
                        ItemSource::CrystalChest,
                        Accessibility::Choice {
                            group: 0,
                            option: 1,
                        },
                    ),
                    (
                        ItemId::Mace,
                        0,
                        Some(Effect::Weapon(WeaponEffect::Annoying)),
                        true,
                        ItemSource::Chest,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::WandLivingEarth,
                        0,
                        None,
                        false,
                        ItemSource::Heap,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::Whip,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::Bolas,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::MailArmor,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::ChillingDart,
                        0,
                        None,
                        false,
                        ItemSource::Shop,
                        Accessibility::Independent,
                    ),
                    (
                        ItemId::Sai,
                        0,
                        Some(Effect::Weapon(WeaponEffect::Blocking)),
                        false,
                        ItemSource::Statue,
                        Accessibility::Independent,
                    ),
                ],
            ),
        ];

        for (code, feeling, size, hash, entrance, exit, occupied, items) in fixtures {
            let seed = DungeonSeed::from_code(code).unwrap();
            let floor = generate_caves_prefix(seed, 11).pop().unwrap();
            assert_eq!(floor.painted.prepared.feeling, feeling, "{code}");
            assert_eq!(
                (floor.painted.level.width(), floor.painted.level.height()),
                size,
                "{code}"
            );
            assert_eq!(floor.painted.level.java_map_hash(), hash, "{code}");
            assert_eq!(floor.painted.level.entrance(), Some(entrance), "{code}");
            assert_eq!(floor.painted.level.exit(), Some(exit), "{code}");
            assert_eq!(occupied_cells(&floor), occupied, "{code}");
            assert_items(&floor, items);
        }
    }

    #[test]
    fn canonical_caves_world_and_four_lane_batch_are_identical() {
        let seeds = [
            DungeonSeed::MIN,
            DungeonSeed::new(1).unwrap(),
            DungeonSeed::from_code("ABC-DEF-GHI").unwrap(),
            DungeonSeed::MAX,
        ];
        let generator = CanonicalCavesWorldGenerator;
        let scalar = seeds
            .iter()
            .copied()
            .map(|seed| generator.generate(seed, 14))
            .collect::<Vec<_>>();
        assert_eq!(generator.generate_batch(&seeds, 14), scalar);
        assert!(
            scalar[0]
                .items
                .iter()
                .any(|item| item.depth == 13 && item.source == ItemSource::BlacksmithReward)
        );
    }
}
