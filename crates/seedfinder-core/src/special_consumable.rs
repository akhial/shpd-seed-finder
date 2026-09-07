//! Exact v3.3.8 painters for the ten consumable-queue `SpecialRoom` classes.
//!
//! The module shares [`crate::special_equipment::SpecialItem`], generator, and
//! prize-pool abstractions with the equipment-special painter.  Terrain and
//! consumable-only side effects use a separate typed context because `Level`
//! intentionally does not own blobs, wells, custom traps, or Blandfruit.
//!
//! `MagicalFireRoom` and `CrystalPathRoom` construct temporary `EmptyRoom`
//! objects while painting.  Their constructor-time size-category floats are
//! observable and are preserved here.  `CrystalPath` accessibility is expressed
//! as exact feasible three-key scenario masks, together with its physical slot
//! prerequisites in the event stream.

#![allow(
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

use std::cmp::Reverse;
use std::fmt;

use crate::catalog::Effect;
use crate::generator::{self, GeneratedItem, GeneratorError};
use crate::geometry::{GridMap, Point, Rect, painter as draw, terrain};
use crate::java_math::div_i32;
use crate::level::{Level, PaintItem};
use crate::model::{Accessibility, ItemSource, WorldItem};
use crate::painter::set_shared_door_type;
use crate::rng::RandomStack;
use crate::room::{DoorType, Room, RoomId, RoomKind, SpecialRoomKind};
use crate::run::{GeneratorCategory, PotionKind, ScrollKind};
use crate::special_equipment::{
    PrizePool, SpecialGeneratorContext, SpecialGeneratorRequest, SpecialItem, SpecialPrizeContext,
};

/// A directly painted reward not representable by `Generator`/`PaintItem`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsumableItem {
    Special(SpecialItem),
    Honeypot,
}

impl From<SpecialItem> for ConsumableItem {
    fn from(item: SpecialItem) -> Self {
        Self::Special(item)
    }
}

/// Heap identities used by the ten consumable-special painters.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConsumableHeapKind {
    Heap,
    Chest,
    Skeleton,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConsumablePlantKind {
    Sungrass,
    Blandfruit,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WellWaterKind {
    Awareness,
    Health,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConsumableBlobKind {
    Foliage,
    WellWater(WellWaterKind),
    ToxicGas,
    ToxicGasSeed,
    EternalFire,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConsumableTrapKind {
    ToxicVent,
    Gripping,
    Teleportation,
    Flock,
    PoisonDart,
    Explosive,
    Flashing,
    Warping,
    Disintegration,
    Grim,
}

/// Physical reward chambers in `CrystalPathRoom`, ordered exactly as Java's
/// temporary `EmptyRoom[6]` array.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum CrystalPathSlot {
    FirstA,
    FirstB,
    SecondA,
    SecondB,
    FinalA,
    FinalB,
}

impl CrystalPathSlot {
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn prerequisite(self) -> Option<Self> {
        match self {
            Self::FinalA => Some(Self::SecondA),
            Self::FinalB => Some(Self::SecondB),
            Self::FirstA | Self::FirstB | Self::SecondA | Self::SecondB => None,
        }
    }

    #[must_use]
    pub const fn key_cost(self) -> u8 {
        1
    }
}

/// Every maximal set of rewards obtainable with the room's three Crystal
/// Keys. Bits are [`CrystalPathSlot`] discriminants. Any obtainable subset is
/// contained in at least one of these plans.
pub const CRYSTAL_PATH_PLANS: [u8; 10] = [
    0b00_0111, 0b00_1011, 0b00_1101, 0b00_1110, 0b01_0101, 0b01_0110, 0b01_1100, 0b10_1001,
    0b10_1010, 0b10_1100,
];

#[must_use]
pub const fn crystal_path_scenario_mask(slot: CrystalPathSlot) -> u64 {
    let slot_bit = 1_u8 << slot as u8;
    let mut result = 0_u64;
    let mut plan = 0_usize;
    while plan < CRYSTAL_PATH_PLANS.len() {
        if CRYSTAL_PATH_PLANS[plan] & slot_bit != 0 {
            result |= 1_u64 << plan;
        }
        plan += 1;
    }
    result
}

/// Exact access information attached to a painted reward.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConsumableAccess {
    Independent,
    CrystalPath(CrystalPathSlot),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsumableReward {
    pub item: ConsumableItem,
    pub access: ConsumableAccess,
}

/// Ordered Java call trace for generation-visible consumable-room effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConsumablePaintEvent {
    /// `heap = Some` mirrors `level.drop(...).type = ...`; `None` retains an
    /// existing heap type or creates an ordinary heap.
    Drop {
        cell: usize,
        heap: Option<ConsumableHeapKind>,
        reward: ConsumableReward,
        auto_explored: bool,
    },
    Mimic {
        cell: usize,
        carried: Vec<ConsumableReward>,
    },
    SpawnItem(SpecialItem),
    Plant {
        cell: usize,
        kind: ConsumablePlantKind,
    },
    Blob {
        cell: usize,
        kind: ConsumableBlobKind,
        volume: i32,
    },
    Trap {
        cell: usize,
        kind: ConsumableTrapKind,
        visible: bool,
        active: bool,
    },
}

/// Post-paint hazard state used by the room's `canPlaceCharacter` overrides.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConsumablePlacementState {
    pub toxic_gas_cells: Vec<usize>,
    pub eternal_fire_cells: Vec<usize>,
}

/// Search-visible and placement-visible output for one handled room.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConsumablePaintReport {
    pub searchable_items: Vec<WorldItem>,
    pub placement: ConsumablePlacementState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConsumablePaintOutcome {
    NotHandled,
    Painted(ConsumablePaintReport),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsumablePaintError {
    MissingRoom(RoomId),
    MissingEntrance(RoomId),
    MissingReverseEntrance { room: RoomId, neighbour: RoomId },
    Generator(GeneratorError),
}

impl fmt::Display for ConsumablePaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::MissingRoom(room) => write!(formatter, "room {room} does not exist"),
            Self::MissingEntrance(room) => {
                write!(formatter, "special room {room} has no placed entrance")
            }
            Self::MissingReverseEntrance { room, neighbour } => write!(
                formatter,
                "special room {room} entrance has no reverse connection from {neighbour}"
            ),
            Self::Generator(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ConsumablePaintError {}

impl From<GeneratorError> for ConsumablePaintError {
    fn from(error: GeneratorError) -> Self {
        Self::Generator(error)
    }
}

/// Terrain/occupancy sink for consumable-special effects not owned by Level.
pub trait ConsumableLevelContext {
    fn depth(&self) -> u32;
    fn map(&self) -> &GridMap;
    fn map_mut(&mut self) -> &mut GridMap;
    fn has_heap(&self, cell: usize) -> bool;
    fn has_mob(&self, cell: usize) -> bool;
    fn has_plant(&self, cell: usize) -> bool;
    fn well_water_override(&self, room: RoomId) -> Option<WellWaterKind>;
    fn record(&mut self, event: ConsumablePaintEvent);
}

/// Adapter for the shared painter Level plus an external typed event stream.
pub struct ConsumableLevelPaintContext<'a> {
    pub level: &'a mut Level,
    pub events: &'a mut Vec<ConsumablePaintEvent>,
    pub well_overrides: &'a [(RoomId, WellWaterKind)],
}

impl<'a> ConsumableLevelPaintContext<'a> {
    #[must_use]
    pub const fn new(level: &'a mut Level, events: &'a mut Vec<ConsumablePaintEvent>) -> Self {
        Self {
            level,
            events,
            well_overrides: &[],
        }
    }

    #[must_use]
    pub const fn with_well_overrides(mut self, overrides: &'a [(RoomId, WellWaterKind)]) -> Self {
        self.well_overrides = overrides;
        self
    }
}

impl ConsumableLevelContext for ConsumableLevelPaintContext<'_> {
    fn depth(&self) -> u32 {
        self.level.depth
    }

    fn map(&self) -> &GridMap {
        &self.level.map
    }

    fn map_mut(&mut self) -> &mut GridMap {
        &mut self.level.map
    }

    fn has_heap(&self, cell: usize) -> bool {
        self.level.heap_cells[cell]
    }

    fn has_mob(&self, cell: usize) -> bool {
        self.level.mob_cells[cell]
    }

    fn has_plant(&self, cell: usize) -> bool {
        self.level.plants.iter().any(|plant| plant.cell == cell)
            || self.events.iter().any(|event| {
                matches!(event, ConsumablePaintEvent::Plant { cell: at, .. } if *at == cell)
            })
    }

    fn well_water_override(&self, room: RoomId) -> Option<WellWaterKind> {
        self.well_overrides
            .iter()
            .find_map(|(at, kind)| (*at == room).then_some(*kind))
    }

    fn record(&mut self, event: ConsumablePaintEvent) {
        if let ConsumablePaintEvent::Plant { cell, .. } = &event {
            if matches!(
                self.level.map.cells[*cell],
                terrain::HIGH_GRASS
                    | terrain::FURROWED_GRASS
                    | terrain::EMPTY
                    | terrain::EMBERS
                    | terrain::EMPTY_DECO
            ) {
                self.level.map.cells[*cell] = terrain::GRASS;
            }
        }
        match &event {
            ConsumablePaintEvent::Drop { cell, .. } => self.level.mark_heap(*cell),
            ConsumablePaintEvent::Mimic { cell, .. } => self.level.mark_mob(*cell),
            ConsumablePaintEvent::Plant {
                cell,
                kind: ConsumablePlantKind::Sungrass,
            } => self.level.plant(generator::SeedKind::Sungrass, *cell),
            ConsumablePaintEvent::SpawnItem(_)
            | ConsumablePaintEvent::Plant { .. }
            | ConsumablePaintEvent::Blob { .. }
            | ConsumablePaintEvent::Trap { .. } => {}
        }
        self.events.push(event);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsumablePrizeMatch {
    Stone,
    Scroll,
}

/// Match-specific `findPrizeItem(Class)` and trinket state layered over the
/// shared insertion-ordered special-room prize pool.
pub trait ConsumablePrizeContext: SpecialPrizeContext {
    fn take_matching(&mut self, wanted: ConsumablePrizeMatch) -> Option<SpecialItem>;
    fn exotic_crystals_level(&self) -> i32;
}

impl ConsumablePrizeContext for PrizePool {
    fn take_matching(&mut self, wanted: ConsumablePrizeMatch) -> Option<SpecialItem> {
        let index = self.items_to_spawn.iter().position(|item| {
            matches!(
                (wanted, item),
                (
                    ConsumablePrizeMatch::Stone,
                    SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Stone(_))),
                ) | (
                    ConsumablePrizeMatch::Scroll,
                    SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Scroll { .. })),
                )
            )
        })?;
        Some(self.items_to_spawn.remove(index))
    }

    fn exotic_crystals_level(&self) -> i32 {
        self.exotic_crystals_level
    }
}

#[derive(Clone, Copy)]
struct Entrance {
    neighbour: RoomId,
    point: Point,
}

