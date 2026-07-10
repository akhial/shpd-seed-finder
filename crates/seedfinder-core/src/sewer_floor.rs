//! Parity-gated composition of one painted regular Sewer floor.
//!
//! The lower-level ports deliberately test room families in isolation.  This
//! module supplies the first integration boundary used by a real world
//! generator: one live `GeneratorState`, one insertion-ordered
//! `Level.itemsToSpawn` pool, the official graph, and the official painter all
//! advance under the same per-depth RNG stream.
//!
//! Secret and forced rooms are rejected until their dispatchers are wired in.
//! Likewise, a standard room selecting a non-`PaintItem` direct prize fails the
//! floor after painting.  Those explicit gates are preferable to returning a
//! plausible but non-canonical searchable world.

#![allow(clippy::missing_panics_doc)]

use std::cell::RefCell;
use std::fmt;

use crate::batch::seed_for_depth_batch4;
use crate::generator::{
    GeneratedItem, GeneratorError, SeedKind, StoneKind, random, random_armor, random_category,
    random_gold, random_missile, random_using_defaults, random_weapon,
};
use crate::geometry::{Point, Rect, painter as draw};
use crate::level::{DirectPaintItem, HeapKind, Level, PaintItem, PaintMob};
use crate::level_flags::LevelFlags;
use crate::level_prelude::LimitedDrops;
use crate::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
use crate::painter::{PaintError, RoomPaintDispatch, SewerPainter, draw_regular_trap_count};
use crate::quest_rooms::{QuestGeneratorContext, QuestGeneratorRequest, QuestPrizeContext};
use crate::quests::QuestState;
use crate::regular_items::{
    QueuedItemKind, RegularItem, RegularItemsError, RegularItemsResult, create_regular_items,
    level_create_queue,
};
use crate::regular_level::{BuiltRegularGraph, PreparedRegularFloor, build_sewer_room_graph};
use crate::regular_placement::{CanonicalRoomPlacementRules, PaintedRegularPlacement};
use crate::rng::{RandomStack, seed_for_depth};
use crate::room::{Room, RoomId, RoomKind, SpecialRoomKind, StandardRoomKind};
use crate::run::{GeneratorCategory, GeneratorState, PotionKind, RunState, ScrollKind};
use crate::search::WorldGenerator;
use crate::secret_rooms::{
    SecretGeneratorContext, SecretGeneratorRequest, SecretLevelPaintContext, SecretPaintEvent,
    SecretPaintOutcome, SecretPrizeContext, can_place_grass as secret_can_place_grass,
    can_place_trap as secret_can_place_trap, can_place_water as secret_can_place_water,
    paint_secret_room,
};
use crate::seed::DungeonSeed;
use crate::sewer_mob_placement::{
    RoomCharacterRules, SewerMobsError, SewerMobsResult, create_sewer_mobs,
};
use crate::sewer_rooms::{SewerRoomContent, SewerRoomDispatcher};
use crate::shop::ShopRunState;
use crate::special_consumable::{
    ConsumableLevelPaintContext, ConsumablePaintEvent, ConsumablePaintOutcome,
    ConsumablePrizeContext, ConsumablePrizeMatch, is_consumable_special, paint_consumable_special,
};
use crate::special_equipment::{
    LevelPaintContext, PrizePool, SpecialGeneratorContext, SpecialGeneratorRequest, SpecialItem,
    SpecialPaintEvent, SpecialPaintOutcome, SpecialPrizeContext, is_equipment_special,
    paint_equipment_special,
};
use crate::special_forced::{
    AlchemyGuidePage, ForcedLevelPaintContext, ForcedPaintEvent, ForcedPaintOutcome,
    ForcedPrizeContext, ForcedShopRoomState, is_forced_special, paint_forced_special,
};

/// Fully painted state immediately before `buildFlagMaps()`.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintedSewerFloor {
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
    pub world_items: Vec<WorldItem>,
    /// `StandardBridgeRoom.spaceRect` values needed by mob/item placement.
    pub bridge_spaces: Vec<(RoomId, Rect)>,
}

/// One complete Sewer `Level.create()` result after mobs and items.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedSewerFloor {
    pub painted: PaintedSewerFloor,
    pub flags: LevelFlags,
    pub mobs: SewerMobsResult,
    pub regular_items: RegularItemsResult,
    pub world_items: Vec<WorldItem>,
}

/// A parity boundary which has not yet been connected to the composite.
#[derive(Clone, Debug, PartialEq)]
pub enum SewerPaintError {
    Generator(GeneratorError),
    Painter(PaintError),
    UnsupportedRoom(RoomKind),
}

impl fmt::Display for SewerPaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Generator(error) => error.fmt(formatter),
            Self::Painter(error) => error.fmt(formatter),
            Self::UnsupportedRoom(kind) => {
                write!(
                    formatter,
                    "room painter is not parity-gated in the Sewer composite: {kind:?}"
                )
            }
        }
    }
}

impl std::error::Error for SewerPaintError {}

impl From<GeneratorError> for SewerPaintError {
    fn from(error: GeneratorError) -> Self {
        Self::Generator(error)
    }
}

impl From<PaintError> for SewerPaintError {
    fn from(error: PaintError) -> Self {
        Self::Painter(error)
    }
}

/// Failure from a later `Level.create()` phase.
#[derive(Clone, Debug, PartialEq)]
pub enum SewerFloorError {
    InvalidMaximumDepth(u8),
    Paint(SewerPaintError),
    Mobs(SewerMobsError),
    Items(RegularItemsError),
}

