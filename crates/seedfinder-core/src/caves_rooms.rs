//! Concrete room painting for regular Caves floors (depths 11 through 14).
//!
//! This module is closed over the exact v3.3.8 classes selectable by the
//! Caves standard, entrance/exit, and connection-room tables. The ten
//! region-independent standard rooms and four generic connection classes
//! delegate to the already parity-pinned implementations in
//! [`crate::sewer_rooms`]. Special, secret, quest, and boss rooms are kept out
//! of this dispatcher rather than approximated.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unused_self
)]

use crate::geometry::{PathFinder, Point, Rect, painter as draw, terrain};
use crate::java_math::{div_i32, f32_to_i32};
use crate::level::{Feeling, Level, TransitionKind, TrapKind, TrapSpec};
use crate::painter::{
    PaintError, RegularPainter, RoomPaintDispatch, fill_room, fill_room_margin, generate_patch,
    merge_rooms, set_shared_door_type, standard_can_merge,
};
use crate::rng::RandomStack;
use crate::room::{
    ConnectionRoomKind, DoorType, Room, RoomId, RoomKind, SizeCategory, StandardRoomKind,
};
use crate::sewer_rooms::{SewerRoomContent, SewerRoomDispatcher};

/// Exact `CavesLevel` trap classes, constructor flags, and relative weights.
#[must_use]
pub fn caves_trap_table() -> (Vec<TrapSpec>, Vec<f32>) {
    (
        vec![
            TrapSpec::new(TrapKind::Burning),
            TrapSpec::new(TrapKind::PoisonDart)
                .avoids_hallways()
                .cannot_be_hidden(),
            TrapSpec::new(TrapKind::Frost),
            TrapSpec::new(TrapKind::Storm),
            TrapSpec::new(TrapKind::Corrosion),
            TrapSpec::new(TrapKind::Gripping).avoids_hallways(),
            TrapSpec::new(TrapKind::Rockfall)
                .avoids_hallways()
                .cannot_be_hidden(),
            TrapSpec::new(TrapKind::Guardian),
            TrapSpec::new(TrapKind::Confusion),
            TrapSpec::new(TrapKind::Summoning),
            TrapSpec::new(TrapKind::Warping),
            TrapSpec::new(TrapKind::Pitfall),
            TrapSpec::new(TrapKind::Gateway).avoids_hallways(),
            TrapSpec::new(TrapKind::Geyser),
        ],
        vec![
            4.0, 4.0, 4.0, 4.0, 4.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
    )
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CavesRoomPaintState {
    patch: Option<Vec<bool>>,
    bridge_space: Option<Rect>,
    bridge: Option<Rect>,
}

/// Exact non-special Caves room dispatcher.
///
/// `common` owns the generator-backed content provider and all state needed
/// by the ten shared standard rooms and generic connection painters.
#[derive(Debug)]
pub struct CavesRoomDispatcher<C> {
    pub common: SewerRoomDispatcher<C>,
    states: Vec<CavesRoomPaintState>,
}

impl<C> CavesRoomDispatcher<C> {
    #[must_use]
    pub const fn new(content: C) -> Self {
        Self {
            common: SewerRoomDispatcher::new(content),
            states: Vec::new(),
        }
    }

    #[must_use]
    pub fn set_revealed_trap_chance(mut self, chance: f32) -> Self {
        self.common = self.common.set_revealed_trap_chance(chance);
        self
    }

    #[must_use]
    pub const fn content(&self) -> &C {
        &self.common.content
    }

    #[must_use]
    pub const fn content_mut(&mut self) -> &mut C {
        &mut self.common.content
    }

    #[must_use]
    pub fn into_content(self) -> C {
        self.common.content
    }

    fn ensure_state(&mut self, room: RoomId) -> &mut CavesRoomPaintState {
        if self.states.len() <= room {
            self.states
                .resize_with(room + 1, CavesRoomPaintState::default);
        }
        &mut self.states[room]
    }

    fn state(&self, room: RoomId) -> &CavesRoomPaintState {
        &self.states[room]
    }

    /// Cached `RegionDecoBridgeRoom.spaceRect`.
    #[must_use]
    pub fn bridge_space(&self, room: RoomId) -> Option<Rect> {
        self.states.get(room).and_then(|state| state.bridge_space)
    }

    /// Cached `RegionDecoBridgeRoom.bridgeRect`.
    #[must_use]
    pub fn bridge_rect(&self, room: RoomId) -> Option<Rect> {
        self.states.get(room).and_then(|state| state.bridge)
    }

    /// Concrete `Room.canPlaceItem` for every handled room.
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
            RoomKind::Standard(StandardRoomKind::RegionDecoBridge)
            | RoomKind::Entrance(StandardRoomKind::RegionDecoBridge)
            | RoomKind::Exit(StandardRoomKind::RegionDecoBridge) => self
                .bridge_space(room)
                .is_none_or(|space| !space.inside(point)),
            RoomKind::Standard(StandardRoomKind::CavesFissure)
            | RoomKind::Entrance(StandardRoomKind::CavesFissure)
            | RoomKind::Exit(StandardRoomKind::CavesFissure) => {
                level.map.cells[level.point_to_cell(point)] != terrain::EMPTY_SP
            }
            kind if is_common_standard(kind) => {
                self.common.can_place_item(level, rooms, room, point)
            }
            _ => true,
        }
    }

    /// Concrete `Room.canPlaceCharacter`, including exit exclusions.
    #[must_use]
    pub fn can_place_character(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        point: Point,
    ) -> bool {
        self.can_place_item(level, rooms, room, point)
            && (!matches!(rooms[room].kind, RoomKind::Exit(_))
                || level.exit() != Some(level.point_to_cell(point)))
    }
}

impl<C> crate::sewer_mob_placement::RoomCharacterRules for CavesRoomDispatcher<C> {
    fn can_place_character(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        point: Point,
    ) -> bool {
        CavesRoomDispatcher::can_place_character(self, level, rooms, room, point)
    }
}

