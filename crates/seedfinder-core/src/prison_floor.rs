//! Parity-gated composition of regular Prison floors (depths 6 through 9).
//!
//! This is the first regional composition with a shop and a quest room.  The
//! shop's inventory is deliberately generated from the builder's first
//! `ShopRoom.minWidth()` call, while the Wandmaker's rewards remain in the
//! later `createMobs()` phase.  Moving either operation would change all
//! subsequent dungeon draws.

#![allow(clippy::missing_panics_doc, clippy::too_many_lines)]

use std::cell::RefCell;
use std::fmt;

use crate::batch::seed_for_depth_batch4;
use crate::builder::{Builder, RegularLevelBuilder};
use crate::geometry::{Point, Rect, painter as draw, terrain};
use crate::level::Level;
use crate::level_flags::LevelFlags;
use crate::level_prelude::LimitedDrops;
use crate::model::{GeneratedWorld, WorldItem};
use crate::painter::{PaintError, RoomPaintDispatch, draw_regular_trap_count};
use crate::prison_mobs::{PrisonMobsError, PrisonMobsResult, create_prison_mobs};
use crate::prison_rooms::{PrisonPainter, PrisonRoomDispatcher};
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
use crate::room::{QuestRoomKind, Room, RoomId, RoomKind, SpecialRoomKind, StandardRoomKind};
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
pub struct PaintedPrisonFloor {
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
    pub bridge_spaces: Vec<(RoomId, Rect)>,
}

/// One complete Prison `Level.create()` result after mobs and items.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedPrisonFloor {
    pub painted: PaintedPrisonFloor,
    pub flags: LevelFlags,
    pub mobs: PrisonMobsResult,
    pub regular_items: RegularItemsResult,
    pub world_items: Vec<WorldItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PrisonPaintError {
    Quest(QuestError),
    Painter(PaintError),
    UnsupportedRoom(RoomKind),
}

impl fmt::Display for PrisonPaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Quest(error) => error.fmt(formatter),
            Self::Painter(error) => error.fmt(formatter),
            Self::UnsupportedRoom(kind) => write!(
                formatter,
                "room painter is not parity-gated in the Prison composite: {kind:?}"
            ),
        }
    }
}

impl std::error::Error for PrisonPaintError {}

impl From<QuestError> for PrisonPaintError {
    fn from(error: QuestError) -> Self {
        Self::Quest(error)
    }
}