struct PaintInputs<'a, L, G, P> {
    level: &'a mut L,
    generator: &'a mut G,
    prizes: &'a mut P,
    rooms: &'a mut [Room],
    room: RoomId,
    bounds: Rect,
    entrance: Entrance,
    random: &'a mut RandomStack,
    report: ConsumablePaintReport,
}

impl<L, G, P> PaintInputs<'_, L, G, P>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    fn depth_i32(&self) -> i32 {
        i32::try_from(self.level.depth()).expect("dungeon depth fits Java int")
    }

    fn generate(
        &mut self,
        request: SpecialGeneratorRequest,
    ) -> Result<SpecialItem, GeneratorError> {
        self.generator
            .generate(request, self.depth_i32(), self.random)
            .map(SpecialItem::generated)
    }

    fn set_door(&mut self, door_type: DoorType) {
        set_shared_door_type(self.rooms, self.room, self.entrance.neighbour, door_type);
    }

    fn spawn(&mut self, item: SpecialItem) {
        self.prizes.add_item_to_spawn(item);
        self.level.record(ConsumablePaintEvent::SpawnItem(item));
    }

    fn drop(
        &mut self,
        cell: usize,
        heap: Option<ConsumableHeapKind>,
        item: ConsumableItem,
        source: ItemSource,
        access: ConsumableAccess,
        auto_explored: bool,
    ) {
        if access == ConsumableAccess::Independent {
            self.record_searchable(item, source, Accessibility::Independent);
        }
        self.level.record(ConsumablePaintEvent::Drop {
            cell,
            heap,
            reward: ConsumableReward { item, access },
            auto_explored,
        });
    }

    fn mimic(&mut self, cell: usize, items: Vec<ConsumableItem>) {
        let carried: Vec<_> = items
            .into_iter()
            .map(|item| {
                self.record_searchable(item, ItemSource::Mimic, Accessibility::Independent);
                ConsumableReward {
                    item,
                    access: ConsumableAccess::Independent,
                }
            })
            .collect();
        self.level
            .record(ConsumablePaintEvent::Mimic { cell, carried });
    }

    fn record_searchable(
        &mut self,
        item: ConsumableItem,
        source: ItemSource,
        accessibility: Accessibility,
    ) {
        if item == ConsumableItem::Special(SpecialItem::Paint(PaintItem::TrinketCatalyst)) {
            self.report.searchable_items.push(WorldItem::catalyst(
                u8::try_from(self.level.depth()).expect("dungeon depth fits u8"),
                source,
                accessibility,
            ));
        }
        let ConsumableItem::Special(SpecialItem::Paint(PaintItem::Generated(item))) = item else {
            return;
        };
        let depth = u8::try_from(self.level.depth()).expect("dungeon depth fits search model");
        let world = item.searchable_equipment().map(|equipment| {
            WorldItem::from_equipment_roll(
                equipment.item,
                equipment.roll,
                depth,
                source,
                accessibility,
            )
        });
        if let Some(world) = world {
            self.report.searchable_items.push(world);
        }
    }
}

/// Paint one of `SpecialRoom.CONSUMABLE_SPECIALS` without consuming any state
/// for unrelated special, secret, quest, or ordinary rooms.
///
/// # Errors
///
/// Returns a graph-shape error for a missing shared entrance or a generator
/// invariant error from an exact item-generation call site.
pub fn paint_consumable_special<L, G, P>(
    level: &mut L,
    rooms: &mut [Room],
    room: RoomId,
    generator: &mut G,
    prizes: &mut P,
    random: &mut RandomStack,
) -> Result<ConsumablePaintOutcome, ConsumablePaintError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    let selected = rooms
        .get(room)
        .ok_or(ConsumablePaintError::MissingRoom(room))?;
    let RoomKind::Special(kind) = selected.kind else {
        return Ok(ConsumablePaintOutcome::NotHandled);
    };
    if !is_consumable_special(kind) {
        return Ok(ConsumablePaintOutcome::NotHandled);
    }

    let connection = selected
        .connected
        .first()
        .ok_or(ConsumablePaintError::MissingEntrance(room))?;
    let entrance = Entrance {
        neighbour: connection.room,
        point: connection
            .door
            .ok_or(ConsumablePaintError::MissingEntrance(room))?
            .point,
    };
    if rooms
        .get(entrance.neighbour)
        .and_then(|neighbour| neighbour.connection_to(room))
        .and_then(|connection| connection.door)
        .is_none()
    {
        return Err(ConsumablePaintError::MissingReverseEntrance {
            room,
            neighbour: entrance.neighbour,
        });
    }

    let bounds = selected.bounds;
    let mut inputs = PaintInputs {
        level,
        generator,
        prizes,
        rooms,
        room,
        bounds,
        entrance,
        random,
        report: ConsumablePaintReport::default(),
    };
    match kind {
        SpecialRoomKind::Runestone => paint_runestone(&mut inputs)?,
        SpecialRoomKind::Garden => paint_garden(&mut inputs),
        SpecialRoomKind::Library => paint_library(&mut inputs)?,
        SpecialRoomKind::Storage => paint_storage(&mut inputs)?,
        SpecialRoomKind::Treasury => paint_treasury(&mut inputs)?,
        SpecialRoomKind::MagicWell => paint_magic_well(&mut inputs),
        SpecialRoomKind::ToxicGas => paint_toxic_gas(&mut inputs),
        SpecialRoomKind::MagicalFire => paint_magical_fire(&mut inputs)?,
        SpecialRoomKind::Traps => paint_traps(&mut inputs)?,
        SpecialRoomKind::CrystalPath => paint_crystal_path(&mut inputs)?,
        _ => unreachable!("consumable-special predicate and dispatch disagree"),
    }
    Ok(ConsumablePaintOutcome::Painted(inputs.report))
}

#[must_use]
pub const fn is_consumable_special(kind: SpecialRoomKind) -> bool {
    matches!(
        kind,
        SpecialRoomKind::Runestone
            | SpecialRoomKind::Garden
            | SpecialRoomKind::Library
            | SpecialRoomKind::Storage
            | SpecialRoomKind::Treasury
            | SpecialRoomKind::MagicWell
            | SpecialRoomKind::ToxicGas
            | SpecialRoomKind::MagicalFire
            | SpecialRoomKind::Traps
            | SpecialRoomKind::CrystalPath
    )
}

fn fill_room<L: ConsumableLevelContext>(level: &mut L, bounds: Rect, value: i32) {
    draw::fill(
        level.map_mut(),
        bounds.left,
        bounds.top,
        inclusive_width(bounds),
        inclusive_height(bounds),
        value,
    );
}

fn fill_room_margin<L: ConsumableLevelContext>(
    level: &mut L,
    bounds: Rect,
    margin: i32,
    value: i32,
) {
    draw::fill(
        level.map_mut(),
        bounds.left.wrapping_add(margin),
        bounds.top.wrapping_add(margin),
        inclusive_width(bounds).wrapping_sub(margin.wrapping_mul(2)),
        inclusive_height(bounds).wrapping_sub(margin.wrapping_mul(2)),
        value,
    );
}

const fn inclusive_width(bounds: Rect) -> i32 {
    bounds.right.wrapping_sub(bounds.left).wrapping_add(1)
}

const fn inclusive_height(bounds: Rect) -> i32 {
    bounds.bottom.wrapping_sub(bounds.top).wrapping_add(1)
}

fn point_to_cell<L: ConsumableLevelContext>(level: &L, point: Point) -> usize {
    level.map().point_to_cell(point)
}

fn random_room_point(bounds: Rect, margin: i32, random: &mut RandomStack) -> Point {
    Point::new(
        random.int_range(
            bounds.left.wrapping_add(margin),
            bounds.right.wrapping_sub(margin),
        ),
        random.int_range(
            bounds.top.wrapping_add(margin),
            bounds.bottom.wrapping_sub(margin),
        ),
    )
}

fn room_center(bounds: Rect, random: &mut RandomStack) -> Point {
    let x_jitter = if bounds.right.wrapping_sub(bounds.left) % 2 == 1 {
        random.int_bound(2)
    } else {
        0
    };
    let y_jitter = if bounds.bottom.wrapping_sub(bounds.top) % 2 == 1 {
        random.int_bound(2)
    } else {
        0
    };
    Point::new(
        div_i32(bounds.left.wrapping_add(bounds.right), 2).wrapping_add(x_jitter),
        div_i32(bounds.top.wrapping_add(bounds.bottom), 2).wrapping_add(y_jitter),
    )
}

fn empty_room_constructor(random: &mut RandomStack) {
    random
        .chances(&[1.0, 0.0, 0.0])
        .expect("EmptyRoom's NORMAL size category is always selectable");
}

const fn direct_potion(kind: PotionKind) -> SpecialItem {
    SpecialItem::generated(GeneratedItem::Potion {
        kind,
        exotic: false,
    })
}

const fn direct_scroll(kind: ScrollKind, exotic: bool) -> SpecialItem {
    SpecialItem::generated(GeneratedItem::Scroll { kind, exotic })
}

const fn direct_gold(quantity: i32) -> SpecialItem {
    SpecialItem::generated(GeneratedItem::Gold { quantity })
}

fn iron_key(depth: i32) -> SpecialItem {
    SpecialItem::IronKey { depth }
}

fn crystal_key(depth: i32) -> SpecialItem {
    SpecialItem::CrystalKey { depth }
}

fn paint_runestone<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::CHASM);
    draw::draw_inside(
        inputs.level.map_mut(),
        inputs.bounds,
        inputs.entrance.point,
        2,
        terrain::EMPTY_SP,
    );
    fill_room_margin(inputs.level, inputs.bounds, 2, terrain::EMPTY);

    let count = inputs.random.normal_int_range(2, 3);
    for _ in 0..count {
        let cell = loop {
            let point = random_room_point(inputs.bounds, 1, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if inputs.level.map().cells[cell] == terrain::EMPTY && !inputs.level.has_heap(cell) {
                break cell;
            }
        };
        let item = if let Some(catalyst) = inputs.prizes.take_trinket_catalyst() {
            catalyst
        } else if let Some(stone) = inputs.prizes.take_matching(ConsumablePrizeMatch::Stone) {
            stone
        } else {
            inputs.generate(SpecialGeneratorRequest::Category(GeneratorCategory::Stone))?
        };
        inputs.drop(
            cell,
            None,
            item.into(),
            ItemSource::Heap,
            ConsumableAccess::Independent,
            false,
        );
    }
    inputs.set_door(DoorType::Locked);
    inputs.spawn(iron_key(inputs.depth_i32()));
    Ok(())
}

fn plant<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>, kind: ConsumablePlantKind)
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    let cell = loop {
        let point = random_room_point(inputs.bounds, 1, inputs.random);
        let cell = point_to_cell(inputs.level, point);
        if !inputs.level.has_plant(cell) {
            break cell;
        }
    };
    inputs
        .level
        .record(ConsumablePaintEvent::Plant { cell, kind });
}