impl fmt::Display for SewerFloorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMaximumDepth(depth) => {
                write!(formatter, "Sewer world depth must be in 1..=4, got {depth}")
            }
            Self::Paint(error) => error.fmt(formatter),
            Self::Mobs(error) => error.fmt(formatter),
            Self::Items(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SewerFloorError {}

impl From<SewerPaintError> for SewerFloorError {
    fn from(error: SewerPaintError) -> Self {
        Self::Paint(error)
    }
}

impl From<SewerMobsError> for SewerFloorError {
    fn from(error: SewerMobsError) -> Self {
        Self::Mobs(error)
    }
}

impl From<RegularItemsError> for SewerFloorError {
    fn from(error: RegularItemsError) -> Self {
        Self::Items(error)
    }
}

/// Exact v3.3.8 generator for the sequential Sewer region (depths 1 through
/// 4). This is useful both as a parity boundary and as the first live slice of
/// the multicore search engine.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanonicalSewerWorldGenerator;

/// Generates one canonical world prefix ending on a Sewer depth.
///
/// # Errors
///
/// Returns [`SewerFloorError::InvalidMaximumDepth`] outside `1..=4`, or the
/// exact floor-phase error encountered while advancing the run.
pub fn generate_sewer_world(
    seed: DungeonSeed,
    maximum_depth: u8,
) -> Result<GeneratedWorld, SewerFloorError> {
    if !(1..=4).contains(&maximum_depth) {
        return Err(SewerFloorError::InvalidMaximumDepth(maximum_depth));
    }
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let roots = (1..=maximum_depth)
        .map(|depth| seed_for_depth(dungeon_seed, u32::from(depth), 0))
        .collect::<Vec<_>>();
    generate_sewer_world_with_roots(seed, &roots)
}

fn generate_sewer_world_with_roots(
    seed: DungeonSeed,
    roots: &[i64],
) -> Result<GeneratedWorld, SewerFloorError> {
    if roots.is_empty() || roots.len() > 4 {
        return Err(SewerFloorError::InvalidMaximumDepth(
            u8::try_from(roots.len()).unwrap_or(u8::MAX),
        ));
    }
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let mut run = RunState::new(dungeon_seed);
    let mut limited_drops = LimitedDrops::default();
    let mut quests = QuestState::new();
    let mut random = RandomStack::with_base_seed(0);
    let mut items = Vec::new();
    let mut next_choice_group = 0_u16;

    for (index, &root) in roots.iter().enumerate() {
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
    Ok(GeneratedWorld { seed, items })
}

pub(crate) fn remap_floor_choice_groups(items: &mut [WorldItem], offset: u16) -> u16 {
    let mut local_group_count = 0_u16;
    for item in items {
        let group = match &mut item.accessibility {
            Accessibility::Independent => continue,
            Accessibility::Choice { group, .. } | Accessibility::Scenarios { group, .. } => group,
        };
        local_group_count = local_group_count.max(group.saturating_add(1));
        *group = group
            .checked_add(offset)
            .expect("v3.3.8 world has fewer than 65536 reward choices");
    }
    offset
        .checked_add(local_group_count)
        .expect("v3.3.8 world has fewer than 65536 reward choices")
}

impl WorldGenerator for CanonicalSewerWorldGenerator {
    fn generate(&self, seed: DungeonSeed, max_depth: u8) -> GeneratedWorld {
        generate_sewer_world(seed, max_depth)
            .expect("CanonicalSewerWorldGenerator only accepts canonical Sewer depths")
    }

    fn generate_batch(&self, seeds: &[DungeonSeed], max_depth: u8) -> Vec<GeneratedWorld> {
        assert!(
            (1..=4).contains(&max_depth),
            "CanonicalSewerWorldGenerator only accepts depths 1..=4"
        );
        let mut output = Vec::with_capacity(seeds.len());
        let mut chunks = seeds.chunks_exact(4);
        for chunk in &mut chunks {
            let dungeon_seeds = std::array::from_fn(|index| {
                i64::try_from(chunk[index].value()).expect("base-26 seed range fits Java long")
            });
            let roots_by_depth = (1..=max_depth)
                .map(|depth| seed_for_depth_batch4(dungeon_seeds, u32::from(depth), 0))
                .collect::<Vec<_>>();
            for lane in 0..4 {
                let roots = roots_by_depth
                    .iter()
                    .map(|depth_roots| depth_roots[lane])
                    .collect::<Vec<_>>();
                output.push(
                    generate_sewer_world_with_roots(chunk[lane], &roots)
                        .expect("a validated Sewer batch satisfies canonical invariants"),
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

/// Completes a parity-gated Sewer floor through flags, Ghost/ordinary mobs,
/// and `RegularLevel.createItems()`. The caller owns the per-depth RNG push
/// and pop so several phase fixtures can inspect the same stream.
///
/// # Errors
///
/// Returns the paint gate/invariant errors plus typed mob and item generator
/// failures.
pub fn generate_sewer_floor(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    depth: u32,
    random: &mut RandomStack,
) -> Result<GeneratedSewerFloor, SewerFloorError> {
    let mut painted = paint_sewer_floor(run, limited_drops, depth, random)?;
    let mut flags = LevelFlags::build_for_generation(&painted.level.map);
    let spatial_rules = SewerSpatialRules {
        bridge_spaces: painted.bridge_spaces.clone(),
    };
    let entrance_cell = painted
        .level
        .entrance()
        .expect("a painted regular floor has an entrance transition");
    let exit_cell = painted
        .level
        .exit()
        .expect("a painted regular floor has an exit transition");
    let mobs = create_sewer_mobs(
        &mut painted.level,
        &mut flags,
        &painted.rooms,
        painted.entrance_room,
        painted.exit_room,
        entrance_cell,
        exit_cell,
        &spatial_rules,
        &mut quests.ghost,
        &mut run.generator,
        random,
    )?;

    let mut world_items = painted.world_items.clone();
    append_painted_room_items(&painted.level, depth, &mut world_items);
    let ghost_group = painted.remaining_prizes.next_choice_group;
    quests
        .ghost
        .append_world_items(ghost_group, &mut world_items);

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
            u8::try_from(depth).expect("Sewer depth fits u8"),
            painted.prepared.feeling,
            queue,
            &mut placement,
        )?
    };
    world_items.extend(regular_items.world_items.iter().cloned());

    Ok(GeneratedSewerFloor {
        painted,
        flags,
        mobs,
        regular_items,
        world_items,
    })
}

/// Runs `Level.create()` from its queue/feeling prefix through
/// `RegularPainter.paint()` for a Sewer depth. `random` must already contain
/// the active `Dungeon.seedCurDepth()` generator and remains active on return.
///
/// Run-global decks and limited-drop counters are updated exactly as Java
/// updates them, including when the selected graph is rejected by a later
/// parity gate.
///
/// # Errors
///
/// Returns a typed error for a generator invariant, structural painter error,
/// or a room/item interaction not yet admitted to the composite.
pub fn paint_sewer_floor(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    depth: u32,
    random: &mut RandomStack,
) -> Result<PaintedSewerFloor, SewerPaintError> {
    let prepared = crate::regular_level::prepare_regular_floor(run, limited_drops, depth, random)?;
    let built = build_sewer_room_graph(run, limited_drops, depth, prepared.feeling, random);
    reject_unwired_rooms(&built)?;

    // `SewerLevel.painter()` evaluates nTraps() after graph construction and
    // immediately before the generic painter starts.
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
        depth: i32::try_from(depth).expect("Sewer depth fits Java int"),
        generator: shared_generator,
        prizes: shared_prizes,
    };
    let regular = SewerRoomDispatcher::new(content);
    let forced_shop_rooms = (0..built.rooms.len())
        .map(|_| ForcedShopRoomState::default())
        .collect();
    let mut dispatch = SewerCompositeDispatcher {
        regular,
        generator: shared_generator,
        prizes: shared_prizes,
        equipment_events: Vec::new(),
        consumable_events: Vec::new(),
        forced_events: Vec::new(),
        secret_events: Vec::new(),
        forced_shop_rooms,
        shop_run: ShopRunState::default(),
        world_items: Vec::new(),
    };

    let mut rooms = built.rooms;
    let mut level = Level::new(depth, prepared.feeling);
    SewerPainter::new(depth, prepared.feeling, trap_count).paint(
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
    let world_items = std::mem::take(&mut dispatch.world_items);
    drop(dispatch);

    // No wrapper retains a borrow after the composite is dropped.
    run.generator = generator.into_inner();
    let remaining_prizes = prizes.into_inner();

    Ok(PaintedSewerFloor {
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
        world_items,
        bridge_spaces,
    })
}

fn reject_unwired_rooms(graph: &BuiltRegularGraph) -> Result<(), SewerPaintError> {
    for room in &graph.rooms {
        match room.kind {
            RoomKind::Entrance(_)
            | RoomKind::Exit(_)
            | RoomKind::Standard(_)
            | RoomKind::Connection(_)
            | RoomKind::Secret(_) => {}
            RoomKind::Special(kind)
                if is_equipment_special(kind)
                    || is_consumable_special(kind)
                    || is_forced_special(kind) => {}
            kind => return Err(SewerPaintError::UnsupportedRoom(kind)),
        }
    }
    Ok(())
}

pub(crate) fn regular_to_special(item: RegularItem) -> SpecialItem {
    match item {
        RegularItem::Generated(item) => SpecialItem::generated(item),
        RegularItem::Queued(QueuedItemKind::PotionOfStrength) => {
            SpecialItem::generated(GeneratedItem::Potion {
                kind: PotionKind::Strength,
                exotic: false,
            })
        }
        RegularItem::Queued(QueuedItemKind::ScrollOfUpgrade) => {
            SpecialItem::generated(GeneratedItem::Scroll {
                kind: ScrollKind::Upgrade,
                exotic: false,
            })
        }
        RegularItem::Queued(QueuedItemKind::Stylus) => SpecialItem::Paint(PaintItem::ArcaneStylus),
        RegularItem::Queued(QueuedItemKind::StoneOfEnchantment) => {
            SpecialItem::generated(GeneratedItem::Stone(StoneKind::Enchantment))
        }
        RegularItem::Queued(QueuedItemKind::StoneOfIntuition) => {
            SpecialItem::generated(GeneratedItem::Stone(StoneKind::Intuition))
        }
        RegularItem::Queued(QueuedItemKind::TrinketCatalyst) => {
            SpecialItem::Paint(PaintItem::TrinketCatalyst)
        }
        RegularItem::Queued(QueuedItemKind::IronKey { depth }) => SpecialItem::IronKey {
            depth: i32::from(depth),
        },
        RegularItem::Queued(QueuedItemKind::CrystalKey { depth }) => SpecialItem::CrystalKey {
            depth: i32::from(depth),
        },
        RegularItem::Queued(QueuedItemKind::PotionOfInvisibility) => {
            SpecialItem::PotionOfInvisibility
        }
        RegularItem::Queued(QueuedItemKind::PotionOfHaste) => SpecialItem::PotionOfHaste,
        RegularItem::Queued(QueuedItemKind::PotionOfLevitation) => {
            SpecialItem::generated(GeneratedItem::Potion {
                kind: PotionKind::Levitation,
                exotic: false,
            })
        }
        RegularItem::Queued(QueuedItemKind::PotionOfLiquidFlame) => {
            SpecialItem::generated(GeneratedItem::Potion {
                kind: PotionKind::LiquidFlame,
                exotic: false,
            })
        }
        RegularItem::Queued(QueuedItemKind::PotionOfPurity) => {
            SpecialItem::generated(GeneratedItem::Potion {
                kind: PotionKind::Purity,
                exotic: false,
            })
        }
        RegularItem::Queued(QueuedItemKind::PotionOfFrost) => {
            SpecialItem::generated(GeneratedItem::Potion {
                kind: PotionKind::Frost,
                exotic: false,
            })
        }
        RegularItem::Queued(QueuedItemKind::GoldenKey { depth }) => {
            SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::GoldenKey {
                depth: i32::from(depth),
            }))
        }
        RegularItem::Queued(QueuedItemKind::CeremonialCandle) => {
            SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::CeremonialCandle))
        }
        RegularItem::Queued(QueuedItemKind::Torch) => {
            SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::Torch))
        }
        RegularItem::Queued(QueuedItemKind::Other(name)) => {
            SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::Other(name)))
        }
    }
}