impl From<PaintError> for PrisonPaintError {
    fn from(error: PaintError) -> Self {
        Self::Painter(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PrisonFloorError {
    InvalidMaximumDepth(u8),
    Sewer(SewerFloorError),
    Paint(PrisonPaintError),
    Mobs(PrisonMobsError),
    Items(RegularItemsError),
}

impl fmt::Display for PrisonFloorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMaximumDepth(depth) => {
                write!(
                    formatter,
                    "Prison world depth must be in 6..=9, got {depth}"
                )
            }
            Self::Sewer(error) => error.fmt(formatter),
            Self::Paint(error) => error.fmt(formatter),
            Self::Mobs(error) => error.fmt(formatter),
            Self::Items(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for PrisonFloorError {}

impl From<SewerFloorError> for PrisonFloorError {
    fn from(error: SewerFloorError) -> Self {
        Self::Sewer(error)
    }
}

impl From<PrisonPaintError> for PrisonFloorError {
    fn from(error: PrisonPaintError) -> Self {
        Self::Paint(error)
    }
}

impl From<PrisonMobsError> for PrisonFloorError {
    fn from(error: PrisonMobsError) -> Self {
        Self::Mobs(error)
    }
}

impl From<RegularItemsError> for PrisonFloorError {
    fn from(error: RegularItemsError) -> Self {
        Self::Items(error)
    }
}

/// Exact world prefix through a regular Prison depth.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanonicalPrisonWorldGenerator;

impl WorldGenerator for CanonicalPrisonWorldGenerator {
    fn generate(&self, seed: DungeonSeed, max_depth: u8) -> GeneratedWorld {
        generate_prison_world(seed, max_depth)
            .expect("CanonicalPrisonWorldGenerator accepts only depths 6..=9")
    }

    fn generate_batch(&self, seeds: &[DungeonSeed], max_depth: u8) -> Vec<GeneratedWorld> {
        assert!(
            (6..=9).contains(&max_depth),
            "CanonicalPrisonWorldGenerator accepts only depths 6..=9"
        );
        let depths = (1..=4).chain(6..=u32::from(max_depth)).collect::<Vec<_>>();
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
                    generate_prison_world_with_roots(chunk[lane], max_depth, &roots)
                        .expect("validated Prison batch satisfies canonical invariants"),
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

/// Generates Sewer floors 1..=4, skips the draw-free Goo floor, and then
/// generates regular Prison floors through `maximum_depth`.
///
/// # Errors
///
/// Returns an invalid-depth error outside 6..=9 or the exact failing Sewer or
/// Prison generation phase.
pub fn generate_prison_world(
    seed: DungeonSeed,
    maximum_depth: u8,
) -> Result<GeneratedWorld, PrisonFloorError> {
    if !(6..=9).contains(&maximum_depth) {
        return Err(PrisonFloorError::InvalidMaximumDepth(maximum_depth));
    }
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let roots = (1..=4)
        .chain(6..=u32::from(maximum_depth))
        .map(|depth| seed_for_depth(dungeon_seed, depth, 0))
        .collect::<Vec<_>>();
    generate_prison_world_with_roots(seed, maximum_depth, &roots)
}

fn generate_prison_world_with_roots(
    seed: DungeonSeed,
    maximum_depth: u8,
    roots: &[i64],
) -> Result<GeneratedWorld, PrisonFloorError> {
    let expected_roots = usize::from(maximum_depth) - 1;
    if !(6..=9).contains(&maximum_depth) || roots.len() != expected_roots {
        return Err(PrisonFloorError::InvalidMaximumDepth(maximum_depth));
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

    // GooLevel depth 5 does not mutate Generator, quest, limited-drop, or
    // room-deck state in the canonical fresh-run profile.
    for (index, &root) in roots[4..].iter().enumerate() {
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
    Ok(GeneratedWorld { seed, items })
}

/// Completes a regular Prison floor through flags, Wandmaker/ordinary mobs,
/// and ordinary item placement.
///
/// # Errors
///
/// Returns a paint, mob, or ordinary-item generation invariant failure.
pub fn generate_prison_floor(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    random: &mut RandomStack,
) -> Result<GeneratedPrisonFloor, PrisonFloorError> {
    let mut painted = paint_prison_floor(run, limited_drops, quests, shop_run, depth, random)?;
    let mut flags = LevelFlags::build_for_generation(&painted.level.map);
    let entrance_cell = painted
        .level
        .entrance()
        .expect("painted regular floor has an entrance");
    let exit_cell = painted
        .level
        .exit()
        .expect("painted regular floor has an exit");
    let spatial_rules = PrisonSpatialRules {
        bridge_spaces: painted.bridge_spaces.clone(),
        quest_state: painted.quest_paint_state.clone(),
    };
    let wandmaker_group = painted.remaining_prizes.next_choice_group;
    let mobs = create_prison_mobs(
        &mut painted.level,
        &mut flags,
        &painted.rooms,
        painted.entrance_room,
        painted.exit_room,
        entrance_cell,
        exit_cell,
        &spatial_rules,
        &mut quests.wandmaker,
        &mut run.generator,
        wandmaker_group,
        random,
    )?;

    let mut world_items = painted.world_items.clone();
    append_painted_room_items(&painted.level, depth, &mut world_items);
    world_items.extend(mobs.quest_rewards.iter().cloned());

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
    if let Some(ritual) = painted.quest_paint_state.ritual_position {
        if let Some(room) = painted
            .rooms
            .iter()
            .position(|room| room.kind == RoomKind::Quest(QuestRoomKind::RitualSite))
        {
            placement_rules.set_ritual_cell(room, ritual);
        }
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
            u8::try_from(depth).expect("Prison depth fits u8"),
            painted.prepared.feeling,
            run.challenges,
            queue,
            &mut placement,
        )?
    };
    world_items.extend(regular_items.world_items.iter().cloned());

    Ok(GeneratedPrisonFloor {
        painted,
        flags,
        mobs,
        regular_items,
        world_items,
    })
}

/// Runs the exact Prison build and painter phases. The caller owns the active
/// per-depth RNG generator.
///
/// # Errors
///
/// Returns a quest scheduling, unsupported-room, or structural painter
/// failure.
pub fn paint_prison_floor(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    random: &mut RandomStack,
) -> Result<PaintedPrisonFloor, PrisonPaintError> {
    assert!((6..=9).contains(&depth), "Prison depths are 6..=9");
    let prepared = prepare_regular_floor(run, limited_drops, depth, random)
        .map_err(QuestError::from)
        .map_err(PrisonPaintError::from)?;
    let (built, shop_room) = build_prison_room_graph(
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
        depth: i32::try_from(depth).expect("Prison depth fits Java int"),
        generator: shared_generator,
        prizes: shared_prizes,
    };
    let regular = PrisonRoomDispatcher::new(content).set_revealed_trap_chance(0.0);
    let mut dispatch = PrisonCompositeDispatcher {
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
    PrisonPainter::new(prepared.feeling, trap_count).paint(
        &mut level,
        &mut rooms,
        &mut dispatch,
        random,
    )?;

    let bridge_spaces = (0..rooms.len())
        .filter_map(|room| dispatch.regular.bridge_space(room).map(|rect| (room, rect)))
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
    Ok(PaintedPrisonFloor {
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

fn build_prison_room_graph(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    shop_run: &mut ShopRunState,
    depth: u32,
    feeling: crate::level_prelude::Feeling,
    random: &mut RandomStack,
) -> Result<(BuiltRegularGraph, ForcedShopRoomState), PrisonPaintError> {
    // Java chooses/configures the builder before `initRooms()`.
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
                    i32::try_from(depth).expect("Prison depth fits Java int"),
                    shop_run,
                )
                .expect("canonical Prison shop generation is valid");
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

fn reject_unwired_rooms(graph: &BuiltRegularGraph) -> Result<(), PrisonPaintError> {
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
            kind @ RoomKind::Special(_) => {
                return Err(PrisonPaintError::UnsupportedRoom(kind));
            }
        }
    }
    Ok(())
}

struct PrisonCompositeDispatcher<'a, 's> {
    regular: PrisonRoomDispatcher<SharedSewerContent<'a>>,
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

impl RoomPaintDispatch for PrisonCompositeDispatcher<'_, '_> {
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
struct PrisonSpatialRules {
    bridge_spaces: Vec<(RoomId, Rect)>,
    quest_state: QuestPaintState,
}

impl RoomCharacterRules for PrisonSpatialRules {
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
            RoomKind::Entrance(StandardRoomKind::ChasmBridge)
            | RoomKind::Exit(StandardRoomKind::ChasmBridge)
            | RoomKind::Standard(StandardRoomKind::ChasmBridge) => {
                if self
                    .bridge_spaces
                    .iter()
                    .find_map(|(id, rect)| (*id == room).then_some(*rect))
                    .is_some_and(|rect| rect.inside(point))
                {
                    return false;
                }
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
    use crate::catalog::{Effect, ItemId, WeaponEffect};
    use crate::level_prelude::Feeling;
    use crate::model::{Accessibility, ItemSource};
    use crate::prison_mobs::PrisonMobKind;

    fn generate_prison_prefix(seed: DungeonSeed, maximum_depth: u32) -> Vec<GeneratedPrisonFloor> {
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

        (6..=maximum_depth)
            .map(|depth| {
                random.push(seed_for_depth(dungeon_seed, depth, 0));
                let floor = generate_prison_floor(
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
    fn aaa_sequential_prison_maps_mobs_and_npc_cells_match_official_oracle() {
        let floors = generate_prison_prefix(DungeonSeed::MIN, 9);
        let expected = [
            (
                6,
                Feeling::Large,
                (39, 52),
                1_815_027_889,
                249,
                1_225,
                None,
                vec![
                    (PrisonMobKind::Skeleton, 417),
                    (PrisonMobKind::Skeleton, 786),
                    (PrisonMobKind::Thief, 837),
                    (PrisonMobKind::Skeleton, 1_028),
                    (PrisonMobKind::Swarm, 1_311),
                    (PrisonMobKind::Skeleton, 1_454),
                ],
                vec![256, 417, 786, 837, 1_028, 1_311, 1_454, 1_704],
            ),
            (
                7,
                Feeling::Large,
                (48, 54),
                229_406_850,
                652,
                2_084,
                None,
                vec![
                    (PrisonMobKind::Guard, 278),
                    (PrisonMobKind::Skeleton, 755),
                    (PrisonMobKind::Thief, 811),
                    (PrisonMobKind::Skeleton, 1_231),
                    (PrisonMobKind::Skeleton, 1_521),
                    (PrisonMobKind::Dm100, 1_568),
                    (PrisonMobKind::Thief, 1_789),
                ],
                vec![278, 755, 811, 1_231, 1_521, 1_568, 1_789, 2_138],
            ),
            (
                8,
                Feeling::None,
                (56, 50),
                -2_001_044_122,
                1_780,
                846,
                None,
                vec![
                    (PrisonMobKind::Necromancer, 465),
                    (PrisonMobKind::Dm100, 1_208),
                    (PrisonMobKind::Guard, 1_301),
                    (PrisonMobKind::Dm100, 1_592),
                    (PrisonMobKind::Guard, 2_336),
                    (PrisonMobKind::Skeleton, 2_506),
                ],
                vec![353, 465, 1_208, 1_301, 1_592, 2_336, 2_506],
            ),
            (
                9,
                Feeling::None,
                (35, 38),
                -1_759_126_364,
                189,
                791,
                Some(328),
                vec![
                    (PrisonMobKind::Thief, 579),
                    (PrisonMobKind::Skeleton, 585),
                    (PrisonMobKind::Guard, 618),
                    (PrisonMobKind::Dm100, 688),
                    (PrisonMobKind::Dm100, 852),
                    (PrisonMobKind::Necromancer, 854),
                    (PrisonMobKind::Guard, 939),
                    (PrisonMobKind::Necromancer, 1_030),
                ],
                vec![328, 579, 585, 618, 688, 852, 854, 939, 1_030],
            ),
        ];

        for (floor, expected) in floors.iter().zip(expected) {
            let (
                depth,
                feeling,
                size,
                map_hash,
                entrance,
                exit,
                wandmaker,
                expected_mobs,
                expected_occupied,
            ) = expected;
            assert_eq!(floor.painted.level.depth, depth);
            assert_eq!(floor.painted.prepared.feeling, feeling, "depth {depth}");
            assert_eq!(
                (floor.painted.level.width(), floor.painted.level.height()),
                size,
                "depth {depth}"
            );
            assert_eq!(
                floor.painted.level.java_map_hash(),
                map_hash,
                "depth {depth}"
            );
            assert_eq!(
                floor.painted.level.entrance(),
                Some(entrance),
                "depth {depth}"
            );
            assert_eq!(floor.painted.level.exit(), Some(exit), "depth {depth}");
            assert_eq!(floor.mobs.wandmaker_cell, wandmaker, "depth {depth}");
            let mut mobs = floor
                .mobs
                .mobs
                .iter()
                .map(|mob| (mob.mob.kind, mob.cell))
                .collect::<Vec<_>>();
            mobs.sort_unstable_by_key(|(_, cell)| *cell);
            assert_eq!(mobs, expected_mobs, "depth {depth}");
            let occupied = floor
                .painted
                .level
                .mob_cells
                .iter()
                .enumerate()
                .filter_map(|(cell, &occupied)| occupied.then_some(cell))
                .collect::<Vec<_>>();
            assert_eq!(occupied, expected_occupied, "depth {depth}");
        }
    }

    #[test]
    fn three_nonzero_depth_six_maps_match_official_oracle() {
        for (code, feeling, size, hash, entrance, exit) in [
            (
                "AAA-AAA-AAB",
                Feeling::None,
                (47, 38),
                554_209_483,
                814,
                366,
            ),
            (
                "ABC-DEF-GHI",
                Feeling::Water,
                (45, 37),
                1_938_948_240,
                411,
                1_115,
            ),
            (
                "ZZZ-ZZZ-ZZZ",
                Feeling::None,
                (39, 45),
                964_050_401,
                1_144,
                145,
            ),
        ] {
            let seed = DungeonSeed::from_code(code).unwrap();
            let floor = generate_prison_prefix(seed, 6).pop().unwrap();
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
    fn aaa_sequential_prison_searchable_items_match_official_oracle() {
        let floors = generate_prison_prefix(DungeonSeed::MIN, 9);
        let expected = [
            vec![
                (
                    ItemId::ScaleArmor,
                    1,
                    None,
                    false,
                    ItemSource::Chest,
                    Accessibility::Independent,
                ),
                (
                    ItemId::WandFrost,
                    1,
                    None,
                    false,
                    ItemSource::CrystalChest,
                    Accessibility::Choice {
                        group: 0,
                        option: 0,
                    },
                ),
                (
                    ItemId::RingTenacity,
                    1,
                    None,
                    true,
                    ItemSource::CrystalChest,
                    Accessibility::Choice {
                        group: 0,
                        option: 1,
                    },
                ),
                (
                    ItemId::ScaleArmor,
                    0,
                    None,
                    false,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
                (
                    ItemId::Sickle,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
                (
                    ItemId::HolyDart,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
                (
                    ItemId::Shuriken,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
                (
                    ItemId::LeatherArmor,
                    0,
                    None,
                    false,
                    ItemSource::Shop,
                    Accessibility::Independent,
                ),
            ],
            vec![
                (
                    ItemId::ScaleArmor,
                    1,
                    None,
                    false,
                    ItemSource::Chest,
                    Accessibility::Independent,
                ),
                (
                    ItemId::WandBlastWave,
                    1,
                    None,
                    false,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
                (
                    ItemId::ThrowingSpear,
                    1,
                    Some(Effect::Weapon(WeaponEffect::Polarized)),
                    true,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
                (
                    ItemId::Sickle,
                    0,
                    Some(Effect::Weapon(WeaponEffect::Blocking)),
                    false,
                    ItemSource::Statue,
                    Accessibility::Independent,
                ),
            ],
            vec![
                (
                    ItemId::ScaleArmor,
                    0,
                    None,
                    false,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
                (
                    ItemId::RingEnergy,
                    0,
                    None,
                    false,
                    ItemSource::Chest,
                    Accessibility::Choice {
                        group: 0,
                        option: 1,
                    },
                ),
            ],
            vec![
                (
                    ItemId::Sai,
                    0,
                    Some(Effect::Weapon(WeaponEffect::Sacrificial)),
                    true,
                    ItemSource::LockedChest,
                    Accessibility::Independent,
                ),
                (
                    ItemId::WandTransfusion,
                    1,
                    None,
                    false,
                    ItemSource::WandmakerReward,
                    Accessibility::Choice {
                        group: 0,
                        option: 0,
                    },
                ),
                (
                    ItemId::WandFrost,
                    1,
                    None,
                    false,
                    ItemSource::WandmakerReward,
                    Accessibility::Choice {
                        group: 0,
                        option: 1,
                    },
                ),
            ],
        ];

        for (floor, expected) in floors.iter().zip(expected) {
            assert_eq!(
                floor.world_items.len(),
                expected.len(),
                "depth {}",
                floor.painted.level.depth
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
    fn prison_world_prefix_reports_sewer_and_prison_items() {
        let world = generate_prison_world(DungeonSeed::MIN, 9).unwrap();
        assert_eq!(world.seed, DungeonSeed::MIN);
        assert!(!world.items.is_empty());
    }

    /// Seed `AAA-AAA-BUH`'s depth-6 golden mimic upgrades a `+0` cursed Ring
    /// of Force, and `Ring.upgrade()` always consumes its curse-clearing
    /// `Int(3)` even though the prize pass cleared the curse first. Skipping
    /// that draw shifted every later draw of the run: depth 7 lost its extra
    /// cursed-wand chest and the depth-9 Wandmaker offered the wrong wands.
    /// Expected values come from the official v3.3.8 oracle
    /// (`tooling/oracle/run.sh AAA-AAA-BUH 6-9`), cross-checked against the
    /// reference Java seed finder.
    #[test]
    fn golden_mimic_ring_upgrade_consumes_the_curse_reroll_draw() {
        let seed = DungeonSeed::from_code("AAA-AAA-BUH").unwrap();
        let floors = generate_prison_prefix(seed, 9);

        let golden: Vec<_> = floors[0]
            .world_items
            .iter()
            .filter(|item| item.source == ItemSource::GoldenMimic)
            .collect();
        assert_eq!(golden.len(), 2);
        assert_eq!(golden[0].item, ItemId::Sword);
        assert_eq!(golden[0].upgrade, 0);
        assert_eq!(golden[1].item, ItemId::RingForce);
        assert_eq!(golden[1].upgrade, 1);
        assert!(!golden[1].cursed);

        // The draw consumed by Ring.upgrade() is what keeps the rest of the
        // run aligned: depth 7 rolls a second chest holding a cursed wand.
        assert!(floors[1].world_items.iter().any(|item| {
            item.item == ItemId::WandFireblast
                && item.upgrade == 0
                && item.cursed
                && item.source == ItemSource::Chest
        }));
        assert!(floors[1].world_items.iter().any(|item| {
            item.item == ItemId::MailArmor && item.upgrade == 1 && item.source == ItemSource::Chest
        }));

        let rewards: Vec<_> = floors[3]
            .world_items
            .iter()
            .filter(|item| item.source == ItemSource::WandmakerReward)
            .collect();
        assert_eq!(rewards.len(), 2);
        assert_eq!(rewards[0].item, ItemId::WandCorrosion);
        assert_eq!(rewards[0].upgrade, 3);
        assert_eq!(rewards[1].item, ItemId::WandWarding);
        assert_eq!(rewards[1].upgrade, 1);
    }

    #[test]
    fn prison_four_lane_depth_roots_match_scalar_worlds() {
        let seeds = [
            DungeonSeed::MIN,
            DungeonSeed::new(1).unwrap(),
            DungeonSeed::from_code("ABC-DEF-GHI").unwrap(),
            DungeonSeed::MAX,
        ];
        let scalar = seeds.map(|seed| generate_prison_world(seed, 9).unwrap());
        let batched = CanonicalPrisonWorldGenerator.generate_batch(&seeds, 9);
        assert_eq!(batched, scalar);
    }
}
