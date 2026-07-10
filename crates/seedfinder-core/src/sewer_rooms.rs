//! Concrete room painting for regular Sewer floors (depths 1 through 4).
//!
//! This module is intentionally closed over the exact v3.3.8 classes which
//! can be selected by Sewer `StandardRoom`, entrance/exit, and connection-room
//! tables. Special, secret, quest, and boss rooms are separate generation
//! slices and are rejected rather than approximated here.

#![allow(
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unused_self
)]

use crate::generator::{
    self, GeneratedItem, SeedKind, random_armor, random_category, random_gold, random_missile,
    random_using_defaults, random_weapon,
};
use crate::geometry::{PathFinder, Point, Rect, painter as draw, terrain};
use crate::java_math::{div_i32, f32_to_i32, f64_to_i32, round_f64};
use crate::level::{
    HeapKind, Level, PaintItem, PaintMob, PlacedTrap, TransitionKind, TrapKind, TrapSpec,
};
use crate::painter::{
    RoomPaintDispatch, fill_room, fill_room_margin, generate_patch, set_shared_door_type,
    standard_can_merge,
};
use crate::rng::{FastBound, RandomStack};
use crate::room::{
    ConnectionRoomKind, DoorType, Room, RoomId, RoomKind, SizeCategory, StandardRoomKind,
};
use crate::run::{GeneratorCategory, GeneratorState};

/// Item-generation and pending-prize operations invoked from room painters.
/// Implementations must perform every underlying Generator draw before
/// returning; room geometry never substitutes a synthetic item.
pub trait SewerRoomContent {
    fn find_prize_item(&mut self, rng: &mut RandomStack) -> Option<PaintItem>;
    fn random_seed_using_defaults(&mut self, rng: &mut RandomStack) -> SeedKind;
    fn random_item(&mut self, rng: &mut RandomStack) -> PaintItem;
    fn random_category(&mut self, category: GeneratorCategory, rng: &mut RandomStack) -> PaintItem;
    fn random_mimic_reward(&mut self, rng: &mut RandomStack) -> PaintItem;
}

/// Exact canonical content provider backed by the live run Generator and the
/// level's insertion-ordered `itemsToSpawn` list.
pub struct GeneratorRoomContent<'a> {
    pub depth: i32,
    pub generator: &'a mut GeneratorState,
    pub items_to_spawn: &'a mut Vec<PaintItem>,
}