pub(crate) const fn special_to_paint_item(item: SpecialItem) -> PaintItem {
    match item {
        SpecialItem::Paint(item) => item,
        SpecialItem::IronKey { depth } => PaintItem::Direct(DirectPaintItem::IronKey { depth }),
        SpecialItem::CrystalKey { depth } => {
            PaintItem::Direct(DirectPaintItem::CrystalKey { depth })
        }
        SpecialItem::PotionOfInvisibility => {
            PaintItem::Direct(DirectPaintItem::PotionOfInvisibility)
        }
        SpecialItem::PotionOfHaste => PaintItem::Direct(DirectPaintItem::PotionOfHaste),
    }
}

pub(crate) fn special_to_regular(item: SpecialItem) -> RegularItem {
    match item {
        SpecialItem::Paint(PaintItem::Generated(item)) => RegularItem::Generated(item),
        SpecialItem::Paint(PaintItem::ArcaneStylus) => RegularItem::Queued(QueuedItemKind::Stylus),
        SpecialItem::Paint(PaintItem::TrinketCatalyst) => {
            RegularItem::Queued(QueuedItemKind::TrinketCatalyst)
        }
        SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::IronKey { depth })) => {
            RegularItem::Queued(QueuedItemKind::IronKey {
                depth: u8::try_from(depth).expect("main-dungeon depth fits u8"),
            })
        }
        SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::CrystalKey { depth })) => {
            RegularItem::Queued(QueuedItemKind::CrystalKey {
                depth: u8::try_from(depth).expect("main-dungeon depth fits u8"),
            })
        }
        SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::GoldenKey { depth })) => {
            RegularItem::Queued(QueuedItemKind::GoldenKey {
                depth: u8::try_from(depth).expect("main-dungeon depth fits u8"),
            })
        }
        SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::PotionOfInvisibility))
        | SpecialItem::PotionOfInvisibility => {
            RegularItem::Queued(QueuedItemKind::PotionOfInvisibility)
        }
        SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::PotionOfHaste))
        | SpecialItem::PotionOfHaste => RegularItem::Queued(QueuedItemKind::PotionOfHaste),
        SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::CeremonialCandle)) => {
            RegularItem::Queued(QueuedItemKind::CeremonialCandle)
        }
        SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::Torch)) => {
            RegularItem::Queued(QueuedItemKind::Torch)
        }
        SpecialItem::Paint(PaintItem::Direct(DirectPaintItem::Other(name))) => {
            RegularItem::Queued(QueuedItemKind::Other(name))
        }
        SpecialItem::IronKey { depth } => RegularItem::Queued(QueuedItemKind::IronKey {
            depth: u8::try_from(depth).expect("main-dungeon depth fits u8"),
        }),
        SpecialItem::CrystalKey { depth } => RegularItem::Queued(QueuedItemKind::CrystalKey {
            depth: u8::try_from(depth).expect("main-dungeon depth fits u8"),
        }),
    }
}