impl<C: SewerRoomContent> RoomPaintDispatch for CavesRoomDispatcher<C> {
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
            RoomKind::Standard(kind) if is_caves_standard(kind) => {
                self.paint_caves_standard(level, rooms, room, kind, rng);
            }
            kind if is_common_standard(kind) || is_caves_connection(kind) => {
                self.common.paint_room(level, rooms, room, rng);
            }
            RoomKind::Special(_) | RoomKind::Secret(_) | RoomKind::Quest(_) => {
                panic!("non-regular Caves room passed to CavesRoomDispatcher")
            }
            RoomKind::Standard(kind) => {
                panic!("non-Caves standard room passed to CavesRoomDispatcher: {kind:?}")
            }
            RoomKind::Connection(kind) => {
                panic!("non-Caves connection room passed to CavesRoomDispatcher: {kind:?}")
            }
        }
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
            RoomKind::Standard(StandardRoomKind::RegionDecoBridge)
            | RoomKind::Entrance(StandardRoomKind::RegionDecoBridge)
            | RoomKind::Exit(StandardRoomKind::RegionDecoBridge) => {
                let cell = level.point_to_cell(rooms[room].point_inside(point, 1));
                level.map.cells[cell] != terrain::REGION_DECO_ALT
            }
            RoomKind::Standard(StandardRoomKind::CavesFissure)
            | RoomKind::Entrance(StandardRoomKind::CavesFissure)
            | RoomKind::Exit(StandardRoomKind::CavesFissure) => {
                merge_terrain == terrain::CHASM || {
                    let cell = level.point_to_cell(rooms[room].point_inside(point, 1));
                    level.map.cells[cell] != terrain::CHASM
                }
            }
            RoomKind::Standard(kind) | RoomKind::Entrance(kind) | RoomKind::Exit(kind)
                if is_caves_standard(kind) =>
            {
                standard_can_merge(level, rooms, room, point)
            }
            kind if is_common_standard(kind) || is_caves_connection(kind) => {
                self.common
                    .can_merge(level, rooms, room, other, point, merge_terrain)
            }
            RoomKind::Connection(_)
            | RoomKind::Special(_)
            | RoomKind::Secret(_)
            | RoomKind::Quest(_)
            | RoomKind::Standard(_)
            | RoomKind::Entrance(_)
            | RoomKind::Exit(_) => false,
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
        if is_common_standard(rooms[room].kind) || is_caves_connection(rooms[room].kind) {
            self.common
                .merge(level, rooms, room, other, merge, merge_terrain);
        } else {
            draw::fill_rect(&mut level.map, merge, merge_terrain);
        }
    }

    fn can_place_water(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        if is_common_standard(rooms[room].kind) {
            self.common.can_place_water(level, rooms, room, point)
        } else {
            true
        }
    }

    fn can_place_grass(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        if is_common_standard(rooms[room].kind) {
            self.common.can_place_grass(level, rooms, room, point)
        } else {
            true
        }
    }

    fn can_place_trap(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        if is_common_standard(rooms[room].kind) {
            self.common.can_place_trap(level, rooms, room, point)
        } else {
            true
        }
    }
}

impl<C: SewerRoomContent> CavesRoomDispatcher<C> {
    fn paint_caves_standard(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        kind: StandardRoomKind,
        rng: &mut RandomStack,
    ) {
        match kind {
            StandardRoomKind::Cave => self.paint_cave(level, rooms, room, rng),
            StandardRoomKind::RegionDecoBridge => {
                self.paint_region_deco_bridge(level, rooms, room, rng);
            }
            StandardRoomKind::CavesFissure => {
                self.paint_caves_fissure(level, rooms, room, rng);
            }
            StandardRoomKind::CirclePit => self.paint_circle_pit(level, rooms, room, rng),
            StandardRoomKind::CircleWall => self.paint_circle_wall(level, rooms, room),
            _ => unreachable!("caller checks Caves standard kinds"),
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
            StandardRoomKind::Cave => {
                self.paint_cave(level, rooms, room, rng);
                self.place_cave_transition(level, rooms, room, entrance, rng);
            }
            StandardRoomKind::RegionDecoBridge => {
                self.paint_region_deco_bridge(level, rooms, room, rng);
                self.place_region_bridge_transition(level, rooms, room, entrance, rng);
            }
            StandardRoomKind::CavesFissure => {
                self.paint_caves_fissure(level, rooms, room, rng);
                self.place_fissure_transition(level, rooms, room, entrance, rng);
            }
            StandardRoomKind::CircleWall => {
                self.paint_circle_wall(level, rooms, room);
                self.place_circle_wall_transition(level, rooms, room, entrance, rng);
            }
            _ => panic!("non-Caves transition room passed to CavesRoomDispatcher: {kind:?}"),
        }
    }

    fn patch_at(&self, rooms: &[Room], room: RoomId, point: Point) -> bool {
        let patch = self.state(room).patch.as_ref().expect("room patch exists");
        patch[patch_coordinate(&rooms[room], point.x, point.y)]
    }