impl SewerRoomContent for GeneratorRoomContent<'_> {
    fn find_prize_item(&mut self, rng: &mut RandomStack) -> Option<PaintItem> {
        if self.items_to_spawn.is_empty() {
            return None;
        }
        if let Some(index) = self
            .items_to_spawn
            .iter()
            .position(|item| *item == PaintItem::TrinketCatalyst)
        {
            return Some(self.items_to_spawn.remove(index));
        }
        let index = usize::try_from(rng.int_bound(
            i32::try_from(self.items_to_spawn.len()).expect("itemsToSpawn exceeds Java int"),
        ))
        .expect("Random.Int is non-negative");
        Some(self.items_to_spawn.remove(index))
    }

    fn random_seed_using_defaults(&mut self, rng: &mut RandomStack) -> SeedKind {
        match random_using_defaults(rng, self.generator, GeneratorCategory::Seed, self.depth)
            .expect("canonical Seed category state is valid")
        {
            GeneratedItem::Seed(seed) => seed,
            _ => unreachable!("Seed category always creates a seed"),
        }
    }

    fn random_item(&mut self, rng: &mut RandomStack) -> PaintItem {
        generator::random(rng, self.generator, self.depth)
            .expect("canonical Generator state is valid")
            .into()
    }

    fn random_category(&mut self, category: GeneratorCategory, rng: &mut RandomStack) -> PaintItem {
        random_category(rng, self.generator, category, self.depth)
            .expect("canonical Generator category state is valid")
            .into()
    }

    fn random_mimic_reward(&mut self, rng: &mut RandomStack) -> PaintItem {
        let floor_set = self.depth / 5;
        let reward = match rng.int_bound(5) {
            0 => random_gold(rng, self.depth),
            1 => GeneratedItem::Missile(
                random_missile(rng, self.generator, floor_set, false)
                    .expect("canonical missile deck is valid"),
            ),
            2 => GeneratedItem::Equipment(
                random_armor(rng, floor_set).expect("canonical armor table is valid"),
            ),
            3 => GeneratedItem::Equipment(
                random_weapon(rng, self.generator, floor_set, false)
                    .expect("canonical weapon deck is valid"),
            ),
            _ => random_category(rng, self.generator, GeneratorCategory::Ring, self.depth)
                .expect("canonical ring deck is valid"),
        };
        reward.into()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct RoomPaintState {
    patch: Option<Vec<bool>>,
    bridge_space: Option<Rect>,
    bridge: Option<Rect>,
    ring_connection_space: Option<Rect>,
}

/// Exact non-special Sewer room dispatcher.
#[derive(Debug)]
pub struct SewerRoomDispatcher<C> {
    pub content: C,
    /// Canonical no-trinket profile uses zero.
    pub revealed_trap_chance: f32,
    states: Vec<RoomPaintState>,
}

impl<C> SewerRoomDispatcher<C> {
    #[must_use]
    pub const fn new(content: C) -> Self {
        Self {
            content,
            revealed_trap_chance: 0.0,
            states: Vec::new(),
        }
    }

    #[must_use]
    pub fn set_revealed_trap_chance(mut self, chance: f32) -> Self {
        self.revealed_trap_chance = chance;
        self
    }

    fn ensure_state(&mut self, room: RoomId) -> &mut RoomPaintState {
        if self.states.len() <= room {
            self.states.resize_with(room + 1, RoomPaintState::default);
        }
        &mut self.states[room]
    }

    fn state(&self, room: RoomId) -> &RoomPaintState {
        &self.states[room]
    }

    /// Cached `StandardBridgeRoom.spaceRect`, used by later item and mob
    /// placement predicates.
    #[must_use]
    pub fn bridge_space(&self, room: RoomId) -> Option<Rect> {
        self.states.get(room).and_then(|state| state.bridge_space)
    }

    /// Concrete `Room.canPlaceItem` for every room handled by this dispatcher.
    #[must_use]
    pub fn can_place_item(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        point: Point,
    ) -> bool {
        if !rooms[room].inside(point) {
            return false;
        }
        match rooms[room].kind {
            RoomKind::Standard(StandardRoomKind::WaterBridge)
            | RoomKind::Entrance(StandardRoomKind::WaterBridge)
            | RoomKind::Exit(StandardRoomKind::WaterBridge) => self
                .bridge_space(room)
                .is_none_or(|space| !space.inside(point)),
            RoomKind::Standard(StandardRoomKind::Plants) => !level
                .plants
                .iter()
                .any(|plant| plant.cell == level.point_to_cell(point)),
            RoomKind::Standard(StandardRoomKind::Aquarium) => {
                level.map.cells[level.point_to_cell(point)] != terrain::WATER
            }
            _ => true,
        }
    }

    /// Concrete `Room.canPlaceCharacter`, including exit and bridge exclusions.
    #[must_use]
    pub fn can_place_character(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        point: Point,
    ) -> bool {
        if !self.can_place_item(level, rooms, room, point) {
            return false;
        }
        !matches!(rooms[room].kind, RoomKind::Exit(_))
            || level.exit() != Some(level.point_to_cell(point))
    }
}

impl<C> crate::sewer_mob_placement::RoomCharacterRules for SewerRoomDispatcher<C> {
    fn can_place_character(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        point: Point,
    ) -> bool {
        SewerRoomDispatcher::can_place_character(self, level, rooms, room, point)
    }
}

impl<C: SewerRoomContent> RoomPaintDispatch for SewerRoomDispatcher<C> {
    fn paint_room(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        self.ensure_state(room);
        match rooms[room].kind {
            RoomKind::Entrance(kind) => self.paint_transition(level, rooms, room, kind, true, rng),
            RoomKind::Exit(kind) => self.paint_transition(level, rooms, room, kind, false, rng),
            RoomKind::Standard(kind) => self.paint_standard(level, rooms, room, kind, rng),
            RoomKind::Connection(kind) => self.paint_connection(level, rooms, room, kind, rng),
            RoomKind::Special(_) | RoomKind::Secret(_) | RoomKind::Quest(_) => {
                panic!("non-regular Sewer room passed to SewerRoomDispatcher")
            }
        }
    }

    fn can_merge(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        _other: RoomId,
        point: Point,
        merge_terrain: i32,
    ) -> bool {
        match rooms[room].kind {
            RoomKind::Entrance(
                StandardRoomKind::WaterBridge | StandardRoomKind::RegionDecoPatch,
            ) if level.depth <= 2 => false,
            RoomKind::Standard(StandardRoomKind::SewerPipe) => false,
            RoomKind::Entrance(StandardRoomKind::WaterBridge)
            | RoomKind::Exit(StandardRoomKind::WaterBridge)
            | RoomKind::Standard(StandardRoomKind::WaterBridge) => {
                let cell = level.point_to_cell(rooms[room].point_inside(point, 1));
                level.map.cells[cell] != terrain::WATER
            }
            RoomKind::Standard(StandardRoomKind::Burned | StandardRoomKind::Minefield) => {
                let cell = level.point_to_cell(rooms[room].point_inside(point, 1));
                level.map.cells[cell] == terrain::EMPTY
            }
            RoomKind::Connection(
                ConnectionRoomKind::Bridge
                | ConnectionRoomKind::Walkway
                | ConnectionRoomKind::RingBridge,
            ) => merge_terrain == terrain::CHASM,
            RoomKind::Standard(_) | RoomKind::Entrance(_) | RoomKind::Exit(_) => {
                standard_can_merge(level, rooms, room, point)
            }
            RoomKind::Connection(_)
            | RoomKind::Special(_)
            | RoomKind::Secret(_)
            | RoomKind::Quest(_) => false,
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
        let room_kind = rooms[room].kind;
        let other_kind = rooms[other].kind;
        let terrain_to_fill = match room_kind {
            RoomKind::Standard(StandardRoomKind::Plants | StandardRoomKind::GrassyGrave)
                if merge_terrain == terrain::EMPTY
                    && matches!(
                        other_kind,
                        RoomKind::Standard(
                            StandardRoomKind::Plants | StandardRoomKind::GrassyGrave
                        )
                    ) =>
            {
                terrain::GRASS
            }
            RoomKind::Standard(StandardRoomKind::Striped)
                if merge_terrain == terrain::EMPTY
                    && matches!(other_kind, RoomKind::Standard(StandardRoomKind::Striped)) =>
            {
                terrain::EMPTY_SP
            }
            RoomKind::Standard(StandardRoomKind::Platform)
                if merge_terrain != terrain::CHASM
                    && matches!(
                        other_kind,
                        RoomKind::Standard(StandardRoomKind::Platform | StandardRoomKind::Chasm)
                    ) =>
            {
                terrain::CHASM
            }
            _ => merge_terrain,
        };
        draw::fill_rect(&mut level.map, merge, terrain_to_fill);
        if matches!(room_kind, RoomKind::Standard(StandardRoomKind::Platform))
            && terrain_to_fill == terrain::CHASM
        {
            let door = rooms[room]
                .connection_to(other)
                .and_then(|connection| connection.door)
                .expect("connected platform door is placed");
            let door_cell = level.point_to_cell(door.point);
            level.map.cells[door_cell] = terrain::EMPTY_SP;
        }
    }

    fn can_place_water(&self, _level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        match rooms[room].kind {
            RoomKind::Standard(StandardRoomKind::SewerPipe | StandardRoomKind::WaterBridge)
            | RoomKind::Entrance(StandardRoomKind::WaterBridge)
            | RoomKind::Exit(StandardRoomKind::WaterBridge) => false,
            RoomKind::Standard(StandardRoomKind::Burned) => {
                !rooms[room].inside(point) || !self.patch_at(rooms, room, point)
            }
            _ => true,
        }
    }

    fn can_place_grass(&self, _level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        !matches!(
            rooms[room].kind,
            RoomKind::Standard(StandardRoomKind::Burned)
        ) || !rooms[room].inside(point)
            || !self.patch_at(rooms, room, point)
    }

    fn can_place_trap(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        match rooms[room].kind {
            RoomKind::Entrance(
                StandardRoomKind::WaterBridge | StandardRoomKind::RegionDecoPatch,
            ) if level.depth == 1 => false,
            RoomKind::Standard(StandardRoomKind::Burned) => {
                !rooms[room].inside(point) || !self.patch_at(rooms, room, point)
            }
            _ => true,
        }
    }
}

impl<C: SewerRoomContent> SewerRoomDispatcher<C> {
    fn paint_standard(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        kind: StandardRoomKind,
        rng: &mut RandomStack,
    ) {
        match kind {
            StandardRoomKind::SewerPipe => self.paint_sewer_pipe(level, rooms, room, rng),
            StandardRoomKind::Ring => self.paint_ring(level, rooms, room, RingDetail::Prize, rng),
            StandardRoomKind::WaterBridge => self.paint_standard_bridge(level, rooms, room, rng),
            StandardRoomKind::RegionDecoPatch => {
                self.paint_region_deco_patch(level, rooms, room, rng);
            }
            StandardRoomKind::CircleBasin => self.paint_circle_basin(level, rooms, room, rng),
            StandardRoomKind::Plants => self.paint_plants(level, rooms, room, rng),
            StandardRoomKind::Aquarium => self.paint_aquarium(level, rooms, room, rng),
            StandardRoomKind::Platform => self.paint_platform(level, rooms, room, rng),
            StandardRoomKind::Burned => self.paint_burned(level, rooms, room, rng),
            StandardRoomKind::Fissure => self.paint_fissure(level, rooms, room, rng),
            StandardRoomKind::GrassyGrave => self.paint_grassy_grave(level, rooms, room, rng),
            StandardRoomKind::Striped => self.paint_striped(level, rooms, room, rng),
            StandardRoomKind::Study => self.paint_study(level, rooms, room, rng),
            StandardRoomKind::SuspiciousChest => {
                self.paint_suspicious_chest(level, rooms, room, rng);
            }
            StandardRoomKind::Minefield => self.paint_minefield(level, rooms, room, rng),
            _ => panic!("non-Sewer standard room passed to SewerRoomDispatcher: {kind:?}"),
        }
    }

    fn paint_transition(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        kind: StandardRoomKind,
        entrance: bool,
        rng: &mut RandomStack,
    ) {
        match kind {
            StandardRoomKind::WaterBridge => {
                self.paint_standard_bridge(level, rooms, room, rng);
                self.place_water_bridge_transition(level, rooms, room, entrance, rng);
            }
            StandardRoomKind::RegionDecoPatch => {
                self.paint_region_deco_patch(level, rooms, room, rng);
                self.place_patch_transition(level, rooms, room, entrance, rng);
            }
            StandardRoomKind::Ring => self.paint_ring(
                level,
                rooms,
                room,
                if entrance {
                    RingDetail::Entrance
                } else {
                    RingDetail::Exit
                },
                rng,
            ),
            StandardRoomKind::CircleBasin => {
                self.paint_circle_basin(level, rooms, room, rng);
                let point = rooms[room].center(rng);
                let cell = level.point_to_cell(point);
                level.map.cells[cell] = if entrance {
                    terrain::ENTRANCE_SP
                } else {
                    terrain::EXIT
                };
                level.add_transition(
                    cell,
                    if entrance {
                        TransitionKind::RegularEntrance
                    } else {
                        TransitionKind::RegularExit
                    },
                );
            }
            _ => panic!("non-Sewer transition room passed to SewerRoomDispatcher: {kind:?}"),
        }
    }

    fn paint_connection(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        kind: ConnectionRoomKind,
        rng: &mut RandomStack,
    ) {
        match kind {
            ConnectionRoomKind::Tunnel => {
                self.paint_tunnel(level, rooms, room, false, rng);
            }
            ConnectionRoomKind::Bridge => {
                if rooms[room].width().min(rooms[room].height()) > 3 {
                    fill_room_margin(level, &rooms[room], 1, terrain::CHASM);
                }
                self.paint_tunnel(level, rooms, room, false, rng);
                paint_bridge_neighbour_edges(level, rooms, room);
            }
            ConnectionRoomKind::Walkway => {
                if rooms[room].width().min(rooms[room].height()) > 3 {
                    fill_room_margin(level, &rooms[room], 1, terrain::CHASM);
                }
                paint_perimeter(level, rooms, room, level.tunnel_tile());
                set_all_doors(rooms, room, DoorType::Tunnel);
                paint_bridge_neighbour_edges(level, rooms, room);
            }
            ConnectionRoomKind::RingTunnel => {
                self.paint_tunnel(level, rooms, room, true, rng);
                let floor = level.tunnel_tile();
                let ring = self
                    .state(room)
                    .ring_connection_space
                    .expect("ring connection space is cached by TunnelRoom.paint");
                draw::fill(&mut level.map, ring.left, ring.top, 3, 3, floor);
                draw::fill(
                    &mut level.map,
                    ring.left + 1,
                    ring.top + 1,
                    1,
                    1,
                    terrain::WALL,
                );
            }
            ConnectionRoomKind::RingBridge => {
                fill_room_margin(level, &rooms[room], 1, terrain::CHASM);
                self.paint_tunnel(level, rooms, room, true, rng);
                let floor = level.tunnel_tile();
                let ring = self
                    .state(room)
                    .ring_connection_space
                    .expect("ring connection space is cached by TunnelRoom.paint");
                draw::fill(&mut level.map, ring.left, ring.top, 3, 3, floor);
                draw::fill(
                    &mut level.map,
                    ring.left + 1,
                    ring.top + 1,
                    1,
                    1,
                    terrain::WALL,
                );
                paint_bridge_neighbour_edges(level, rooms, room);
            }
            ConnectionRoomKind::Perimeter => {
                paint_perimeter(level, rooms, room, level.tunnel_tile());
                set_all_doors(rooms, room, DoorType::Tunnel);
            }
            ConnectionRoomKind::Maze => paint_maze_connection(level, rooms, room, rng),
        }
    }

    fn patch_at(&self, rooms: &[Room], room: RoomId, point: Point) -> bool {
        let patch = self.state(room).patch.as_ref().expect("room patch exists");
        patch[patch_coordinate(&rooms[room], point.x, point.y)]
    }

    fn setup_patch(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        mut fill: f32,
        clustering: i32,
        ensure_path: bool,
        clean_edges: bool,
        rng: &mut RandomStack,
    ) {
        let room_width = rooms[room].width();
        let room_height = rooms[room].height();
        let patch_width = room_width - 2;
        let patch_height = room_height - 2;
        let mut attempts = 0_i32;
        let mut patch;
        if ensure_path {
            loop {
                patch = generate_patch(patch_width, patch_height, fill, clustering, true, rng);
                // Java evaluates this even though every connected room below
                // overwrites `startPoint`; `center()` may consume two draws.
                let center = rooms[room].center(rng);
                let mut start_point = level.point_to_cell(center);
                let doors = room_doors(rooms, room);
                for (_, door) in doors {
                    if door.x == rooms[room].bounds.left {
                        start_point = patch_coordinate(&rooms[room], door.x + 1, door.y);
                        patch[patch_coordinate(&rooms[room], door.x + 1, door.y)] = false;
                        patch[patch_coordinate(&rooms[room], door.x + 2, door.y)] = false;
                    } else if door.x == rooms[room].bounds.right {
                        start_point = patch_coordinate(&rooms[room], door.x - 1, door.y);
                        patch[patch_coordinate(&rooms[room], door.x - 1, door.y)] = false;
                        patch[patch_coordinate(&rooms[room], door.x - 2, door.y)] = false;
                    } else if door.y == rooms[room].bounds.top {
                        start_point = patch_coordinate(&rooms[room], door.x, door.y + 1);
                        patch[patch_coordinate(&rooms[room], door.x, door.y + 1)] = false;
                        patch[patch_coordinate(&rooms[room], door.x, door.y + 2)] = false;
                    } else {
                        start_point = patch_coordinate(&rooms[room], door.x, door.y - 1);
                        patch[patch_coordinate(&rooms[room], door.x, door.y - 1)] = false;
                        patch[patch_coordinate(&rooms[room], door.x, door.y - 2)] = false;
                    }
                }
                let valid = PathFinder::all_open_cells_connected(
                    patch_width,
                    patch_height,
                    start_point,
                    |cell| !patch[cell],
                );
                attempts = attempts.wrapping_add(1);
                if attempts > 100 {
                    fill -= 0.01_f32;
                    attempts = 0;
                }
                if valid {
                    break;
                }
            }
        } else {
            patch = generate_patch(patch_width, patch_height, fill, clustering, true, rng);
        }
        if clean_edges {
            clean_diagonal_edges(&mut patch, patch_width);
        }
        self.ensure_state(room).patch = Some(patch);
    }

    fn fill_patch(&self, level: &mut Level, rooms: &[Room], room: RoomId, tile: i32) {
        let bounds = rooms[room].bounds;
        for y in bounds.top + 1..bounds.bottom {
            for x in bounds.left + 1..bounds.right {
                if self.patch_at(rooms, room, Point::new(x, y)) {
                    level.map.set(x, y, tile);
                }
            }
        }
    }

    fn paint_region_deco_patch(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        set_all_doors(rooms, room, DoorType::Regular);
        let scale = rooms[room]
            .width()
            .wrapping_mul(rooms[room].height())
            .min(100);
        #[allow(clippy::cast_precision_loss)]
        let fill = 0.20_f32 + scale as f32 / 1024.0_f32;
        self.setup_patch(level, rooms, room, fill, 1, true, true, rng);
        self.fill_patch(level, rooms, room, terrain::REGION_DECO);
    }

    fn paint_circle_basin(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        let bounds = rooms[room].bounds;
        let width = rooms[room].width();
        let height = rooms[room].height();
        fill_room(level, &rooms[room], terrain::WALL);
        draw::fill_ellipse(
            &mut level.map,
            bounds.left + 1,
            bounds.top + 1,
            width - 2,
            height - 2,
            terrain::EMPTY,
        );
        let doors = room_doors(rooms, room);
        for (neighbour, door) in doors {
            set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
            let count = if door.x == bounds.left || door.x == bounds.right {
                div_i32(width, 2)
            } else {
                div_i32(height, 2)
            };
            draw::draw_inside(&mut level.map, bounds, door, count, terrain::EMPTY);
        }
        draw::fill_ellipse(
            &mut level.map,
            bounds.left + 3,
            bounds.top + 3,
            width - 6,
            height - 6,
            terrain::CHASM,
        );
        draw::draw_line(
            &mut level.map,
            Point::new(bounds.left + div_i32(width, 2), bounds.top + 3),
            Point::new(bounds.left + div_i32(width, 2), bounds.bottom - 3),
            terrain::EMPTY_SP,
        );
        draw::draw_line(
            &mut level.map,
            Point::new(bounds.left + 3, bounds.top + div_i32(height, 2)),
            Point::new(bounds.right - 3, bounds.top + div_i32(height, 2)),
            terrain::EMPTY_SP,
        );
        if width > 11 || height > 11 {
            let center = rooms[room].center(rng);
            draw::fill(
                &mut level.map,
                center.x - 1,
                center.y - 1,
                3,
                3,
                terrain::EMPTY_SP,
            );
            level.map.set_point(center, terrain::WALL);
        }
        self.setup_patch(level, rooms, room, 0.5, 5, false, false, rng);
        for y in bounds.top + 1..bounds.bottom {
            for x in bounds.left + 1..bounds.right {
                let cell = level.map.cell(x, y);
                if level.map.cells[cell] == terrain::EMPTY
                    && self.patch_at(rooms, room, Point::new(x, y))
                {
                    level.map.cells[cell] = terrain::WATER;
                    let above = usize::try_from(
                        i32::try_from(cell).expect("map exceeds Java int") - level.width(),
                    )
                    .expect("basin interior has an upper row");
                    if level.map.cells[above] == terrain::WALL {
                        level.map.cells[above] = terrain::WALL_DECO;
                    }
                }
            }
        }
    }

    fn paint_ring(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        detail: RingDetail,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        let minimum_dimension = rooms[room].width().min(rooms[room].height());
        #[allow(clippy::cast_precision_loss)]
        let passage_width = f32_to_i32((minimum_dimension + 3) as f32 * 0.2_f32).max(0);
        fill_room_margin(level, &rooms[room], passage_width + 1, terrain::WALL);
        if minimum_dimension >= 10 {
            fill_room_margin(
                level,
                &rooms[room],
                passage_width + 2,
                if detail == RingDetail::Prize {
                    terrain::REGION_DECO_ALT
                } else {
                    terrain::EMPTY_SP
                },
            );
            let mut center = rooms[room].center(rng);
            let mut x_direction = 0_i32;
            let mut y_direction = 0_i32;
            if rng.int_bound(2) == 0 {
                #[allow(clippy::cast_precision_loss)]
                let exact_center = rooms[room]
                    .bounds
                    .left
                    .wrapping_add(rooms[room].bounds.right)
                    as f32
                    / 2.0_f32;
                #[allow(clippy::cast_precision_loss)]
                if (center.x as f32) < exact_center {
                    x_direction = 1;
                } else if (center.x as f32) > exact_center {
                    x_direction = -1;
                } else {
                    x_direction = if rng.int_bound(2) == 0 { 1 } else { -1 };
                }
            } else {
                #[allow(clippy::cast_precision_loss)]
                let exact_center = rooms[room]
                    .bounds
                    .top
                    .wrapping_add(rooms[room].bounds.bottom)
                    as f32
                    / 2.0_f32;
                #[allow(clippy::cast_precision_loss)]
                if (center.y as f32) < exact_center {
                    y_direction = 1;
                } else if (center.y as f32) > exact_center {
                    y_direction = -1;
                } else {
                    y_direction = if rng.int_bound(2) == 0 { 1 } else { -1 };
                }
            }
            level.map.set_point(center, terrain::EMPTY_SP);
            let center_cell = level.point_to_cell(center);
            match detail {
                RingDetail::Prize => {
                    if let Some(item) = self.content.find_prize_item(rng) {
                        level.drop_item(item, center_cell, HeapKind::Heap);
                    }
                }
                RingDetail::Entrance => {
                    level.map.cells[center_cell] = terrain::ENTRANCE_SP;
                    level.add_transition(center_cell, TransitionKind::RegularEntrance);
                }
                RingDetail::Exit => {
                    level.map.cells[center_cell] = terrain::EXIT;
                    level.add_transition(center_cell, TransitionKind::RegularExit);
                }
            }
            center.offset(x_direction, y_direction);
            while level.map.cells[level.point_to_cell(center)] != terrain::WALL {
                level.map.set_point(center, terrain::EMPTY_SP);
                center.offset(x_direction, y_direction);
            }
            level.map.set_point(center, terrain::DOOR);
        }
        set_all_doors(rooms, room, DoorType::Regular);
    }

    fn paint_standard_bridge(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        let bounds = rooms[room].bounds;
        let mut doors_xy = 0_i32;
        let doors = room_doors(rooms, room);
        for &(neighbour, door) in &doors {
            set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
            if door.x == bounds.left || door.x == bounds.right {
                doors_xy += 1;
            } else {
                doors_xy -= 1;
            }
        }
        doors_xy += div_i32(rooms[room].width() - rooms[room].height(), 2);
        let vertical_space = doors_xy > 0 || (doors_xy == 0 && rng.int_bound(2) == 0);
        let (space, bridge) = if vertical_space {
            let mut points: Vec<Point> = doors
                .iter()
                .filter_map(|(_, door)| {
                    (door.y == bounds.top || door.y == bounds.bottom).then_some(*door)
                })
                .collect();
            points.push(Point::new(bounds.left + 1, 0));
            points.push(Point::new(bounds.right - 1, 0));
            points.sort_by_key(|point| point.x);
            let (mut start, mut end) = largest_gap(&points, true);
            let maximum = if rooms[room].width() >= 8 { 3 } else { 2 };
            while end - start > maximum + 1 {
                if rng.int_bound(2) == 0 {
                    start += 1;
                } else {
                    end -= 1;
                }
            }
            let space = Rect::new(start + 1, bounds.top + 1, end, bounds.bottom);
            let bridge_y = rng.normal_int_range(space.top + 1, space.bottom - 2);
            let bridge = Rect::new(space.left, bridge_y, space.right, bridge_y + 1);
            (space, bridge)
        } else {
            let mut points: Vec<Point> = doors
                .iter()
                .filter_map(|(_, door)| {
                    (door.x == bounds.left || door.x == bounds.right).then_some(*door)
                })
                .collect();
            points.push(Point::new(0, bounds.top + 1));
            points.push(Point::new(0, bounds.bottom - 1));
            points.sort_by_key(|point| point.y);
            let (mut start, mut end) = largest_gap(&points, false);
            let maximum = if rooms[room].height() >= 8 { 3 } else { 2 };
            while end - start > maximum + 1 {
                if rng.int_bound(2) == 0 {
                    start += 1;
                } else {
                    end -= 1;
                }
            }
            let space = Rect::new(bounds.left + 1, start + 1, bounds.right, end);
            let bridge_x = rng.normal_int_range(space.left + 1, space.right - 2);
            let bridge = Rect::new(bridge_x, space.top, bridge_x + 1, space.bottom);
            (space, bridge)
        };
        draw::fill_rect(&mut level.map, space, terrain::WATER);
        draw::fill_rect(&mut level.map, bridge, terrain::EMPTY_SP);
        let state = self.ensure_state(room);
        state.bridge_space = Some(space);
        state.bridge = Some(bridge);
    }

    fn place_water_bridge_transition(
        &mut self,
        level: &mut Level,
        rooms: &[Room],
        room: RoomId,
        entrance: bool,
        rng: &mut RandomStack,
    ) {
        let space = self
            .state(room)
            .bridge_space
            .expect("bridge space exists before transition placement");
        let point = loop {
            let point = random_room_point(&rooms[room], 2, rng);
            let cell = level.point_to_cell(point);
            if !space.inside(point) && !level.mob_cells[cell] {
                break point;
            }
        };
        let cell = level.point_to_cell(point);
        clear_neighbours8(level, cell, terrain::EMPTY);
        level.map.cells[cell] = if entrance {
            terrain::ENTRANCE
        } else {
            terrain::EXIT
        };
        level.add_transition(
            cell,
            if entrance {
                if level.depth == 1 {
                    TransitionKind::Surface
                } else {
                    TransitionKind::RegularEntrance
                }
            } else {
                TransitionKind::RegularExit
            },
        );
    }

    fn place_patch_transition(
        &mut self,
        level: &mut Level,
        rooms: &[Room],
        room: RoomId,
        entrance: bool,
        rng: &mut RandomStack,
    ) {
        let mut tries = 30_i32;
        let point = loop {
            let point = random_room_point(&rooms[room], 2, rng);
            let cell = level.point_to_cell(point);
            let valid = if tries > 0 {
                level.map.cells[cell] != terrain::REGION_DECO && !level.mob_cells[cell]
            } else {
                let width = level.width();
                let offsets = [-width, -1, 1, width];
                offsets.iter().any(|offset| {
                    let neighbour = usize::try_from(
                        i32::try_from(cell).expect("map exceeds Java int") + offset,
                    )
                    .expect("transition is inside padded map");
                    level.map.cells[neighbour] != terrain::REGION_DECO
                }) && !level.mob_cells[cell]
            };
            tries = tries.wrapping_sub(1);
            if valid {
                break point;
            }
        };
        let cell = level.point_to_cell(point);
        level.map.cells[cell] = if entrance {
            terrain::ENTRANCE
        } else {
            terrain::EXIT
        };
        clear_neighbours8(level, cell, terrain::EMPTY);
        level.add_transition(
            cell,
            if entrance {
                if level.depth == 1 {
                    TransitionKind::Surface
                } else {
                    TransitionKind::RegularEntrance
                }
            } else {
                TransitionKind::RegularExit
            },
        );
    }

    fn paint_sewer_pipe(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        let connection_space = if rooms[room].connected.len() <= 1 {
            let center = rooms[room].center(rng);
            Rect::new(center.x, center.y, center.x, center.y)
        } else {
            let center = door_center(&rooms[room], 2, rng);
            Rect::new(center.x, center.y, center.x, center.y)
        };
        let doors = room_doors(rooms, room);
        if doors.len() == 1
            || (doors.len() == 2 && rooms[room].size_category == Some(SizeCategory::Normal))
        {
            for &(_, door) in &doors {
                let mut start = door;
                if start.x == rooms[room].bounds.left {
                    start.x += 2;
                } else if start.y == rooms[room].bounds.top {
                    start.y += 2;
                } else if start.x == rooms[room].bounds.right {
                    start.x -= 2;
                } else {
                    start.y -= 2;
                }
                let right_shift = if start.x < connection_space.left {
                    connection_space.left - start.x
                } else if start.x > connection_space.right {
                    connection_space.right - start.x
                } else {
                    0
                };
                let down_shift = if start.y < connection_space.top {
                    connection_space.top - start.y
                } else if start.y > connection_space.bottom {
                    connection_space.bottom - start.y
                } else {
                    0
                };
                let (middle, end) =
                    if door.x == rooms[room].bounds.left || door.x == rooms[room].bounds.right {
                        let middle = Point::new(start.x + right_shift, start.y);
                        (middle, Point::new(middle.x, middle.y + down_shift))
                    } else {
                        let middle = Point::new(start.x, start.y + down_shift);
                        (middle, Point::new(middle.x + right_shift, middle.y))
                    };
                draw::draw_line(&mut level.map, start, middle, terrain::WATER);
                draw::draw_line(&mut level.map, middle, end, terrain::WATER);
            }
        } else {
            let mut door_points: Vec<Point> = doors.iter().map(|(_, door)| *door).collect();
            if door_points.len() == 2 {
                let phantom = loop {
                    let point = if rng.int_bound(2) == 0 {
                        Point::new(
                            if rng.int_bound(2) == 0 {
                                rooms[room].bounds.left
                            } else {
                                rooms[room].bounds.right
                            },
                            rng.int_range(
                                rooms[room].bounds.top + 2,
                                rooms[room].bounds.bottom - 2,
                            ),
                        )
                    } else {
                        Point::new(
                            rng.int_range(
                                rooms[room].bounds.left + 2,
                                rooms[room].bounds.right - 2,
                            ),
                            if rng.int_bound(2) == 0 {
                                rooms[room].bounds.top
                            } else {
                                rooms[room].bounds.bottom
                            },
                        )
                    };
                    if doors
                        .iter()
                        .all(|(_, door)| door.x != point.x && door.y != point.y)
                    {
                        break point;
                    }
                };
                door_points.push(phantom);
            }
            let mut points_to_fill: Vec<Point> = door_points
                .into_iter()
                .map(|mut point| {
                    if point.y == rooms[room].bounds.top {
                        point.y += 2;
                    } else if point.y == rooms[room].bounds.bottom {
                        point.y -= 2;
                    } else if point.x == rooms[room].bounds.left {
                        point.x += 2;
                    } else {
                        point.x -= 2;
                    }
                    point
                })
                .collect();
            let mut points_filled = vec![points_to_fill.remove(0)];
            while !points_to_fill.is_empty() {
                let mut shortest = i32::MAX;
                let mut selected = (Point::default(), Point::default(), 0_usize);
                for &from in &points_filled {
                    for (index, &to) in points_to_fill.iter().enumerate() {
                        let distance = perimeter_distance(&rooms[room], from, to, 2);
                        if distance < shortest {
                            shortest = distance;
                            selected = (from, to, index);
                        }
                    }
                }
                fill_perimeter_between(
                    &mut level.map,
                    &rooms[room],
                    selected.0,
                    selected.1,
                    2,
                    terrain::WATER,
                );
                points_filled.push(selected.1);
                points_to_fill.remove(selected.2);
            }
        }
        let width = level.width();
        let neighbours = [
            -width - 1,
            -width,
            -width + 1,
            -1,
            1,
            width - 1,
            width,
            width + 1,
        ];
        for point in rooms[room].bounds.points() {
            let cell = level.point_to_cell(point);
            if level.map.cells[cell] == terrain::WATER {
                let cell = i32::try_from(cell).expect("map exceeds Java int");
                for offset in neighbours {
                    let neighbour =
                        usize::try_from(cell + offset).expect("pipe water lies inside padded map");
                    if level.map.cells[neighbour] == terrain::WALL {
                        level.map.cells[neighbour] = terrain::EMPTY;
                    }
                }
            }
        }
        for (neighbour, door) in doors {
            if matches!(
                rooms[neighbour].kind,
                RoomKind::Standard(StandardRoomKind::SewerPipe)
            ) {
                draw::fill(&mut level.map, door.x - 1, door.y - 1, 3, 3, terrain::EMPTY);
                if door.x == rooms[room].bounds.left || door.x == rooms[room].bounds.right {
                    draw::fill(&mut level.map, door.x - 1, door.y, 3, 1, terrain::WATER);
                } else {
                    draw::fill(&mut level.map, door.x, door.y - 1, 1, 3, terrain::WATER);
                }
                set_shared_door_type(rooms, room, neighbour, DoorType::Water);
            } else {
                set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
            }
        }
    }

    fn paint_tunnel(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        ring: bool,
        rng: &mut RandomStack,
    ) {
        let floor = level.tunnel_tile();
        let connection_space = if ring {
            if let Some(space) = self.ensure_state(room).ring_connection_space {
                space
            } else {
                let mut center = door_center(&rooms[room], 1, rng);
                center.x = center
                    .x
                    .clamp(rooms[room].bounds.left + 2, rooms[room].bounds.right - 2);
                center.y = center
                    .y
                    .clamp(rooms[room].bounds.top + 2, rooms[room].bounds.bottom - 2);
                let space = Rect::new(center.x - 1, center.y - 1, center.x + 1, center.y + 1);
                self.ensure_state(room).ring_connection_space = Some(space);
                space
            }
        } else {
            let center = door_center(&rooms[room], 1, rng);
            Rect::new(center.x, center.y, center.x, center.y)
        };
        for (_, door) in room_doors(rooms, room) {
            let mut start = door;
            if start.x == rooms[room].bounds.left {
                start.x += 1;
            } else if start.y == rooms[room].bounds.top {
                start.y += 1;
            } else if start.x == rooms[room].bounds.right {
                start.x -= 1;
            } else {
                start.y -= 1;
            }
            let right_shift = if start.x < connection_space.left {
                connection_space.left - start.x
            } else if start.x > connection_space.right {
                connection_space.right - start.x
            } else {
                0
            };
            let down_shift = if start.y < connection_space.top {
                connection_space.top - start.y
            } else if start.y > connection_space.bottom {
                connection_space.bottom - start.y
            } else {
                0
            };
            let (middle, end) =
                if door.x == rooms[room].bounds.left || door.x == rooms[room].bounds.right {
                    let middle = Point::new(start.x + right_shift, start.y);
                    (middle, Point::new(middle.x, middle.y + down_shift))
                } else {
                    let middle = Point::new(start.x, start.y + down_shift);
                    (middle, Point::new(middle.x + right_shift, middle.y))
                };
            draw::draw_line(&mut level.map, start, middle, floor);
            draw::draw_line(&mut level.map, middle, end, floor);
        }
        if rooms[room].width() >= 7
            && rooms[room].height() >= 7
            && rooms[room].connected.len() >= 4
            && connection_space.square() == 0
        {
            let cell = level.point_to_cell(Point::new(connection_space.left, connection_space.top));
            let offset_index = usize::try_from(2 * rng.int_bound(4)).unwrap();
            let circle = circle8(level.width());
            let cell_i32 = i32::try_from(cell).expect("map exceeds Java int");
            let before = usize::try_from(cell_i32 + circle[(offset_index + 7) % 8]).unwrap();
            let after = usize::try_from(cell_i32 + circle[(offset_index + 1) % 8]).unwrap();
            if level.map.cells[before] == floor && level.map.cells[after] == floor {
                let target = usize::try_from(cell_i32 + circle[offset_index]).unwrap();
                level.map.cells[target] = floor;
            }
        }
        set_all_doors(rooms, room, DoorType::Tunnel);
    }

    fn paint_plants(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::GRASS);
        fill_room_margin(level, &rooms[room], 2, terrain::HIGH_GRASS);
        if rooms[room].width().min(rooms[room].height()) >= 7 {
            fill_room_margin(level, &rooms[room], 3, terrain::GRASS);
        }
        let center = rooms[room].center(rng);
        let mut plant_points = Vec::new();
        if rooms[room].width().max(rooms[room].height()) >= 9 {
            if rooms[room].width().min(rooms[room].height()) >= 11 {
                draw::draw_line(
                    &mut level.map,
                    Point::new(rooms[room].bounds.left + 2, center.y),
                    Point::new(rooms[room].bounds.right - 2, center.y),
                    terrain::HIGH_GRASS,
                );
                draw::draw_line(
                    &mut level.map,
                    Point::new(center.x, rooms[room].bounds.top + 2),
                    Point::new(center.x, rooms[room].bounds.bottom - 2),
                    terrain::HIGH_GRASS,
                );
                plant_points.extend([
                    Point::new(center.x - 1, center.y - 1),
                    Point::new(center.x + 1, center.y - 1),
                    Point::new(center.x - 1, center.y + 1),
                    Point::new(center.x + 1, center.y + 1),
                ]);
            } else if rooms[room].width() > rooms[room].height()
                || (rooms[room].width() == rooms[room].height() && rng.int_bound(2) == 0)
            {
                draw::draw_line(
                    &mut level.map,
                    Point::new(center.x, rooms[room].bounds.top + 2),
                    Point::new(center.x, rooms[room].bounds.bottom - 2),
                    terrain::HIGH_GRASS,
                );
                plant_points.extend([
                    Point::new(center.x - 1, center.y),
                    Point::new(center.x + 1, center.y),
                ]);
            } else {
                draw::draw_line(
                    &mut level.map,
                    Point::new(rooms[room].bounds.left + 2, center.y),
                    Point::new(rooms[room].bounds.right - 2, center.y),
                    terrain::HIGH_GRASS,
                );
                plant_points.extend([
                    Point::new(center.x, center.y - 1),
                    Point::new(center.x, center.y + 1),
                ]);
            }
        } else {
            plant_points.push(center);
        }
        for point in plant_points {
            let seed = loop {
                let seed = self.content.random_seed_using_defaults(rng);
                if seed != SeedKind::Firebloom {
                    break seed;
                }
            };
            level.plant(seed, level.point_to_cell(point));
        }
        set_all_doors(rooms, room, DoorType::Regular);
    }

    fn paint_aquarium(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        fill_room_margin(level, &rooms[room], 2, terrain::EMPTY_SP);
        fill_room_margin(level, &rooms[room], 3, terrain::WATER);
        let fish_count = div_i32(rooms[room].width().min(rooms[room].height()) - 4, 3);
        for _ in 0..fish_count {
            let phantom = rng.float() < 1.0_f32 / 50.0_f32;
            let cell = loop {
                let point = random_room_point(&rooms[room], 3, rng);
                let cell = level.point_to_cell(point);
                if level.map.cells[cell] == terrain::WATER && !level.mob_cells[cell] {
                    break cell;
                }
            };
            level.add_mob(PaintMob::Piranha { cell, phantom });
        }
        set_all_doors(rooms, room, DoorType::Regular);
    }

    fn paint_platform(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::CHASM);
        let bounds = rooms[room].bounds;
        let mut platforms = Vec::new();
        split_platforms(
            Rect::new(
                bounds.left + 2,
                bounds.top + 2,
                bounds.right - 2,
                bounds.bottom - 2,
            ),
            &mut platforms,
            rng,
        );
        for platform in platforms {
            draw::fill(
                &mut level.map,
                platform.left,
                platform.top,
                platform.width() + 1,
                platform.height() + 1,
                terrain::EMPTY_SP,
            );
        }
        for (neighbour, door) in room_doors(rooms, room) {
            set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
            draw::draw_inside(
                &mut level.map,
                rooms[room].bounds,
                door,
                2,
                terrain::EMPTY_SP,
            );
        }
    }

    fn paint_burned(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        set_all_doors(rooms, room, DoorType::Regular);
        #[allow(clippy::cast_precision_loss)]
        let fill = (1.48_f32
            - (rooms[room].width().wrapping_add(rooms[room].height()) as f32) * 0.03_f32)
            .min(1.0_f32);
        self.setup_patch(level, rooms, room, fill, 2, false, false, rng);
        let mut reveal_increment = 0.0_f32;
        let bounds = rooms[room].bounds;
        for y in bounds.top + 1..bounds.bottom {
            for x in bounds.left + 1..bounds.right {
                let point = Point::new(x, y);
                if !self.patch_at(rooms, room, point) {
                    continue;
                }
                let cell = level.point_to_cell(point);
                match rng.int_bound(5) {
                    0 => level.map.cells[cell] = terrain::EMPTY,
                    1 => level.map.cells[cell] = terrain::EMBERS,
                    2 => {
                        level.map.cells[cell] = terrain::TRAP;
                        level.set_trap(PlacedTrap {
                            spec: TrapSpec::new(TrapKind::Burning),
                            cell,
                            visible: true,
                            active: true,
                        });
                    }
                    3 => {
                        reveal_increment += self.revealed_trap_chance;
                        let visible = reveal_increment >= 1.0;
                        if visible {
                            reveal_increment -= 1.0;
                        }
                        level.map.cells[cell] = if visible {
                            terrain::TRAP
                        } else {
                            terrain::SECRET_TRAP
                        };
                        level.set_trap(PlacedTrap {
                            spec: TrapSpec::new(TrapKind::Burning),
                            cell,
                            visible,
                            active: true,
                        });
                    }
                    _ => {
                        level.map.cells[cell] = terrain::INACTIVE_TRAP;
                        level.set_trap(PlacedTrap {
                            spec: TrapSpec::new(TrapKind::Burning),
                            cell,
                            visible: true,
                            active: false,
                        });
                    }
                }
            }
        }
    }

    fn paint_fissure(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        set_all_doors(rooms, room, DoorType::Regular);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        if rooms[room].width().wrapping_mul(rooms[room].height()) <= 25 {
            let center = rooms[room].center(rng);
            level.map.set_point(center, terrain::CHASM);
            return;
        }
        let smallest = rooms[room].width().min(rooms[room].height());
        let square_root = f64::from(smallest).sqrt();
        let floor_width = f64_to_i32(square_root);
        #[allow(clippy::cast_possible_truncation)]
        let mut edge_floor_chance = (square_root as f32) % 1.0_f32;
        #[allow(clippy::cast_precision_loss)]
        {
            edge_floor_chance =
                (edge_floor_chance + (floor_width - 1) as f32 * 0.5_f32) / floor_width as f32;
        }
        let bounds = rooms[room].bounds;
        for y in bounds.top + 2..=bounds.bottom - 2 {
            for x in bounds.left + 2..=bounds.right - 2 {
                let vertical = (y - bounds.top).min(bounds.bottom - y);
                let horizontal = (x - bounds.left).min(bounds.right - x);
                let edge = vertical.min(horizontal);
                if edge > floor_width || (edge == floor_width && rng.float() > edge_floor_chance) {
                    level.map.set(x, y, terrain::CHASM);
                }
            }
        }
    }

    fn paint_grassy_grave(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        set_all_doors(rooms, room, DoorType::Regular);
        fill_room_margin(level, &rooms[room], 1, terrain::GRASS);
        let width = rooms[room].width() - 2;
        let height = rooms[room].height() - 2;
        let graves = div_i32(width.max(height), 2);
        let prize_index = rng.int_bound(graves);
        let shift = rng.int_bound(2);
        for index in 0..graves {
            let cell_i32 = if width > height {
                rooms[room].bounds.left
                    + 1
                    + shift
                    + index * 2
                    + (rooms[room].bounds.top + 2 + rng.int_bound(height - 2)) * level.width()
            } else {
                rooms[room].bounds.left
                    + 2
                    + rng.int_bound(width - 2)
                    + (rooms[room].bounds.top + 1 + shift + index * 2) * level.width()
            };
            let cell = usize::try_from(cell_i32).expect("grave cell is non-negative");
            let item = if index == prize_index {
                self.content.random_item(rng)
            } else {
                random_gold(rng, i32::try_from(level.depth).unwrap_or(i32::MAX)).into()
            };
            level.drop_item(item, cell, HeapKind::Tomb);
        }
    }

    fn paint_striped(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        set_all_doors(rooms, room, DoorType::Regular);
        if rooms[room].size_category == Some(SizeCategory::Normal) {
            fill_room_margin(level, &rooms[room], 1, terrain::EMPTY_SP);
            if rooms[room].width() > rooms[room].height()
                || (rooms[room].width() == rooms[room].height() && rng.int_bound(2) == 0)
            {
                let mut x = rooms[room].bounds.left + 2;
                while x < rooms[room].bounds.right {
                    draw::fill(
                        &mut level.map,
                        x,
                        rooms[room].bounds.top + 1,
                        1,
                        rooms[room].height() - 2,
                        terrain::HIGH_GRASS,
                    );
                    x += 2;
                }
            } else {
                let mut y = rooms[room].bounds.top + 2;
                while y < rooms[room].bounds.bottom {
                    draw::fill(
                        &mut level.map,
                        rooms[room].bounds.left + 1,
                        y,
                        rooms[room].width() - 2,
                        1,
                        terrain::HIGH_GRASS,
                    );
                    y += 2;
                }
            }
        } else if rooms[room].size_category == Some(SizeCategory::Large) {
            let layers = div_i32(rooms[room].width().min(rooms[room].height()) - 1, 2);
            for layer in 1..=layers {
                fill_room_margin(
                    level,
                    &rooms[room],
                    layer,
                    if layer % 2 == 1 {
                        terrain::EMPTY_SP
                    } else {
                        terrain::HIGH_GRASS
                    },
                );
            }
        }
    }

    fn paint_study(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::BOOKSHELF);
        fill_room_margin(level, &rooms[room], 2, terrain::EMPTY_SP);
        for (neighbour, door) in room_doors(rooms, room) {
            draw::draw_inside(
                &mut level.map,
                rooms[room].bounds,
                door,
                2,
                terrain::EMPTY_SP,
            );
            set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
        }
        if rooms[room].size_category == Some(SizeCategory::Large) {
            let pillar_width = div_i32(rooms[room].width() - 7, 2);
            let pillar_height = div_i32(rooms[room].height() - 7, 2);
            let bounds = rooms[room].bounds;
            for (x, y, width, height) in [
                (bounds.left + 3, bounds.top + 3, pillar_width, 1),
                (bounds.left + 3, bounds.top + 3, 1, pillar_height),
                (bounds.left + 3, bounds.bottom - 3, pillar_width, 1),
                (
                    bounds.left + 3,
                    bounds.bottom - 2 - pillar_height,
                    1,
                    pillar_height,
                ),
                (
                    bounds.right - 2 - pillar_width,
                    bounds.top + 3,
                    pillar_width,
                    1,
                ),
                (bounds.right - 3, bounds.top + 3, 1, pillar_height),
                (
                    bounds.right - 2 - pillar_width,
                    bounds.bottom - 3,
                    pillar_width,
                    1,
                ),
                (
                    bounds.right - 3,
                    bounds.bottom - 2 - pillar_height,
                    1,
                    pillar_height,
                ),
            ] {
                draw::fill(&mut level.map, x, y, width, height, terrain::BOOKSHELF);
            }
        }
        let center = rooms[room].center(rng);
        let cell = level.point_to_cell(center);
        level.map.cells[cell] = terrain::PEDESTAL;
        let prize = if rng.int_bound(2) == 0 {
            self.content.find_prize_item(rng)
        } else {
            None
        };
        let item = prize.unwrap_or_else(|| {
            let category = if rng.int_bound(2) == 0 {
                GeneratorCategory::Potion
            } else {
                GeneratorCategory::Scroll
            };
            self.content.random_category(category, rng)
        });
        level.drop_item(item, cell, HeapKind::Heap);
    }

    fn paint_suspicious_chest(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        set_all_doors(rooms, room, DoorType::Regular);
        let item = self.content.find_prize_item(rng).unwrap_or_else(|| {
            random_gold(rng, i32::try_from(level.depth).unwrap_or(i32::MAX)).into()
        });
        let center = rooms[room].center(rng);
        let cell = level.point_to_cell(center);
        level.map.cells[cell] = terrain::PEDESTAL;
        if rng.float() < 1.0_f32 / 3.0_f32 {
            let reward = self.content.random_mimic_reward(rng);
            level.add_mob(PaintMob::Mimic {
                cell,
                items: vec![item, reward],
            });
        } else {
            level.drop_item(item, cell, HeapKind::Chest);
        }
    }

    fn paint_minefield(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        set_all_doors(rooms, room, DoorType::Regular);
        let square = rooms[room].width().wrapping_mul(rooms[room].height());
        let mut mines = i32::try_from(round_f64(f64::from(square).sqrt())).unwrap_or(i32::MAX);
        mines += match rooms[room].size_category {
            Some(SizeCategory::Normal) => -3,
            Some(SizeCategory::Large) => 3,
            Some(SizeCategory::Giant) => 9,
            None => 0,
        };
        let neighbours = [
            -level.width() - 1,
            -level.width(),
            -level.width() + 1,
            -1,
            1,
            level.width() - 1,
            level.width(),
            level.width() + 1,
        ];
        let mut reveal_increment = 0.0_f32;
        for _ in 0..mines {
            let cell = loop {
                let cell = level.point_to_cell(random_room_point(&rooms[room], 1, rng));
                if !level.traps.iter().any(|trap| trap.cell == cell) {
                    break cell;
                }
            };
            let cell_i32 = i32::try_from(cell).expect("map exceeds Java int");
            for _ in 0..8 {
                let offset = neighbours[usize::try_from(rng.int_bound(8)).unwrap()];
                let neighbour =
                    usize::try_from(cell_i32 + offset).expect("mine lies inside padded map");
                if !level.traps.iter().any(|trap| trap.cell == neighbour)
                    && level.map.cells[neighbour] == terrain::EMPTY
                {
                    level.map.cells[neighbour] = terrain::EMBERS;
                }
            }
            reveal_increment += self.revealed_trap_chance;
            let visible = reveal_increment >= 1.0;
            if visible {
                reveal_increment -= 1.0;
            }
            level.map.cells[cell] = if visible {
                terrain::TRAP
            } else {
                terrain::SECRET_TRAP
            };
            level.set_trap(PlacedTrap {
                spec: TrapSpec::new(TrapKind::Explosive),
                cell,
                visible,
                active: true,
            });
        }
    }
}