pub(crate) fn append_painted_room_items(level: &Level, depth: u32, output: &mut Vec<WorldItem>) {
    let depth = u8::try_from(depth).expect("main-dungeon depth fits u8");
    for heap in &level.heaps {
        let source = match heap.kind {
            HeapKind::Heap => ItemSource::Heap,
            HeapKind::Chest => ItemSource::Chest,
            HeapKind::Tomb => ItemSource::Tomb,
        };
        for item in &heap.items {
            append_paint_world_item(*item, depth, source, output);
        }
    }
    for mob in &level.mobs {
        if let PaintMob::Mimic { items, .. } = mob {
            for item in items {
                append_paint_world_item(*item, depth, ItemSource::Mimic, output);
            }
        }
    }
}

fn append_paint_world_item(
    item: PaintItem,
    depth: u8,
    source: ItemSource,
    output: &mut Vec<WorldItem>,
) {
    let PaintItem::Generated(generated) = item else {
        return;
    };
    let equipment = generated.searchable_equipment();
    if let Some(equipment) = equipment {
        output.push(WorldItem::from_equipment_roll(
            equipment.item,
            equipment.roll,
            depth,
            source,
            Accessibility::Independent,
        ));
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SewerSpatialRules {
    bridge_spaces: Vec<(RoomId, Rect)>,
}

impl RoomCharacterRules for SewerSpatialRules {
    fn can_place_character(
        &self,
        level: &Level,
        rooms: &[Room],
        room_id: RoomId,
        point: Point,
    ) -> bool {
        let room = &rooms[room_id];
        if !room.inside(point) {
            return false;
        }
        let cell = level.point_to_cell(point);
        match room.kind {
            RoomKind::Entrance(StandardRoomKind::WaterBridge)
            | RoomKind::Exit(StandardRoomKind::WaterBridge)
            | RoomKind::Standard(StandardRoomKind::WaterBridge) => {
                if self
                    .bridge_spaces
                    .iter()
                    .find_map(|(id, rect)| (*id == room_id).then_some(*rect))
                    .is_some_and(|rect| rect.inside(point))
                {
                    return false;
                }
            }
            RoomKind::Standard(StandardRoomKind::Plants)
                if level.plants.iter().any(|plant| plant.cell == cell) =>
            {
                return false;
            }
            RoomKind::Standard(StandardRoomKind::Aquarium)
                if level.map.cells[cell] == crate::geometry::terrain::WATER =>
            {
                return false;
            }
            _ => {}
        }
        !room.is_exit() || level.exit() != Some(cell)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SharedGenerator<'a>(pub(crate) &'a RefCell<GeneratorState>);

impl SpecialGeneratorContext for SharedGenerator<'_> {
    fn generate(
        &mut self,
        request: SpecialGeneratorRequest,
        depth: i32,
        random: &mut RandomStack,
    ) -> Result<GeneratedItem, GeneratorError> {
        SpecialGeneratorContext::generate(&mut *self.0.borrow_mut(), request, depth, random)
    }
}

impl SecretGeneratorContext for SharedGenerator<'_> {
    fn generate_secret(
        &mut self,
        request: SecretGeneratorRequest,
        depth: i32,
        random: &mut RandomStack,
    ) -> Result<GeneratedItem, GeneratorError> {
        SecretGeneratorContext::generate_secret(&mut *self.0.borrow_mut(), request, depth, random)
    }
}

impl QuestGeneratorContext for SharedGenerator<'_> {
    fn generate(
        &mut self,
        request: QuestGeneratorRequest,
        depth: i32,
        random: &mut RandomStack,
    ) -> Result<GeneratedItem, GeneratorError> {
        QuestGeneratorContext::generate(&mut *self.0.borrow_mut(), request, depth, random)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SharedPrizes<'a>(pub(crate) &'a RefCell<PrizePool>);

impl SpecialPrizeContext for SharedPrizes<'_> {
    fn take_prize_item(&mut self, random: &mut RandomStack) -> Option<SpecialItem> {
        SpecialPrizeContext::take_prize_item(&mut *self.0.borrow_mut(), random)
    }

    fn take_trinket_catalyst(&mut self) -> Option<SpecialItem> {
        SpecialPrizeContext::take_trinket_catalyst(&mut *self.0.borrow_mut())
    }

    fn add_item_to_spawn(&mut self, item: SpecialItem) {
        SpecialPrizeContext::add_item_to_spawn(&mut *self.0.borrow_mut(), item);
    }

    fn is_item_blocked(&self, item: &SpecialItem) -> bool {
        SpecialPrizeContext::is_item_blocked(&*self.0.borrow(), item)
    }

    fn rat_skull_level(&self) -> i32 {
        SpecialPrizeContext::rat_skull_level(&*self.0.borrow())
    }

    fn mimic_tooth_level(&self) -> i32 {
        SpecialPrizeContext::mimic_tooth_level(&*self.0.borrow())
    }

    fn allocate_choice_group(&mut self) -> u16 {
        SpecialPrizeContext::allocate_choice_group(&mut *self.0.borrow_mut())
    }
}

