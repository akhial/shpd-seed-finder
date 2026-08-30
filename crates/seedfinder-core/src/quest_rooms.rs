//! Exact v4.0.0 painters for regular quest rooms and `DemonSpawnerRoom`.
//!
//! These rooms are appended by quest schedulers rather than either shuffled
//! `SpecialRoom` deck.  Their paint-time side effects nevertheless share the
//! same three integration boundaries as the special-room ports: one terrain
//! and occupancy sink, one run-global `Generator` state, and one ordered
//! `Level.itemsToSpawn` queue.  Direct quest objects use [`RegularItem`], whose
//! queued variants already cover keys, candles, and guaranteed potions.
//!
//! Two upstream details are deliberately visible here. `MassGraveRoom` calls
//! `Random.Int(2)` from its `for` loop condition on every condition check, and
//! `RotGardenRoom` uses the reverse Fisher-Yates `Collections.shuffle` overload
//! before trimming heart candidates. Both alter every later draw on the floor.

#![allow(
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

use std::fmt;

use crate::generator::{self, GeneratedItem, GeneratorError};
use crate::geometry::{GridMap, PathFinder, Point, Rect, painter as draw, terrain};
use crate::java_math::div_i32;
use crate::level::{Level, PlacedTrap, TrapKind, TrapSpec};
use crate::model::{Accessibility, ItemSource, WorldItem};
use crate::painter::{set_shared_door_type, standard_can_merge};
use crate::regular_items::{QueuedItemKind, RegularItem};
use crate::rng::RandomStack;
use crate::room::{DoorType, QuestRoomKind, Room, RoomId, RoomKind, SpecialRoomKind};
use crate::run::{GeneratorCategory, GeneratorState};

const BLACKSMITH_CATEGORIES: [GeneratorCategory; 3] = [
    GeneratorCategory::Armor,
    GeneratorCategory::Weapon,
    GeneratorCategory::Missile,
];

/// Heap identity after the room's chained `Level.drop` mutations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QuestHeapKind {
    Heap,
    Skeleton,
}

/// Generation-visible actor classes created during quest-room painting.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QuestMobKind {
    Skeleton,
    RotHeart,
    RotLasher,
    Blacksmith,
    Imp,
    DemonSpawner { spawn_recorded: bool },
}

/// Rendering-only features retained because their bounds encode room geometry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QuestFeatureKind {
    MassGraveBones,
    RitualMarker,
    BlacksmithEntrance,
    ImpEntrance,
    ImpBarrier,
    DemonSpawnerFloor,
}

/// The two branch exits created by Blacksmith and Ambitious Imp rooms.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QuestBranchTransition {
    pub cell: usize,
    pub depth: u32,
    pub branch: u32,
}

/// Ordered paint-time effects which are not represented directly by `Level`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QuestPaintEvent {
    Drop {
        cell: usize,
        heap: QuestHeapKind,
        haunted: bool,
        item: RegularItem,
    },
    Mob {
        cell: usize,
        kind: QuestMobKind,
    },
    SpawnItem(RegularItem),
    Trap {
        cell: usize,
        kind: TrapKind,
        visible: bool,
        active: bool,
    },
    Transition(QuestBranchTransition),
    Feature {
        point: Point,
        width: i32,
        height: i32,
        kind: QuestFeatureKind,
    },
}

/// Run/static state mutated by these painters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QuestPaintState {
    /// `CeremonialCandle.ritualPos`; `None` is the pre-paint sentinel.
    pub ritual_position: Option<usize>,
    /// `Statistics.spawnersAlive` for the generated run.
    pub spawners_alive: i32,
}

/// Search- and placement-visible output from one handled room.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QuestPaintReport {
    pub searchable_items: Vec<WorldItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QuestPaintOutcome {
    NotHandled,
    Painted(QuestPaintReport),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestPaintError {
    MissingRoom(RoomId),
    MissingDoor { room: RoomId, neighbour: RoomId },
    MissingReverseDoor { room: RoomId, neighbour: RoomId },
    MissingEntrance(RoomId),
    Generator(GeneratorError),
}

impl fmt::Display for QuestPaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::MissingRoom(room) => write!(formatter, "room {room} does not exist"),
            Self::MissingDoor { room, neighbour } => {
                write!(formatter, "room {room} has no placed door to {neighbour}")
            }
            Self::MissingReverseDoor { room, neighbour } => write!(
                formatter,
                "room {room} door has no reverse connection from {neighbour}"
            ),
            Self::MissingEntrance(room) => write!(formatter, "room {room} has no entrance"),
            Self::Generator(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for QuestPaintError {}

impl From<GeneratorError> for QuestPaintError {
    fn from(error: GeneratorError) -> Self {
        Self::Generator(error)
    }
}

/// Terrain/occupancy sink compatible with the special-room painter adapters.
pub trait QuestLevelContext {
    fn depth(&self) -> u32;
    fn map(&self) -> &GridMap;
    fn map_mut(&mut self) -> &mut GridMap;
    fn has_heap(&self, cell: usize) -> bool;
    fn has_mob(&self, cell: usize) -> bool;
    fn record(&mut self, event: QuestPaintEvent);
}

/// Adapter for the shared regular-painter `Level` plus a typed event stream.
pub struct QuestLevelPaintContext<'a> {
    pub level: &'a mut Level,
    pub events: &'a mut Vec<QuestPaintEvent>,
}

impl<'a> QuestLevelPaintContext<'a> {
    #[must_use]
    pub const fn new(level: &'a mut Level, events: &'a mut Vec<QuestPaintEvent>) -> Self {
        Self { level, events }
    }
}

impl QuestLevelContext for QuestLevelPaintContext<'_> {
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

    fn record(&mut self, event: QuestPaintEvent) {
        match &event {
            QuestPaintEvent::Drop { cell, .. } => self.level.mark_heap(*cell),
            QuestPaintEvent::Mob { cell, .. } => self.level.mark_mob(*cell),
            QuestPaintEvent::Trap {
                cell,
                kind,
                visible,
                active,
            } => self.level.set_trap(PlacedTrap {
                spec: TrapSpec::new(*kind),
                cell: *cell,
                visible: *visible,
                active: *active,
            }),
            QuestPaintEvent::SpawnItem(_)
            | QuestPaintEvent::Transition(_)
            | QuestPaintEvent::Feature { .. } => {}
        }
        self.events.push(event);
    }
}