fn split_platforms(platform: Rect, output: &mut Vec<Rect>, rng: &mut RandomStack) {
    let area = (platform.width() + 1).wrapping_mul(platform.height() + 1);
    #[allow(clippy::cast_precision_loss)]
    let split_chance = (area - 25) as f32 / 11.0_f32;
    if rng.float() < split_chance {
        if platform.width() > platform.height()
            || (platform.width() == platform.height() && rng.int_bound(2) == 0)
        {
            let split = rng.int_range(platform.left + 2, platform.right - 2);
            split_platforms(
                Rect::new(platform.left, platform.top, split - 1, platform.bottom),
                output,
                rng,
            );
            split_platforms(
                Rect::new(split + 1, platform.top, platform.right, platform.bottom),
                output,
                rng,
            );
            let bridge_y = rng.normal_int_range(platform.top, platform.bottom);
            output.push(Rect::new(split - 1, bridge_y, split + 1, bridge_y));
        } else {
            let split = rng.int_range(platform.top + 2, platform.bottom - 2);
            split_platforms(
                Rect::new(platform.left, platform.top, platform.right, split - 1),
                output,
                rng,
            );
            split_platforms(
                Rect::new(platform.left, split + 1, platform.right, platform.bottom),
                output,
                rng,
            );
            let bridge_x = rng.normal_int_range(platform.left, platform.right);
            output.push(Rect::new(bridge_x, split - 1, bridge_x, split + 1));
        }
    } else {
        output.push(platform);
    }
}