impl ConsumablePrizeContext for SharedPrizes<'_> {
    fn take_matching(&mut self, wanted: ConsumablePrizeMatch) -> Option<SpecialItem> {
        ConsumablePrizeContext::take_matching(&mut *self.0.borrow_mut(), wanted)
    }

    fn exotic_crystals_level(&self) -> i32 {
        ConsumablePrizeContext::exotic_crystals_level(&*self.0.borrow())
    }
}

impl SecretPrizeContext for SharedPrizes<'_> {
    fn exotic_crystals_level(&self) -> i32 {
        -1
    }

    fn trap_mechanism_level(&self) -> i32 {
        -1
    }
}

impl ForcedPrizeContext for SharedPrizes<'_> {
    fn take_trinket_catalyst(&mut self) -> Option<RegularItem> {
        let mut pool = self.0.borrow_mut();
        let index = pool
            .items_to_spawn
            .iter()
            .position(|item| *item == SpecialItem::Paint(PaintItem::TrinketCatalyst))?;
        Some(special_to_regular(pool.items_to_spawn.remove(index)))
    }

    fn take_strength_potion(&mut self) -> Option<RegularItem> {
        let mut pool = self.0.borrow_mut();
        let index = pool.items_to_spawn.iter().position(|item| {
            matches!(
                item,
                SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Potion {
                    kind: PotionKind::Strength,
                    exotic: false,
                }))
            )
        })?;
        Some(special_to_regular(pool.items_to_spawn.remove(index)))
    }

    fn add_item_to_spawn(&mut self, item: RegularItem) {
        let item = regular_to_special(item);
        self.0.borrow_mut().items_to_spawn.push(item);
    }

    fn is_item_blocked(&self, _item: RegularItem) -> bool {
        false
    }

    fn is_alchemy_page_found(&self, _page: AlchemyGuidePage) -> bool {
        true
    }
}