/// Exact generator call sites used by Mass Grave and Blacksmith rooms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestGeneratorRequest {
    Overall,
    Category(GeneratorCategory),
}

pub trait QuestGeneratorContext {
    /// # Errors
    ///
    /// Returns the underlying version-pinned generator invariant failure.
    fn generate(
        &mut self,
        request: QuestGeneratorRequest,
        depth: i32,
        random: &mut RandomStack,
    ) -> Result<GeneratedItem, GeneratorError>;
}

impl QuestGeneratorContext for GeneratorState {
    fn generate(
        &mut self,
        request: QuestGeneratorRequest,
        depth: i32,
        random: &mut RandomStack,
    ) -> Result<GeneratedItem, GeneratorError> {
        match request {
            QuestGeneratorRequest::Overall => generator::random(random, self, depth),
            QuestGeneratorRequest::Category(category) => {
                generator::random_category(random, self, category, depth)
            }
        }
    }
}

/// Ordered `Level.itemsToSpawn` mutation used by quest painters.
pub trait QuestPrizeContext {
    fn add_item_to_spawn(&mut self, item: RegularItem);
}

impl QuestPrizeContext for Vec<RegularItem> {
    fn add_item_to_spawn(&mut self, item: RegularItem) {
        self.push(item);
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
    state: &'a mut QuestPaintState,
    rooms: &'a mut [Room],
    room: RoomId,
    bounds: Rect,
    entrance: Option<Entrance>,
    random: &'a mut RandomStack,
    report: QuestPaintReport,
}

impl<L, G, P> PaintInputs<'_, L, G, P>
where
    L: QuestLevelContext,
    G: QuestGeneratorContext,
    P: QuestPrizeContext,
{
    fn depth_i32(&self) -> i32 {
        i32::try_from(self.level.depth()).expect("main-dungeon depth fits Java int")
    }

    fn entrance(&self) -> Entrance {
        self.entrance
            .expect("handled one-connection room has entrance")
    }

    fn set_entrance_door(&mut self, door_type: DoorType) {
        let entrance = self.entrance();
        set_shared_door_type(self.rooms, self.room, entrance.neighbour, door_type);
    }

    fn set_all_doors(&mut self, door_type: DoorType) {
        let neighbours: Vec<RoomId> = self.rooms[self.room]
            .connected
            .iter()
            .map(|connection| connection.room)
            .collect();
        for neighbour in neighbours {
            set_shared_door_type(self.rooms, self.room, neighbour, door_type);
        }
    }

    fn spawn(&mut self, item: RegularItem) {
        self.prizes.add_item_to_spawn(item);
        self.level.record(QuestPaintEvent::SpawnItem(item));
    }

    fn generate(&mut self, request: QuestGeneratorRequest) -> Result<RegularItem, GeneratorError> {
        self.generator
            .generate(request, self.depth_i32(), self.random)
            .map(RegularItem::Generated)
    }

    fn drop(&mut self, cell: usize, heap: QuestHeapKind, item: RegularItem, haunted: bool) {
        if let Some(world_item) = searchable_world_item(
            item,
            u8::try_from(self.level.depth()).expect("main-dungeon depth fits u8"),
        ) {
            self.report.searchable_items.push(world_item);
        }
        self.level.record(QuestPaintEvent::Drop {
            cell,
            heap,
            haunted,
            item,
        });
    }

    fn mob(&mut self, cell: usize, kind: QuestMobKind) {
        self.level.record(QuestPaintEvent::Mob { cell, kind });
    }

    fn feature(&mut self, point: Point, width: i32, height: i32, kind: QuestFeatureKind) {
        self.level.record(QuestPaintEvent::Feature {
            point,
            width,
            height,
            kind,
        });
    }
}

/// Paint one of the five `rooms.quest` classes or Halls' mandatory spawner.
///
/// # Errors
///
/// Returns a graph-door invariant or generator invariant failure.
pub fn paint_quest_room<L, G, P>(
    level: &mut L,
    rooms: &mut [Room],
    room: RoomId,
    generator: &mut G,
    prizes: &mut P,
    state: &mut QuestPaintState,
    random: &mut RandomStack,
) -> Result<QuestPaintOutcome, QuestPaintError>
where
    L: QuestLevelContext,
    G: QuestGeneratorContext,
    P: QuestPrizeContext,
{
    let selected = rooms.get(room).ok_or(QuestPaintError::MissingRoom(room))?;
    let handled = matches!(selected.kind, RoomKind::Quest(_))
        || selected.kind == RoomKind::Special(SpecialRoomKind::DemonSpawner);
    if !handled {
        return Ok(QuestPaintOutcome::NotHandled);
    }

    for connection in &selected.connected {
        if connection.door.is_none() {
            return Err(QuestPaintError::MissingDoor {
                room,
                neighbour: connection.room,
            });
        }
        if rooms
            .get(connection.room)
            .and_then(|neighbour| neighbour.connection_to(room))
            .and_then(|reverse| reverse.door)
            .is_none()
        {
            return Err(QuestPaintError::MissingReverseDoor {
                room,
                neighbour: connection.room,
            });
        }
    }
    let entrance = selected.connected.first().map(|connection| Entrance {
        neighbour: connection.room,
        point: connection.door.expect("validated door").point,
    });
    if requires_entrance(selected.kind) && entrance.is_none() {
        return Err(QuestPaintError::MissingEntrance(room));
    }

    let kind = selected.kind;
    let bounds = selected.bounds;
    let mut inputs = PaintInputs {
        level,
        generator,
        prizes,
        state,
        rooms,
        room,
        bounds,
        entrance,
        random,
        report: QuestPaintReport::default(),
    };
    match kind {
        RoomKind::Quest(QuestRoomKind::MassGrave) => paint_mass_grave(&mut inputs)?,
        RoomKind::Quest(QuestRoomKind::RitualSite) => paint_ritual_site(&mut inputs),
        RoomKind::Quest(QuestRoomKind::RotGarden) => paint_rot_garden(&mut inputs),
        RoomKind::Quest(QuestRoomKind::Blacksmith) => paint_blacksmith(&mut inputs)?,
        RoomKind::Quest(QuestRoomKind::AmbitiousImp) => paint_ambitious_imp(&mut inputs),
        RoomKind::Special(SpecialRoomKind::DemonSpawner) => paint_demon_spawner(&mut inputs),
        _ => unreachable!("handled quest-room predicate and dispatch disagree"),
    }
    Ok(QuestPaintOutcome::Painted(inputs.report))
}

const fn requires_entrance(kind: RoomKind) -> bool {
    matches!(
        kind,
        RoomKind::Quest(
            QuestRoomKind::MassGrave | QuestRoomKind::RotGarden | QuestRoomKind::AmbitiousImp
        ) | RoomKind::Special(SpecialRoomKind::DemonSpawner)
    )
}

fn inclusive_width(bounds: Rect) -> i32 {
    bounds.right.wrapping_sub(bounds.left).wrapping_add(1)
}

fn inclusive_height(bounds: Rect) -> i32 {
    bounds.bottom.wrapping_sub(bounds.top).wrapping_add(1)
}

fn fill_room<L: QuestLevelContext>(level: &mut L, bounds: Rect, value: i32) {
    draw::fill(
        level.map_mut(),
        bounds.left,
        bounds.top,
        inclusive_width(bounds),
        inclusive_height(bounds),
        value,
    );
}

fn fill_room_margin<L: QuestLevelContext>(level: &mut L, bounds: Rect, margin: i32, value: i32) {
    draw::fill(
        level.map_mut(),
        bounds.left.wrapping_add(margin),
        bounds.top.wrapping_add(margin),
        inclusive_width(bounds).wrapping_sub(margin.wrapping_mul(2)),
        inclusive_height(bounds).wrapping_sub(margin.wrapping_mul(2)),
        value,
    );
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

fn fixed_room_center(bounds: Rect) -> Point {
    Point::new(
        div_i32(bounds.left.wrapping_add(bounds.right), 2),
        div_i32(bounds.top.wrapping_add(bounds.bottom), 2),
    )
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

fn point_to_cell<L: QuestLevelContext>(level: &L, point: Point) -> usize {
    level.map().point_to_cell(point)
}

fn searchable_world_item(item: RegularItem, depth: u8) -> Option<WorldItem> {
    let RegularItem::Generated(generated) = item else {
        return None;
    };
    let equipment = generated.searchable_equipment()?;
    Some(WorldItem::from_equipment_roll(
        equipment.item,
        equipment.roll,
        depth,
        ItemSource::Heap,
        Accessibility::Independent,
    ))
}

fn is_cursed(item: RegularItem) -> bool {
    match item {
        RegularItem::Queued(QueuedItemKind::Other("CorpseDust")) => true,
        RegularItem::Generated(GeneratedItem::Equipment(item)) => item.roll.cursed,
        RegularItem::Generated(GeneratedItem::Missile(item)) => item.roll.cursed,
        RegularItem::Generated(GeneratedItem::Ring(item)) => item.roll.cursed,
        RegularItem::Generated(GeneratedItem::Artifact(item)) => item.cursed,
        _ => false,
    }
}

fn paint_mass_grave<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: QuestLevelContext,
    G: QuestGeneratorContext,
    P: QuestPrizeContext,
{
    inputs.set_entrance_door(DoorType::Barricade);
    inputs.spawn(RegularItem::Queued(QueuedItemKind::PotionOfLiquidFlame));

    let bounds = inputs.bounds;
    fill_room(inputs.level, bounds, terrain::WALL);
    fill_room_margin(inputs.level, bounds, 1, terrain::CUSTOM_DECO_EMPTY);
    // v4.0.0 fixed 11x10 layout: two recessed top corners, a wall banner, and
    // two raised statues flanking the top opening.
    {
        let map = inputs.level.map_mut();
        draw::fill(map, bounds.left + 1, bounds.top + 1, 3, 1, terrain::WALL);
        draw::fill(map, bounds.left + 1, bounds.top + 2, 2, 1, terrain::WALL);
        draw::fill(map, bounds.right - 3, bounds.top + 1, 3, 1, terrain::WALL);
        draw::fill(map, bounds.right - 2, bounds.top + 2, 2, 1, terrain::WALL);
        map.set(bounds.left + 5, bounds.top, terrain::WALL_DECO);
        map.set(bounds.left + 3, bounds.top + 2, terrain::STATUE);
        map.set(bounds.right - 3, bounds.top + 2, terrain::STATUE);
    }
    inputs.feature(
        Point::new(bounds.left.wrapping_add(1), bounds.top),
        inclusive_width(bounds).wrapping_sub(2),
        inclusive_height(bounds).wrapping_sub(1),
        QuestFeatureKind::MassGraveBones,
    );

    // The upstream `for` condition reevaluates Random.Int(2), including its
    // final false check. This is not equivalent to drawing a count once.
    let mut index = 0;
    loop {
        if index > inputs.random.int_bound(2) {
            break;
        }
        let cell = loop {
            let point = mass_grave_point(bounds, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if point.y <= bounds.top + 3 && !inputs.level.has_mob(cell) {
                break cell;
            }
        };
        inputs.mob(cell, QuestMobKind::Skeleton);
        index += 1;
    }

    let mut items = vec![
        RegularItem::Queued(QueuedItemKind::Other("CorpseDust")),
        RegularItem::Generated(GeneratedItem::Gold { quantity: 1 }),
        RegularItem::Generated(GeneratedItem::Gold { quantity: 1 }),
    ];
    if inputs.random.float() <= 0.3_f32 {
        items.push(RegularItem::Generated(GeneratedItem::Gold { quantity: 1 }));
    }
    if inputs.random.float() <= 0.3_f32 {
        items.push(RegularItem::Generated(GeneratedItem::Gold { quantity: 1 }));
    }
    if inputs.random.float() <= 0.6_f32 {
        items.push(inputs.generate(QuestGeneratorRequest::Overall)?);
    }
    if inputs.random.float() <= 0.3_f32 {
        items.push(inputs.generate(QuestGeneratorRequest::Category(GeneratorCategory::Armor))?);
    }

    for item in items {
        let cell = loop {
            let point = mass_grave_point(bounds, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if point.y <= bounds.top + 5 && !inputs.level.has_heap(cell) {
                break cell;
            }
        };
        inputs.drop(cell, QuestHeapKind::Skeleton, item, is_cursed(item));
    }
    Ok(())
}

/// One v4.0.0 `MassGraveRoom` placement candidate: `random(1)`, then a
/// narrowed x re-roll for the two rows inside the recessed corners.
fn mass_grave_point(bounds: Rect, random: &mut RandomStack) -> Point {
    let mut point = random_room_point(bounds, 1, random);
    if point.y <= bounds.top + 2 {
        point.x = random.int_range(bounds.left + 4, bounds.right - 4);
    }
    point
}

fn paint_ritual_site<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: QuestLevelContext,
    G: QuestGeneratorContext,
    P: QuestPrizeContext,
{
    inputs.set_all_doors(DoorType::Regular);
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);

    let center = room_center(inputs.bounds, inputs.random);
    let marker = Point::new(center.x.wrapping_sub(1), center.y.wrapping_sub(1));
    inputs.feature(marker, 3, 3, QuestFeatureKind::RitualMarker);
    draw::fill(
        inputs.level.map_mut(),
        marker.x,
        marker.y,
        3,
        3,
        terrain::CUSTOM_DECO_EMPTY,
    );
    for _ in 0..4 {
        inputs.spawn(RegularItem::Queued(QueuedItemKind::CeremonialCandle));
    }
    inputs.state.ritual_position = Some(point_to_cell(inputs.level, center));
}

fn paint_rot_garden<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: QuestLevelContext,
    G: QuestGeneratorContext,
    P: QuestPrizeContext,
{
    inputs.set_entrance_door(DoorType::Locked);
    inputs.spawn(RegularItem::Queued(QueuedItemKind::IronKey {
        depth: u8::try_from(inputs.level.depth()).expect("main-dungeon depth fits u8"),
    }));

    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    let entrance = inputs.entrance().point;
    draw::set_point(inputs.level.map_mut(), entrance, terrain::LOCKED_DOOR);
    let entry_cell = point_to_cell(inputs.level, entrance);
    let mut finder = PathFinder::new(inputs.level.map().width, inputs.level.map().height);

    let (mut passable, candidates) = loop {
        fill_room_margin(inputs.level, inputs.bounds, 1, terrain::HIGH_GRASS);
        for _ in 0..12 {
            let point = random_room_point(inputs.bounds, 1, inputs.random);
            draw::set_point(inputs.level.map_mut(), point, terrain::WALL);
        }
        for _ in 0..8 {
            let point = random_room_point(inputs.bounds, 2, inputs.random);
            draw::set_point(inputs.level.map_mut(), point, terrain::WALL);
        }
        for _ in 0..4 {
            let point = random_room_point(inputs.bounds, 3, inputs.random);
            draw::set_point(inputs.level.map_mut(), point, terrain::WALL);
        }
        draw::draw_inside(
            inputs.level.map_mut(),
            inputs.bounds,
            entrance,
            3,
            terrain::HIGH_GRASS,
        );

        let passable: Vec<bool> = inputs
            .level
            .map()
            .cells
            .iter()
            .map(|&cell| cell != terrain::WALL)
            .collect();
        finder.build_distance_map(entry_cell, &passable);
        let mut candidates = Vec::new();
        let mut open_cells = 0;
        for x in inputs.bounds.left..=inputs.bounds.right {
            for y in inputs.bounds.top..=inputs.bounds.bottom {
                let cell = inputs.level.map().cell(x, y);
                if finder.distance[cell] != i32::MAX {
                    open_cells += 1;
                    if finder.distance[cell] >= 7 {
                        candidates.push(cell);
                    }
                } else if inputs.level.map().cells[cell] == terrain::HIGH_GRASS {
                    inputs.level.map_mut().cells[cell] = terrain::WALL;
                }
            }
        }

        inputs.random.shuffle_list(&mut candidates);
        let mut closest = 7;
        while candidates.len() > 5 {
            let mut index = 0;
            while index < candidates.len() && candidates.len() > 5 {
                if finder.distance[candidates[index]] == closest {
                    candidates.remove(index);
                } else {
                    index += 1;
                }
            }
            closest += 1;
        }
        if !candidates.is_empty() && open_cells >= 35 {
            break (passable, candidates);
        }
    };

    let heart_index = usize::try_from(
        inputs
            .random
            .int_bound(i32::try_from(candidates.len()).expect("candidate count fits Java int")),
    )
    .expect("Random.Int is non-negative");
    let heart_cell = candidates[heart_index];
    place_plant(inputs, heart_cell, QuestMobKind::RotHeart);

    let mut new_passable = passable.clone();
    for _ in 1..=6 {
        let mut tries = 50;
        let placement = loop {
            let point = random_room_point(inputs.bounds, 1, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            tries -= 1;
            if tries <= 0 {
                break None;
            }
            if valid_plant_pos(
                inputs.level,
                &mut passable,
                &mut new_passable,
                &mut finder,
                cell,
                heart_cell,
                entry_cell,
            ) {
                break Some(cell);
            }
        };
        let Some(cell) = placement else {
            break;
        };
        place_plant(inputs, cell, QuestMobKind::RotLasher);
    }

    for index in (0..finder.circle8.len()).step_by(2) {
        let diagonal = offset_cell(heart_cell, finder.circle8[index]);
        if inputs.level.map().cells[diagonal] != terrain::WALL {
            let cardinal = offset_cell(heart_cell, finder.circle8[index + 1]);
            inputs.level.map_mut().cells[cardinal] = terrain::HIGH_GRASS;
        }
    }
}

fn place_plant<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>, cell: usize, kind: QuestMobKind)
where
    L: QuestLevelContext,
    G: QuestGeneratorContext,
    P: QuestPrizeContext,
{
    inputs.mob(cell, kind);
    inputs.level.map_mut().cells[cell] = terrain::GRASS;
}

fn valid_plant_pos<L: QuestLevelContext>(
    level: &L,
    passable: &mut [bool],
    new_passable: &mut [bool],
    finder: &mut PathFinder,
    cell: usize,
    heart_cell: usize,
    entry_cell: usize,
) -> bool {
    if level.map().cells[cell] != terrain::HIGH_GRASS {
        return false;
    }
    for offset in finder.neighbours9 {
        if level.has_mob(offset_cell(cell, offset)) {
            return false;
        }
    }

    new_passable[cell] = false;
    let neighbours = if level_distance(level.map(), cell, heart_cell) > 2 {
        &finder.neighbours4[..]
    } else {
        &finder.neighbours8[..]
    };
    for &offset in neighbours {
        new_passable[offset_cell(cell, offset)] = false;
    }
    finder.build_distance_map(heart_cell, new_passable);
    if finder.distance[entry_cell] == i32::MAX {
        new_passable.copy_from_slice(passable);
        false
    } else {
        passable.copy_from_slice(new_passable);
        true
    }
}

fn offset_cell(cell: usize, offset: i32) -> usize {
    usize::try_from(
        i32::try_from(cell)
            .expect("Java map cell fits int")
            .wrapping_add(offset),
    )
    .expect("generated room neighbour is inside the level")
}

fn level_distance(map: &GridMap, first: usize, second: usize) -> i32 {
    let first = map.cell_to_point(first);
    let second = map.cell_to_point(second);
    first
        .x
        .wrapping_sub(second.x)
        .wrapping_abs()
        .max(first.y.wrapping_sub(second.y).wrapping_abs())
}

fn paint_blacksmith<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: QuestLevelContext,
    G: QuestGeneratorContext,
    P: QuestPrizeContext,
{
    let bounds = inputs.bounds;
    let width = inputs.level.map().width;
    fill_room(inputs.level, bounds, terrain::WALL);
    fill_room_margin(inputs.level, bounds, 1, terrain::EMPTY);
    fill_room_margin(inputs.level, bounds, 2, terrain::EMPTY_SP);
    draw::fill(
        inputs.level.map_mut(),
        bounds.left + 2,
        bounds.top + 1,
        inclusive_width(bounds) - 4,
        1,
        terrain::REGION_DECO_ALT,
    );
    inputs.set_all_doors(DoorType::Regular);
    let doors: Vec<Point> = inputs.rooms[inputs.room]
        .connected
        .iter()
        .map(|connection| connection.door.expect("validated door").point)
        .collect();
    for &door in &doors {
        draw::draw_inside(inputs.level.map_mut(), bounds, door, 1, terrain::EMPTY);
    }
    fill_room_margin(inputs.level, bounds, 2, terrain::EMPTY_SP);

    // Fixed smithy layout: the NPC beside its anvil and furnace.
    let npc_cell = point_to_cell(inputs.level, Point::new(bounds.left + 3, bounds.top + 3));
    inputs.mob(npc_cell, QuestMobKind::Blacksmith);
    {
        let map = inputs.level.map_mut();
        map.cells[add_cell_offset(npc_cell, 1)] = terrain::CUSTOM_DECO_WTR;
        map.cells[add_cell_offset(npc_cell, 1 - width)] = terrain::CUSTOM_DECO;
        map.cells[add_cell_offset(npc_cell, -width)] = terrain::CUSTOM_DECO;
        map.cells[add_cell_offset(npc_cell, -1 - width)] = terrain::CUSTOM_DECO;
    }

    let mut equipment_cell = point_to_cell(
        inputs.level,
        Point::new(bounds.right - 3, bounds.bottom - 3),
    );
    if inclusive_height(bounds) == 8 {
        equipment_cell = add_cell_offset(equipment_cell, width);
    }
    for _ in 0..2 {
        let category = BLACKSMITH_CATEGORIES
            [usize::try_from(inputs.random.int_bound(3)).expect("Random.Int is non-negative")];
        let item = inputs.generate(QuestGeneratorRequest::Category(category))?;
        inputs.drop(equipment_cell, QuestHeapKind::Heap, item, false);
        inputs.level.map_mut().cells[equipment_cell] = terrain::PEDESTAL;
        equipment_cell = add_cell_offset(equipment_cell, -1);
    }

    // The mine entrance sits in one of the two top corners; a door on the top
    // wall forces it to the opposite corner.
    let mut entrance_on_left = inputs.random.int_bound(2) == 0;
    for door in doors {
        if door.y <= bounds.top + 2 {
            if door.x <= bounds.left + 1 {
                entrance_on_left = false;
            } else if door.x >= bounds.right - 1 {
                entrance_on_left = true;
            }
        }
    }
    let entrance_point = Point::new(
        if entrance_on_left {
            bounds.left + 1
        } else {
            bounds.right - 1
        },
        bounds.top + 1,
    );
    let entrance_cell = point_to_cell(inputs.level, entrance_point);
    inputs
        .level
        .record(QuestPaintEvent::Transition(QuestBranchTransition {
            cell: entrance_cell,
            depth: inputs.level.depth(),
            branch: 1,
        }));
    inputs.level.map_mut().cells[entrance_cell] = terrain::EXIT;
    inputs.feature(entrance_point, 1, 1, QuestFeatureKind::BlacksmithEntrance);
    Ok(())
}

fn paint_ambitious_imp<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: QuestLevelContext,
    G: QuestGeneratorContext,
    P: QuestPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL_DECO);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    let center = room_center(inputs.bounds, inputs.random);

    for (x_offset, y_offset) in [(-2, -2), (2, -2), (-2, 2), (2, 2)] {
        draw::set_point(
            inputs.level.map_mut(),
            Point::new(
                center.x.wrapping_add(x_offset),
                center.y.wrapping_add(y_offset),
            ),
            terrain::REGION_DECO,
        );
    }
    for (x_offset, y_offset) in [(-3, -3), (3, -3), (-3, 3), (3, 3)] {
        draw::set_point(
            inputs.level.map_mut(),
            Point::new(
                center.x.wrapping_add(x_offset),
                center.y.wrapping_add(y_offset),
            ),
            terrain::WALL_DECO,
        );
    }

    let entrance = inputs.entrance().point;
    let mut npc_cell = point_to_cell(inputs.level, center);
    if entrance.x == inputs.bounds.left || entrance.x == inputs.bounds.right {
        npc_cell = add_cell_offset(
            npc_cell,
            inputs
                .random
                .int_range(-1, 1)
                .wrapping_mul(inputs.level.map().width),
        );
        npc_cell = add_cell_offset(
            npc_cell,
            if entrance.x == inputs.bounds.left {
                -2
            } else {
                2
            },
        );
    } else if entrance.y == inputs.bounds.top || entrance.y == inputs.bounds.bottom {
        npc_cell = add_cell_offset(npc_cell, inputs.random.int_range(-1, 1));
        npc_cell = add_cell_offset(
            npc_cell,
            inputs
                .level
                .map()
                .width
                .wrapping_mul(if entrance.y == inputs.bounds.top {
                    -2
                } else {
                    2
                }),
        );
    }
    inputs.mob(npc_cell, QuestMobKind::Imp);
    draw::draw_inside(
        inputs.level.map_mut(),
        inputs.bounds,
        entrance,
        1,
        terrain::EMPTY,
    );
    inputs.set_entrance_door(DoorType::Regular);
    // v4.0.0 carpets: a horizontal and a vertical strip through the centre.
    {
        let bounds = inputs.bounds;
        let map = inputs.level.map_mut();
        draw::fill(
            map,
            bounds.left + 1,
            bounds.top + 3,
            7,
            3,
            terrain::CUSTOM_DECO_EMPTY,
        );
        draw::fill(
            map,
            bounds.left + 3,
            bounds.top + 1,
            3,
            7,
            terrain::CUSTOM_DECO_EMPTY,
        );
    }

    inputs.feature(
        Point::new(center.x.wrapping_sub(2), center.y.wrapping_sub(2)),
        5,
        5,
        QuestFeatureKind::ImpEntrance,
    );
    inputs.feature(
        Point::new(center.x.wrapping_sub(1), center.y.wrapping_sub(1)),
        3,
        3,
        QuestFeatureKind::ImpBarrier,
    );
    let entrance_cell = point_to_cell(inputs.level, center);
    inputs
        .level
        .record(QuestPaintEvent::Transition(QuestBranchTransition {
            cell: entrance_cell,
            depth: inputs.level.depth(),
            branch: 1,
        }));
    inputs.level.map_mut().cells[entrance_cell] = terrain::EXIT;
}

fn add_cell_offset(cell: usize, offset: i32) -> usize {
    usize::try_from(
        i32::try_from(cell)
            .expect("Java map cell fits int")
            .wrapping_add(offset),
    )
    .expect("generated actor cell is non-negative")
}

fn paint_demon_spawner<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: QuestLevelContext,
    G: QuestGeneratorContext,
    P: QuestPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    let center = room_center(inputs.bounds, inputs.random);
    inputs.set_entrance_door(DoorType::Unlocked);
    inputs.mob(
        point_to_cell(inputs.level, center),
        QuestMobKind::DemonSpawner {
            spawn_recorded: true,
        },
    );
    inputs.state.spawners_alive = inputs.state.spawners_alive.wrapping_add(1);
    inputs.feature(
        Point::new(
            inputs.bounds.left.wrapping_add(1),
            inputs.bounds.top.wrapping_add(1),
        ),
        inclusive_width(inputs.bounds).wrapping_sub(2),
        inclusive_height(inputs.bounds).wrapping_sub(2),
        QuestFeatureKind::DemonSpawnerFloor,
    );
}

/// Exact `StandardRoom` merge predicate for the Ritual Site; the v4.0.0
/// Blacksmith room never merges.
#[must_use]
pub fn can_merge(level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
    matches!(rooms[room].kind, RoomKind::Quest(QuestRoomKind::RitualSite))
        && standard_can_merge(level, rooms, room, point)
}

/// `Room.canPlaceItem` overrides for all handled classes.
#[must_use]
pub fn can_place_item(
    level: &Level,
    rooms: &[Room],
    room: RoomId,
    point: Point,
    state: &QuestPaintState,
) -> bool {
    let selected = &rooms[room];
    if !selected.inside(point) {
        return false;
    }
    match selected.kind {
        RoomKind::Quest(QuestRoomKind::RitualSite) => {
            let ritual = state
                .ritual_position
                .expect("painted RitualSiteRoom sets CeremonialCandle.ritualPos");
            level_distance(&level.map, ritual, level.point_to_cell(point)) >= 2
        }
        RoomKind::Quest(QuestRoomKind::AmbitiousImp) => false,
        RoomKind::Quest(QuestRoomKind::Blacksmith) => {
            level.map.cells[level.point_to_cell(point)] == terrain::EMPTY
        }
        _ => true,
    }
}

/// `Room.canPlaceCharacter` overrides for all handled classes.
#[must_use]
pub fn can_place_character(
    level: &Level,
    rooms: &[Room],
    room: RoomId,
    point: Point,
    state: &QuestPaintState,
) -> bool {
    let selected = &rooms[room];
    match selected.kind {
        RoomKind::Quest(QuestRoomKind::AmbitiousImp | QuestRoomKind::Blacksmith) => false,
        RoomKind::Quest(QuestRoomKind::RitualSite) => {
            selected.inside(point)
                && level_distance(
                    &level.map,
                    state
                        .ritual_position
                        .expect("painted RitualSiteRoom sets ritual position"),
                    level.point_to_cell(point),
                ) >= 2
        }
        _ => selected.inside(point),
    }
}

/// `Room.canPlaceTrap`; the painter only calls this for points in the room.
#[must_use]
pub const fn can_place_trap(kind: RoomKind) -> bool {
    !matches!(
        kind,
        RoomKind::Quest(QuestRoomKind::AmbitiousImp | QuestRoomKind::Blacksmith)
            | RoomKind::Special(SpecialRoomKind::DemonSpawner)
    )
}

/// `Room.canPlaceGrass`, including the Imp room's Euclidean radius.
#[must_use]
pub fn can_place_grass(rooms: &[Room], room: RoomId, point: Point) -> bool {
    match rooms[room].kind {
        RoomKind::Quest(QuestRoomKind::AmbitiousImp) => {
            Point::distance(point, fixed_room_center(rooms[room].bounds)) >= 5.0_f32
        }
        RoomKind::Quest(QuestRoomKind::Blacksmith)
        | RoomKind::Special(SpecialRoomKind::DemonSpawner) => false,
        _ => true,
    }
}

/// `Room.canPlaceWater`, including the Imp room's Euclidean radius.
#[must_use]
pub fn can_place_water(rooms: &[Room], room: RoomId, point: Point) -> bool {
    match rooms[room].kind {
        RoomKind::Quest(QuestRoomKind::AmbitiousImp) => {
            Point::distance(point, fixed_room_center(rooms[room].bounds)) >= 5.0_f32
        }
        RoomKind::Quest(QuestRoomKind::Blacksmith)
        | RoomKind::Special(SpecialRoomKind::DemonSpawner) => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{ArmorEffect, Effect, ItemId};
    use crate::geometry::Rect;
    use crate::level::Feeling;
    use crate::room::{ConnectionRoomKind, Door, RoomConnection};
    use crate::run::RunState;

    struct Fixture {
        map_hash: i32,
        door: DoorType,
        next: i64,
        events: Vec<QuestPaintEvent>,
        queue: Vec<RegularItem>,
        state: QuestPaintState,
        report: QuestPaintReport,
        level: Level,
        rooms: Vec<Room>,
    }

    fn fixture(kind: RoomKind, depth: u32, seed: i64, size: i32, door: Point) -> Fixture {
        let mut constructor_random = RandomStack::with_base_seed(9_991);
        let mut selected = match kind {
            RoomKind::Quest(kind) => Room::quest(kind, &mut constructor_random),
            RoomKind::Special(SpecialRoomKind::DemonSpawner) => {
                Room::special(SpecialRoomKind::DemonSpawner)
            }
            _ => panic!("fixture requires a handled room"),
        };
        selected.bounds = Rect::new(2, 2, 2 + size - 1, 2 + size - 1);
        selected.connected.push(RoomConnection {
            room: 1,
            door: Some(Door::new(door)),
        });
        let mut neighbour = Room::connection(ConnectionRoomKind::Tunnel);
        neighbour.connected.push(RoomConnection {
            room: 0,
            door: Some(Door::new(door)),
        });
        let mut rooms = vec![selected, neighbour];

        let mut run = RunState::new(0);
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed);
        let mut level = Level::new(depth, Feeling::None);
        level.set_size(15, 15);
        let mut events = Vec::new();
        let mut queue = Vec::new();
        let mut state = QuestPaintState::default();
        let outcome = {
            let mut context = QuestLevelPaintContext::new(&mut level, &mut events);
            paint_quest_room(
                &mut context,
                &mut rooms,
                0,
                &mut run.generator,
                &mut queue,
                &mut state,
                &mut random,
            )
            .unwrap()
        };
        let QuestPaintOutcome::Painted(report) = outcome else {
            panic!("fixture kind was not handled")
        };
        Fixture {
            map_hash: level.java_map_hash(),
            door: rooms[0].connected[0].door.unwrap().door_type,
            next: random.long(),
            events,
            queue,
            state,
            report,
            level,
            rooms,
        }
    }

    fn mob_cells(events: &[QuestPaintEvent], kind: QuestMobKind) -> Vec<usize> {
        let mut cells: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                QuestPaintEvent::Mob { cell, kind: actual } if *actual == kind => Some(*cell),
                _ => None,
            })
            .collect();
        cells.sort_unstable();
        cells
    }

    fn drop_cells(events: &[QuestPaintEvent]) -> Vec<(usize, QuestHeapKind, RegularItem, bool)> {
        let mut drops: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                QuestPaintEvent::Drop {
                    cell,
                    heap,
                    haunted,
                    item,
                } => Some((*cell, *heap, *item, *haunted)),
                _ => None,
            })
            .collect();
        drops.sort_unstable_by_key(|drop| drop.0);
        drops
    }

    #[test]
    fn mass_grave_matches_official_java_fixtures() {
        let seed_zero = fixture(
            RoomKind::Quest(QuestRoomKind::MassGrave),
            8,
            0,
            10,
            Point::new(2, 6),
        );
        assert_eq!(seed_zero.map_hash, -2_133_185_797);
        assert_eq!(seed_zero.door, DoorType::Barricade);
        assert_eq!(seed_zero.next, 2_704_323_167_362_897_208);
        assert_eq!(
            mob_cells(&seed_zero.events, QuestMobKind::Skeleton),
            [67, 84]
        );
        assert_eq!(
            seed_zero.queue,
            [RegularItem::Queued(QueuedItemKind::PotionOfLiquidFlame)]
        );
        assert_eq!(seed_zero.report.searchable_items, []);
        let drops = drop_cells(&seed_zero.events);
        assert_eq!(drops.len(), 3);
        assert_eq!(
            drops.iter().map(|drop| drop.0).collect::<Vec<_>>(),
            [52, 66, 79]
        );
        assert!(drops.iter().any(|drop| drop.0 == 52 && drop.3));

        let seed_nineteen = fixture(
            RoomKind::Quest(QuestRoomKind::MassGrave),
            8,
            19,
            10,
            Point::new(2, 6),
        );
        // The fixed 11x10 layout paints the same terrain for every seed.
        assert_eq!(seed_nineteen.map_hash, -2_133_185_797);
        assert_eq!(seed_nineteen.next, 5_307_994_216_896_367_667);
        assert_eq!(
            mob_cells(&seed_nineteen.events, QuestMobKind::Skeleton),
            [51, 80]
        );
        assert_eq!(
            seed_nineteen.report.searchable_items,
            [WorldItem {
                item: ItemId::MailArmor,
                upgrade: 1,
                effect: Some(Effect::Armor(ArmorEffect::Metabolism)),
                cursed: true,
                depth: 8,
                source: ItemSource::Heap,
                accessibility: Accessibility::Independent,
                secret: false,
            }]
        );
        assert_eq!(
            drop_cells(&seed_nineteen.events)
                .iter()
                .map(|drop| drop.0)
                .collect::<Vec<_>>(),
            [51, 52, 66, 94, 97, 99]
        );
    }

    #[test]
    fn ritual_site_and_placement_radius_match_official_fixture() {
        let result = fixture(
            RoomKind::Quest(QuestRoomKind::RitualSite),
            8,
            0,
            10,
            Point::new(2, 6),
        );
        assert_eq!(result.map_hash, -901_739_966);
        assert_eq!(result.door, DoorType::Regular);
        assert_eq!(result.next, 4_437_113_781_045_784_766);
        assert_eq!(result.state.ritual_position, Some(112));
        assert_eq!(
            result.queue,
            [RegularItem::Queued(QueuedItemKind::CeremonialCandle); 4]
        );
        assert!(!can_place_item(
            &result.level,
            &result.rooms,
            0,
            Point::new(7, 7),
            &result.state,
        ));
        assert!(!can_place_character(
            &result.level,
            &result.rooms,
            0,
            Point::new(8, 8),
            &result.state,
        ));
        assert!(can_place_item(
            &result.level,
            &result.rooms,
            0,
            Point::new(9, 9),
            &result.state,
        ));
    }

    #[test]
    fn rot_garden_pathfinding_and_retry_state_match_official_fixtures() {
        for (seed, map_hash, next, heart, lashers) in [
            (
                0,
                -493_824_554,
                -7_259_568_352_408_974_738,
                128,
                vec![50, 53, 63, 84, 95, 125],
            ),
            (
                31,
                170_102_254,
                -6_140_443_781_600_553_158,
                100,
                vec![54, 67, 78, 95, 98, 156],
            ),
        ] {
            let result = fixture(
                RoomKind::Quest(QuestRoomKind::RotGarden),
                8,
                seed,
                10,
                Point::new(2, 6),
            );
            assert_eq!(result.map_hash, map_hash);
            assert_eq!(result.door, DoorType::Locked);
            assert_eq!(result.next, next);
            assert_eq!(mob_cells(&result.events, QuestMobKind::RotHeart), [heart]);
            assert_eq!(mob_cells(&result.events, QuestMobKind::RotLasher), lashers);
            assert_eq!(
                result.queue,
                [RegularItem::Queued(QueuedItemKind::IronKey { depth: 8 })]
            );
        }
    }

    #[test]
    fn blacksmith_items_traps_transition_and_search_records_match_java() {
        let seed_zero = fixture(
            RoomKind::Quest(QuestRoomKind::Blacksmith),
            13,
            0,
            10,
            Point::new(2, 6),
        );
        assert_eq!(seed_zero.map_hash, -107_896_294);
        assert_eq!(seed_zero.next, 7_105_486_291_024_734_541);
        assert_eq!(mob_cells(&seed_zero.events, QuestMobKind::Blacksmith), [80]);
        assert_eq!(
            seed_zero.report.searchable_items,
            [
                WorldItem {
                    item: ItemId::ScaleArmor,
                    upgrade: 1,
                    effect: Some(Effect::Armor(ArmorEffect::Metabolism)),
                    cursed: true,
                    depth: 13,
                    source: ItemSource::Heap,
                    accessibility: Accessibility::Independent,
                    secret: false,
                },
                WorldItem {
                    item: ItemId::ThrowingSpear,
                    upgrade: 0,
                    effect: None,
                    cursed: false,
                    depth: 13,
                    source: ItemSource::Heap,
                    accessibility: Accessibility::Independent,
                    secret: false,
                },
            ]
        );
        assert_eq!(
            drop_cells(&seed_zero.events)
                .iter()
                .map(|d| d.0)
                .collect::<Vec<_>>(),
            [127, 128]
        );
        let traps = seed_zero
            .events
            .iter()
            .filter(|event| matches!(event, QuestPaintEvent::Trap { .. }))
            .count();
        // v4.0.0 smithies no longer ring the room with burning traps.
        assert_eq!(traps, 0);
        assert!(
            seed_zero
                .events
                .contains(&QuestPaintEvent::Transition(QuestBranchTransition {
                    cell: 48,
                    depth: 13,
                    branch: 1,
                }))
        );

        let seed_seventy_seven = fixture(
            RoomKind::Quest(QuestRoomKind::Blacksmith),
            13,
            77,
            10,
            Point::new(2, 6),
        );
        assert_eq!(seed_seventy_seven.map_hash, 1_919_139_820);
        assert_eq!(seed_seventy_seven.next, -6_264_321_152_254_219_996);
        assert_eq!(
            mob_cells(&seed_seventy_seven.events, QuestMobKind::Blacksmith),
            [80]
        );
        assert_eq!(
            seed_seventy_seven.report.searchable_items,
            [
                WorldItem {
                    item: ItemId::Crossbow,
                    upgrade: 0,
                    effect: None,
                    cursed: false,
                    depth: 13,
                    source: ItemSource::Heap,
                    accessibility: Accessibility::Independent,
                    secret: false,
                },
                WorldItem {
                    item: ItemId::ScaleArmor,
                    upgrade: 0,
                    effect: Some(Effect::Armor(ArmorEffect::Camouflage)),
                    cursed: false,
                    depth: 13,
                    source: ItemSource::Heap,
                    accessibility: Accessibility::Independent,
                    secret: false,
                },
            ]
        );
        assert!(
            seed_seventy_seven
                .events
                .contains(&QuestPaintEvent::Transition(QuestBranchTransition {
                    cell: 55,
                    depth: 13,
                    branch: 1,
                }))
        );
    }

    #[test]
    fn imp_directional_offsets_and_placement_overrides_match_java() {
        for (door, expected_cell) in [
            (Point::new(2, 6), 79),
            (Point::new(10, 6), 83),
            (Point::new(6, 2), 65),
            (Point::new(6, 10), 125),
        ] {
            let result = fixture(RoomKind::Quest(QuestRoomKind::AmbitiousImp), 18, 0, 9, door);
            assert_eq!(result.map_hash, 1_952_879_827);
            assert_eq!(result.next, -3_109_364_765_729_502_342);
            assert_eq!(
                mob_cells(&result.events, QuestMobKind::Imp),
                [expected_cell]
            );
            assert!(
                result
                    .events
                    .contains(&QuestPaintEvent::Transition(QuestBranchTransition {
                        cell: 96,
                        depth: 18,
                        branch: 1,
                    }))
            );
            assert!(!can_place_item(
                &result.level,
                &result.rooms,
                0,
                Point::new(4, 4),
                &result.state,
            ));
            assert!(!can_place_trap(result.rooms[0].kind));
            assert!(!can_place_grass(&result.rooms, 0, Point::new(6, 6)));
            assert!(can_place_grass(&result.rooms, 0, Point::new(2, 2)));
        }
    }

    #[test]
    fn demon_spawner_center_jitter_statistics_and_floor_bounds_match_java() {
        for (seed, size, expected_hash, expected_next, expected_cell, expected_feature) in [
            (0, 7, 1_662_710_304, -4_962_768_465_676_381_896, 80, (5, 5)),
            (9, 8, 484_528_163, -3_952_900_872_052_014_752, 95, (6, 6)),
        ] {
            let result = fixture(
                RoomKind::Special(SpecialRoomKind::DemonSpawner),
                22,
                seed,
                size,
                Point::new(2, 5),
            );
            assert_eq!(result.map_hash, expected_hash);
            assert_eq!(result.door, DoorType::Unlocked);
            assert_eq!(result.next, expected_next);
            assert_eq!(
                mob_cells(
                    &result.events,
                    QuestMobKind::DemonSpawner {
                        spawn_recorded: true,
                    },
                ),
                [expected_cell]
            );
            assert_eq!(result.state.spawners_alive, 1);
            assert!(result.events.contains(&QuestPaintEvent::Feature {
                point: Point::new(3, 3),
                width: expected_feature.0,
                height: expected_feature.1,
                kind: QuestFeatureKind::DemonSpawnerFloor,
            }));
            assert!(!can_place_grass(&result.rooms, 0, Point::new(4, 4)));
            assert!(!can_place_water(&result.rooms, 0, Point::new(4, 4)));
            assert!(!can_place_trap(result.rooms[0].kind));
        }
    }
}