fn door_center(room: &Room, margin: i32, rng: &mut RandomStack) -> Point {
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    for connection in &room.connected {
        let point = connection.door.expect("door is placed").point;
        #[allow(clippy::cast_precision_loss)]
        {
            x += point.x as f32;
            y += point.y as f32;
        }
    }
    let count = i32::try_from(room.connected.len()).expect("connection count exceeds Java int");
    let mut center = Point::new(div_i32(f32_to_i32(x), count), div_i32(f32_to_i32(y), count));
    if rng.float() < x % 1.0_f32 {
        center.x += 1;
    }
    if rng.float() < y % 1.0_f32 {
        center.y += 1;
    }
    center.x = center
        .x
        .clamp(room.bounds.left + margin, room.bounds.right - margin);
    center.y = center
        .y
        .clamp(room.bounds.top + margin, room.bounds.bottom - margin);
    center
}

fn circle8(width: i32) -> [i32; 8] {
    [
        -width - 1,
        -width,
        -width + 1,
        1,
        width + 1,
        width,
        width - 1,
        -1,
    ]
}

fn paint_perimeter(level: &mut Level, rooms: &[Room], room: RoomId, floor: i32) {
    let mut points_to_fill: Vec<Point> = room_doors(rooms, room)
        .into_iter()
        .map(|(_, mut point)| {
            if point.y == rooms[room].bounds.top {
                point.y += 1;
            } else if point.y == rooms[room].bounds.bottom {
                point.y -= 1;
            } else if point.x == rooms[room].bounds.left {
                point.x += 1;
            } else {
                point.x -= 1;
            }
            point
        })
        .collect();
    let mut points_filled = vec![points_to_fill.remove(0)];
    while !points_to_fill.is_empty() {
        let mut shortest = i32::MAX;
        let mut selected = (Point::default(), Point::default(), 0_usize);
        for &from in &points_filled {
            for (index, &to) in points_to_fill.iter().enumerate() {
                let distance = perimeter_distance(&rooms[room], from, to, 1);
                if distance < shortest {
                    shortest = distance;
                    selected = (from, to, index);
                }
            }
        }
        fill_perimeter_between(
            &mut level.map,
            &rooms[room],
            selected.0,
            selected.1,
            1,
            floor,
        );
        points_filled.push(selected.1);
        points_to_fill.remove(selected.2);
    }
}