impl QuestPrizeContext for SharedPrizes<'_> {
    fn add_item_to_spawn(&mut self, item: RegularItem) {
        self.0
            .borrow_mut()
            .items_to_spawn
            .push(regular_to_special(item));
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SharedSewerContent<'a> {
    pub(crate) depth: i32,
    pub(crate) generator: SharedGenerator<'a>,
    pub(crate) prizes: SharedPrizes<'a>,
}

impl SewerRoomContent for SharedSewerContent<'_> {
    fn find_prize_item(&mut self, random: &mut RandomStack) -> Option<PaintItem> {
        self.prizes
            .take_prize_item(random)
            .map(special_to_paint_item)
    }

    fn random_seed_using_defaults(&mut self, random_stack: &mut RandomStack) -> SeedKind {
        match random_using_defaults(
            random_stack,
            &mut self.generator.0.borrow_mut(),
            GeneratorCategory::Seed,
            self.depth,
        )
        .expect("canonical Seed category state is valid")
        {
            GeneratedItem::Seed(seed) => seed,
            _ => unreachable!("Seed category always creates a seed"),
        }
    }

    fn random_item(&mut self, random_stack: &mut RandomStack) -> PaintItem {
        random(random_stack, &mut self.generator.0.borrow_mut(), self.depth)
            .expect("canonical Generator state is valid")
            .into()
    }

    fn random_category(
        &mut self,
        category: GeneratorCategory,
        random_stack: &mut RandomStack,
    ) -> PaintItem {
        random_category(
            random_stack,
            &mut self.generator.0.borrow_mut(),
            category,
            self.depth,
        )
        .expect("canonical Generator category state is valid")
        .into()
    }

    fn random_mimic_reward(&mut self, random_stack: &mut RandomStack) -> PaintItem {
        let floor_set = self.depth / 5;
        let reward = match random_stack.int_bound(5) {
            0 => random_gold(random_stack, self.depth),
            1 => GeneratedItem::Missile(
                random_missile(
                    random_stack,
                    &mut self.generator.0.borrow_mut(),
                    floor_set,
                    false,
                )
                .expect("canonical missile deck is valid"),
            ),
            2 => GeneratedItem::Equipment(
                random_armor(random_stack, floor_set).expect("canonical armor table is valid"),
            ),
            3 => GeneratedItem::Equipment(
                random_weapon(
                    random_stack,
                    &mut self.generator.0.borrow_mut(),
                    floor_set,
                    false,
                )
                .expect("canonical weapon deck is valid"),
            ),
            _ => random_category(
                random_stack,
                &mut self.generator.0.borrow_mut(),
                GeneratorCategory::Ring,
                self.depth,
            )
            .expect("canonical ring deck is valid"),
        };
        reward.into()
    }
}

struct SewerCompositeDispatcher<'a> {
    regular: SewerRoomDispatcher<SharedSewerContent<'a>>,
    generator: SharedGenerator<'a>,
    prizes: SharedPrizes<'a>,
    equipment_events: Vec<SpecialPaintEvent>,
    consumable_events: Vec<ConsumablePaintEvent>,
    forced_events: Vec<ForcedPaintEvent>,
    secret_events: Vec<SecretPaintEvent>,
    forced_shop_rooms: Vec<ForcedShopRoomState>,
    shop_run: ShopRunState,
    world_items: Vec<WorldItem>,
}