fn paint_garden<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::HIGH_GRASS);
    fill_room_margin(inputs.level, inputs.bounds, 2, terrain::GRASS);
    inputs.set_door(DoorType::Locked);
    inputs.spawn(iron_key(inputs.depth_i32()));

    match inputs.random.int_bound(3) {
        0 => plant(inputs, ConsumablePlantKind::Sungrass),
        1 => plant(inputs, ConsumablePlantKind::Blandfruit),
        _ if inputs.random.int_bound(5) == 0 => {
            plant(inputs, ConsumablePlantKind::Sungrass);
            plant(inputs, ConsumablePlantKind::Blandfruit);
        }
        _ => {}
    }

    for y in inputs.bounds.top.wrapping_add(1)..inputs.bounds.bottom {
        for x in inputs.bounds.left.wrapping_add(1)..inputs.bounds.right {
            let cell = point_to_cell(inputs.level, Point::new(x, y));
            inputs.level.record(ConsumablePaintEvent::Blob {
                cell,
                kind: ConsumableBlobKind::Foliage,
                volume: 1,
            });
        }
    }
}

fn paint_library<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY_SP);
    draw::fill(
        inputs.level.map_mut(),
        inputs.bounds.left.wrapping_add(1),
        inputs.bounds.top.wrapping_add(1),
        inclusive_width(inputs.bounds).wrapping_sub(2),
        1,
        terrain::BOOKSHELF,
    );
    draw::draw_inside(
        inputs.level.map_mut(),
        inputs.bounds,
        inputs.entrance.point,
        1,
        terrain::EMPTY_SP,
    );

    let count = inputs.random.normal_int_range(1, 3);
    for index in 0..count {
        let cell = loop {
            let point = random_room_point(inputs.bounds, 1, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if inputs.level.map().cells[cell] == terrain::EMPTY_SP && !inputs.level.has_heap(cell) {
                break cell;
            }
        };
        let item = if index == 0 {
            direct_scroll(
                if inputs.random.int_bound(2) == 0 {
                    ScrollKind::Identify
                } else {
                    ScrollKind::RemoveCurse
                },
                false,
            )
        } else if let Some(catalyst) = inputs.prizes.take_trinket_catalyst() {
            catalyst
        } else if let Some(scroll) = inputs.prizes.take_matching(ConsumablePrizeMatch::Scroll) {
            scroll
        } else {
            inputs.generate(SpecialGeneratorRequest::Category(GeneratorCategory::Scroll))?
        };
        inputs.drop(
            cell,
            None,
            item.into(),
            ItemSource::Heap,
            ConsumableAccess::Independent,
            false,
        );
    }
    inputs.set_door(DoorType::Locked);
    inputs.spawn(iron_key(inputs.depth_i32()));
    Ok(())
}

fn storage_prize<L, G, P>(
    inputs: &mut PaintInputs<'_, L, G, P>,
) -> Result<SpecialItem, GeneratorError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    if inputs.random.int_bound(3) != 0 {
        if let Some(prize) = inputs.prizes.take_prize_item(inputs.random) {
            return Ok(prize);
        }
    }
    let categories = [
        GeneratorCategory::Potion,
        GeneratorCategory::Scroll,
        GeneratorCategory::Food,
        GeneratorCategory::Gold,
    ];
    let selected = usize::try_from(inputs.random.int_bound(4)).unwrap_or_default();
    inputs.generate(SpecialGeneratorRequest::Category(categories[selected]))
}

fn paint_storage<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY_SP);
    let mut honeypot = inputs.random.int_bound(2) == 0;
    let count = inputs.random.int_range(3, 4);
    for _ in 0..count {
        let cell = loop {
            let point = random_room_point(inputs.bounds, 1, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if inputs.level.map().cells[cell] == terrain::EMPTY_SP && !inputs.level.has_heap(cell) {
                break cell;
            }
        };
        let item = if honeypot {
            honeypot = false;
            ConsumableItem::Honeypot
        } else {
            storage_prize(inputs)?.into()
        };
        inputs.drop(
            cell,
            None,
            item,
            ItemSource::Heap,
            ConsumableAccess::Independent,
            false,
        );
    }
    inputs.set_door(DoorType::Barricade);
    inputs.spawn(direct_potion(PotionKind::LiquidFlame));
    Ok(())
}

fn mimic_tooth_multiplier(level: i32) -> f32 {
    if level == -1 {
        1.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            1.5 + 0.5 * level as f32
        }
    }
}

fn mimic_reward<L, G, P>(
    inputs: &mut PaintInputs<'_, L, G, P>,
) -> Result<SpecialItem, GeneratorError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    let floor_set = inputs.depth_i32() / 5;
    let request = match inputs.random.int_bound(5) {
        0 => SpecialGeneratorRequest::Gold,
        1 => SpecialGeneratorRequest::Missile {
            floor_set,
            use_defaults: false,
        },
        2 => SpecialGeneratorRequest::Armor { floor_set },
        3 => SpecialGeneratorRequest::Weapon {
            floor_set,
            use_defaults: false,
        },
        _ => SpecialGeneratorRequest::Category(GeneratorCategory::Ring),
    };
    inputs.generate(request)
}

fn paint_treasury<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    let center = room_center(inputs.bounds, inputs.random);
    draw::set_point(inputs.level.map_mut(), center, terrain::STATUE);
    let heap = if inputs.random.int_bound(2) == 0 {
        ConsumableHeapKind::Chest
    } else {
        ConsumableHeapKind::Heap
    };
    let count = inputs.random.int_range(2, 3);
    let mimic_chance =
        1.0_f32 / 5.0_f32 * mimic_tooth_multiplier(inputs.prizes.mimic_tooth_level());
    for _ in 0..count {
        let item = inputs.prizes.take_trinket_catalyst().unwrap_or_else(|| {
            SpecialItem::generated(generator::random_gold(inputs.random, inputs.depth_i32()))
        });
        let cell = loop {
            let point = random_room_point(inputs.bounds, 1, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if inputs.level.map().cells[cell] == terrain::EMPTY
                && !inputs.level.has_heap(cell)
                && !inputs.level.has_mob(cell)
            {
                break cell;
            }
        };
        if heap == ConsumableHeapKind::Chest
            && inputs.level.depth() > 1
            && inputs.random.float() < mimic_chance
        {
            let reward = mimic_reward(inputs)?;
            let mut items = vec![item.into(), reward.into()];
            if inputs.random.trinket.is(crate::catalog::ItemId::MimicTooth) {
                items.push(
                    inputs
                        .generate(SpecialGeneratorRequest::DefaultOverall)?
                        .into(),
                );
            }
            inputs.mimic(cell, items);
        } else {
            inputs.drop(
                cell,
                Some(heap),
                item.into(),
                if heap == ConsumableHeapKind::Chest {
                    ItemSource::Chest
                } else {
                    ItemSource::Heap
                },
                ConsumableAccess::Independent,
                false,
            );
        }
    }

    if heap == ConsumableHeapKind::Heap {
        for _ in 0..6 {
            let cell = loop {
                let point = random_room_point(inputs.bounds, 1, inputs.random);
                let cell = point_to_cell(inputs.level, point);
                if inputs.level.map().cells[cell] == terrain::EMPTY {
                    break cell;
                }
            };
            let quantity = inputs.random.int_range(5, 12);
            inputs.drop(
                cell,
                None,
                direct_gold(quantity).into(),
                ItemSource::Heap,
                ConsumableAccess::Independent,
                false,
            );
        }
    }
    inputs.set_door(DoorType::Locked);
    inputs.spawn(iron_key(inputs.depth_i32()));
    Ok(())
}

fn paint_magic_well<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    let center = room_center(inputs.bounds, inputs.random);
    draw::set_point(inputs.level.map_mut(), center, terrain::WELL);
    let water = inputs
        .level
        .well_water_override(inputs.room)
        .unwrap_or_else(|| {
            if inputs.random.int_bound(2) == 0 {
                WellWaterKind::Awareness
            } else {
                WellWaterKind::Health
            }
        });
    inputs.level.record(ConsumablePaintEvent::Blob {
        cell: point_to_cell(inputs.level, center),
        kind: ConsumableBlobKind::WellWater(water),
        volume: 1,
    });
    inputs.set_door(DoorType::Locked);
    inputs.spawn(iron_key(inputs.depth_i32()));
}

fn paint_toxic_gas<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    let center = room_center(inputs.bounds, inputs.random);
    draw::set_point(inputs.level.map_mut(), center, terrain::STATUE);

    for x in inputs.bounds.left..=inputs.bounds.right {
        for y in inputs.bounds.top..=inputs.bounds.bottom {
            let cell = point_to_cell(inputs.level, Point::new(x, y));
            if inputs.level.map().cells[cell] == terrain::EMPTY {
                inputs.report.placement.toxic_gas_cells.push(cell);
                inputs.level.record(ConsumablePaintEvent::Blob {
                    cell,
                    kind: ConsumableBlobKind::ToxicGas,
                    volume: 30,
                });
            }
        }
    }

    let trap_count = inclusive_width(inputs.bounds)
        .wrapping_sub(2)
        .min(inclusive_height(inputs.bounds).wrapping_sub(2));
    for _ in 0..trap_count {
        let cell = loop {
            let point = random_room_point(inputs.bounds, 2, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if inputs.level.map().cells[cell] == terrain::EMPTY {
                break cell;
            }
        };
        inputs.level.record(ConsumablePaintEvent::Trap {
            cell,
            kind: ConsumableTrapKind::ToxicVent,
            visible: true,
            active: false,
        });
        inputs.level.record(ConsumablePaintEvent::Blob {
            cell,
            kind: ConsumableBlobKind::ToxicGasSeed,
            volume: 12,
        });
        draw::set(inputs.level.map_mut(), cell, terrain::INACTIVE_TRAP);
    }

    let mut gold_positions = Vec::with_capacity(8);
    while gold_positions.len() < 8 {
        let point = random_room_point(inputs.bounds, 2, inputs.random);
        let cell = point_to_cell(inputs.level, point);
        if inputs.level.map().cells[cell] != terrain::STATUE && !gold_positions.contains(&cell) {
            gold_positions.push(cell);
        }
    }

    let entrance_cell = point_to_cell(inputs.level, inputs.entrance.point);
    let furthest_index = gold_positions
        .iter()
        .enumerate()
        .max_by_key(|(index, cell)| {
            // `max_by_key` keeps the final tie, while Java keeps the first.
            // Reverse the index in the key to retain the first equal distance.
            (
                true_distance_square(inputs.level.map(), entrance_cell, **cell),
                Reverse(*index),
            )
        })
        .map(|(index, _)| index)
        .expect("eight toxic-gas gold positions were generated");
    let furthest = gold_positions.remove(furthest_index);
    let mut main_gold = generator::random_gold(inputs.random, inputs.depth_i32());
    if let GeneratedItem::Gold { quantity } = &mut main_gold {
        *quantity = quantity.wrapping_mul(2);
    }
    inputs.drop(
        furthest,
        Some(ConsumableHeapKind::Skeleton),
        SpecialItem::generated(main_gold).into(),
        ItemSource::Tomb,
        ConsumableAccess::Independent,
        false,
    );

    for _ in 0..2 {
        let item = inputs.prizes.take_trinket_catalyst().unwrap_or_else(|| {
            SpecialItem::generated(generator::random_gold(inputs.random, inputs.depth_i32()))
        });
        inputs.drop(
            gold_positions.remove(0),
            Some(ConsumableHeapKind::Chest),
            item.into(),
            ItemSource::Chest,
            ConsumableAccess::Independent,
            false,
        );
    }
    inputs.spawn(direct_potion(PotionKind::Purity));
    inputs.set_door(DoorType::Regular);
}