fn space_between(first: i32, second: i32) -> i32 {
    first.wrapping_sub(second).abs().wrapping_sub(1)
}

fn perimeter_distance(room: &Room, first: Point, second: Point, inset: i32) -> i32 {
    let bounds = room.bounds;
    if ((first.x == bounds.left + inset || first.x == bounds.right - inset) && first.y == second.y)
        || ((first.y == bounds.top + inset || first.y == bounds.bottom - inset)
            && first.x == second.x)
    {
        return space_between(first.x, second.x).max(space_between(first.y, second.y));
    }
    (space_between(bounds.left, first.x) + space_between(bounds.left, second.x))
        .min(space_between(bounds.right, first.x) + space_between(bounds.right, second.x))
        + (space_between(bounds.top, first.y) + space_between(bounds.top, second.y))
            .min(space_between(bounds.bottom, first.y) + space_between(bounds.bottom, second.y))
        - 1
}

fn fill_perimeter_between(
    map: &mut crate::geometry::GridMap,
    room: &Room,
    from: Point,
    to: Point,
    inset: i32,
    floor: i32,
) {
    let bounds = room.bounds;
    if ((from.x == bounds.left + inset || from.x == bounds.right - inset) && from.x == to.x)
        || ((from.y == bounds.top + inset || from.y == bounds.bottom - inset) && from.y == to.y)
    {
        draw::fill(
            map,
            from.x.min(to.x),
            from.y.min(to.y),
            space_between(from.x, to.x) + 2,
            space_between(from.y, to.y) + 2,
            floor,
        );
        return;
    }
    let corners = [
        Point::new(bounds.left + inset, bounds.top + inset),
        Point::new(bounds.right - inset, bounds.top + inset),
        Point::new(bounds.right - inset, bounds.bottom - inset),
        Point::new(bounds.left + inset, bounds.bottom - inset),
    ];
    for corner in corners {
        if (corner.x == from.x || corner.y == from.y) && (corner.x == to.x || corner.y == to.y) {
            draw::draw_line(map, from, corner, floor);
            draw::draw_line(map, corner, to, floor);
            return;
        }
    }
    let side = if from.y == bounds.top + inset || from.y == bounds.bottom - inset {
        if space_between(bounds.left, from.x) + space_between(bounds.left, to.x)
            <= space_between(bounds.right, from.x) + space_between(bounds.right, to.x)
        {
            Point::new(bounds.left + inset, bounds.top + div_i32(room.height(), 2))
        } else {
            Point::new(bounds.right - inset, bounds.top + div_i32(room.height(), 2))
        }
    } else if space_between(bounds.top, from.y) + space_between(bounds.top, to.y)
        <= space_between(bounds.bottom, from.y) + space_between(bounds.bottom, to.y)
    {
        Point::new(bounds.left + div_i32(room.width(), 2), bounds.top + inset)
    } else {
        Point::new(
            bounds.left + div_i32(room.width(), 2),
            bounds.bottom - inset,
        )
    };
    fill_perimeter_between(map, room, from, side, inset, floor);
    fill_perimeter_between(map, room, side, to, inset, floor);
}