    fn setup_cave_patch(
        &mut self,
        level: &Level,
        rooms: &mut [Room],
        room: RoomId,
        mut fill: f32,
        rng: &mut RandomStack,
    ) {
        let patch_width = rooms[room].width() - 2;
        let patch_height = rooms[room].height() - 2;
        let mut attempts = 0_i32;
        let mut patch;
        loop {
            patch = generate_patch(patch_width, patch_height, fill, 3, true, rng);
            // Java evaluates center() before the connected-door loop even
            // though every legal painted room overwrites the start point.
            let center = rooms[room].center(rng);
            let mut start_point = level.point_to_cell(center);
            for (_, door) in room_doors(rooms, room) {
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
        clean_diagonal_edges(&mut patch, patch_width);
        self.ensure_state(room).patch = Some(patch);
    }

    fn paint_cave(
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
            .min(18 * 18);
        let fill = 0.30_f32 + scale as f32 / 1024.0_f32;
        self.setup_cave_patch(level, rooms, room, fill, rng);
        let bounds = rooms[room].bounds;
        for y in bounds.top + 1..bounds.bottom {
            for x in bounds.left + 1..bounds.right {
                if self.patch_at(rooms, room, Point::new(x, y)) {
                    level.map.set(x, y, terrain::WALL);
                }
            }
        }
    }

    fn paint_region_deco_bridge(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        let bounds = rooms[room].bounds;
        let doors = room_doors(rooms, room);
        let mut doors_xy = 0_i32;
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
            points.extend([
                Point::new(bounds.left + 1, 0),
                Point::new(bounds.right - 1, 0),
            ]);
            points.sort_by_key(|point| point.x);
            let (mut start, mut end) = largest_gap(&points, true);
            while end - start > 2 {
                if rng.int_bound(2) == 0 {
                    start += 1;
                } else {
                    end -= 1;
                }
            }
            let space = Rect::new(start + 1, bounds.top + 1, end, bounds.bottom);
            let y = rng.normal_int_range(space.top + 1, space.bottom - 2);
            (space, Rect::new(space.left, y, space.right, y + 1))
        } else {
            let mut points: Vec<Point> = doors
                .iter()
                .filter_map(|(_, door)| {
                    (door.x == bounds.left || door.x == bounds.right).then_some(*door)
                })
                .collect();
            points.extend([
                Point::new(0, bounds.top + 1),
                Point::new(0, bounds.bottom - 1),
            ]);
            points.sort_by_key(|point| point.y);
            let (mut start, mut end) = largest_gap(&points, false);
            while end - start > 2 {
                if rng.int_bound(2) == 0 {
                    start += 1;
                } else {
                    end -= 1;
                }
            }
            let space = Rect::new(bounds.left + 1, start + 1, bounds.right, end);
            let x = rng.normal_int_range(space.left + 1, space.right - 2);
            (space, Rect::new(x, space.top, x + 1, space.bottom))
        };
        self.ensure_state(room).bridge_space = Some(space);
        self.ensure_state(room).bridge = Some(bridge);
        draw::fill_rect(&mut level.map, space, terrain::REGION_DECO_ALT);
        draw::fill_rect(&mut level.map, bridge, terrain::EMPTY_SP);
    }

    fn paint_caves_fissure(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        let width = rooms[room].width();
        let height = rooms[room].height();
        let bounds = rooms[room].bounds;
        let category = rooms[room]
            .size_category
            .expect("CavesFissureRoom is a StandardRoom");
        let line_count = 1 + category.room_value();

        // Loop-invariant scratch: the connectivity check below runs once per
        // retry over a fixed-size interior grid.
        let interior = usize::try_from((width - 2) * (height - 2)).unwrap();
        let mut passable = vec![false; interior];
        loop {
            fill_room(level, &rooms[room], terrain::WALL);
            fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
            set_all_doors(rooms, room, DoorType::Regular);

            let center_point = rooms[room].center(rng);
            let center = (
                center_point.x as f32 + 0.5_f32,
                center_point.y as f32 + 0.5_f32,
            );
            let doors = room_doors(rooms, room);
            let door_angles: Vec<f32> = doors
                .iter()
                .map(|(_, door)| {
                    angle_between_points(center, (door.x as f32 + 0.5_f32, door.y as f32 + 0.5_f32))
                })
                .collect();

            let mut line_angles = Vec::new();
            for _ in 0..line_count {
                let mut tries = 100_i32;
                loop {
                    let line_angle = rng.float_between(0.0, 360.0);
                    let clear_of_doors = door_angles.iter().all(|&door_angle| {
                        angular_difference(line_angle, door_angle)
                            > if category == SizeCategory::Normal {
                                30.0
                            } else {
                                15.0
                            }
                    });
                    let clear_of_lines = line_angles.iter().all(|&existing| {
                        angular_difference(line_angle, existing)
                            > if line_count == 2 { 120.0 } else { 60.0 }
                    });
                    if clear_of_doors && clear_of_lines {
                        line_angles.push(line_angle);
                        break;
                    }
                    tries -= 1;
                    if tries <= 0 {
                        break;
                    }
                }
            }

            if line_angles.len() < 2 {
                return;
            }

            for &angle in &line_angles {
                let (mut dx, mut dy) = fissure_vector(angle);
                let horizontal = dx.abs() >= dy.abs();
                if horizontal {
                    dy /= dx.abs();
                    dx /= dx.abs();
                } else {
                    dx /= dy.abs();
                    dy /= dy.abs();
                }

                let (mut x, mut y) = center;
                let map_width = usize::try_from(level.width()).expect("level width is positive");
                let mut cell = pointf_cell(x, y, level.width());
                level.map.cells[cell] = terrain::CHASM;
                loop {
                    if horizontal {
                        if level.map.cells[cell - map_width] == terrain::EMPTY
                            && (y % 1.0 <= 0.5 || category == SizeCategory::Giant)
                        {
                            level.map.cells[cell - map_width] = terrain::CHASM;
                        }
                        if level.map.cells[cell] == terrain::EMPTY {
                            level.map.cells[cell] = terrain::CHASM;
                        }
                        if level.map.cells[cell + map_width] == terrain::EMPTY
                            && (y % 1.0 > 0.5 || category == SizeCategory::Giant)
                        {
                            level.map.cells[cell + map_width] = terrain::CHASM;
                        }
                    } else {
                        if level.map.cells[cell - 1] == terrain::EMPTY
                            && (x % 1.0 <= 0.5 || category == SizeCategory::Giant)
                        {
                            level.map.cells[cell - 1] = terrain::CHASM;
                        }
                        if level.map.cells[cell] == terrain::EMPTY {
                            level.map.cells[cell] = terrain::CHASM;
                        }
                        if level.map.cells[cell + 1] == terrain::EMPTY
                            && (x % 1.0 > 0.5 || category == SizeCategory::Giant)
                        {
                            level.map.cells[cell + 1] = terrain::CHASM;
                        }
                    }
                    x += dx;
                    y += dy;
                    cell = pointf_cell(x, y, level.width());
                    if !matches!(level.map.cells[cell], terrain::EMPTY | terrain::CHASM) {
                        break;
                    }
                }
            }

            if line_angles.len() >= 3 {
                let margin = if category == SizeCategory::Giant {
                    2
                } else {
                    1
                };
                let side = margin * 2 + 1;
                draw::fill(
                    &mut level.map,
                    f32_to_i32(center.0) - margin,
                    f32_to_i32(center.1) - margin,
                    side,
                    side,
                    terrain::CHASM,
                );
            }

            if line_angles.len() == 2 {
                let index = usize::try_from(rng.int_bound(2)).unwrap();
                build_fissure_bridge(level, bounds, line_angles[index], center, 1, rng);
            } else {
                for &angle in &line_angles {
                    build_fissure_bridge(level, bounds, angle, center, category.room_value(), rng);
                }
            }

            let mut door_point = 0_usize;
            for (_, door) in doors {
                draw::draw_inside(&mut level.map, bounds, door, 1, terrain::EMPTY);
                let inside = rooms[room].point_inside(door, 1);
                door_point = local_room_cell(&rooms[room], inside);
            }

            passable.fill(false);
            for point in bounds.shrink(1).points() {
                let index = local_room_cell(&rooms[room], point);
                passable[index] = level.map.cells[level.point_to_cell(point)] != terrain::CHASM;
            }
            let pathable =
                PathFinder::all_open_cells_connected(width - 2, height - 2, door_point, |cell| {
                    passable[cell]
                });
            if pathable {
                break;
            }
        }
    }

    fn paint_circle_pit(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        let bounds = rooms[room].bounds;
        fill_room(level, &rooms[room], terrain::WALL);
        draw::fill_ellipse(
            &mut level.map,
            bounds.left + 1,
            bounds.top + 1,
            rooms[room].width() - 2,
            rooms[room].height() - 2,
            terrain::EMPTY,
        );
        for (neighbour, door) in room_doors(rooms, room) {
            set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
            let count = if door.x == bounds.left || door.x == bounds.right {
                div_i32(rooms[room].width(), 2)
            } else {
                div_i32(rooms[room].height(), 2)
            };
            draw::draw_inside(&mut level.map, bounds, door, count, terrain::EMPTY);
        }
        draw::fill_ellipse(
            &mut level.map,
            bounds.left + 3,
            bounds.top + 3,
            rooms[room].width() - 6,
            rooms[room].height() - 6,
            terrain::CHASM,
        );

        let category = rooms[room]
            .size_category
            .expect("CirclePitRoom is a StandardRoom");
        if category != SizeCategory::Normal && rng.int_bound(4 - category.room_value()) == 0 {
            let mut center = rooms[room].center(rng);
            center.x += rng.int_range(-1, 1);
            center.y += rng.int_range(-1, 1);
            let mut edge = center;
            match rng.int_bound(4) {
                0 => edge.x = bounds.left,
                1 => edge.y = bounds.top,
                2 => edge.x = bounds.right,
                _ => edge.y = bounds.bottom,
            }
            if room_doors(rooms, room)
                .iter()
                .all(|(_, door)| *door != edge)
            {
                draw::draw_line(&mut level.map, edge, center, terrain::REGION_DECO_ALT);
                draw::draw_inside(&mut level.map, bounds, edge, 1, terrain::EMPTY_SP);
                level.map.set_point(edge, terrain::WALL);
            }
        }
    }

    fn paint_circle_wall(&self, level: &mut Level, rooms: &mut [Room], room: RoomId) {
        let bounds = rooms[room].bounds;
        fill_room(level, &rooms[room], terrain::WALL);
        draw::fill_ellipse(
            &mut level.map,
            bounds.left + 1,
            bounds.top + 1,
            rooms[room].width() - 2,
            rooms[room].height() - 2,
            terrain::EMPTY,
        );
        for (neighbour, door) in room_doors(rooms, room) {
            set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
            let count = if door.x == bounds.left || door.x == bounds.right {
                div_i32(rooms[room].width(), 2)
            } else {
                div_i32(rooms[room].height(), 2)
            };
            draw::draw_inside(&mut level.map, bounds, door, count, terrain::EMPTY);
        }
        draw::fill_ellipse(
            &mut level.map,
            bounds.left + 3,
            bounds.top + 3,
            rooms[room].width() - 6,
            rooms[room].height() - 6,
            terrain::WALL,
        );
    }

    fn place_cave_transition(
        &self,
        level: &mut Level,
        rooms: &[Room],
        room: RoomId,
        entrance: bool,
        rng: &mut RandomStack,
    ) {
        let mut tries = 30_i32;
        let cell = loop {
            let cell = level.point_to_cell(random_room_point(&rooms[room], 2, rng));
            let primary = tries > 0;
            tries -= 1;
            let valid = if primary {
                level.map.cells[cell] != terrain::WALL && !level.mob_cells[cell]
            } else {
                let cell_i32 = i32::try_from(cell).expect("map exceeds Java int");
                neighbours4(level.width()).iter().any(|offset| {
                    let neighbour = usize::try_from(cell_i32 + offset).unwrap();
                    level.map.cells[neighbour] != terrain::WALL
                }) && !level.mob_cells[cell]
            };
            if valid {
                break cell;
            }
        };
        level.map.cells[cell] = transition_tile(entrance, false);
        clear_neighbours8(level, cell, terrain::EMPTY);
        add_regular_transition(level, cell, entrance);
    }

    fn place_region_bridge_transition(
        &self,
        level: &mut Level,
        rooms: &[Room],
        room: RoomId,
        entrance: bool,
        rng: &mut RandomStack,
    ) {
        let space = self
            .state(room)
            .bridge_space
            .expect("RegionDecoBridgeRoom caches spaceRect during paint");
        let offsets = neighbours8(level.width());
        let cell = loop {
            let point = random_room_point(&rooms[room], 2, rng);
            let cell = level.point_to_cell(point);
            let cell_i32 = i32::try_from(cell).expect("map exceeds Java int");
            if !space.inside(point)
                && offsets.iter().all(|offset| {
                    let neighbour = usize::try_from(cell_i32 + offset).unwrap();
                    level.map.cells[neighbour] != terrain::REGION_DECO_ALT
                })
            {
                break cell;
            }
        };
        level.map.cells[cell] = transition_tile(entrance, false);
        add_regular_transition(level, cell, entrance);
    }

    fn place_fissure_transition(
        &self,
        level: &mut Level,
        rooms: &[Room],
        room: RoomId,
        entrance: bool,
        rng: &mut RandomStack,
    ) {
        let cell = loop {
            let cell = level.point_to_cell(random_room_point(&rooms[room], 2, rng));
            if !matches!(level.map.cells[cell], terrain::CHASM | terrain::EMPTY_SP)
                && !level.mob_cells[cell]
            {
                break cell;
            }
        };
        let cell_i32 = i32::try_from(cell).expect("map exceeds Java int");
        for offset in neighbours4(level.width()) {
            let neighbour = usize::try_from(cell_i32 + offset).unwrap();
            if level.map.cells[neighbour] == terrain::CHASM {
                level.map.cells[neighbour] = terrain::EMPTY;
            }
        }
        level.map.cells[cell] = transition_tile(entrance, false);
        add_regular_transition(level, cell, entrance);
    }

    fn place_circle_wall_transition(
        &self,
        level: &mut Level,
        rooms: &[Room],
        room: RoomId,
        entrance: bool,
        rng: &mut RandomStack,
    ) {
        let mut point = rooms[room].center(rng);
        let cell = level.point_to_cell(point);
        let cell_i32 = i32::try_from(cell).expect("map exceeds Java int");
        for offset in neighbours8(level.width()) {
            let far = usize::try_from(cell_i32 + 2 * offset).unwrap();
            if level.map.cells[far] == terrain::WALL {
                let near = usize::try_from(cell_i32 + offset).unwrap();
                level.map.cells[near] = terrain::EMPTY;
            }
        }
        level.map.cells[cell] = transition_tile(entrance, false);
        add_regular_transition(level, cell, entrance);

        let (x_direction, y_direction) = if rng.int_bound(2) == 0 {
            (if rng.int_bound(2) == 0 { 1 } else { -1 }, 0)
        } else {
            (0, if rng.int_bound(2) == 0 { 1 } else { -1 })
        };
        point.offset(2 * x_direction, 2 * y_direction);
        while level.map.cells[level.point_to_cell(point)] == terrain::WALL {
            level.map.set_point(point, terrain::EMPTY);
            point.offset(x_direction, y_direction);
        }
    }
}

/// Caves-region wrapper supplying exact water/grass parameters, inherited trap
/// table, and `CavesPainter.decorate` behavior.
#[derive(Clone, Debug, PartialEq)]
pub struct CavesPainter {
    pub regular: RegularPainter,
}

impl CavesPainter {
    #[must_use]
    pub fn new(feeling: Feeling, trap_count: i32) -> Self {
        let (classes, chances) = caves_trap_table();
        Self {
            regular: RegularPainter::default()
                .set_water(
                    if feeling == Feeling::Water {
                        0.85
                    } else {
                        0.30
                    },
                    6,
                )
                .set_grass(
                    if feeling == Feeling::Grass {
                        0.65
                    } else {
                        0.15
                    },
                    3,
                )
                .set_traps(trap_count, classes, chances),
        }
    }