fn true_distance_square(map: &GridMap, first: usize, second: usize) -> i64 {
    let width = usize::try_from(map.width).expect("map width is positive");
    let first_x = first % width;
    let first_y = first / width;
    let second_x = second % width;
    let second_y = second / width;
    let dx = first_x.abs_diff(second_x);
    let dy = first_y.abs_diff(second_y);
    i64::try_from(dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy))).unwrap_or(i64::MAX)
}

fn paint_magical_fire<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    inputs.set_door(DoorType::Regular);

    let mut fire = room_center(inputs.bounds, inputs.random);
    empty_room_constructor(inputs.random);
    let behind = if inputs.entrance.point.x == inputs.bounds.left
        || inputs.entrance.point.x == inputs.bounds.right
    {
        fire.y = inputs.bounds.top.wrapping_add(1);
        while fire.y != inputs.bounds.bottom {
            let cell = point_to_cell(inputs.level, fire);
            inputs.report.placement.eternal_fire_cells.push(cell);
            inputs.level.record(ConsumablePaintEvent::Blob {
                cell,
                kind: ConsumableBlobKind::EternalFire,
                volume: 1,
            });
            draw::set_point(inputs.level.map_mut(), fire, terrain::EMPTY_SP);
            fire.y = fire.y.wrapping_add(1);
        }
        if inputs.entrance.point.x == inputs.bounds.left {
            Rect::new(
                fire.x.wrapping_add(1),
                inputs.bounds.top.wrapping_add(1),
                inputs.bounds.right.wrapping_sub(1),
                inputs.bounds.bottom.wrapping_sub(1),
            )
        } else {
            Rect::new(
                inputs.bounds.left.wrapping_add(1),
                inputs.bounds.top.wrapping_add(1),
                fire.x.wrapping_sub(1),
                inputs.bounds.bottom.wrapping_sub(1),
            )
        }
    } else {
        fire.x = inputs.bounds.left.wrapping_add(1);
        while fire.x != inputs.bounds.right {
            let cell = point_to_cell(inputs.level, fire);
            inputs.report.placement.eternal_fire_cells.push(cell);
            inputs.level.record(ConsumablePaintEvent::Blob {
                cell,
                kind: ConsumableBlobKind::EternalFire,
                volume: 1,
            });
            draw::set_point(inputs.level.map_mut(), fire, terrain::EMPTY_SP);
            fire.x = fire.x.wrapping_add(1);
        }
        if inputs.entrance.point.y == inputs.bounds.top {
            Rect::new(
                inputs.bounds.left.wrapping_add(1),
                fire.y.wrapping_add(1),
                inputs.bounds.right.wrapping_sub(1),
                inputs.bounds.bottom.wrapping_sub(1),
            )
        } else {
            Rect::new(
                inputs.bounds.left.wrapping_add(1),
                inputs.bounds.top.wrapping_add(1),
                inputs.bounds.right.wrapping_sub(1),
                fire.y.wrapping_sub(1),
            )
        }
    };
    fill_room(inputs.level, behind, terrain::EMPTY_SP);

    let mut honeypot = inputs.random.int_bound(2) == 0;
    let count = inputs.random.int_range(3, 4);
    for _ in 0..count {
        let cell = loop {
            let point = random_room_point(behind, 0, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if !inputs.level.has_heap(cell) {
                break cell;
            }
        };
        let item = if honeypot {
            honeypot = false;
            ConsumableItem::Honeypot
        } else {
            storage_prize(inputs)?.into()
        };
        inputs.drop(
            cell,
            None,
            item,
            ItemSource::Heap,
            ConsumableAccess::Independent,
            false,
        );
    }
    inputs.spawn(direct_potion(PotionKind::Frost));
    Ok(())
}

const SEWER_TRAPS: [ConsumableTrapKind; 3] = [
    ConsumableTrapKind::Gripping,
    ConsumableTrapKind::Teleportation,
    ConsumableTrapKind::Flock,
];
const PRISON_TRAPS: [ConsumableTrapKind; 3] = [
    ConsumableTrapKind::PoisonDart,
    ConsumableTrapKind::Gripping,
    ConsumableTrapKind::Explosive,
];
const CAVES_TRAPS: [ConsumableTrapKind; 3] = [
    ConsumableTrapKind::PoisonDart,
    ConsumableTrapKind::Flashing,
    ConsumableTrapKind::Explosive,
];
const CITY_TRAPS: [ConsumableTrapKind; 3] = [
    ConsumableTrapKind::Warping,
    ConsumableTrapKind::Flashing,
    ConsumableTrapKind::Disintegration,
];
const HALLS_TRAPS: [ConsumableTrapKind; 1] = [ConsumableTrapKind::Grim];

fn trap_table(depth: u32) -> &'static [ConsumableTrapKind] {
    match depth / 5 {
        0 => &SEWER_TRAPS,
        1 => &PRISON_TRAPS,
        2 => &CAVES_TRAPS,
        3 => &CITY_TRAPS,
        4 => &HALLS_TRAPS,
        _ => panic!("main-dungeon trap rooms exist only through depth 24"),
    }
}

fn clear_curse_effect(item: &mut SpecialItem) {
    let SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Equipment(equipment))) = item else {
        return;
    };
    if equipment.roll.effect.is_some_and(|effect| match effect {
        Effect::Weapon(effect) => effect.is_curse(),
        Effect::Armor(effect) => effect.is_curse(),
    }) {
        equipment.roll.effect = None;
    }
    equipment.roll.cursed = false;
}

fn upgrade_equipment(item: &mut SpecialItem) {
    let SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Equipment(equipment))) = item else {
        return;
    };
    equipment.roll.upgrade = equipment.roll.upgrade.wrapping_add(1);
}

fn traps_prize<L, G, P>(
    inputs: &mut PaintInputs<'_, L, G, P>,
) -> Result<SpecialItem, GeneratorError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    if inputs.random.int_bound(3) != 0 {
        if let Some(prize) = inputs.prizes.take_prize_item(inputs.random) {
            return Ok(prize);
        }
    }
    let floor_set = inputs.depth_i32() / 5 + 1;
    let mut prize = if inputs.random.int_bound(2) == 0 {
        inputs.generate(SpecialGeneratorRequest::Weapon {
            floor_set,
            use_defaults: false,
        })?
    } else {
        inputs.generate(SpecialGeneratorRequest::Armor { floor_set })?
    };
    clear_curse_effect(&mut prize);
    if inputs.random.int_bound(3) == 0 {
        upgrade_equipment(&mut prize);
    }
    Ok(prize)
}

fn paint_traps<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    let trap_kind = if inputs.random.int_bound(4) == 0 {
        None
    } else {
        let table = trap_table(inputs.level.depth());
        let index = usize::try_from(
            inputs
                .random
                .int_bound(i32::try_from(table.len()).expect("trap table fits Java int")),
        )
        .unwrap_or_default();
        Some(table[index])
    };
    fill_room_margin(
        inputs.level,
        inputs.bounds,
        1,
        if trap_kind.is_some() {
            terrain::TRAP
        } else {
            terrain::CHASM
        },
    );
    inputs.set_door(DoorType::Regular);
    let first_inner = point_to_cell(
        inputs.level,
        Point::new(
            inputs.bounds.left.wrapping_add(1),
            inputs.bounds.top.wrapping_add(1),
        ),
    );
    let last_row = if inputs.level.map().cells[first_inner] == terrain::CHASM {
        terrain::CHASM
    } else {
        terrain::EMPTY
    };

    let (x, y) = if inputs.entrance.point.x == inputs.bounds.left {
        let x = inputs.bounds.right.wrapping_sub(1);
        let y = inputs
            .bounds
            .top
            .wrapping_add(div_i32(inclusive_height(inputs.bounds), 2));
        draw::fill(
            inputs.level.map_mut(),
            x,
            inputs.bounds.top.wrapping_add(1),
            1,
            inclusive_height(inputs.bounds).wrapping_sub(2),
            last_row,
        );
        (x, y)
    } else if inputs.entrance.point.x == inputs.bounds.right {
        let x = inputs.bounds.left.wrapping_add(1);
        let y = inputs
            .bounds
            .top
            .wrapping_add(div_i32(inclusive_height(inputs.bounds), 2));
        draw::fill(
            inputs.level.map_mut(),
            x,
            inputs.bounds.top.wrapping_add(1),
            1,
            inclusive_height(inputs.bounds).wrapping_sub(2),
            last_row,
        );
        (x, y)
    } else if inputs.entrance.point.y == inputs.bounds.top {
        let x = inputs
            .bounds
            .left
            .wrapping_add(div_i32(inclusive_width(inputs.bounds), 2));
        let y = inputs.bounds.bottom.wrapping_sub(1);
        draw::fill(
            inputs.level.map_mut(),
            inputs.bounds.left.wrapping_add(1),
            y,
            inclusive_width(inputs.bounds).wrapping_sub(2),
            1,
            last_row,
        );
        (x, y)
    } else {
        let x = inputs
            .bounds
            .left
            .wrapping_add(div_i32(inclusive_width(inputs.bounds), 2));
        let y = inputs.bounds.top.wrapping_add(1);
        draw::fill(
            inputs.level.map_mut(),
            inputs.bounds.left.wrapping_add(1),
            y,
            inclusive_width(inputs.bounds).wrapping_sub(2),
            1,
            last_row,
        );
        (x, y)
    };

    if let Some(kind) = trap_kind {
        for x in inputs.bounds.left..=inputs.bounds.right {
            for y in inputs.bounds.top..=inputs.bounds.bottom {
                let cell = point_to_cell(inputs.level, Point::new(x, y));
                if inputs.level.map().cells[cell] == terrain::TRAP {
                    inputs.level.record(ConsumablePaintEvent::Trap {
                        cell,
                        kind,
                        visible: true,
                        active: true,
                    });
                }
            }
        }
    }

    let cell = point_to_cell(inputs.level, Point::new(x, y));
    if inputs.random.int_bound(3) == 0 {
        if last_row == terrain::CHASM {
            draw::set(inputs.level.map_mut(), cell, terrain::EMPTY);
        }
    } else {
        draw::set(inputs.level.map_mut(), cell, terrain::PEDESTAL);
    }
    let prize = traps_prize(inputs)?;
    inputs.drop(
        cell,
        Some(ConsumableHeapKind::Chest),
        prize.into(),
        ItemSource::Chest,
        ConsumableAccess::Independent,
        false,
    );
    inputs.spawn(direct_potion(PotionKind::Levitation));
    Ok(())
}

fn set_crystal_door<L: ConsumableLevelContext>(level: &mut L, point: Point) {
    draw::set_point(level.map_mut(), point, terrain::CRYSTAL_DOOR);
}