fn connection_maze_index(height: i32, x: i32, y: i32) -> usize {
    usize::try_from(x.wrapping_mul(height).wrapping_add(y))
        .expect("maze coordinate is non-negative")
}

fn connection_maze_valid_move(
    maze: &[bool],
    width: i32,
    height: i32,
    mut x: i32,
    mut y: i32,
    dx: i32,
    dy: i32,
) -> bool {
    let side_x = 1 - dx.abs();
    let side_y = 1 - dy.abs();
    // Column-major cell index plus loop-invariant step/side strides; the
    // in-bounds checks below keep every probed index inside the maze, so the
    // strided adds probe the same cells `connection_maze_index` would.
    let step_stride = dx.wrapping_mul(height).wrapping_add(dy);
    let side_stride = side_x.wrapping_mul(height).wrapping_add(side_y);
    let mut index = x.wrapping_mul(height).wrapping_add(y);
    x += dx;
    y += dy;
    index = index.wrapping_add(step_stride);
    if x <= 0 || x >= width - 1 || y <= 0 || y >= height - 1 {
        return false;
    }
    if maze[cell_index(index)]
        || maze[cell_index(index + side_stride)]
        || maze[cell_index(index - side_stride)]
    {
        return false;
    }
    x += dx;
    y += dy;
    index = index.wrapping_add(step_stride);
    if x <= 0 || x >= width - 1 || y <= 0 || y >= height - 1 {
        return false;
    }
    !maze[cell_index(index)]
        && !maze[cell_index(index + side_stride)]
        && !maze[cell_index(index - side_stride)]
}

fn cell_index(index: i32) -> usize {
    usize::try_from(index).expect("maze coordinate is non-negative")
}