impl RoomPaintDispatch for SewerCompositeDispatcher<'_> {
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
                    .expect("a builder-produced secret room has a valid shared entrance")
                };
                let SecretPaintOutcome::Painted(report) = outcome else {
                    unreachable!("a secret room is handled by the secret dispatcher")
                };
                self.world_items.extend(report.searchable_items);
                return;
            }
            RoomKind::Special(_) => {}
            RoomKind::Quest(_) => unreachable!("quest rooms are rejected before Sewer painting"),
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
            .expect("a builder-produced special room has a valid shared entrance")
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
            .expect("a builder-produced special room has a valid shared entrance")
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
                &mut self.forced_shop_rooms[room],
                &mut self.shop_run,
                random,
            )
            .expect("a builder-produced forced room has a valid shared entrance")
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
        if matches!(rooms[room].kind, RoomKind::Special(_)) {
            false
        } else {
            self.regular
                .can_merge(level, rooms, room, other, point, merge_terrain)
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
        if matches!(rooms[room].kind, RoomKind::Special(_)) {
            draw::fill_rect(&mut level.map, merge, merge_terrain);
        } else {
            self.regular
                .merge(level, rooms, room, other, merge, merge_terrain);
        }
    }

    fn can_place_water(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        match rooms[room].kind {
            kind @ RoomKind::Secret(_) => secret_can_place_water(kind),
            RoomKind::Special(SpecialRoomKind::CrystalPath) => false,
            RoomKind::Special(_) => true,
            _ => self.regular.can_place_water(level, rooms, room, point),
        }
    }

    fn can_place_grass(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        match rooms[room].kind {
            kind @ RoomKind::Secret(_) => secret_can_place_grass(kind),
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
            RoomKind::Special(
                SpecialRoomKind::ToxicGas | SpecialRoomKind::CrystalPath | SpecialRoomKind::Pit,
            ) => false,
            RoomKind::Special(_) => true,
            _ => self.regular.can_place_trap(level, rooms, room, point),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::Arc;

    use crate::catalog::ItemId;
    use crate::level_prelude::Feeling;
    use crate::mobs::SewerMobKind;
    use crate::query::{Requirement, SearchQuery};
    use crate::quests::QuestState;
    use crate::rng::{RandomStack, seed_for_depth};
    use crate::search::{SearchOptions, spawn_streaming_search};
    use crate::seed::DungeonSeed;

    use super::*;

    #[test]
    fn aaa_floor_one_full_painted_map_matches_official_v338_oracle() {
        let mut run = RunState::new(0);
        let mut limited = LimitedDrops::default();
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed_for_depth(0, 1, 0));

        let floor = paint_sewer_floor(&mut run, &mut limited, 1, &mut random).unwrap();

        assert_eq!(floor.prepared.feeling, Feeling::None);
        assert_eq!((floor.level.width(), floor.level.height()), (40, 30));
        assert_eq!(floor.level.java_map_hash(), -188_128_262);
        assert_eq!(floor.level.entrance(), Some(287));
        assert_eq!(floor.level.exit(), Some(596));
        assert_eq!(floor.rooms.len(), 12);
        let special_mobs = floor
            .equipment_events
            .iter()
            .filter(|event| matches!(event, SpecialPaintEvent::Mob { .. }))
            .count()
            + floor
                .consumable_events
                .iter()
                .filter(|event| matches!(event, ConsumablePaintEvent::Mimic { .. }))
                .count();
        let special_heaps = floor
            .equipment_events
            .iter()
            .filter(|event| matches!(event, SpecialPaintEvent::Drop { .. }))
            .count()
            + floor
                .consumable_events
                .iter()
                .filter(|event| matches!(event, ConsumablePaintEvent::Drop { .. }))
                .count();
        assert_eq!(special_mobs, 3, "PoolRoom paints three piranhas");
        assert_eq!(special_heaps, 3, "Pool and Runestone paint three heaps");
    }

    #[test]
    fn aaa_floor_one_mobs_and_searchable_items_match_official_v338_oracle() {
        let mut run = RunState::new(0);
        let mut limited = LimitedDrops::default();
        let mut quests = QuestState::new();
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed_for_depth(0, 1, 0));

        let floor =
            generate_sewer_floor(&mut run, &mut limited, &mut quests, 1, &mut random).unwrap();

        let mut mobs = floor
            .mobs
            .mobs
            .iter()
            .map(|mob| (mob.mob.kind, mob.cell))
            .collect::<Vec<_>>();
        mobs.sort_unstable_by_key(|(_, cell)| *cell);
        assert_eq!(
            mobs,
            [
                (SewerMobKind::Rat, 175),
                (SewerMobKind::Snake, 404),
                (SewerMobKind::Rat, 497),
                (SewerMobKind::Rat, 524),
                (SewerMobKind::Rat, 738),
                (SewerMobKind::Rat, 752),
                (SewerMobKind::Snake, 778),
                (SewerMobKind::Rat, 902),
            ]
        );
        assert_eq!(floor.regular_items.placements.len(), 8);
        assert_eq!(floor.world_items.len(), 1);
        assert_eq!(floor.world_items[0].item, ItemId::ScaleArmor);
        assert_eq!(floor.world_items[0].upgrade, 0);
        assert_eq!(floor.painted.level.java_map_hash(), -188_128_262);
    }

    #[test]
    fn three_more_floor_one_maps_match_official_v338_oracle() {
        for (code, expected_size, expected_hash) in [
            ("AAA-AAA-AAB", (37, 38), 714_538_507),
            ("ABC-DEF-GHI", (38, 28), -536_401_596),
            ("ZZZ-ZZZ-ZZZ", (32, 42), 963_133_431),
        ] {
            let seed = DungeonSeed::from_code(code).unwrap();
            let seed = i64::try_from(seed.value()).unwrap();
            let mut run = RunState::new(seed);
            let mut limited = LimitedDrops::default();
            let mut quests = QuestState::new();
            let mut random = RandomStack::with_base_seed(0);
            random.push(seed_for_depth(seed, 1, 0));

            let floor = generate_sewer_floor(&mut run, &mut limited, &mut quests, 1, &mut random)
                .unwrap_or_else(|error| panic!("{code}: {error}"));
            assert_eq!(
                (floor.painted.level.width(), floor.painted.level.height()),
                expected_size,
                "{code}"
            );
            assert_eq!(floor.painted.level.java_map_hash(), expected_hash, "{code}");

            let mut expected_items = match code {
                "AAA-AAA-AAB" => vec![(ItemId::Sword, 0), (ItemId::Shuriken, 0)],
                "ABC-DEF-GHI" => Vec::new(),
                "ZZZ-ZZZ-ZZZ" => {
                    vec![(ItemId::ThrowingClub, 0), (ItemId::ScaleArmor, 0)]
                }
                _ => unreachable!(),
            };
            let mut actual_items = floor
                .world_items
                .iter()
                .map(|item| (item.item, item.upgrade))
                .collect::<Vec<_>>();
            actual_items.sort_unstable();
            expected_items.sort_unstable();
            assert_eq!(actual_items, expected_items, "{code}");

            let expected_mob_cells: &[usize] = match code {
                "AAA-AAA-AAB" => &[103, 139, 550, 751, 789, 862, 914, 942],
                "ABC-DEF-GHI" => &[132, 473, 538, 585, 589, 665, 717, 857],
                "ZZZ-ZZZ-ZZZ" => &[184, 188, 237, 331, 484, 534, 597, 775],
                _ => unreachable!(),
            };
            let mut actual_mob_cells = floor
                .mobs
                .mobs
                .iter()
                .map(|mob| mob.cell)
                .collect::<Vec<_>>();
            actual_mob_cells.sort_unstable();
            assert_eq!(actual_mob_cells, expected_mob_cells, "{code}");
        }
    }

    #[test]
    fn aaa_sequential_sewer_maps_match_official_v338_oracle() {
        let mut run = RunState::new(0);
        let mut limited = LimitedDrops::default();
        let mut quests = QuestState::new();
        let mut random = RandomStack::with_base_seed(0);
        for (depth, expected_size, expected_hash, expected_mobs) in [
            (1, (40, 30), -188_128_262, 11),
            (2, (37, 48), -1_411_436_327, 7),
            (3, (36, 43), -1_000_158_352, 6),
            (4, (47, 40), 177_881_339, 9),
        ] {
            random.push(seed_for_depth(0, depth, 0));
            let floor =
                generate_sewer_floor(&mut run, &mut limited, &mut quests, depth, &mut random)
                    .unwrap_or_else(|error| panic!("depth {depth}: {error}"));
            random.pop();
            assert_eq!(
                (floor.painted.level.width(), floor.painted.level.height()),
                expected_size,
                "depth {depth}"
            );
            assert_eq!(
                floor.painted.level.java_map_hash(),
                expected_hash,
                "depth {depth}"
            );
            let painted_mobs = floor
                .painted
                .equipment_events
                .iter()
                .filter(|event| matches!(event, SpecialPaintEvent::Mob { .. }))
                .count()
                + floor
                    .painted
                    .consumable_events
                    .iter()
                    .filter(|event| matches!(event, ConsumablePaintEvent::Mimic { .. }))
                    .count()
                + floor
                    .painted
                    .secret_events
                    .iter()
                    .filter(|event| matches!(event, SecretPaintEvent::Mob { .. }))
                    .count()
                + floor
                    .painted
                    .forced_events
                    .iter()
                    .filter(|event| matches!(event, ForcedPaintEvent::Mob { .. }))
                    .count();
            let total_mobs =
                floor.mobs.mobs.len() + usize::from(floor.mobs.ghost_cell.is_some()) + painted_mobs;
            assert_eq!(total_mobs, expected_mobs, "depth {depth}");

            let mut actual_items = floor
                .world_items
                .iter()
                .map(|item| (item.item, item.upgrade))
                .collect::<Vec<_>>();
            actual_items.sort_unstable();
            let mut expected_items = match depth {
                1 => vec![(ItemId::ScaleArmor, 0)],
                2 => vec![(ItemId::LeatherArmor, 1), (ItemId::MailArmor, 2)],
                3 => vec![
                    (ItemId::ScaleArmor, 0),
                    (ItemId::Spear, 1),
                    (ItemId::RingTenacity, 0),
                ],
                4 => vec![
                    (ItemId::Quarterstaff, 1),
                    (ItemId::LeatherArmor, 1),
                    (ItemId::HandAxe, 1),
                    (ItemId::Tomahawk, 1),
                ],
                _ => unreachable!(),
            };
            expected_items.sort_unstable();
            assert_eq!(actual_items, expected_items, "depth {depth}");
        }
    }

    #[test]
    fn canonical_sewer_world_and_four_lane_batch_are_identical() {
        let seeds = [
            DungeonSeed::MIN,
            DungeonSeed::new(1).unwrap(),
            DungeonSeed::from_code("ABC-DEF-GHI").unwrap(),
            DungeonSeed::MAX,
        ];
        let generator = CanonicalSewerWorldGenerator;
        let scalar = seeds.map(|seed| generate_sewer_world(seed, 4).unwrap());
        let batched = generator.generate_batch(&seeds, 4);

        assert_eq!(batched, scalar);
        assert_eq!(scalar[0].items.len(), 10);
        assert_eq!(scalar[0].seed, DungeonSeed::MIN);
    }

    #[test]
    fn live_streaming_search_finds_a_real_multi_floor_seed() {
        let query = SearchQuery {
            requirements: vec![Requirement {
                kind: crate::catalog::ItemKind::Armor,
                item: Some(ItemId::MailArmor),
                upgrade: crate::query::UpgradeRequirement::Exact(2),
                effect: None,
                source: None,
                identity_group: None,
            }],
            max_depth: 4,
            require_blacksmith: false,
            fast_mode: false,
        };
        let options = SearchOptions {
            start_seed: 0,
            end_seed_exclusive: 16,
            workers: NonZeroUsize::MIN,
            chunk_size: NonZeroUsize::new(4).unwrap(),
            max_results: NonZeroUsize::MIN,
        };
        let generator = Arc::new(CanonicalSewerWorldGenerator);
        let handle = spawn_streaming_search(&generator, query, options).unwrap();
        while !handle.is_finished() {
            std::thread::yield_now();
        }
        let results = handle.drain_results(1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].seed, DungeonSeed::MIN);
    }
}