fn crystal_path_geometry<L, G, P>(
    inputs: &mut PaintInputs<'_, L, G, P>,
) -> ([Rect; 6], usize, usize)
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    let mut rooms = [Rect::default(); 6];
    for _ in &rooms {
        empty_room_constructor(inputs.random);
    }
    let entry = inputs.entrance.point;
    let bounds = inputs.bounds;
    let prize1;
    let prize2;
    if entry.x == bounds.left || entry.x == bounds.right {
        draw::draw_inside(
            inputs.level.map_mut(),
            bounds,
            entry,
            if inclusive_width(bounds) > 8 { 5 } else { 3 },
            terrain::EMPTY,
        );
        let room_w1 = if inclusive_width(bounds) >= 9 { 2 } else { 1 };
        let room_w2 = if inclusive_width(bounds) % 2 == 0 {
            2
        } else {
            1
        };
        let room_h = if inclusive_height(bounds) >= 9 { 2 } else { 1 };
        if entry.x == bounds.left {
            rooms[0]
                .set_pos(bounds.left + 1, entry.y - room_h - 1)
                .resize(room_w1 - 1, room_h - 1);
            set_crystal_door(inputs.level, Point::new(rooms[0].left, rooms[0].bottom + 1));
            rooms[1]
                .set_pos(bounds.left + 1, entry.y + 2)
                .resize(room_w1 - 1, room_h - 1);
            set_crystal_door(inputs.level, Point::new(rooms[1].left, rooms[1].top - 1));
            rooms[2]
                .set_pos(rooms[1].right + 2, entry.y - room_h - 1)
                .resize(room_w1 - 1, room_h - 1);
            set_crystal_door(inputs.level, Point::new(rooms[2].left, rooms[2].bottom + 1));
            rooms[3]
                .set_pos(rooms[1].right + 2, entry.y + 2)
                .resize(room_w1 - 1, room_h - 1);
            set_crystal_door(inputs.level, Point::new(rooms[3].left, rooms[3].top - 1));
            rooms[4]
                .set_pos(rooms[3].right + 2, entry.y - room_h - 1)
                .resize(room_w2 - 1, room_h);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[4].left - 1, rooms[4].bottom - 1),
            );
            rooms[5]
                .set_pos(rooms[3].right + 2, entry.y + 1)
                .resize(room_w2 - 1, room_h);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[5].left - 1, rooms[5].top + 1),
            );
            prize1 = point_to_cell(inputs.level, Point::new(rooms[4].left, rooms[4].bottom));
            prize2 = point_to_cell(inputs.level, Point::new(rooms[5].left, rooms[5].top));
        } else {
            rooms[0]
                .set_pos(bounds.right - room_w1, entry.y - room_h - 1)
                .resize(room_w1 - 1, room_h - 1);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[0].right, rooms[0].bottom + 1),
            );
            rooms[1]
                .set_pos(bounds.right - room_w1, entry.y + 2)
                .resize(room_w1 - 1, room_h - 1);
            set_crystal_door(inputs.level, Point::new(rooms[1].right, rooms[1].top - 1));
            rooms[2]
                .set_pos(rooms[1].left - room_w1 - 1, entry.y - room_h - 1)
                .resize(room_w1 - 1, room_h - 1);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[2].right, rooms[2].bottom + 1),
            );
            rooms[3]
                .set_pos(rooms[1].left - room_w1 - 1, entry.y + 2)
                .resize(room_w1 - 1, room_h - 1);
            set_crystal_door(inputs.level, Point::new(rooms[3].right, rooms[3].top - 1));
            rooms[4]
                .set_pos(rooms[3].left - room_w2 - 1, entry.y - room_h - 1)
                .resize(room_w2 - 1, room_h);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[4].right + 1, rooms[4].bottom - 1),
            );
            rooms[5]
                .set_pos(rooms[3].left - room_w2 - 1, entry.y + 1)
                .resize(room_w2 - 1, room_h);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[5].right + 1, rooms[5].top + 1),
            );
            prize1 = point_to_cell(inputs.level, Point::new(rooms[4].right, rooms[4].bottom));
            prize2 = point_to_cell(inputs.level, Point::new(rooms[5].right, rooms[5].top));
        }
    } else {
        draw::draw_inside(
            inputs.level.map_mut(),
            bounds,
            entry,
            if inclusive_height(bounds) > 8 { 5 } else { 3 },
            terrain::EMPTY,
        );
        let room_w = if inclusive_width(bounds) >= 9 { 2 } else { 1 };
        let room_h1 = if inclusive_height(bounds) >= 9 { 2 } else { 1 };
        let room_h2 = if inclusive_height(bounds) % 2 == 0 {
            2
        } else {
            1
        };
        if entry.y == bounds.top {
            rooms[0]
                .set_pos(entry.x - room_w - 1, bounds.top + 1)
                .resize(room_w - 1, room_h1 - 1);
            set_crystal_door(inputs.level, Point::new(rooms[0].right + 1, rooms[0].top));
            rooms[1]
                .set_pos(entry.x + 2, bounds.top + 1)
                .resize(room_w - 1, room_h1 - 1);
            set_crystal_door(inputs.level, Point::new(rooms[1].left - 1, rooms[1].top));
            rooms[2]
                .set_pos(entry.x - room_w - 1, rooms[1].bottom + 2)
                .resize(room_w - 1, room_h1 - 1);
            set_crystal_door(inputs.level, Point::new(rooms[2].right + 1, rooms[2].top));
            rooms[3]
                .set_pos(entry.x + 2, rooms[1].bottom + 2)
                .resize(room_w - 1, room_h1 - 1);
            set_crystal_door(inputs.level, Point::new(rooms[3].left - 1, rooms[3].top));
            rooms[4]
                .set_pos(entry.x - room_w - 1, rooms[3].bottom + 2)
                .resize(room_w, room_h2 - 1);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[4].right - 1, rooms[4].top - 1),
            );
            rooms[5]
                .set_pos(entry.x + 1, rooms[3].bottom + 2)
                .resize(room_w, room_h2 - 1);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[5].left + 1, rooms[5].top - 1),
            );
            prize1 = point_to_cell(inputs.level, Point::new(rooms[4].right, rooms[4].top));
            prize2 = point_to_cell(inputs.level, Point::new(rooms[5].left, rooms[5].top));
        } else {
            rooms[0]
                .set_pos(entry.x - room_w - 1, bounds.bottom - room_h1)
                .resize(room_w - 1, room_h1 - 1);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[0].right + 1, rooms[0].bottom),
            );
            rooms[1]
                .set_pos(entry.x + 2, bounds.bottom - room_h1)
                .resize(room_w - 1, room_h1 - 1);
            set_crystal_door(inputs.level, Point::new(rooms[1].left - 1, rooms[1].bottom));
            rooms[2]
                .set_pos(entry.x - room_w - 1, rooms[1].top - room_h1 - 1)
                .resize(room_w - 1, room_h1 - 1);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[2].right + 1, rooms[2].bottom),
            );
            rooms[3]
                .set_pos(entry.x + 2, rooms[1].top - room_h1 - 1)
                .resize(room_w - 1, room_h1 - 1);
            set_crystal_door(inputs.level, Point::new(rooms[3].left - 1, rooms[3].bottom));
            rooms[4]
                .set_pos(entry.x - room_w - 1, rooms[3].top - room_h2 - 1)
                .resize(room_w, room_h2 - 1);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[4].right - 1, rooms[4].bottom + 1),
            );
            rooms[5]
                .set_pos(entry.x + 1, rooms[3].top - room_h2 - 1)
                .resize(room_w, room_h2 - 1);
            set_crystal_door(
                inputs.level,
                Point::new(rooms[5].left + 1, rooms[5].bottom + 1),
            );
            prize1 = point_to_cell(inputs.level, Point::new(rooms[4].right, rooms[4].bottom));
            prize2 = point_to_cell(inputs.level, Point::new(rooms[5].left, rooms[5].bottom));
        }
    }
    for room in rooms {
        fill_room(inputs.level, room, terrain::EMPTY_SP);
    }
    draw::set(inputs.level.map_mut(), prize1, terrain::PEDESTAL);
    draw::set(inputs.level.map_mut(), prize2, terrain::PEDESTAL);
    (rooms, prize1, prize2)
}

fn same_consumable_identity(first: SpecialItem, second: SpecialItem) -> bool {
    match (first, second) {
        (
            SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Potion { kind: first, .. })),
            SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Potion {
                kind: second, ..
            })),
        ) => first == second,
        (
            SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Scroll { kind: first, .. })),
            SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Scroll {
                kind: second, ..
            })),
        ) => first == second,
        _ => false,
    }
}

fn add_crystal_reward<L, G, P>(
    inputs: &mut PaintInputs<'_, L, G, P>,
    category: GeneratorCategory,
    items: &mut Vec<SpecialItem>,
    duplicates: &mut Vec<SpecialItem>,
) -> Result<(), GeneratorError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    loop {
        let reward = inputs.generate(SpecialGeneratorRequest::Category(category))?;
        if items
            .iter()
            .any(|existing| same_consumable_identity(*existing, reward))
        {
            // Java calls Generator.undoDrop(concreteClass) later. Its reversed
            // assignability test makes that operation a no-op for every
            // concrete potion and scroll class, including de-exotified ones.
            duplicates.push(reward);
        } else {
            items.push(reward);
            return Ok(());
        }
    }
}

#[allow(clippy::cast_possible_truncation)] // Java casts integral f32 table entries to int.
fn consumable_value(item: SpecialItem) -> i32 {
    let (category, index) = match item {
        SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Potion { kind, .. })) => {
            (GeneratorCategory::Potion, kind as usize)
        }
        SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Scroll { kind, .. })) => {
            (GeneratorCategory::Scroll, kind as usize)
        }
        _ => panic!("CrystalPath rewards are potions or scrolls"),
    };
    category
        .default_probabilities_total()
        .and_then(|values| values.get(index))
        .copied()
        .unwrap_or_default() as i32
}

fn exotic_crystals_chance(level: i32) -> f32 {
    if level == -1 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            0.125 + 0.125 * level as f32
        }
    }
}

fn slot_for_room(index: usize) -> CrystalPathSlot {
    match index {
        0 => CrystalPathSlot::FirstA,
        1 => CrystalPathSlot::FirstB,
        2 => CrystalPathSlot::SecondA,
        3 => CrystalPathSlot::SecondB,
        4 => CrystalPathSlot::FinalA,
        5 => CrystalPathSlot::FinalB,
        _ => panic!("CrystalPath has exactly six rooms"),
    }
}