fn connection_maze_direction(
    maze: &[bool],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    random: &mut crate::rng::JavaRandom,
) -> Option<(i32, i32)> {
    if random.next_i32_bound(4) == 0 && connection_maze_valid_move(maze, width, height, x, y, 0, -1)
    {
        return Some((0, -1));
    }
    if random.next_i32_bound(3) == 0 && connection_maze_valid_move(maze, width, height, x, y, 1, 0)
    {
        return Some((1, 0));
    }
    if random.next_i32_bound(2) == 0 && connection_maze_valid_move(maze, width, height, x, y, 0, 1)
    {
        return Some((0, 1));
    }
    connection_maze_valid_move(maze, width, height, x, y, -1, 0).then_some((-1, 0))
}

fn generate_connection_maze(rooms: &[Room], room: RoomId, random: &mut RandomStack) -> Vec<bool> {
    let selected = &rooms[room];
    let width = selected.width();
    let height = selected.height();
    if (crate::maze::MIN_BIT_MAZE_SIDE..=crate::maze::MAX_BIT_MAZE_HEIGHT).contains(&height)
        && width >= crate::maze::MIN_BIT_MAZE_SIDE
    {
        let mut cols = crate::maze::walled_border_cols(width, height);
        for (_, door) in room_doors(rooms, room) {
            let x = usize::try_from(door.x - selected.bounds.left).expect("door is in the room");
            let y = door.y - selected.bounds.top;
            cols[x] &= !(1_u64 << y);
        }
        crate::maze::grow_maze(&mut cols, width, height, random);
        return crate::maze::cols_to_column_major(&cols, height);
    }

    let mut maze = vec![false; usize::try_from(width * height).expect("positive maze size")];
    for x in 0..width {
        for y in 0..height {
            if x == 0 || x == width - 1 || y == 0 || y == height - 1 {
                maze[connection_maze_index(height, x, y)] = true;
            }
        }
    }
    for (_, door) in room_doors(rooms, room) {
        maze[connection_maze_index(
            height,
            door.x - selected.bounds.left,
            door.y - selected.bounds.top,
        )] = false;
    }

    // The wall-growing loop only exits after 2,500 consecutive failed draw
    // rounds, so per-draw overhead dominates: hoist the generator and the
    // division-free reciprocals for the loop-invariant pick bounds. Each
    // replacement performs the identical canonical draw sequence.
    let generator = random.current_generator();
    let x_bound = FastBound::new(width);
    let y_bound = FastBound::new(height);
    let mut failures = 0_i32;
    while failures < 2_500 {
        let (mut x, mut y) = loop {
            let x = generator.next_i32_fast_bound(&x_bound);
            let y = generator.next_i32_fast_bound(&y_bound);
            if maze[connection_maze_index(height, x, y)] {
                break (x, y);
            }
        };
        let Some((dx, dy)) = connection_maze_direction(&maze, width, height, x, y, generator)
        else {
            failures += 1;
            continue;
        };
        failures = 0;
        let mut moves = 0_i32;
        loop {
            x += dx;
            y += dy;
            maze[connection_maze_index(height, x, y)] = true;
            moves += 1;
            if generator.next_i32_bound(moves) != 0
                || !connection_maze_valid_move(&maze, width, height, x, y, dx, dy)
            {
                break;
            }
        }
    }
    maze
}

fn paint_maze_connection(
    level: &mut Level,
    rooms: &mut [Room],
    room: RoomId,
    random: &mut RandomStack,
) {
    fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
    let width = rooms[room].width();
    let height = rooms[room].height();
    let maze = loop {
        let candidate = generate_connection_maze(rooms, room, random);
        let small = width >= 5 && height >= 5 && (width <= 7 || height <= 7);
        if !small || candidate[connection_maze_index(height, width / 2, height / 2)] {
            break candidate;
        }
    };

    fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
    let bounds = rooms[room].bounds;
    for x in 0..width {
        for y in 0..height {
            if maze[connection_maze_index(height, x, y)] {
                level
                    .map
                    .set(bounds.left + x, bounds.top + y, terrain::WALL);
            }
        }
    }
    set_all_doors(rooms, room, DoorType::Hidden);
}

fn is_bridge_kind(kind: RoomKind) -> bool {
    matches!(
        kind,
        RoomKind::Connection(
            ConnectionRoomKind::Bridge
                | ConnectionRoomKind::RingBridge
                | ConnectionRoomKind::Walkway
        )
    )
}

fn paint_bridge_neighbour_edges(level: &mut Level, rooms: &[Room], room: RoomId) {
    for &neighbour in &rooms[room].neighbours {
        if is_bridge_kind(rooms[neighbour].kind) {
            let mut intersection = rooms[room].bounds.intersect(rooms[neighbour].bounds);
            if intersection.width() != 0 {
                intersection.left += 1;
                intersection.right -= 1;
            } else {
                intersection.top += 1;
                intersection.bottom -= 1;
            }
            draw::fill(
                &mut level.map,
                intersection.left,
                intersection.top,
                intersection.width() + 1,
                intersection.height() + 1,
                terrain::CHASM,
            );
        }
    }
}

fn largest_gap(points: &[Point], x_axis: bool) -> (i32, i32) {
    let mut start = -1_i32;
    let mut end = -1_i32;
    for pair in points.windows(2) {
        let first = if x_axis { pair[0].x } else { pair[0].y };
        let second = if x_axis { pair[1].x } else { pair[1].y };
        if end - start < second - first {
            start = first;
            end = second;
        }
    }
    (start, end)
}