    /// Paint a Caves map using the supplied exact room dispatcher.
    ///
    /// # Errors
    ///
    /// Returns a structural [`PaintError`] for disconnected special rooms or
    /// builder edges without a mutually legal door point.
    pub fn paint<D: RoomPaintDispatch>(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        dispatch: &mut D,
        rng: &mut RandomStack,
    ) -> Result<(), PaintError> {
        self.regular
            .paint_with_decorator(level, rooms, dispatch, rng, decorate_caves)
    }
}

/// Exact v3.3.8 `CavesPainter.decorate`, including disconnected-neighbour
/// merges, standard-room corner erosion, floor speckling, and wall gold.
pub fn decorate_caves(level: &mut Level, rooms: &[Room], order: &[RoomId], rng: &mut RandomStack) {
    let mut merge_dispatch = CavesDecorationMergeDispatch;
    for &room in order {
        for &neighbour in &rooms[room].neighbours {
            if rooms[room].connection_to(neighbour).is_none() {
                let tile = if rng.int_bound(3) == 0 {
                    terrain::REGION_DECO
                } else {
                    terrain::CHASM
                };
                merge_rooms(
                    level,
                    rooms,
                    room,
                    neighbour,
                    None,
                    tile,
                    &mut merge_dispatch,
                    rng,
                );
            }
        }
    }

    let width = level.width();
    let width_usize = usize::try_from(width).expect("level width is negative");
    for &room in order {
        if !rooms[room].is_standard() || rooms[room].width() <= 4 || rooms[room].height() <= 4 {
            continue;
        }
        let bounds = rooms[room].bounds;
        let square = rooms[room].width().wrapping_mul(rooms[room].height());
        let corners = [
            (
                Point::new(bounds.left + 1, bounds.top + 1),
                [-1, -width, 1, width],
            ),
            (
                Point::new(bounds.right - 1, bounds.top + 1),
                [1, -width, -1, width],
            ),
            (
                Point::new(bounds.left + 1, bounds.bottom - 1),
                [-1, width, 1, -width],
            ),
            (
                Point::new(bounds.right - 1, bounds.bottom - 1),
                [1, width, -1, -width],
            ),
        ];
        for (corner, offsets) in corners {
            if rng.int_bound(square) <= 8 {
                continue;
            }
            let cell = level.point_to_cell(corner);
            let cell_i32 = i32::try_from(cell).expect("map exceeds Java int");
            let first_wall = usize::try_from(cell_i32 + offsets[0]).unwrap();
            let second_wall = usize::try_from(cell_i32 + offsets[1]).unwrap();
            let first_open = usize::try_from(cell_i32 + offsets[2]).unwrap();
            let second_open = usize::try_from(cell_i32 + offsets[3]).unwrap();
            if terrain::flags(level.map.cells[cell]) & terrain::SOLID == 0
                && level.map.cells[first_wall] == terrain::WALL
                && !has_door_at(rooms, room, level.map.cell_to_point(first_wall))
                && level.map.cells[second_wall] == terrain::WALL
                && !has_door_at(rooms, room, level.map.cell_to_point(second_wall))
                && level.map.cells[first_open] != terrain::TRAP
                && level.map.cells[second_open] != terrain::TRAP
            {
                level.map.cells[cell] = terrain::WALL;
                level.traps.retain(|trap| trap.cell != cell);
            }
        }
    }

    let length = level.len();
    for cell in width_usize + 1..length - width_usize {
        if level.map.cells[cell] == terrain::EMPTY {
            let wall_count = i32::from(level.map.cells[cell + 1] == terrain::WALL)
                + i32::from(level.map.cells[cell - 1] == terrain::WALL)
                + i32::from(level.map.cells[cell + width_usize] == terrain::WALL)
                + i32::from(level.map.cells[cell - width_usize] == terrain::WALL);
            if rng.int_bound(6) <= wall_count {
                level.map.cells[cell] = terrain::EMPTY_DECO;
            }
        }
    }

    for cell in 0..length - width_usize {
        if level.map.cells[cell] == terrain::WALL
            && caves_floor_tile(level.map.cells[cell + width_usize])
            && rng.int_bound(4) == 0
        {
            level.map.cells[cell] = terrain::WALL_DECO;
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct CavesDecorationMergeDispatch;

impl RoomPaintDispatch for CavesDecorationMergeDispatch {
    fn paint_room(
        &mut self,
        _level: &mut Level,
        _rooms: &mut [Room],
        _room: RoomId,
        _rng: &mut RandomStack,
    ) {
        unreachable!("decoration merge dispatcher never paints rooms");
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
            RoomKind::Standard(StandardRoomKind::RegionDecoBridge)
            | RoomKind::Entrance(StandardRoomKind::RegionDecoBridge)
            | RoomKind::Exit(StandardRoomKind::RegionDecoBridge) => {
                let cell = level.point_to_cell(rooms[room].point_inside(point, 1));
                level.map.cells[cell] != terrain::REGION_DECO_ALT
            }
            RoomKind::Standard(StandardRoomKind::CavesFissure)
            | RoomKind::Entrance(StandardRoomKind::CavesFissure)
            | RoomKind::Exit(StandardRoomKind::CavesFissure) => {
                merge_terrain == terrain::CHASM || {
                    let cell = level.point_to_cell(rooms[room].point_inside(point, 1));
                    level.map.cells[cell] != terrain::CHASM
                }
            }
            RoomKind::Standard(StandardRoomKind::Burned | StandardRoomKind::Minefield) => {
                let cell = level.point_to_cell(rooms[room].point_inside(point, 1));
                level.map.cells[cell] == terrain::EMPTY
            }
            RoomKind::Standard(_)
            | RoomKind::Entrance(_)
            | RoomKind::Exit(_)
            | RoomKind::Quest(
                crate::room::QuestRoomKind::RitualSite | crate::room::QuestRoomKind::Blacksmith,
            ) => standard_can_merge(level, rooms, room, point),
            RoomKind::Connection(ConnectionRoomKind::Walkway | ConnectionRoomKind::RingBridge) => {
                merge_terrain == terrain::CHASM
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
        let tile = match rooms[room].kind {
            RoomKind::Standard(StandardRoomKind::Plants | StandardRoomKind::GrassyGrave)
                if merge_terrain == terrain::EMPTY
                    && matches!(
                        rooms[other].kind,
                        RoomKind::Standard(
                            StandardRoomKind::Plants | StandardRoomKind::GrassyGrave
                        )
                    ) =>
            {
                terrain::GRASS
            }
            RoomKind::Standard(StandardRoomKind::Striped)
                if merge_terrain == terrain::EMPTY
                    && matches!(
                        rooms[other].kind,
                        RoomKind::Standard(StandardRoomKind::Striped)
                    ) =>
            {
                terrain::EMPTY_SP
            }
            RoomKind::Standard(StandardRoomKind::Platform)
                if merge_terrain != terrain::CHASM
                    && matches!(
                        rooms[other].kind,
                        RoomKind::Standard(StandardRoomKind::Platform | StandardRoomKind::Chasm)
                    ) =>
            {
                terrain::CHASM
            }
            _ => merge_terrain,
        };
        draw::fill_rect(&mut level.map, merge, tile);
        if matches!(
            rooms[room].kind,
            RoomKind::Standard(StandardRoomKind::Platform)
        ) && tile == terrain::CHASM
        {
            if let Some(door) = rooms[room]
                .connection_to(other)
                .and_then(|connection| connection.door)
            {
                let cell = level.point_to_cell(door.point);
                level.map.cells[cell] = terrain::EMPTY_SP;
            }
        }
    }
}

const fn is_caves_standard(kind: StandardRoomKind) -> bool {
    matches!(
        kind,
        StandardRoomKind::Cave
            | StandardRoomKind::RegionDecoBridge
            | StandardRoomKind::CavesFissure
            | StandardRoomKind::CirclePit
            | StandardRoomKind::CircleWall
    )
}

const fn is_common_standard(kind: RoomKind) -> bool {
    matches!(
        kind,
        RoomKind::Standard(
            StandardRoomKind::Plants
                | StandardRoomKind::Aquarium
                | StandardRoomKind::Platform
                | StandardRoomKind::Burned
                | StandardRoomKind::Fissure
                | StandardRoomKind::GrassyGrave
                | StandardRoomKind::Striped
                | StandardRoomKind::Study
                | StandardRoomKind::SuspiciousChest
                | StandardRoomKind::Minefield
        )
    )
}

const fn is_caves_connection(kind: RoomKind) -> bool {
    matches!(
        kind,
        RoomKind::Connection(
            ConnectionRoomKind::Tunnel
                | ConnectionRoomKind::Walkway
                | ConnectionRoomKind::RingTunnel
                | ConnectionRoomKind::RingBridge
                | ConnectionRoomKind::Maze
        )
    )
}

fn add_regular_transition(level: &mut Level, cell: usize, entrance: bool) {
    level.add_transition(
        cell,
        if entrance {
            TransitionKind::RegularEntrance
        } else {
            TransitionKind::RegularExit
        },
    );
}

const fn transition_tile(entrance: bool, special: bool) -> i32 {
    if entrance {
        if special {
            terrain::ENTRANCE_SP
        } else {
            terrain::ENTRANCE
        }
    } else {
        terrain::EXIT
    }
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
                    .expect("room door is placed before paint")
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
    .expect("patch coordinate is non-negative")
}

fn clean_diagonal_edges(patch: &mut [bool], width: i32) {
    let width = usize::try_from(width).expect("patch width is positive");
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

fn angular_difference(first: f32, second: f32) -> f32 {
    let mut difference = (first - second).abs();
    if difference > 180.0 {
        difference = 360.0 - difference;
    }
    difference
}

fn angle_between_points(from: (f32, f32), to: (f32, f32)) -> f32 {
    const A: f64 = 180.0 / std::f64::consts::PI;
    let slope = (to.1 - from.1) / (to.0 - from.0);
    let mut angle = (A * (f64::from(slope).atan() + std::f64::consts::PI / 2.0)) as f32;
    if from.0 > to.0 {
        angle -= 180.0;
    }
    if angle < 0.0 {
        angle += 360.0;
    }
    angle
}

fn fissure_vector(angle: f32) -> (f32, f32) {
    const A: f64 = 180.0 / std::f64::consts::PI;
    let radians = f64::from(angle) / A - std::f64::consts::PI / 2.0;
    (radians.cos() as f32, radians.sin() as f32)
}

fn pointf_cell(x: f32, y: f32, width: i32) -> usize {
    usize::try_from(f32_to_i32(x) + f32_to_i32(y) * width)
        .expect("fissure point lies inside the level")
}

fn local_room_cell(room: &Room, point: Point) -> usize {
    usize::try_from(
        point.x - room.bounds.left - 1 + (point.y - room.bounds.top - 1) * (room.width() - 2),
    )
    .expect("point lies in room interior")
}

fn build_fissure_bridge(
    level: &mut Level,
    bounds: Rect,
    angle: f32,
    center: (f32, f32),
    center_margin: i32,
    rng: &mut RandomStack,
) {
    let (dx, dy) = fissure_vector(angle);
    let edge_margin = 2;
    if dy.abs() >= dx.abs() {
        let y = if dy > 0.0 {
            rng.int_range(
                f32_to_i32(center.1) + center_margin,
                bounds.bottom - edge_margin,
            )
        } else {
            rng.int_range(
                bounds.top + edge_margin,
                f32_to_i32(center.1) - center_margin,
            )
        };
        let mut found = false;
        if dx <= 0.0 {
            for x in bounds.left + 1..bounds.right {
                let cell = level.map.cell(x, y);
                if level.map.cells[cell] == terrain::CHASM {
                    found = true;
                    level.map.cells[cell] = terrain::EMPTY_SP;
                } else if found {
                    break;
                }
            }
        } else {
            for x in (bounds.left + 1..bounds.right).rev() {
                let cell = level.map.cell(x, y);
                if level.map.cells[cell] == terrain::CHASM {
                    found = true;
                    level.map.cells[cell] = terrain::EMPTY_SP;
                } else if found {
                    break;
                }
            }
        }
    } else {
        let x = if dx > 0.0 {
            rng.int_range(
                f32_to_i32(center.0) + center_margin,
                bounds.right - edge_margin,
            )
        } else {
            rng.int_range(
                bounds.left + edge_margin,
                f32_to_i32(center.0) - center_margin,
            )
        };
        let mut found = false;
        if dy <= 0.0 {
            for y in bounds.top + 1..bounds.bottom {
                let cell = level.map.cell(x, y);
                if level.map.cells[cell] == terrain::CHASM {
                    found = true;
                    level.map.cells[cell] = terrain::EMPTY_SP;
                } else if found {
                    break;
                }
            }
        } else {
            for y in (bounds.top + 1..bounds.bottom).rev() {
                let cell = level.map.cell(x, y);
                if level.map.cells[cell] == terrain::CHASM {
                    found = true;
                    level.map.cells[cell] = terrain::EMPTY_SP;
                } else if found {
                    break;
                }
            }
        }
    }
}

const fn neighbours4(width: i32) -> [i32; 4] {
    [-width, -1, 1, width]
}

const fn neighbours8(width: i32) -> [i32; 8] {
    [
        -width - 1,
        -width,
        -width + 1,
        -1,
        1,
        width - 1,
        width,
        width + 1,
    ]
}

fn clear_neighbours8(level: &mut Level, cell: usize, tile: i32) {
    let cell = i32::try_from(cell).expect("map exceeds Java int");
    for offset in neighbours8(level.width()) {
        let neighbour = usize::try_from(cell + offset).expect("cell lies inside padded map");
        level.map.cells[neighbour] = tile;
    }
}

fn has_door_at(rooms: &[Room], room: RoomId, point: Point) -> bool {
    rooms[room]
        .connected
        .iter()
        .any(|connection| connection.door.is_some_and(|door| door.point == point))
}

const fn caves_floor_tile(tile: i32) -> bool {
    matches!(
        tile,
        terrain::EMPTY
            | terrain::GRASS
            | terrain::EMPTY_WELL
            | terrain::ENTRANCE
            | terrain::EXIT
            | terrain::EMBERS
            | terrain::PEDESTAL
            | terrain::EMPTY_SP
            | terrain::ENTRANCE_SP
            | terrain::SECRET_TRAP
            | terrain::TRAP
            | terrain::INACTIVE_TRAP
            | terrain::CUSTOM_DECO
            | terrain::CUSTOM_DECO_EMPTY
            | terrain::EMPTY_DECO
            | terrain::WELL
            | terrain::WATER
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::{GeneratedItem, SeedKind};
    use crate::level::PaintItem;
    use crate::room::{Door, RoomConnection};
    use crate::run::GeneratorCategory;

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
        let mut level = Level::new(13, Feeling::None);
        level.set_size(24, 24);
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        let mut dispatcher = CavesRoomDispatcher::new(FixtureContent);
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
    fn caves_trap_table_is_exact() {
        let (classes, chances) = caves_trap_table();
        assert_eq!(
            chances,
            [
                4.0, 4.0, 4.0, 4.0, 4.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
            ]
        );
        assert_eq!(
            classes,
            [
                TrapSpec::new(TrapKind::Burning),
                TrapSpec::new(TrapKind::PoisonDart)
                    .avoids_hallways()
                    .cannot_be_hidden(),
                TrapSpec::new(TrapKind::Frost),
                TrapSpec::new(TrapKind::Storm),
                TrapSpec::new(TrapKind::Corrosion),
                TrapSpec::new(TrapKind::Gripping).avoids_hallways(),
                TrapSpec::new(TrapKind::Rockfall)
                    .avoids_hallways()
                    .cannot_be_hidden(),
                TrapSpec::new(TrapKind::Guardian),
                TrapSpec::new(TrapKind::Confusion),
                TrapSpec::new(TrapKind::Summoning),
                TrapSpec::new(TrapKind::Warping),
                TrapSpec::new(TrapKind::Pitfall),
                TrapSpec::new(TrapKind::Gateway).avoids_hallways(),
                TrapSpec::new(TrapKind::Geyser),
            ]
        );
        let painter = CavesPainter::new(Feeling::None, 4);
        assert_eq!(painter.regular.water_fill.to_bits(), 0.30_f32.to_bits());
        assert_eq!(painter.regular.water_smoothness, 6);
        assert_eq!(painter.regular.grass_fill.to_bits(), 0.15_f32.to_bits());
        assert_eq!(painter.regular.grass_smoothness, 3);
    }

    #[test]
    fn every_reachable_caves_standard_class_matches_official_fixture() {
        let classes = [
            (StandardRoomKind::Cave, SizeCategory::Normal, 9),
            (StandardRoomKind::RegionDecoBridge, SizeCategory::Normal, 9),
            (StandardRoomKind::CavesFissure, SizeCategory::Normal, 9),
            (StandardRoomKind::CirclePit, SizeCategory::Large, 13),
            (StandardRoomKind::CircleWall, SizeCategory::Large, 13),
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
        // Temurin 21 output from tooling/parity/CavesRoomPaintOracle.java
        // against the official v3.3.8 jars. Plants and GrassyGrave delegate
        // generator draws to FixtureContent, so only their map/event values
        // (not their outer RNG checkpoints) are provider-independent.
        let expected = vec![
            (1_456_950_750, 1_779_264_910, 0),
            (1_660_104_927, -380_678_566, 0),
            (-1_705_852_308, -380_678_566, 0),
            (-1_758_559_069, -541_705_610, 0),
            (1_372_792_833, 1_678_708_340, 0),
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
    fn caves_specific_size_variants_match_official_fixtures() {
        let variants = [
            (
                StandardRoomKind::Cave,
                SizeCategory::Large,
                13,
                (1_475_499_655, -6_173_782, 0),
            ),
            (
                StandardRoomKind::Cave,
                SizeCategory::Giant,
                17,
                (-1_966_338_519, -1_406_374_303, 0),
            ),
            (
                StandardRoomKind::RegionDecoBridge,
                SizeCategory::Large,
                13,
                (1_437_303_177, -1_079_465_464, 0),
            ),
            (
                StandardRoomKind::CavesFissure,
                SizeCategory::Large,
                13,
                (883_810_497, 1_963_573_150, 0),
            ),
            (
                StandardRoomKind::CavesFissure,
                SizeCategory::Giant,
                17,
                (-649_639_853, 537_584_153, 0),
            ),
            (
                StandardRoomKind::CirclePit,
                SizeCategory::Normal,
                9,
                (-21_247_057, 1_678_708_340, 0),
            ),
            (
                StandardRoomKind::CirclePit,
                SizeCategory::Giant,
                17,
                (-677_269_591, -541_705_610, 0),
            ),
            (
                StandardRoomKind::CircleWall,
                SizeCategory::Giant,
                17,
                (1_246_184_903, 1_678_708_340, 0),
            ),
        ];
        for (kind, category, size, expected) in variants {
            assert_eq!(
                paint_fixture(RoomKind::Standard(kind), Some(category), size),
                expected,
                "official fixture differs for {kind:?} {category:?}"
            );
        }
    }

    #[test]
    fn every_reachable_transition_and_connection_matches_official_fixture() {
        let transitions = [
            (true, StandardRoomKind::Cave, SizeCategory::Normal, 9),
            (false, StandardRoomKind::Cave, SizeCategory::Normal, 9),
            (
                true,
                StandardRoomKind::RegionDecoBridge,
                SizeCategory::Normal,
                9,
            ),
            (
                false,
                StandardRoomKind::RegionDecoBridge,
                SizeCategory::Normal,
                9,
            ),
            (
                true,
                StandardRoomKind::CavesFissure,
                SizeCategory::Normal,
                9,
            ),
            (
                false,
                StandardRoomKind::CavesFissure,
                SizeCategory::Normal,
                9,
            ),
            (true, StandardRoomKind::CircleWall, SizeCategory::Large, 13),
            (false, StandardRoomKind::CircleWall, SizeCategory::Large, 13),
        ];
        let mut actual: Vec<_> = transitions
            .into_iter()
            .map(|(entrance, kind, category, size)| {
                paint_fixture(
                    if entrance {
                        RoomKind::Entrance(kind)
                    } else {
                        RoomKind::Exit(kind)
                    },
                    Some(category),
                    size,
                )
            })
            .collect();
        for kind in [
            ConnectionRoomKind::Tunnel,
            ConnectionRoomKind::Walkway,
            ConnectionRoomKind::RingTunnel,
            ConnectionRoomKind::RingBridge,
        ] {
            actual.push(paint_fixture(RoomKind::Connection(kind), None, 9));
        }
        assert_eq!(
            actual,
            vec![
                (-2_089_527_630, -1_279_993_237, 1),
                (-2_130_482_991, -1_279_993_237, 1),
                (-259_802_727, 489_849_364, 1),
                (1_567_696_312, 489_849_364, 1),
                (-241_672_409, 489_849_364, 1),
                (1_585_826_630, 489_849_364, 1),
                (60_892_402, -2_040_237_378, 1),
                (-693_131_887, -2_040_237_378, 1),
                (986_689_066, -2_040_237_378, 0),
                (-697_177_396, 1_678_708_340, 0),
                (-622_723_153, -2_040_237_378, 0),
                (2_105_788_071, -2_040_237_378, 0),
            ]
        );
    }

    #[test]
    fn assembled_caves_painter_matches_official_fixture() {
        let mut constructor_rng = RandomStack::with_base_seed(17);
        let mut entrance = Room::standard(StandardRoomKind::Cave, &mut constructor_rng);
        entrance.kind = RoomKind::Entrance(StandardRoomKind::Cave);
        entrance.size_category = Some(SizeCategory::Normal);
        entrance.bounds = Rect::new(0, 0, 8, 8);
        let mut middle = Room::standard(StandardRoomKind::CavesFissure, &mut constructor_rng);
        middle.size_category = Some(SizeCategory::Normal);
        middle.bounds = Rect::new(8, 0, 16, 8);
        let mut exit = Room::standard(StandardRoomKind::RegionDecoBridge, &mut constructor_rng);
        exit.kind = RoomKind::Exit(StandardRoomKind::RegionDecoBridge);
        exit.size_category = Some(SizeCategory::Normal);
        exit.bounds = Rect::new(16, 0, 24, 8);
        let mut rooms = vec![entrance, middle, exit];
        connect_unplaced(&mut rooms, 0, 1);
        connect_unplaced(&mut rooms, 1, 2);

        let mut level = Level::new(13, Feeling::None);
        let mut random = RandomStack::with_base_seed(0);
        random.push(0x0123_4567_89ab_cdef);
        let mut dispatcher = CavesRoomDispatcher::new(FixtureContent);
        let mut painter = CavesPainter::new(Feeling::None, 3);
        painter
            .paint(&mut level, &mut rooms, &mut dispatcher, &mut random)
            .unwrap();

        assert_eq!(level.java_map_hash(), 833_288_015);
        assert_eq!(random.int(), -206_599_981);
        assert_eq!(level.transitions.len(), 2);
        assert_eq!(level.traps.len(), 3);
        assert_eq!(level.heaps.len(), 0);
        assert_eq!(level.room_order, [2, 0, 1]);
    }

    #[test]
    fn caves_decoration_and_disconnected_neighbour_merge_match_official_fixture() {
        let mut constructor_rng = RandomStack::with_base_seed(17);
        let mut first = Room::standard(StandardRoomKind::Cave, &mut constructor_rng);
        first.size_category = Some(SizeCategory::Normal);
        first.bounds = Rect::new(1, 1, 9, 9);
        let mut second = Room::standard(StandardRoomKind::CirclePit, &mut constructor_rng);
        second.size_category = Some(SizeCategory::Normal);
        second.bounds = Rect::new(9, 1, 17, 9);
        first.neighbours.push(1);
        second.neighbours.push(0);
        let rooms = vec![first, second];

        let mut level = Level::new(13, Feeling::None);
        level.set_size(20, 12);
        fill_room(&mut level, &rooms[0], terrain::WALL);
        fill_room_margin(&mut level, &rooms[0], 1, terrain::EMPTY);
        fill_room(&mut level, &rooms[1], terrain::WALL);
        fill_room_margin(&mut level, &rooms[1], 1, terrain::EMPTY);

        let mut random = RandomStack::with_base_seed(0);
        random.push(0x0123_4567_89ab_cdef);
        decorate_caves(&mut level, &rooms, &[0, 1], &mut random);
        assert_eq!(level.java_map_hash(), -1_517_373_050);
        assert_eq!(random.int(), -210_791_023);
        assert!(level.traps.is_empty());
    }

    #[test]
    fn placement_rules_preserve_caves_specific_exclusions() {
        let mut level = Level::new(13, Feeling::None);
        level.set_size(24, 24);
        let mut dispatcher = CavesRoomDispatcher::new(FixtureContent);

        let bridge_rooms = fixture_room(
            RoomKind::Standard(StandardRoomKind::RegionDecoBridge),
            Some(SizeCategory::Normal),
            9,
        );
        dispatcher.ensure_state(0).bridge_space = Some(Rect::new(7, 7, 10, 10));
        assert!(!dispatcher.can_place_item(&level, &bridge_rooms, 0, Point::new(8, 8)));
        assert!(dispatcher.can_place_item(&level, &bridge_rooms, 0, Point::new(6, 6)));

        let fissure_rooms = fixture_room(
            RoomKind::Standard(StandardRoomKind::CavesFissure),
            Some(SizeCategory::Normal),
            9,
        );
        let bridge_point = Point::new(8, 8);
        let bridge_cell = level.point_to_cell(bridge_point);
        level.map.cells[bridge_cell] = terrain::EMPTY_SP;
        assert!(!dispatcher.can_place_item(&level, &fissure_rooms, 0, bridge_point));

        let exit_rooms = fixture_room(
            RoomKind::Exit(StandardRoomKind::Cave),
            Some(SizeCategory::Normal),
            9,
        );
        level.add_transition(bridge_cell, TransitionKind::RegularExit);
        assert!(dispatcher.can_place_item(&level, &exit_rooms, 0, bridge_point));
        assert!(!dispatcher.can_place_character(&level, &exit_rooms, 0, bridge_point));
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