fn paint_crystal_path<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: ConsumableLevelContext,
    G: SpecialGeneratorContext,
    P: ConsumablePrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    let (rooms, prize1, prize2) = crystal_path_geometry(inputs);
    let mut potions = Vec::with_capacity(3);
    let mut scrolls = Vec::with_capacity(3);
    let mut duplicates = Vec::new();
    let exotic_chance = exotic_crystals_chance(inputs.prizes.exotic_crystals_level());
    if inputs.random.int_bound(2) == 0 {
        add_crystal_reward(
            inputs,
            GeneratorCategory::Potion,
            &mut potions,
            &mut duplicates,
        )?;
        scrolls.push(direct_scroll(
            ScrollKind::Transmutation,
            inputs.random.float() < exotic_chance,
        ));
    } else {
        potions.push(SpecialItem::generated(GeneratedItem::Potion {
            kind: PotionKind::Experience,
            exotic: inputs.random.float() < exotic_chance,
        }));
        add_crystal_reward(
            inputs,
            GeneratorCategory::Scroll,
            &mut scrolls,
            &mut duplicates,
        )?;
    }
    add_crystal_reward(
        inputs,
        GeneratorCategory::Potion,
        &mut potions,
        &mut duplicates,
    )?;
    add_crystal_reward(
        inputs,
        GeneratorCategory::Scroll,
        &mut scrolls,
        &mut duplicates,
    )?;
    add_crystal_reward(
        inputs,
        GeneratorCategory::Potion,
        &mut potions,
        &mut duplicates,
    )?;
    add_crystal_reward(
        inputs,
        GeneratorCategory::Scroll,
        &mut scrolls,
        &mut duplicates,
    )?;
    // See `add_crystal_reward`: every official undo call is a no-op.
    let _ = duplicates;
    potions.sort_by_key(|item| Reverse(consumable_value(*item)));
    scrolls.sort_by_key(|item| Reverse(consumable_value(*item)));

    let shuffle = inputs.random.int_bound(2) == 1;
    let placements = [
        (potions.remove(0), if shuffle { 2 } else { 3 }),
        (scrolls.remove(0), if shuffle { 3 } else { 2 }),
        (potions.remove(0), usize::from(!shuffle)),
        (scrolls.remove(0), usize::from(shuffle)),
    ];
    let mut slot_items = [None; 6];
    for (item, room_index) in placements {
        let cell = point_to_cell(inputs.level, room_center(rooms[room_index], inputs.random));
        let slot = slot_for_room(room_index);
        slot_items[room_index] = Some(item);
        inputs.drop(
            cell,
            None,
            item.into(),
            ItemSource::Heap,
            ConsumableAccess::CrystalPath(slot),
            false,
        );
    }
    let final_placements = [
        (
            potions.remove(0),
            if shuffle { 4 } else { 5 },
            if shuffle { prize1 } else { prize2 },
        ),
        (
            scrolls.remove(0),
            if shuffle { 5 } else { 4 },
            if shuffle { prize2 } else { prize1 },
        ),
    ];
    for (item, room_index, cell) in final_placements {
        let slot = slot_for_room(room_index);
        slot_items[room_index] = Some(item);
        inputs.drop(
            cell,
            None,
            item.into(),
            ItemSource::Heap,
            ConsumableAccess::CrystalPath(slot),
            true,
        );
    }

    let group = inputs.prizes.allocate_choice_group();
    for (index, item) in slot_items.into_iter().enumerate() {
        let slot = slot_for_room(index);
        inputs.record_searchable(
            item.expect("every CrystalPath slot receives one reward")
                .into(),
            ItemSource::Heap,
            Accessibility::Scenarios {
                group,
                mask: crystal_path_scenario_mask(slot),
            },
        );
    }
    for _ in 0..3 {
        inputs.spawn(crystal_key(inputs.depth_i32()));
    }
    inputs.set_door(DoorType::Regular);
    Ok(())
}

/// `Room.canPlaceCharacter` for the handled consumable-special classes after
/// painting, including `ToxicGas` and `EternalFire` blob state.
#[must_use]
pub fn can_place_character(
    level: &Level,
    rooms: &[Room],
    room: RoomId,
    point: Point,
    state: &ConsumablePlacementState,
) -> bool {
    if !rooms[room].inside(point) {
        return false;
    }
    let cell = level.point_to_cell(point);
    match rooms[room].kind {
        RoomKind::Special(SpecialRoomKind::ToxicGas) => !state.toxic_gas_cells.contains(&cell),
        RoomKind::Special(SpecialRoomKind::MagicalFire) if !state.eternal_fire_cells.is_empty() => {
            if level.map.cells[cell] == terrain::EMPTY_SP
                || state.eternal_fire_cells.contains(&cell)
            {
                return false;
            }
            let neighbours = [
                Point::new(point.x - 1, point.y),
                Point::new(point.x + 1, point.y),
                Point::new(point.x, point.y - 1),
                Point::new(point.x, point.y + 1),
            ];
            !neighbours.iter().any(|neighbour| {
                state
                    .eternal_fire_cells
                    .contains(&level.point_to_cell(*neighbour))
            })
        }
        _ => true,
    }
}

#[must_use]
pub const fn can_place_grass(kind: RoomKind) -> bool {
    !matches!(
        kind,
        RoomKind::Special(SpecialRoomKind::MagicalFire | SpecialRoomKind::CrystalPath)
    )
}

#[must_use]
pub const fn can_place_water(kind: RoomKind) -> bool {
    !matches!(kind, RoomKind::Special(SpecialRoomKind::CrystalPath))
}