fn clear_neighbours8(level: &mut Level, cell: usize, tile: i32) {
    let width = level.width();
    let offsets = [
        -width - 1,
        -width,
        -width + 1,
        -1,
        1,
        width - 1,
        width,
        width + 1,
    ];
    let cell = i32::try_from(cell).expect("map exceeds Java int");
    for offset in offsets {
        let neighbour = usize::try_from(cell + offset).expect("transition lies inside padded map");
        level.map.cells[neighbour] = tile;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RingDetail {
    Prize,
    Entrance,
    Exit,
}

fn room_doors(rooms: &[Room], room: RoomId) -> Vec<(RoomId, Point)> {
    rooms[room]
        .connected
        .iter()
        .map(|connection| {
            (
                connection.room,
                connection
                    .door
                    .expect("door is placed before painting")
                    .point,
            )
        })
        .collect()
}

fn set_all_doors(rooms: &mut [Room], room: RoomId, door_type: DoorType) {
    let neighbours: Vec<RoomId> = rooms[room]
        .connected
        .iter()
        .map(|connection| connection.room)
        .collect();
    for neighbour in neighbours {
        set_shared_door_type(rooms, room, neighbour, door_type);
    }
}

fn random_room_point(room: &Room, margin: i32, rng: &mut RandomStack) -> Point {
    Point::new(
        rng.int_range(room.bounds.left + margin, room.bounds.right - margin),
        rng.int_range(room.bounds.top + margin, room.bounds.bottom - margin),
    )
}

fn patch_coordinate(room: &Room, x: i32, y: i32) -> usize {
    usize::try_from(
        x.wrapping_sub(room.bounds.left)
            .wrapping_sub(1)
            .wrapping_add(
                y.wrapping_sub(room.bounds.top)
                    .wrapping_sub(1)
                    .wrapping_mul(room.width().wrapping_sub(2)),
            ),
    )
    .expect("patch coordinate is negative")
}

fn clean_diagonal_edges(patch: &mut [bool], width: i32) {
    let width = usize::try_from(width).expect("patch width is negative");
    for index in 0..patch.len() - width {
        if !patch[index] {
            continue;
        }
        if index % width != 0
            && patch[index - 1 + width]
            && !(patch[index - 1] || patch[index + width])
        {
            patch[index - 1 + width] = false;
        }
        if (index + 1) % width != 0
            && patch[index + 1 + width]
            && !(patch[index + 1] || patch[index + width])
        {
            patch[index + 1 + width] = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::Feeling;
    use crate::painter::{RegularPainter, SewerPainter};
    use crate::room::{Door, RoomConnection};

    #[derive(Default)]
    struct FixtureContent;

    impl SewerRoomContent for FixtureContent {
        fn find_prize_item(&mut self, _rng: &mut RandomStack) -> Option<PaintItem> {
            Some(GeneratedItem::Gold { quantity: 41 }.into())
        }

        fn random_seed_using_defaults(&mut self, _rng: &mut RandomStack) -> SeedKind {
            SeedKind::Sungrass
        }

        fn random_item(&mut self, _rng: &mut RandomStack) -> PaintItem {
            GeneratedItem::Gold { quantity: 43 }.into()
        }

        fn random_category(
            &mut self,
            _category: GeneratorCategory,
            _rng: &mut RandomStack,
        ) -> PaintItem {
            GeneratedItem::Gold { quantity: 47 }.into()
        }

        fn random_mimic_reward(&mut self, _rng: &mut RandomStack) -> PaintItem {
            GeneratedItem::Gold { quantity: 53 }.into()
        }
    }

    fn fixture_room(kind: RoomKind, category: Option<SizeCategory>, size: i32) -> Vec<Room> {
        let mut constructor_rng = RandomStack::with_base_seed(17);
        let standard_kind = match kind {
            RoomKind::Entrance(kind) | RoomKind::Exit(kind) | RoomKind::Standard(kind) => kind,
            RoomKind::Connection(connection) => {
                let mut room = Room::connection(connection);
                room.bounds = Rect::new(4, 4, 4 + size - 1, 4 + size - 1);
                return attach_two_tunnel_neighbours(room);
            }
            _ => unreachable!(),
        };
        let mut room = Room::standard(standard_kind, &mut constructor_rng);
        room.kind = kind;
        room.size_category = category;
        room.bounds = Rect::new(4, 4, 4 + size - 1, 4 + size - 1);
        attach_two_tunnel_neighbours(room)
    }

    fn attach_two_tunnel_neighbours(mut room: Room) -> Vec<Room> {
        let left = room.bounds.left;
        let right = room.bounds.right;
        let top = room.bounds.top;
        let bottom = room.bounds.bottom;
        let first_door = Door::new(Point::new(left, top + 2));
        let second_door = Door::new(Point::new(right, bottom - 2));
        room.connected = vec![
            RoomConnection {
                room: 1,
                door: Some(first_door),
            },
            RoomConnection {
                room: 2,
                door: Some(second_door),
            },
        ];
        room.neighbours = vec![1, 2];
        let mut first = Room::connection(ConnectionRoomKind::Tunnel);
        first.bounds = Rect::new(left - 2, top, left, bottom);
        first.connected.push(RoomConnection {
            room: 0,
            door: Some(first_door),
        });
        first.neighbours.push(0);
        let mut second = Room::connection(ConnectionRoomKind::Tunnel);
        second.bounds = Rect::new(right, top, right + 2, bottom);
        second.connected.push(RoomConnection {
            room: 0,
            door: Some(second_door),
        });
        second.neighbours.push(0);
        vec![room, first, second]
    }

    fn paint_fixture(
        kind: RoomKind,
        category: Option<SizeCategory>,
        size: i32,
    ) -> (i32, i32, usize) {
        let mut rooms = fixture_room(kind, category, size);
        let mut level = Level::new(3, Feeling::None);
        level.set_size(24, 24);
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        let mut dispatcher = SewerRoomDispatcher::new(FixtureContent);
        dispatcher.paint_room(&mut level, &mut rooms, 0, &mut rng);
        let expected_door = if matches!(kind, RoomKind::Connection(_)) {
            DoorType::Tunnel
        } else {
            DoorType::Regular
        };
        assert!(rooms[0].connected.iter().all(|connection| {
            connection
                .door
                .is_some_and(|door| door.door_type == expected_door)
        }));
        (level.java_map_hash(), rng.int(), level.paint_events.len())
    }

    #[test]
    fn every_reachable_sewer_standard_class_matches_official_map_fixture() {
        let classes = [
            (StandardRoomKind::SewerPipe, SizeCategory::Normal, 9),
            (StandardRoomKind::Ring, SizeCategory::Large, 13),
            (StandardRoomKind::WaterBridge, SizeCategory::Normal, 9),
            (StandardRoomKind::RegionDecoPatch, SizeCategory::Normal, 9),
            (StandardRoomKind::CircleBasin, SizeCategory::Large, 13),
            (StandardRoomKind::Plants, SizeCategory::Normal, 9),
            (StandardRoomKind::Aquarium, SizeCategory::Normal, 9),
            (StandardRoomKind::Platform, SizeCategory::Normal, 9),
            (StandardRoomKind::Burned, SizeCategory::Normal, 9),
            (StandardRoomKind::Fissure, SizeCategory::Normal, 9),
            (StandardRoomKind::GrassyGrave, SizeCategory::Normal, 9),
            (StandardRoomKind::Striped, SizeCategory::Normal, 9),
            (StandardRoomKind::Study, SizeCategory::Normal, 9),
            (StandardRoomKind::SuspiciousChest, SizeCategory::Normal, 9),
            (StandardRoomKind::Minefield, SizeCategory::Normal, 9),
        ];
        let actual: Vec<_> = classes
            .into_iter()
            .map(|(kind, category, size)| {
                paint_fixture(RoomKind::Standard(kind), Some(category), size)
            })
            .collect();
        // Captured by tooling/parity/SewerRoomPaintOracle.java against the
        // official v3.3.8 desktop jar. Plants and GrassyGrave delegate item
        // draws to the explicit content provider, so their outer RNG value is
        // provider-specific; their official map and event counts are pinned.
        let expected = vec![
            (-1_251_822_016, -2_040_237_378, 0),
            (-2_051_997_750, -2_040_237_378, 1),
            (1_869_468_759, -541_705_610, 0),
            (546_340_108, -1_279_993_237, 0),
            (568_023_773, 213_181_497, 0),
            (-2_056_344_892, -541_705_610, 2),
            (-911_301_192, 299_112_627, 1),
            (485_060_859, 1_381_175_215, 0),
            (54_342_669, -2_032_478_675, 22),
            (657_247_178, 489_849_364, 0),
            (-1_268_764_621, 663_543_753, 3),
            (-1_056_382_126, 1_381_175_215, 0),
            (-299_849_428, 1_381_175_215, 1),
            (-1_917_505_278, 1_381_175_215, 1),
            (1_592_076_380, 1_578_891_353, 6),
        ];
        for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
            assert_eq!(actual.0, expected.0, "map hash differs for class {index}");
            assert_eq!(
                actual.2, expected.2,
                "paint events differ for class {index}"
            );
            if index != 5 && index != 10 {
                assert_eq!(actual.1, expected.1, "RNG differs for class {index}");
            }
        }
    }

    #[test]
    fn every_reachable_transition_and_connection_matches_official_fixture() {
        let transitions = [
            (true, StandardRoomKind::WaterBridge, 9),
            (false, StandardRoomKind::WaterBridge, 9),
            (true, StandardRoomKind::RegionDecoPatch, 9),
            (false, StandardRoomKind::RegionDecoPatch, 9),
            (true, StandardRoomKind::Ring, 13),
            (false, StandardRoomKind::Ring, 13),
            (true, StandardRoomKind::CircleBasin, 13),
            (false, StandardRoomKind::CircleBasin, 13),
        ];
        let mut actual: Vec<_> = transitions
            .into_iter()
            .map(|(entrance, kind, size)| {
                paint_fixture(
                    if entrance {
                        RoomKind::Entrance(kind)
                    } else {
                        RoomKind::Exit(kind)
                    },
                    Some(SizeCategory::Large),
                    size,
                )
            })
            .collect();
        for kind in [
            ConnectionRoomKind::Tunnel,
            ConnectionRoomKind::Bridge,
            ConnectionRoomKind::Walkway,
            ConnectionRoomKind::RingTunnel,
            ConnectionRoomKind::RingBridge,
        ] {
            actual.push(paint_fixture(RoomKind::Connection(kind), None, 9));
        }
        // Official output from SewerRoomPaintOracle.java. These classes do not
        // invoke the content provider, so both map and outer RNG are exact.
        assert_eq!(
            actual,
            vec![
                (-50_438_895, 489_849_364, 1),
                (1_777_060_144, 489_849_364, 1),
                (317_161_670, -311_585_061, 1),
                (-436_862_619, -311_585_061, 1),
                (784_238_039, -2_040_237_378, 1),
                (1_176_105_940, -2_040_237_378, 1),
                (1_455_026_012, 213_181_497, 1),
                (1_846_893_913, 213_181_497, 1),
                (986_689_066, -2_040_237_378, 0),
                (819_765_506, -2_040_237_378, 0),
                (-697_177_396, 1_678_708_340, 0),
                (-622_723_153, -2_040_237_378, 0),
                (2_105_788_071, -2_040_237_378, 0),
            ]
        );
    }

    #[test]
    fn assembled_regular_painter_matches_official_sewer_fixture() {
        let mut constructor_rng = RandomStack::with_base_seed(17);
        let mut entrance = Room::standard(StandardRoomKind::WaterBridge, &mut constructor_rng);
        entrance.kind = RoomKind::Entrance(StandardRoomKind::WaterBridge);
        entrance.size_category = Some(SizeCategory::Normal);
        entrance.bounds = Rect::new(0, 0, 8, 8);
        let mut middle = Room::standard(StandardRoomKind::Fissure, &mut constructor_rng);
        middle.size_category = Some(SizeCategory::Normal);
        middle.bounds = Rect::new(8, 0, 16, 8);
        let mut exit = Room::standard(StandardRoomKind::RegionDecoPatch, &mut constructor_rng);
        exit.kind = RoomKind::Exit(StandardRoomKind::RegionDecoPatch);
        exit.size_category = Some(SizeCategory::Normal);
        exit.bounds = Rect::new(16, 0, 24, 8);
        let mut rooms = vec![entrance, middle, exit];
        connect_unplaced(&mut rooms, 0, 1);
        connect_unplaced(&mut rooms, 1, 2);

        let mut level = Level::new(3, Feeling::None);
        let mut random = RandomStack::with_base_seed(0);
        random.push(0x0123_4567_89ab_cdef);
        let mut dispatcher = SewerRoomDispatcher::new(FixtureContent);
        let mut painter = SewerPainter {
            regular: RegularPainter::default(),
        };
        painter
            .paint(&mut level, &mut rooms, &mut dispatcher, &mut random)
            .unwrap();

        // Official SewerRoomPaintOracle.java assembled checkpoint.
        assert_eq!(level.java_map_hash(), 1_921_247_341);
        assert_eq!(random.int(), -25_864_179);
        assert_eq!(level.transitions.len(), 2);
        assert_eq!(level.room_order, [2, 0, 1]);
    }

    #[test]
    fn placement_rules_preserve_room_specific_java_exclusions() {
        use crate::sewer_mob_placement::RoomCharacterRules;

        let mut level = Level::new(3, Feeling::None);
        level.set_size(24, 24);
        let mut dispatcher = SewerRoomDispatcher::new(FixtureContent);

        let bridge_rooms = fixture_room(
            RoomKind::Standard(StandardRoomKind::WaterBridge),
            Some(SizeCategory::Normal),
            9,
        );
        dispatcher.ensure_state(0).bridge_space = Some(Rect::new(7, 7, 9, 9));
        let bridge_cell = Point::new(8, 8);
        assert!(!dispatcher.can_place_item(&level, &bridge_rooms, 0, bridge_cell));
        assert!(!RoomCharacterRules::can_place_character(
            &dispatcher,
            &level,
            &bridge_rooms,
            0,
            bridge_cell
        ));
        assert!(dispatcher.can_place_item(&level, &bridge_rooms, 0, Point::new(5, 5)));

        let plants_rooms = fixture_room(
            RoomKind::Standard(StandardRoomKind::Plants),
            Some(SizeCategory::Normal),
            9,
        );
        let plant_point = Point::new(6, 6);
        level.plant(SeedKind::Sungrass, level.point_to_cell(plant_point));
        assert!(!dispatcher.can_place_item(&level, &plants_rooms, 0, plant_point));

        let aquarium_rooms = fixture_room(
            RoomKind::Standard(StandardRoomKind::Aquarium),
            Some(SizeCategory::Normal),
            9,
        );
        let water_point = Point::new(7, 7);
        let water_cell = level.point_to_cell(water_point);
        level.map.cells[water_cell] = terrain::WATER;
        assert!(!dispatcher.can_place_item(&level, &aquarium_rooms, 0, water_point));

        let exit_rooms = fixture_room(
            RoomKind::Exit(StandardRoomKind::Ring),
            Some(SizeCategory::Large),
            13,
        );
        let exit_point = Point::new(8, 8);
        level.add_transition(level.point_to_cell(exit_point), TransitionKind::RegularExit);
        assert!(dispatcher.can_place_item(&level, &exit_rooms, 0, exit_point));
        assert!(!RoomCharacterRules::can_place_character(
            &dispatcher,
            &level,
            &exit_rooms,
            0,
            exit_point
        ));
    }

    fn connect_unplaced(rooms: &mut [Room], first: RoomId, second: RoomId) {
        rooms[first].connected.push(RoomConnection {
            room: second,
            door: None,
        });
        rooms[second].connected.push(RoomConnection {
            room: first,
            door: None,
        });
        rooms[first].neighbours.push(second);
        rooms[second].neighbours.push(first);
    }
}