#[must_use]
pub const fn can_place_trap(kind: RoomKind) -> bool {
    !matches!(
        kind,
        RoomKind::Special(SpecialRoomKind::ToxicGas | SpecialRoomKind::CrystalPath)
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::generator::{FoodKind, StoneKind};
    use crate::level::Feeling;
    use crate::room::{ConnectionRoomKind, Door, RoomConnection};
    use crate::run::RunState;

    struct Fixture {
        map_hash: i32,
        door: DoorType,
        next: i64,
        events: Vec<ConsumablePaintEvent>,
        report: ConsumablePaintReport,
        pool: PrizePool,
        level: Level,
        rooms: Vec<Room>,
    }

    fn paint_fixture(kind: SpecialRoomKind, size: i32, door: Point) -> Fixture {
        paint_fixture_seed(kind, size, door, 0)
    }

    fn paint_fixture_seed(
        kind: SpecialRoomKind,
        size: i32,
        door: Point,
        outer_seed: i64,
    ) -> Fixture {
        let mut run = RunState::new(0);
        let mut random = RandomStack::with_base_seed(0);
        random.push(outer_seed);
        let mut level = Level::new(3, Feeling::None);
        level.set_size(13, 13);
        let mut events = Vec::new();
        let mut pool = PrizePool::empty(41);
        let mut special = Room::special(kind);
        special.bounds = Rect::new(2, 2, 2 + size - 1, 2 + size - 1);
        special.connected.push(RoomConnection {
            room: 1,
            door: Some(Door::new(door)),
        });
        let mut neighbour = Room::connection(ConnectionRoomKind::Tunnel);
        neighbour.connected.push(RoomConnection {
            room: 0,
            door: Some(Door::new(door)),
        });
        let mut rooms = vec![special, neighbour];
        let outcome = {
            let mut context = ConsumableLevelPaintContext::new(&mut level, &mut events);
            paint_consumable_special(
                &mut context,
                &mut rooms,
                0,
                &mut run.generator,
                &mut pool,
                &mut random,
            )
            .unwrap()
        };
        let ConsumablePaintOutcome::Painted(report) = outcome else {
            panic!("requested consumable special was not handled");
        };
        Fixture {
            map_hash: level.java_map_hash(),
            door: rooms[0].connected[0].door.unwrap().door_type,
            next: random.long(),
            events,
            report,
            pool,
            level,
            rooms,
        }
    }

    #[test]
    fn treasury_mimic_generation_matches_official_fixture() {
        let result = paint_fixture_seed(SpecialRoomKind::Treasury, 7, Point::new(2, 5), 4);
        assert_eq!(result.map_hash, -1_464_547_848);
        assert_eq!(result.next, 4_485_853_605_302_284_072);
        assert_eq!(event_hash(&result.events, result.level.len()), 61_121_673);
        let mimics: Vec<_> = result
            .events
            .iter()
            .filter_map(|event| match event {
                ConsumablePaintEvent::Mimic { cell, carried } => Some((*cell, carried)),
                _ => None,
            })
            .collect();
        assert_eq!(mimics.len(), 1);
        assert_eq!(mimics[0].0, 81);
        assert_eq!(mimics[0].1.len(), 2);
    }

    #[derive(Default)]
    struct FinalHeap {
        kind: Option<ConsumableHeapKind>,
        auto_explored: bool,
        items: Vec<ConsumableItem>,
    }

    fn merge_heap_item(items: &mut Vec<ConsumableItem>, item: ConsumableItem) {
        let ConsumableItem::Special(SpecialItem::Paint(PaintItem::Generated(
            GeneratedItem::Gold { quantity },
        ))) = item
        else {
            items.insert(0, item);
            return;
        };
        if let Some(index) = items.iter().position(|existing| {
            matches!(
                existing,
                ConsumableItem::Special(SpecialItem::Paint(PaintItem::Generated(
                    GeneratedItem::Gold { .. }
                )))
            )
        }) {
            let ConsumableItem::Special(SpecialItem::Paint(PaintItem::Generated(
                GeneratedItem::Gold { quantity: existing },
            ))) = &mut items[index]
            else {
                unreachable!()
            };
            *existing = existing.wrapping_add(quantity);
            let merged = items.remove(index);
            items.insert(0, merged);
        } else {
            items.insert(0, item);
        }
    }

    fn potion_name(kind: PotionKind, exotic: bool) -> &'static str {
        if exotic {
            return match kind {
                PotionKind::Experience => "PotionOfDivineInspiration",
                _ => panic!("fixture only directly exotics Potion of Experience"),
            };
        }
        match kind {
            PotionKind::Strength => "PotionOfStrength",
            PotionKind::Healing => "PotionOfHealing",
            PotionKind::MindVision => "PotionOfMindVision",
            PotionKind::Frost => "PotionOfFrost",
            PotionKind::LiquidFlame => "PotionOfLiquidFlame",
            PotionKind::ToxicGas => "PotionOfToxicGas",
            PotionKind::Haste => "PotionOfHaste",
            PotionKind::Invisibility => "PotionOfInvisibility",
            PotionKind::Levitation => "PotionOfLevitation",
            PotionKind::ParalyticGas => "PotionOfParalyticGas",
            PotionKind::Purity => "PotionOfPurity",
            PotionKind::Experience => "PotionOfExperience",
        }
    }

    fn scroll_name(kind: ScrollKind, exotic: bool) -> &'static str {
        if exotic {
            return match kind {
                ScrollKind::Transmutation => "ScrollOfMetamorphosis",
                _ => panic!("fixture only directly exotics Scroll of Transmutation"),
            };
        }
        match kind {
            ScrollKind::Upgrade => "ScrollOfUpgrade",
            ScrollKind::Identify => "ScrollOfIdentify",
            ScrollKind::RemoveCurse => "ScrollOfRemoveCurse",
            ScrollKind::MirrorImage => "ScrollOfMirrorImage",
            ScrollKind::Recharging => "ScrollOfRecharging",
            ScrollKind::Teleportation => "ScrollOfTeleportation",
            ScrollKind::Lullaby => "ScrollOfLullaby",
            ScrollKind::MagicMapping => "ScrollOfMagicMapping",
            ScrollKind::Rage => "ScrollOfRage",
            ScrollKind::Retribution => "ScrollOfRetribution",
            ScrollKind::Terror => "ScrollOfTerror",
            ScrollKind::Transmutation => "ScrollOfTransmutation",
        }
    }

    fn stone_name(kind: StoneKind) -> &'static str {
        match kind {
            StoneKind::Enchantment => "StoneOfEnchantment",
            StoneKind::Intuition => "StoneOfIntuition",
            StoneKind::DetectMagic => "StoneOfDetectMagic",
            StoneKind::Flock => "StoneOfFlock",
            StoneKind::Shock => "StoneOfShock",
            StoneKind::Blink => "StoneOfBlink",
            StoneKind::DeepSleep => "StoneOfDeepSleep",
            StoneKind::Clairvoyance => "StoneOfClairvoyance",
            StoneKind::Aggression => "StoneOfAggression",
            StoneKind::Blast => "StoneOfBlast",
            StoneKind::Fear => "StoneOfFear",
            StoneKind::Augmentation => "StoneOfAugmentation",
        }
    }

    fn item_text(item: ConsumableItem) -> String {
        let (class, level, cursed, effect, quantity) = match item {
            ConsumableItem::Honeypot => ("Honeypot".into(), 0, false, "none".into(), 1),
            ConsumableItem::Special(SpecialItem::IronKey { .. }) => {
                ("IronKey".into(), 0, false, "none".into(), 1)
            }
            ConsumableItem::Special(SpecialItem::CrystalKey { .. }) => {
                ("CrystalKey".into(), 0, false, "none".into(), 1)
            }
            ConsumableItem::Special(SpecialItem::PotionOfInvisibility) => {
                ("PotionOfInvisibility".into(), 0, false, "none".into(), 1)
            }
            ConsumableItem::Special(SpecialItem::PotionOfHaste) => {
                ("PotionOfHaste".into(), 0, false, "none".into(), 1)
            }
            ConsumableItem::Special(SpecialItem::Paint(PaintItem::ArcaneStylus)) => {
                ("ArcaneStylus".into(), 0, false, "none".into(), 1)
            }
            ConsumableItem::Special(SpecialItem::Paint(PaintItem::TrinketCatalyst)) => {
                ("TrinketCatalyst".into(), 0, false, "none".into(), 1)
            }
            ConsumableItem::Special(SpecialItem::Paint(PaintItem::Direct(item))) => (
                match item {
                    crate::level::DirectPaintItem::IronKey { .. } => "IronKey",
                    crate::level::DirectPaintItem::CrystalKey { .. } => "CrystalKey",
                    crate::level::DirectPaintItem::GoldenKey { .. } => "GoldenKey",
                    crate::level::DirectPaintItem::PotionOfInvisibility => "PotionOfInvisibility",
                    crate::level::DirectPaintItem::PotionOfHaste => "PotionOfHaste",
                    crate::level::DirectPaintItem::CeremonialCandle => "CeremonialCandle",
                    crate::level::DirectPaintItem::Torch => "Torch",
                    crate::level::DirectPaintItem::Other(name) => name,
                }
                .into(),
                0,
                false,
                "none".into(),
                1,
            ),
            ConsumableItem::Special(SpecialItem::Paint(PaintItem::Generated(item))) => match item {
                GeneratedItem::Equipment(equipment) => (
                    format!("{:?}", equipment.item),
                    i32::from(equipment.roll.upgrade),
                    equipment.roll.cursed,
                    equipment.roll.effect.map_or_else(
                        || "none".into(),
                        |effect| match effect {
                            Effect::Weapon(effect) => format!("{effect:?}"),
                            Effect::Armor(effect) => format!("{effect:?}"),
                        },
                    ),
                    1,
                ),
                GeneratedItem::Food(kind) => (
                    match kind {
                        FoodKind::Ration => "Food",
                        FoodKind::Pasty => "Pasty",
                        FoodKind::MysteryMeat => "MysteryMeat",
                    }
                    .into(),
                    0,
                    false,
                    "none".into(),
                    1,
                ),
                GeneratedItem::Potion { kind, exotic } => {
                    (potion_name(kind, exotic).into(), 0, false, "none".into(), 1)
                }
                GeneratedItem::Scroll { kind, exotic } => {
                    (scroll_name(kind, exotic).into(), 0, false, "none".into(), 1)
                }
                GeneratedItem::Stone(kind) => (stone_name(kind).into(), 0, false, "none".into(), 1),
                GeneratedItem::Gold { quantity } => {
                    ("Gold".into(), 0, false, "none".into(), quantity)
                }
                other => panic!("unsupported official fixture item: {other:?}"),
            },
        };
        format!(
            "{class}:+{level}:{}:{effect}:q{quantity}",
            if cursed { "cursed" } else { "clean" }
        )
    }

    fn heap_name(kind: Option<ConsumableHeapKind>) -> &'static str {
        match kind.unwrap_or(ConsumableHeapKind::Heap) {
            ConsumableHeapKind::Heap => "HEAP",
            ConsumableHeapKind::Chest => "CHEST",
            ConsumableHeapKind::Skeleton => "SKELETON",
        }
    }

    fn blob_name(kind: ConsumableBlobKind) -> &'static str {
        match kind {
            ConsumableBlobKind::Foliage => "Foliage",
            ConsumableBlobKind::WellWater(WellWaterKind::Awareness) => "WaterOfAwareness",
            ConsumableBlobKind::WellWater(WellWaterKind::Health) => "WaterOfHealth",
            ConsumableBlobKind::ToxicGas => "ToxicGas",
            ConsumableBlobKind::ToxicGasSeed => "ToxicGasSeed",
            ConsumableBlobKind::EternalFire => "EternalFire",
        }
    }

    fn trap_name(kind: ConsumableTrapKind) -> &'static str {
        match kind {
            ConsumableTrapKind::ToxicVent => "ToxicVent",
            ConsumableTrapKind::Gripping => "GrippingTrap",
            ConsumableTrapKind::Teleportation => "TeleportationTrap",
            ConsumableTrapKind::Flock => "FlockTrap",
            ConsumableTrapKind::PoisonDart => "PoisonDartTrap",
            ConsumableTrapKind::Explosive => "ExplosiveTrap",
            ConsumableTrapKind::Flashing => "FlashingTrap",
            ConsumableTrapKind::Warping => "WarpingTrap",
            ConsumableTrapKind::Disintegration => "DisintegrationTrap",
            ConsumableTrapKind::Grim => "GrimTrap",
        }
    }

    fn java_int_array_hash(values: &[i32]) -> i32 {
        values.iter().fold(1_i32, |hash, value| {
            hash.wrapping_mul(31).wrapping_add(*value)
        })
    }

    fn java_string_hash(value: &str) -> i32 {
        value.encode_utf16().fold(0_i32, |hash, unit| {
            hash.wrapping_mul(31).wrapping_add(i32::from(unit))
        })
    }

    fn event_hash(events: &[ConsumablePaintEvent], level_len: usize) -> i32 {
        let mut heaps = BTreeMap::<usize, FinalHeap>::new();
        let mut mobs = Vec::new();
        let mut spawns = Vec::new();
        let mut plants = BTreeMap::new();
        let mut traps = BTreeMap::new();
        let mut blobs = BTreeMap::<&str, (i32, Vec<i32>)>::new();
        for event in events {
            match event {
                ConsumablePaintEvent::Drop {
                    cell,
                    heap,
                    reward,
                    auto_explored,
                } => {
                    let final_heap = heaps.entry(*cell).or_default();
                    merge_heap_item(&mut final_heap.items, reward.item);
                    if heap.is_some() {
                        final_heap.kind = *heap;
                    }
                    final_heap.auto_explored |= *auto_explored;
                }
                ConsumablePaintEvent::Mimic { cell, carried } => {
                    mobs.push(format!(
                        "{cell}:Mimic:{}",
                        carried
                            .iter()
                            .map(|reward| item_text(reward.item))
                            .collect::<Vec<_>>()
                            .join(",")
                    ));
                }
                ConsumablePaintEvent::SpawnItem(item) => {
                    spawns.push(item_text((*item).into()));
                }
                ConsumablePaintEvent::Plant { cell, kind } => {
                    plants.insert(
                        *cell,
                        match kind {
                            ConsumablePlantKind::Sungrass => "Sungrass",
                            ConsumablePlantKind::Blandfruit => "BlandfruitBush",
                        },
                    );
                }
                ConsumablePaintEvent::Blob { cell, kind, volume } => {
                    let entry = blobs
                        .entry(blob_name(*kind))
                        .or_insert_with(|| (0, vec![0; level_len]));
                    entry.0 = entry.0.wrapping_add(*volume);
                    entry.1[*cell] = entry.1[*cell].wrapping_add(*volume);
                }
                ConsumablePaintEvent::Trap {
                    cell,
                    kind,
                    visible,
                    active,
                } => {
                    traps.insert(*cell, (trap_name(*kind), *visible, *active));
                }
            }
        }
        mobs.sort();
        let heap_text = heaps
            .iter()
            .map(|(cell, heap)| {
                format!(
                    "{cell}:{}:{}:{}",
                    heap_name(heap.kind),
                    heap.auto_explored,
                    heap.items
                        .iter()
                        .copied()
                        .map(item_text)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let plant_text = plants
            .iter()
            .map(|(cell, kind)| format!("{cell}:{kind}"))
            .collect::<Vec<_>>()
            .join(";");
        let trap_text = traps
            .iter()
            .map(|(cell, (kind, visible, active))| format!("{cell}:{kind}:{visible}:{active}"))
            .collect::<Vec<_>>()
            .join(";");
        let blob_text = blobs
            .iter()
            .map(|(kind, (volume, cells))| {
                format!("{kind}:{volume}:{}", java_int_array_hash(cells))
            })
            .collect::<Vec<_>>()
            .join(";");
        java_string_hash(&format!(
            "heaps=[{heap_text}]|mobs=[{}]|spawn=[{}]|plants=[{plant_text}]|traps=[{trap_text}]|blobs=[{blob_text}]",
            mobs.join(";"),
            spawns.join(",")
        ))
    }

    #[test]
    fn every_consumable_special_and_directional_branch_matches_official_map_rng() {
        let cases = [
            (
                SpecialRoomKind::Runestone,
                7,
                Point::new(2, 5),
                -186_491_666,
                -279_624_296_851_435_688_i64,
            ),
            (
                SpecialRoomKind::Garden,
                7,
                Point::new(2, 5),
                1_915_395_150,
                -7_261_648_964_369_397_258,
            ),
            (
                SpecialRoomKind::Library,
                7,
                Point::new(2, 5),
                -19_311_814,
                4_662_897_195_779_605_027,
            ),
            (
                SpecialRoomKind::Storage,
                7,
                Point::new(2, 5),
                -1_567_033_171,
                6_787_954_838_522_539_928,
            ),
            (
                SpecialRoomKind::Treasury,
                7,
                Point::new(2, 5),
                -1_464_547_848,
                -8_062_155_292_093_192_501,
            ),
            (
                SpecialRoomKind::MagicWell,
                7,
                Point::new(2, 5),
                -1_980_068_297,
                -3_109_364_765_729_502_342,
            ),
            (
                SpecialRoomKind::ToxicGas,
                7,
                Point::new(2, 5),
                -933_689_846,
                5_058_965_686_473_586_628,
            ),
            (
                SpecialRoomKind::MagicalFire,
                7,
                Point::new(2, 5),
                1_423_957_581,
                -1_083_761_183_081_836_303,
            ),
            (
                SpecialRoomKind::Traps,
                7,
                Point::new(2, 5),
                -1_447_616_598,
                6_146_794_652_083_548_235,
            ),
            (
                SpecialRoomKind::CrystalPath,
                7,
                Point::new(2, 5),
                -488_291_316,
                -279_624_296_851_435_688,
            ),
            (
                SpecialRoomKind::MagicalFire,
                7,
                Point::new(8, 5),
                1_846_867_725,
                -1_083_761_183_081_836_303,
            ),
            (
                SpecialRoomKind::MagicalFire,
                7,
                Point::new(5, 2),
                586_815_949,
                -1_083_761_183_081_836_303,
            ),
            (
                SpecialRoomKind::MagicalFire,
                7,
                Point::new(5, 8),
                1_153_457_037,
                -1_083_761_183_081_836_303,
            ),
            (
                SpecialRoomKind::Traps,
                7,
                Point::new(8, 5),
                1_382_054_698,
                6_146_794_652_083_548_235,
            ),
            (
                SpecialRoomKind::Traps,
                7,
                Point::new(5, 2),
                1_948_523_178,
                6_146_794_652_083_548_235,
            ),
            (
                SpecialRoomKind::Traps,
                7,
                Point::new(5, 8),
                -2_027_044_822,
                6_146_794_652_083_548_235,
            ),
            (
                SpecialRoomKind::CrystalPath,
                8,
                Point::new(9, 5),
                905_542_746,
                -279_624_296_851_435_688,
            ),
            (
                SpecialRoomKind::CrystalPath,
                9,
                Point::new(6, 2),
                -275_525_238,
                2_377_732_757_510_138_102,
            ),
            (
                SpecialRoomKind::CrystalPath,
                7,
                Point::new(5, 8),
                -390_331_060,
                -279_624_296_851_435_688,
            ),
        ];
        for (kind, size, door, map, next) in cases {
            let result = paint_fixture(kind, size, door);
            assert_eq!(result.map_hash, map, "{kind:?} {size} {door:?} map");
            assert_eq!(result.next, next, "{kind:?} {size} {door:?} RNG");
            assert_eq!(
                result.door,
                match kind {
                    SpecialRoomKind::Storage => DoorType::Barricade,
                    SpecialRoomKind::Runestone
                    | SpecialRoomKind::Garden
                    | SpecialRoomKind::Library
                    | SpecialRoomKind::Treasury
                    | SpecialRoomKind::MagicWell => DoorType::Locked,
                    _ => DoorType::Regular,
                }
            );
        }
    }

    #[test]
    fn every_consumable_special_matches_official_typed_event_fixture() {
        let cases = [
            (
                SpecialRoomKind::Runestone,
                7,
                Point::new(2, 5),
                -836_255_628,
            ),
            (SpecialRoomKind::Garden, 7, Point::new(2, 5), -366_377_162),
            (
                SpecialRoomKind::Library,
                7,
                Point::new(2, 5),
                -1_228_133_405,
            ),
            (SpecialRoomKind::Storage, 7, Point::new(2, 5), 1_716_455_818),
            (
                SpecialRoomKind::Treasury,
                7,
                Point::new(2, 5),
                -1_270_961_250,
            ),
            (
                SpecialRoomKind::MagicWell,
                7,
                Point::new(2, 5),
                1_843_141_694,
            ),
            (
                SpecialRoomKind::ToxicGas,
                7,
                Point::new(2, 5),
                -1_635_226_644,
            ),
            (
                SpecialRoomKind::MagicalFire,
                7,
                Point::new(2, 5),
                334_109_207,
            ),
            (SpecialRoomKind::Traps, 7, Point::new(2, 5), -2_007_553_542),
            (
                SpecialRoomKind::CrystalPath,
                7,
                Point::new(2, 5),
                996_755_303,
            ),
            (
                SpecialRoomKind::MagicalFire,
                7,
                Point::new(8, 5),
                -1_944_830_113,
            ),
            (
                SpecialRoomKind::MagicalFire,
                7,
                Point::new(5, 2),
                -946_004_752,
            ),
            (
                SpecialRoomKind::MagicalFire,
                7,
                Point::new(5, 8),
                -640_192_447,
            ),
            (SpecialRoomKind::Traps, 7, Point::new(8, 5), 1_959_202_312),
            (SpecialRoomKind::Traps, 7, Point::new(5, 2), -1_149_182_126),
            (SpecialRoomKind::Traps, 7, Point::new(5, 8), -2_086_916_828),
            (
                SpecialRoomKind::CrystalPath,
                8,
                Point::new(9, 5),
                -843_206_171,
            ),
            (
                SpecialRoomKind::CrystalPath,
                9,
                Point::new(6, 2),
                -952_157_545,
            ),
            (
                SpecialRoomKind::CrystalPath,
                7,
                Point::new(5, 8),
                -1_428_415_559,
            ),
        ];
        for (kind, size, door, expected) in cases {
            let result = paint_fixture(kind, size, door);
            assert_eq!(
                event_hash(&result.events, result.level.len()),
                expected,
                "{kind:?} {size} {door:?} events"
            );
            let spawned: Vec<_> = result
                .events
                .iter()
                .filter_map(|event| match event {
                    ConsumablePaintEvent::SpawnItem(item) => Some(*item),
                    _ => None,
                })
                .collect();
            assert_eq!(result.pool.items_to_spawn, spawned);
        }
    }

    #[test]
    fn crystal_path_scenarios_are_the_exact_three_key_feasible_sets() {
        let mut enumerated = Vec::new();
        for subset in 0_u8..64 {
            if subset.count_ones() != 3 {
                continue;
            }
            let final_a_valid = subset & (1 << CrystalPathSlot::FinalA as u8) == 0
                || subset & (1 << CrystalPathSlot::SecondA as u8) != 0;
            let final_b_valid = subset & (1 << CrystalPathSlot::FinalB as u8) == 0
                || subset & (1 << CrystalPathSlot::SecondB as u8) != 0;
            if final_a_valid && final_b_valid {
                enumerated.push(subset);
            }
        }
        assert_eq!(CRYSTAL_PATH_PLANS, enumerated.as_slice());
        assert_eq!(
            [
                crystal_path_scenario_mask(CrystalPathSlot::FirstA),
                crystal_path_scenario_mask(CrystalPathSlot::FirstB),
                crystal_path_scenario_mask(CrystalPathSlot::SecondA),
                crystal_path_scenario_mask(CrystalPathSlot::SecondB),
                crystal_path_scenario_mask(CrystalPathSlot::FinalA),
                crystal_path_scenario_mask(CrystalPathSlot::FinalB),
            ],
            [151, 299, 637, 974, 112, 896]
        );

        let result = paint_fixture(SpecialRoomKind::CrystalPath, 7, Point::new(2, 5));
        let mut slots: Vec<_> = result
            .events
            .iter()
            .filter_map(|event| match event {
                ConsumablePaintEvent::Drop {
                    reward:
                        ConsumableReward {
                            access: ConsumableAccess::CrystalPath(slot),
                            ..
                        },
                    ..
                } => Some(*slot),
                _ => None,
            })
            .collect();
        slots.sort_unstable_by_key(|slot| *slot as u8);
        assert_eq!(
            slots,
            [
                CrystalPathSlot::FirstA,
                CrystalPathSlot::FirstB,
                CrystalPathSlot::SecondA,
                CrystalPathSlot::SecondB,
                CrystalPathSlot::FinalA,
                CrystalPathSlot::FinalB,
            ]
        );
        assert_eq!(result.pool.next_choice_group, 42);
    }

    #[test]
    fn consumable_placement_overrides_match_blob_and_room_rules() {
        let toxic = paint_fixture(SpecialRoomKind::ToxicGas, 7, Point::new(2, 5));
        let gas_cell = toxic.report.placement.toxic_gas_cells[0];
        let gas_point = toxic.level.map.cell_to_point(gas_cell);
        assert!(!can_place_character(
            &toxic.level,
            &toxic.rooms,
            0,
            gas_point,
            &toxic.report.placement,
        ));
        assert!(can_place_trap(RoomKind::Special(SpecialRoomKind::Garden)));
        assert!(!can_place_trap(RoomKind::Special(
            SpecialRoomKind::ToxicGas
        )));

        let fire = paint_fixture(SpecialRoomKind::MagicalFire, 7, Point::new(2, 5));
        let fire_cell = fire.report.placement.eternal_fire_cells[0];
        let fire_point = fire.level.map.cell_to_point(fire_cell);
        assert!(!can_place_character(
            &fire.level,
            &fire.rooms,
            0,
            fire_point,
            &fire.report.placement,
        ));
        assert!(!can_place_grass(RoomKind::Special(
            SpecialRoomKind::MagicalFire
        )));
        assert!(!can_place_water(RoomKind::Special(
            SpecialRoomKind::CrystalPath
        )));
    }

    #[test]
    fn match_specific_prizes_preserve_insertion_order_without_rng() {
        let scroll = direct_scroll(ScrollKind::Identify, false);
        let stone = SpecialItem::generated(GeneratedItem::Stone(StoneKind::Blink));
        let mut pool = PrizePool::empty(9);
        pool.items_to_spawn = vec![
            SpecialItem::IronKey { depth: 3 },
            scroll,
            stone,
            SpecialItem::Paint(PaintItem::TrinketCatalyst),
        ];
        let mut random = RandomStack::with_base_seed(0);
        random.push(91);
        let mut checkpoint = random.clone();
        assert_eq!(pool.take_matching(ConsumablePrizeMatch::Stone), Some(stone));
        assert_eq!(
            pool.take_matching(ConsumablePrizeMatch::Scroll),
            Some(scroll)
        );
        assert_eq!(random.long(), checkpoint.long());
        assert_eq!(
            pool.take_prize_item(&mut random),
            Some(SpecialItem::Paint(PaintItem::TrinketCatalyst))
        );
        assert_eq!(pool.items_to_spawn, [SpecialItem::IronKey { depth: 3 }]);
    }

    #[test]
    fn unhandled_rooms_leave_every_context_untouched() {
        for kind in [
            SpecialRoomKind::WeakFloor,
            SpecialRoomKind::Laboratory,
            SpecialRoomKind::Pit,
            SpecialRoomKind::Shop,
            SpecialRoomKind::DemonSpawner,
        ] {
            let mut run = RunState::new(0);
            let expected_generator = run.generator.clone();
            let mut level = Level::new(3, Feeling::None);
            level.set_size(13, 13);
            let expected_level = level.clone();
            let mut events = Vec::new();
            let mut pool = PrizePool::empty(9);
            let expected_pool = pool.clone();
            let mut rooms = vec![Room::special(kind)];
            let mut random = RandomStack::with_base_seed(0);
            random.push(17);
            let mut expected_random = random.clone();
            let outcome = {
                let mut context = ConsumableLevelPaintContext::new(&mut level, &mut events);
                paint_consumable_special(
                    &mut context,
                    &mut rooms,
                    0,
                    &mut run.generator,
                    &mut pool,
                    &mut random,
                )
                .unwrap()
            };
            assert_eq!(outcome, ConsumablePaintOutcome::NotHandled);
            assert_eq!(level, expected_level);
            assert_eq!(run.generator, expected_generator);
            assert_eq!(pool, expected_pool);
            assert!(events.is_empty());
            assert_eq!(random.long(), expected_random.long());
        }
    }
}
