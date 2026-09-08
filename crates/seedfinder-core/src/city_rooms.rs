//! Concrete room painting for regular City floors (depths 16 through 19).
//!
//! The five City standard classes, their four entrance/exit variants, and
//! City decoration are implemented directly from Shattered Pixel Dungeon
//! v4.0.0. Region-independent standard rooms and generic connection rooms
//! reuse [`crate::sewer_rooms::SewerRoomDispatcher`], including its cached
//! ring-tunnel and builder-injected maze state.

#![allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unused_self
)]

use crate::geometry::{Point, Rect, painter as draw, terrain};
use crate::java_math::{div_i32, rem_i32};
use crate::level::{Feeling, Level, TransitionKind, TrapKind, TrapSpec};
use crate::painter::{
    PaintError, RegularPainter, RoomPaintDispatch, fill_room, fill_room_margin,
    set_shared_door_type, standard_can_merge,
};
use crate::rng::RandomStack;
use crate::room::{
    ConnectionRoomKind, DoorType, Room, RoomId, RoomKind, SizeCategory, StandardRoomKind,
};
use crate::sewer_rooms::{SewerRoomContent, SewerRoomDispatcher};

/// Exact City trap classes, constructor flags, and relative weights.
#[must_use]
pub fn city_trap_table() -> (Vec<TrapSpec>, Vec<f32>) {
    (
        vec![
            TrapSpec::new(TrapKind::Frost),
            TrapSpec::new(TrapKind::Storm),
            TrapSpec::new(TrapKind::Corrosion),
            TrapSpec::new(TrapKind::Blazing),
            TrapSpec::new(TrapKind::Disintegration)
                .avoids_hallways()
                .cannot_be_hidden(),
            TrapSpec::new(TrapKind::Rockfall)
                .avoids_hallways()
                .cannot_be_hidden(),
            TrapSpec::new(TrapKind::Flashing).avoids_hallways(),
            TrapSpec::new(TrapKind::Guardian),
            TrapSpec::new(TrapKind::Weakening),
            TrapSpec::new(TrapKind::Disarming),
            TrapSpec::new(TrapKind::Summoning),
            TrapSpec::new(TrapKind::Warping),
            TrapSpec::new(TrapKind::Cursing),
            TrapSpec::new(TrapKind::Pitfall),
            TrapSpec::new(TrapKind::Distortion),
            TrapSpec::new(TrapKind::Gateway).avoids_hallways(),
            TrapSpec::new(TrapKind::Geyser),
        ],
        vec![
            4.0, 4.0, 4.0, 4.0, 4.0, 2.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
    )
}

/// Exact non-special City room dispatcher.
///
/// `common` owns the generator-backed content provider as well as the cached
/// state used by shared patch, ring-tunnel, and maze painters.
#[derive(Debug)]
pub struct CityRoomDispatcher<C> {
    pub common: SewerRoomDispatcher<C>,
}

impl<C> CityRoomDispatcher<C> {
    #[must_use]
    pub const fn new(content: C) -> Self {
        Self {
            common: SewerRoomDispatcher::new(content),
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

    /// Concrete `Room.canPlaceItem` for all rooms handled here.
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
        if is_common_standard(rooms[room].kind) {
            self.common.can_place_item(level, rooms, room, point)
        } else {
            true
        }
    }

    /// Concrete `Room.canPlaceCharacter`, including every exit exclusion.
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

impl<C> crate::sewer_mob_placement::RoomCharacterRules for CityRoomDispatcher<C> {
    fn can_place_character(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        point: Point,
    ) -> bool {
        CityRoomDispatcher::can_place_character(self, level, rooms, room, point)
    }
}

impl<C: SewerRoomContent> RoomPaintDispatch for CityRoomDispatcher<C> {
    fn paint_room(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        match rooms[room].kind {
            RoomKind::Entrance(kind) => self.paint_transition(level, rooms, room, kind, true, rng),
            RoomKind::Exit(kind) => self.paint_transition(level, rooms, room, kind, false, rng),
            RoomKind::Standard(kind) if is_city_standard(kind) => {
                self.paint_city_standard(level, rooms, room, kind, rng);
            }
            kind if is_common_standard(kind) || is_city_connection(kind) => {
                self.common.paint_room(level, rooms, room, rng);
            }
            RoomKind::Special(_) | RoomKind::Secret(_) | RoomKind::Quest(_) => {
                panic!("non-regular City room passed to CityRoomDispatcher")
            }
            RoomKind::Standard(kind) => {
                panic!("non-City standard room passed to CityRoomDispatcher: {kind:?}")
            }
            RoomKind::Connection(kind) => {
                panic!("non-City connection room passed to CityRoomDispatcher: {kind:?}")
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
            RoomKind::Standard(StandardRoomKind::Hallway)
            | RoomKind::Entrance(StandardRoomKind::Hallway)
            | RoomKind::Exit(StandardRoomKind::Hallway) => {
                is_hallway(rooms[other].kind) && standard_can_merge(level, rooms, room, point)
            }
            RoomKind::Standard(kind) | RoomKind::Entrance(kind) | RoomKind::Exit(kind)
                if is_city_standard(kind) =>
            {
                standard_can_merge(level, rooms, room, point)
            }
            kind if is_common_standard(kind) || is_city_connection(kind) => {
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
        if is_common_standard(rooms[room].kind) || is_city_connection(rooms[room].kind) {
            self.common
                .merge(level, rooms, room, other, merge, merge_terrain);
            return;
        }
        draw::fill_rect(&mut level.map, merge, merge_terrain);
        if is_hallway(rooms[room].kind) {
            let door = rooms[room]
                .connection_to(other)
                .and_then(|connection| connection.door)
                .expect("merged HallwayRoom has a shared door");
            level.map.set_point(door.point, terrain::EMPTY_SP);
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

impl<C: SewerRoomContent> CityRoomDispatcher<C> {
    fn paint_city_standard(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        kind: StandardRoomKind,
        rng: &mut RandomStack,
    ) {
        match kind {
            StandardRoomKind::Hallway => self.paint_hallway(level, rooms, room, rng),
            StandardRoomKind::LibraryHall => self.paint_library_hall(level, rooms, room, rng),
            StandardRoomKind::LibraryRing => self.paint_library_ring(level, rooms, room, rng),
            StandardRoomKind::Statues => self.paint_statues(level, rooms, room, rng),
            StandardRoomKind::SegmentedLibrary => {
                self.paint_segmented_library(level, rooms, room, rng);
            }
            _ => unreachable!("caller checks City standard kinds"),
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
            StandardRoomKind::Hallway => {
                // v4.0.0 HallwayRoom paints the transition tile itself; the
                // entrance/exit subclasses only locate it.
                self.paint_hallway(level, rooms, room, rng);
                let wanted = if entrance {
                    terrain::ENTRANCE_SP
                } else {
                    terrain::EXIT
                };
                let transition = rooms[room]
                    .bounds
                    .points()
                    .into_iter()
                    .find(|&point| level.map.cells[level.point_to_cell(point)] == wanted)
                    .expect("HallwayRoom always paints its transition tile");
                place_transition(level, transition, entrance, true);
            }
            StandardRoomKind::LibraryHall => {
                self.paint_library_hall(level, rooms, room, rng);
                loop {
                    let point = random_room_point(&rooms[room], 2, rng);
                    if level.map.cells[level.point_to_cell(point)] == terrain::REGION_DECO {
                        place_transition(level, point, entrance, false);
                        break;
                    }
                }
            }
            StandardRoomKind::LibraryRing => {
                self.paint_library_ring(level, rooms, room, rng);
                fill_room_margin(level, &rooms[room], 5, terrain::EMPTY_SP);
                let mut point = rooms[room].center(rng);
                place_transition(level, point, entrance, entrance);
                let (dx, dy) = if rng.int_bound(2) == 0 {
                    (if rng.int_bound(2) == 0 { 1 } else { -1 }, 0)
                } else {
                    (0, if rng.int_bound(2) == 0 { 1 } else { -1 })
                };
                point.offset(dx, dy);
                while level.map.cells[level.point_to_cell(point)] != terrain::EMPTY {
                    level.map.set_point(point, terrain::EMPTY_SP);
                    point.offset(dx, dy);
                }
            }
            StandardRoomKind::Statues => {
                self.paint_statues(level, rooms, room, rng);
                let point = if rooms[room].width() < 11 && rooms[room].height() < 11 {
                    // A single statue block painted its own transition tile.
                    let wanted = if entrance {
                        terrain::ENTRANCE
                    } else {
                        terrain::EXIT
                    };
                    rooms[room]
                        .bounds
                        .points()
                        .into_iter()
                        .find(|&point| level.map.cells[level.point_to_cell(point)] == wanted)
                        .expect("single-block StatuesRoom paints its transition tile")
                } else {
                    let point = rooms[room].center(rng);
                    let cell = level.point_to_cell(point);
                    let width = level.width();
                    let cell_i32 = i32::try_from(cell).expect("map exceeds Java int indexing");
                    for offset in [
                        -width - 1,
                        -width,
                        -width + 1,
                        -1,
                        1,
                        width - 1,
                        width,
                        width + 1,
                    ] {
                        let neighbour = usize::try_from(cell_i32 + offset)
                            .expect("transition center lies inside the padded room");
                        if level.map.cells[neighbour] != terrain::STATUE {
                            level.map.cells[neighbour] = terrain::EMPTY_DECO;
                        }
                    }
                    point
                };
                place_transition(level, point, entrance, false);
            }
            _ => panic!("non-City transition room passed to CityRoomDispatcher: {kind:?}"),
        }
    }

    fn paint_hallway(
        &self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        let bounds = rooms[room].bounds;
        let mut center = rooms[room].center(rng);
        center.x = center.x.clamp(bounds.left + 2, bounds.right - 2);
        center.y = center.y.clamp(bounds.top + 2, bounds.bottom - 2);
        let connection_space = Rect::new(center.x - 1, center.y - 1, center.x + 1, center.y + 1);

        for (neighbour, door) in room_doors(rooms, room) {
            let mut start = door;
            if start.x == bounds.left {
                start.x += 1;
            } else if start.y == bounds.top {
                start.y += 1;
            } else if start.x == bounds.right {
                start.x -= 1;
            } else if start.y == bounds.bottom {
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
            let (middle, end) = if door.x == bounds.left || door.x == bounds.right {
                let middle = Point::new(start.x + right_shift, start.y);
                (middle, Point::new(middle.x, middle.y + down_shift))
            } else {
                let middle = Point::new(start.x, start.y + down_shift);
                (middle, Point::new(middle.x + right_shift, middle.y))
            };
            draw::draw_line(&mut level.map, start, middle, terrain::EMPTY_SP);
            draw::draw_line(&mut level.map, middle, end, terrain::EMPTY_SP);
            set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
        }
        draw::fill(
            &mut level.map,
            connection_space.left,
            connection_space.top,
            3,
            3,
            terrain::EMPTY_SP,
        );
        let center_tile = match rooms[room].kind {
            RoomKind::Entrance(_) => terrain::ENTRANCE_SP,
            RoomKind::Exit(_) => terrain::EXIT,
            _ if rng.int_bound(2) == 0 => terrain::STATUE_SP,
            _ => terrain::REGION_DECO_ALT,
        };
        level.map.set(
            connection_space.left + 1,
            connection_space.top + 1,
            center_tile,
        );
    }

    fn paint_library_hall(
        &self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        let bounds = rooms[room].bounds;
        let width = rooms[room].width();
        let height = rooms[room].height();
        let mut top_bottom_books = 0.0_f32;
        let mut left_right_books = 0.0_f32;
        if width > height {
            top_bottom_books += (width - height) as f32;
        } else if height > width {
            left_right_books += (height - width) as f32;
        }
        if rem_i32(width, 2) == 0 && rem_i32(height, 2) != 0 {
            top_bottom_books += 2.0;
        } else if rem_i32(width, 2) != 0 && rem_i32(height, 2) == 0 {
            left_right_books += 2.0;
        }
        let doors = room_doors(rooms, room);
        for &(_, door) in &doors {
            if door.x == bounds.left || door.x == bounds.right {
                top_bottom_books += 0.1;
            } else {
                left_right_books += 0.1;
            }
        }
        let left_to_right = top_bottom_books > left_right_books
            || (top_bottom_books == left_right_books && rng.int_bound(2) == 0);
        let major_dimension = if left_to_right { height } else { width };
        let minor_dimension = if left_to_right { width } else { height };
        if !(9..11).contains(&major_dimension) {
            if left_to_right {
                draw::draw_line(
                    &mut level.map,
                    Point::new(bounds.left + 1, bounds.top + 1),
                    Point::new(bounds.right - 1, bounds.top + 1),
                    terrain::BOOKSHELF,
                );
                draw::draw_line(
                    &mut level.map,
                    Point::new(bounds.left + 1, bounds.bottom - 1),
                    Point::new(bounds.right - 1, bounds.bottom - 1),
                    terrain::BOOKSHELF,
                );
            } else {
                draw::draw_line(
                    &mut level.map,
                    Point::new(bounds.left + 1, bounds.top + 1),
                    Point::new(bounds.left + 1, bounds.bottom - 1),
                    terrain::BOOKSHELF,
                );
                draw::draw_line(
                    &mut level.map,
                    Point::new(bounds.right - 1, bounds.top + 1),
                    Point::new(bounds.right - 1, bounds.bottom - 1),
                    terrain::BOOKSHELF,
                );
            }
        }
        let center = rooms[room].center(rng);
        let mut length_inset = 2;
        if major_dimension >= 9 {
            if minor_dimension >= 13 {
                length_inset += 1;
            }
            if left_to_right {
                draw::draw_line(
                    &mut level.map,
                    Point::new(bounds.left + length_inset, center.y - 2),
                    Point::new(bounds.right - length_inset, center.y - 2),
                    terrain::BOOKSHELF,
                );
                draw::draw_line(
                    &mut level.map,
                    Point::new(bounds.left + length_inset, center.y + 2),
                    Point::new(bounds.right - length_inset, center.y + 2),
                    terrain::BOOKSHELF,
                );
            } else {
                draw::draw_line(
                    &mut level.map,
                    Point::new(center.x - 2, bounds.top + length_inset),
                    Point::new(center.x - 2, bounds.bottom - length_inset),
                    terrain::BOOKSHELF,
                );
                draw::draw_line(
                    &mut level.map,
                    Point::new(center.x + 2, bounds.top + length_inset),
                    Point::new(center.x + 2, bounds.bottom - length_inset),
                    terrain::BOOKSHELF,
                );
            }
        }
        if rem_i32(minor_dimension, 2) == 1 && minor_dimension < 9 {
            level.map.set_point(center, terrain::REGION_DECO);
        } else {
            let mut inset = 2;
            if minor_dimension >= 10 {
                inset += 1;
                if minor_dimension >= 13 {
                    inset += 1;
                }
            }
            if left_to_right {
                level
                    .map
                    .set(bounds.left + inset, center.y, terrain::REGION_DECO);
                level
                    .map
                    .set(bounds.right - inset, center.y, terrain::REGION_DECO);
            } else {
                level
                    .map
                    .set(center.x, bounds.top + inset, terrain::REGION_DECO);
                level
                    .map
                    .set(center.x, bounds.bottom - inset, terrain::REGION_DECO);
            }
        }
        for (neighbour, door) in doors {
            draw::draw_inside(&mut level.map, bounds, door, 1, terrain::EMPTY);
            set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
        }
    }

    fn paint_library_ring(
        &self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        _rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::BOOKSHELF);
        fill_room_margin(level, &rooms[room], 2, terrain::EMPTY);
        fill_room_margin(level, &rooms[room], 4, terrain::BOOKSHELF);
        let bounds = rooms[room].bounds;
        if rooms[room].size_category == Some(SizeCategory::Giant) {
            let center = Point::new(
                div_i32(bounds.left + bounds.right, 2),
                div_i32(bounds.top + bounds.bottom, 2),
            );
            draw::fill(
                &mut level.map,
                center.x - 4,
                center.y,
                10,
                2,
                terrain::EMPTY,
            );
            draw::fill(
                &mut level.map,
                center.x,
                center.y - 4,
                2,
                10,
                terrain::EMPTY,
            );
        }
        for (neighbour, door) in room_doors(rooms, room) {
            set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
            draw::draw_inside(&mut level.map, bounds, door, 2, terrain::EMPTY);
        }
    }

    fn paint_statues(
        &self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        set_all_doors(rooms, room, DoorType::Regular);
        let width = rooms[room].width();
        let height = rooms[room].height();
        let columns = div_i32(width + 1, 6);
        let rows = div_i32(height + 1, 6);
        let block_width = div_i32(width - 4 - (columns - 1), columns);
        let block_height = div_i32(height - 4 - (rows - 1), rows);
        let horizontal_spacing = if rem_i32(columns, 2) == rem_i32(width, 2) {
            2
        } else {
            1
        };
        let vertical_spacing = if rem_i32(rows, 2) == rem_i32(height, 2) {
            2
        } else {
            1
        };
        let bounds = rooms[room].bounds;
        for column in 0..columns {
            for row in 0..rows {
                let left = bounds.left + 2 + column * (block_width + horizontal_spacing);
                let top = bounds.top + 2 + row * (block_height + vertical_spacing);
                draw::fill(
                    &mut level.map,
                    left,
                    top,
                    block_width,
                    block_height,
                    terrain::CUSTOM_DECO_EMPTY,
                );
                for point in [
                    Point::new(left, top),
                    Point::new(left + block_width - 1, top),
                    Point::new(left, top + block_height - 1),
                    Point::new(left + block_width - 1, top + block_height - 1),
                ] {
                    level.map.set_point(point, terrain::STATUE);
                }
                let transition_room =
                    matches!(rooms[room].kind, RoomKind::Entrance(_) | RoomKind::Exit(_));
                let single_block = columns == 1 && rows == 1;
                if (block_width >= 5 && block_height >= 5) || (transition_room && single_block) {
                    let mut x = left + div_i32(block_width, 2);
                    if rem_i32(block_width, 2) == 0 && rng.int_bound(2) == 0 {
                        x -= 1;
                    }
                    let mut y = top + div_i32(block_height, 2);
                    if rem_i32(block_height, 2) == 0 && rng.int_bound(2) == 0 {
                        y -= 1;
                    }
                    let tile = match rooms[room].kind {
                        RoomKind::Entrance(_) if single_block => terrain::ENTRANCE,
                        RoomKind::Exit(_) if single_block => terrain::EXIT,
                        _ => terrain::REGION_DECO,
                    };
                    level.map.set(x, y, tile);
                }
            }
        }
    }

    fn paint_segmented_library(
        &self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::BOOKSHELF);
        fill_room_margin(level, &rooms[room], 2, terrain::EMPTY_SP);
        let bounds = rooms[room].bounds;
        for (neighbour, door) in room_doors(rooms, room) {
            set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
            draw::draw_inside(&mut level.map, bounds, door, 2, terrain::EMPTY_SP);
        }
        create_library_walls(
            level,
            Rect::new(
                bounds.left + 2,
                bounds.top + 2,
                bounds.right - 2,
                bounds.bottom - 2,
            ),
            rng,
        );
    }
}

/// City-region wrapper supplying exact water/grass, trap, and decoration
/// behavior from v3.3.8 `CityLevel` and `CityPainter`.
#[derive(Clone, Debug, PartialEq)]
pub struct CityPainter {
    pub regular: RegularPainter,
}

impl CityPainter {
    #[must_use]
    pub fn new(feeling: Feeling, trap_count: i32) -> Self {
        let (classes, chances) = city_trap_table();
        Self {
            regular: RegularPainter::default()
                .set_water(
                    if feeling == Feeling::Water {
                        0.90
                    } else {
                        0.30
                    },
                    4,
                )
                .set_grass(
                    if feeling == Feeling::Grass {
                        0.80
                    } else {
                        0.20
                    },
                    3,
                )
                .set_traps(trap_count, classes, chances),
        }
    }

    /// Paint a City map using an exact composite room dispatcher.
    ///
    /// # Errors
    ///
    /// Returns a structural [`PaintError`] when a special room is disconnected
    /// or a builder edge has no mutually legal door position.
    pub fn paint<D: RoomPaintDispatch>(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        dispatch: &mut D,
        rng: &mut RandomStack,
    ) -> Result<(), PaintError> {
        self.regular
            .paint_with_decorator(level, rooms, dispatch, rng, decorate_city)
    }
}

/// Exact v3.3.8 `CityPainter.decorate` map mutation and draw conditions.
pub fn decorate_city(level: &mut Level, _rooms: &[Room], _order: &[RoomId], rng: &mut RandomStack) {
    let width = usize::try_from(level.width()).expect("level width is positive");
    let depth = i32::try_from(level.depth).expect("depth fits Java int");
    for cell in 0..level.len() - width {
        if level.map.cells[cell] == terrain::EMPTY && rng.int_bound(10) == 0 {
            level.map.cells[cell] = terrain::EMPTY_DECO;
        } else if level.map.cells[cell] == terrain::WALL
            && !wall_stitchable(level.map.cells[cell + width])
            && rng.int_bound(21 - depth) == 0
        {
            level.map.cells[cell] = terrain::WALL_DECO;
        }
    }
}

fn create_library_walls(level: &mut Level, area: Rect, rng: &mut RandomStack) {
    if (area.width() + 1).max(area.height() + 1) < 4
        || (area.width() + 1).min(area.height() + 1) < 3
    {
        return;
    }
    let split_vertical =
        area.width() > area.height() || (area.width() == area.height() && rng.int_bound(2) == 0);
    let mut tries = 10;
    while tries > 0 {
        if split_vertical {
            let split = rng.int_range(area.left + 2, area.right - 2);
            if level.map.cells[level.point_to_cell(Point::new(split, area.top - 1))]
                == terrain::BOOKSHELF
                && level.map.cells[level.point_to_cell(Point::new(split, area.bottom + 1))]
                    == terrain::BOOKSHELF
            {
                draw::draw_line(
                    &mut level.map,
                    Point::new(split, area.top),
                    Point::new(split, area.bottom),
                    terrain::BOOKSHELF,
                );
                let opening = rng.int_range(area.top, area.bottom - 1);
                level.map.set(split, opening, terrain::EMPTY_SP);
                create_library_walls(
                    level,
                    Rect::new(area.left, area.top, split - 1, area.bottom),
                    rng,
                );
                create_library_walls(
                    level,
                    Rect::new(split + 1, area.top, area.right, area.bottom),
                    rng,
                );
                return;
            }
        } else {
            let split = rng.int_range(area.top + 2, area.bottom - 2);
            if level.map.cells[level.point_to_cell(Point::new(area.left - 1, split))]
                == terrain::BOOKSHELF
                && level.map.cells[level.point_to_cell(Point::new(area.right + 1, split))]
                    == terrain::BOOKSHELF
            {
                draw::draw_line(
                    &mut level.map,
                    Point::new(area.left, split),
                    Point::new(area.right, split),
                    terrain::BOOKSHELF,
                );
                let opening = rng.int_range(area.left, area.right - 1);
                level.map.set(opening, split, terrain::EMPTY_SP);
                create_library_walls(
                    level,
                    Rect::new(area.left, area.top, area.right, split - 1),
                    rng,
                );
                create_library_walls(
                    level,
                    Rect::new(area.left, split + 1, area.right, area.bottom),
                    rng,
                );
                // RC1 returns after a successful horizontal split. BETA-4
                // incorrectly retried this now-partitioned rectangle.
                return;
            }
        }
        tries -= 1;
    }
}

fn place_transition(level: &mut Level, point: Point, entrance: bool, special_entrance: bool) {
    let cell = level.point_to_cell(point);
    level.map.cells[cell] = if entrance {
        if special_entrance {
            terrain::ENTRANCE_SP
        } else {
            terrain::ENTRANCE
        }
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

fn random_room_point(room: &Room, margin: i32, rng: &mut RandomStack) -> Point {
    Point::new(
        rng.int_range(room.bounds.left + margin, room.bounds.right - margin),
        rng.int_range(room.bounds.top + margin, room.bounds.bottom - margin),
    )
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
    let neighbours: Vec<_> = rooms[room]
        .connected
        .iter()
        .map(|connection| connection.room)
        .collect();
    for neighbour in neighbours {
        set_shared_door_type(rooms, room, neighbour, door_type);
    }
}

const fn is_city_standard(kind: StandardRoomKind) -> bool {
    matches!(
        kind,
        StandardRoomKind::Hallway
            | StandardRoomKind::LibraryHall
            | StandardRoomKind::LibraryRing
            | StandardRoomKind::Statues
            | StandardRoomKind::SegmentedLibrary
    )
}

const fn is_hallway(kind: RoomKind) -> bool {
    matches!(
        kind,
        RoomKind::Standard(StandardRoomKind::Hallway)
            | RoomKind::Entrance(StandardRoomKind::Hallway)
            | RoomKind::Exit(StandardRoomKind::Hallway)
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

const fn is_city_connection(kind: RoomKind) -> bool {
    matches!(
        kind,
        RoomKind::Connection(
            ConnectionRoomKind::Perimeter
                | ConnectionRoomKind::Walkway
                | ConnectionRoomKind::RingTunnel
                | ConnectionRoomKind::RingBridge
                | ConnectionRoomKind::Maze
        )
    )
}

const fn wall_stitchable(tile: i32) -> bool {
    matches!(
        tile,
        terrain::WALL
            | terrain::WALL_DECO
            | terrain::SECRET_DOOR
            | terrain::LOCKED_EXIT
            | terrain::UNLOCKED_EXIT
            | terrain::BOOKSHELF
    ) || tile == -1
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
                room.bounds = Rect::new(5, 5, 5 + size - 1, 5 + size - 1);
                return attach_two_tunnel_neighbours(room);
            }
            _ => unreachable!(),
        };
        let mut room = Room::standard(standard_kind, &mut constructor_rng);
        room.kind = kind;
        room.size_category = category;
        room.bounds = Rect::new(5, 5, 5 + size - 1, 5 + size - 1);
        attach_two_tunnel_neighbours(room)
    }

    fn attach_two_tunnel_neighbours(mut room: Room) -> Vec<Room> {
        let bounds = room.bounds;
        let first_door = Door::new(Point::new(bounds.left, bounds.top + 2));
        let second_door = Door::new(Point::new(bounds.right, bounds.bottom - 2));
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
        first.bounds = Rect::new(bounds.left - 2, bounds.top, bounds.left, bounds.bottom);
        first.connected.push(RoomConnection {
            room: 0,
            door: Some(first_door),
        });
        first.neighbours.push(0);
        let mut second = Room::connection(ConnectionRoomKind::Tunnel);
        second.bounds = Rect::new(bounds.right, bounds.top, bounds.right + 2, bounds.bottom);
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
        let mut level = Level::new(18, Feeling::None);
        level.set_size(28, 28);
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        let mut dispatcher = CityRoomDispatcher::new(FixtureContent);
        dispatcher.paint_room(&mut level, &mut rooms, 0, &mut rng);
        let expected_door = match kind {
            RoomKind::Connection(ConnectionRoomKind::Maze) => DoorType::Hidden,
            RoomKind::Connection(_) => DoorType::Tunnel,
            _ => DoorType::Regular,
        };
        assert!(rooms[0].connected.iter().all(|connection| {
            connection
                .door
                .is_some_and(|door| door.door_type == expected_door)
        }));
        (level.java_map_hash(), rng.int(), level.paint_events.len())
    }

    #[test]
    fn city_trap_table_and_painter_parameters_are_exact() {
        let (classes, chances) = city_trap_table();
        assert_eq!(
            chances,
            [
                4.0, 4.0, 4.0, 4.0, 4.0, 2.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0
            ]
        );
        assert_eq!(
            classes,
            [
                TrapSpec::new(TrapKind::Frost),
                TrapSpec::new(TrapKind::Storm),
                TrapSpec::new(TrapKind::Corrosion),
                TrapSpec::new(TrapKind::Blazing),
                TrapSpec::new(TrapKind::Disintegration)
                    .avoids_hallways()
                    .cannot_be_hidden(),
                TrapSpec::new(TrapKind::Rockfall)
                    .avoids_hallways()
                    .cannot_be_hidden(),
                TrapSpec::new(TrapKind::Flashing).avoids_hallways(),
                TrapSpec::new(TrapKind::Guardian),
                TrapSpec::new(TrapKind::Weakening),
                TrapSpec::new(TrapKind::Disarming),
                TrapSpec::new(TrapKind::Summoning),
                TrapSpec::new(TrapKind::Warping),
                TrapSpec::new(TrapKind::Cursing),
                TrapSpec::new(TrapKind::Pitfall),
                TrapSpec::new(TrapKind::Distortion),
                TrapSpec::new(TrapKind::Gateway).avoids_hallways(),
                TrapSpec::new(TrapKind::Geyser),
            ]
        );
        let painter = CityPainter::new(Feeling::None, 4);
        assert_eq!(painter.regular.water_fill.to_bits(), 0.30_f32.to_bits());
        assert_eq!(painter.regular.water_smoothness, 4);
        assert_eq!(painter.regular.grass_fill.to_bits(), 0.20_f32.to_bits());
        assert_eq!(painter.regular.grass_smoothness, 3);
    }

    #[test]
    fn every_city_standard_size_variant_matches_official_fixture() {
        let fixtures = [
            (
                StandardRoomKind::Hallway,
                SizeCategory::Normal,
                9,
                (1_226_493_409, 1_381_175_215, 0),
            ),
            (
                StandardRoomKind::LibraryHall,
                SizeCategory::Normal,
                9,
                (-932_441_270, 1_678_708_340, 0),
            ),
            (
                StandardRoomKind::LibraryHall,
                SizeCategory::Large,
                13,
                (779_396_090, 1_678_708_340, 0),
            ),
            (
                StandardRoomKind::LibraryRing,
                SizeCategory::Normal,
                9,
                (1_724_530_540, 1_678_708_340, 0),
            ),
            (
                StandardRoomKind::LibraryRing,
                SizeCategory::Large,
                13,
                (454_467_280, 1_678_708_340, 0),
            ),
            (
                StandardRoomKind::LibraryRing,
                SizeCategory::Giant,
                16,
                (-859_516_671, 1_678_708_340, 0),
            ),
            (
                StandardRoomKind::Statues,
                SizeCategory::Normal,
                9,
                (800_827_478, 1_678_708_340, 0),
            ),
            (
                StandardRoomKind::Statues,
                SizeCategory::Large,
                13,
                (2_099_663_906, 1_678_708_340, 0),
            ),
            (
                StandardRoomKind::Statues,
                SizeCategory::Giant,
                17,
                (-677_839_119, 1_678_708_340, 0),
            ),
            (
                StandardRoomKind::SegmentedLibrary,
                SizeCategory::Large,
                13,
                (-843_143_093, 1_963_573_150, 0),
            ),
            (
                StandardRoomKind::SegmentedLibrary,
                SizeCategory::Giant,
                17,
                (2_043_516_531, -909_398_608, 0),
            ),
        ];
        for (kind, category, size, expected) in fixtures {
            assert_eq!(
                paint_fixture(RoomKind::Standard(kind), Some(category), size),
                expected,
                "official fixture differs for {kind:?} {category:?}"
            );
        }
    }

    #[test]
    fn every_reachable_common_standard_matches_official_fixture() {
        let fixtures = [
            (
                StandardRoomKind::Plants,
                SizeCategory::Normal,
                9,
                (1_686_035_038, -541_705_610, 2),
                true,
            ),
            (
                StandardRoomKind::Plants,
                SizeCategory::Large,
                13,
                (-2_097_853_144, 845_149_648, 4),
                true,
            ),
            (
                StandardRoomKind::Aquarium,
                SizeCategory::Normal,
                9,
                (1_040_478_954, 299_112_627, 1),
                false,
            ),
            (
                StandardRoomKind::Aquarium,
                SizeCategory::Large,
                13,
                (255_679_534, 2_035_918_930, 3),
                false,
            ),
            (
                StandardRoomKind::Platform,
                SizeCategory::Normal,
                9,
                (1_874_203_335, 1_381_175_215, 0),
                false,
            ),
            (
                StandardRoomKind::Platform,
                SizeCategory::Large,
                13,
                (2_073_077_541, -1_221_409_574, 0),
                false,
            ),
            (
                StandardRoomKind::Platform,
                SizeCategory::Giant,
                17,
                (-562_859_979, 1_516_431_125, 0),
                false,
            ),
            (
                StandardRoomKind::Burned,
                SizeCategory::Normal,
                9,
                (-512_764_171, -2_032_478_675, 22),
                false,
            ),
            (
                StandardRoomKind::Burned,
                SizeCategory::Large,
                13,
                (1_880_136_824, 1_965_085_638, 56),
                false,
            ),
            (
                StandardRoomKind::Fissure,
                SizeCategory::Normal,
                9,
                (295_681_688, 489_849_364, 0),
                false,
            ),
            (
                StandardRoomKind::Fissure,
                SizeCategory::Large,
                13,
                (-333_467_645, -1_061_704_019, 0),
                false,
            ),
            (
                StandardRoomKind::Fissure,
                SizeCategory::Giant,
                17,
                (632_683_564, 1_516_431_125, 0),
                false,
            ),
            (
                StandardRoomKind::GrassyGrave,
                SizeCategory::Normal,
                9,
                (1_458_919_567, 663_543_753, 3),
                true,
            ),
            (
                StandardRoomKind::Striped,
                SizeCategory::Normal,
                9,
                (-1_518_851_312, 1_381_175_215, 0),
                false,
            ),
            (
                StandardRoomKind::Striped,
                SizeCategory::Large,
                13,
                (-395_070_548, 1_678_708_340, 0),
                false,
            ),
            (
                StandardRoomKind::Study,
                SizeCategory::Normal,
                9,
                (142_340_854, 1_381_175_215, 1),
                false,
            ),
            (
                StandardRoomKind::Study,
                SizeCategory::Large,
                13,
                (964_990_474, 1_381_175_215, 1),
                false,
            ),
            (
                StandardRoomKind::SuspiciousChest,
                SizeCategory::Normal,
                9,
                (-1_218_699_424, 1_381_175_215, 1),
                false,
            ),
            (
                StandardRoomKind::Minefield,
                SizeCategory::Normal,
                9,
                (-1_759_768_122, 1_578_891_353, 6),
                false,
            ),
            (
                StandardRoomKind::Minefield,
                SizeCategory::Large,
                13,
                (1_327_032_058, -1_939_413_705, 16),
                false,
            ),
        ];
        for (kind, category, size, expected, provider_dependent_rng) in fixtures {
            let actual = paint_fixture(RoomKind::Standard(kind), Some(category), size);
            assert_eq!(actual.0, expected.0, "map differs for {kind:?}");
            assert_eq!(actual.2, expected.2, "events differ for {kind:?}");
            if !provider_dependent_rng {
                assert_eq!(actual.1, expected.1, "RNG differs for {kind:?}");
            }
        }
    }

    #[test]
    fn every_city_transition_size_variant_matches_official_fixture() {
        let fixtures = [
            (
                true,
                StandardRoomKind::Hallway,
                SizeCategory::Normal,
                9,
                (-248_529_876, 1_678_708_340, 1),
            ),
            (
                false,
                StandardRoomKind::Hallway,
                SizeCategory::Normal,
                9,
                (-1_045_250_993, 1_678_708_340, 1),
            ),
            (
                true,
                StandardRoomKind::Statues,
                SizeCategory::Normal,
                9,
                (382_730_428, 1_678_708_340, 1),
            ),
            (
                true,
                StandardRoomKind::Statues,
                SizeCategory::Large,
                13,
                (-26_596_312, 1_678_708_340, 1),
            ),
            (
                false,
                StandardRoomKind::Statues,
                SizeCategory::Normal,
                9,
                (-922_717_315, 1_678_708_340, 1),
            ),
            (
                false,
                StandardRoomKind::Statues,
                SizeCategory::Large,
                13,
                (-1_426_042_327, 1_678_708_340, 1),
            ),
            (
                true,
                StandardRoomKind::LibraryHall,
                SizeCategory::Normal,
                9,
                (1_003_219_504, -2_040_237_378, 1),
            ),
            (
                true,
                StandardRoomKind::LibraryHall,
                SizeCategory::Large,
                13,
                (2_008_770_144, 1_370_505_553, 1),
            ),
            (
                false,
                StandardRoomKind::LibraryHall,
                SizeCategory::Normal,
                9,
                (598_388_913, -2_040_237_378, 1),
            ),
            (
                false,
                StandardRoomKind::LibraryHall,
                SizeCategory::Large,
                13,
                (1_465_913_377, 1_370_505_553, 1),
            ),
            (
                true,
                StandardRoomKind::LibraryRing,
                SizeCategory::Large,
                13,
                (-1_552_342_591, -2_040_237_378, 1),
            ),
            (
                false,
                StandardRoomKind::LibraryRing,
                SizeCategory::Large,
                13,
                (376_886_180, -2_040_237_378, 1),
            ),
        ];
        for (entrance, kind, category, size, expected) in fixtures {
            assert_eq!(
                paint_fixture(
                    if entrance {
                        RoomKind::Entrance(kind)
                    } else {
                        RoomKind::Exit(kind)
                    },
                    Some(category),
                    size,
                ),
                expected,
                "official transition fixture differs for {kind:?} {category:?}"
            );
        }
    }

    #[test]
    fn every_city_connection_including_injected_maze_matches_official_fixture() {
        let fixtures = [
            (
                ConnectionRoomKind::Perimeter,
                (-616_921_130, 1_678_708_340, 0),
            ),
            (
                ConnectionRoomKind::Walkway,
                (-1_193_191_466, 1_678_708_340, 0),
            ),
            (
                ConnectionRoomKind::RingTunnel,
                (-1_911_947_373, -2_040_237_378, 0),
            ),
            (
                ConnectionRoomKind::RingBridge,
                (1_174_971_803, -2_040_237_378, 0),
            ),
            (ConnectionRoomKind::Maze, (-1_409_288_409, 758_821_230, 0)),
        ];
        for (kind, expected) in fixtures {
            assert_eq!(paint_fixture(RoomKind::Connection(kind), None, 9), expected);
        }
    }

    #[test]
    fn city_decoration_matches_official_fixture() {
        let mut constructor_rng = RandomStack::with_base_seed(17);
        let mut room = Room::standard(StandardRoomKind::Hallway, &mut constructor_rng);
        room.size_category = Some(SizeCategory::Normal);
        room.bounds = Rect::new(1, 1, 17, 9);
        let rooms = vec![room];
        let mut level = Level::new(18, Feeling::None);
        level.set_size(20, 12);
        fill_room(&mut level, &rooms[0], terrain::WALL);
        fill_room_margin(&mut level, &rooms[0], 1, terrain::EMPTY);
        draw::fill(&mut level.map, 4, 4, 3, 1, terrain::BOOKSHELF);
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        decorate_city(&mut level, &rooms, &[0], &mut rng);
        assert_eq!(level.java_map_hash(), -2_060_542_595);
        assert_eq!(rng.int(), -358_699_829);
    }

    #[test]
    fn placement_and_hallway_merge_rules_preserve_city_overrides() {
        let mut level = Level::new(18, Feeling::None);
        level.set_size(28, 28);
        let dispatcher = CityRoomDispatcher::new(FixtureContent);
        let exit_rooms = fixture_room(
            RoomKind::Exit(StandardRoomKind::Hallway),
            Some(SizeCategory::Normal),
            9,
        );
        let point = Point::new(8, 8);
        let cell = level.point_to_cell(point);
        level.map.cells[cell] = terrain::EMPTY;
        level.add_transition(cell, TransitionKind::RegularExit);
        assert!(dispatcher.can_place_item(&level, &exit_rooms, 0, point));
        assert!(!dispatcher.can_place_character(&level, &exit_rooms, 0, point));
        assert!(!dispatcher.can_place_item(&level, &exit_rooms, 0, Point::new(5, 5)));

        let mut hallways = fixture_room(
            RoomKind::Standard(StandardRoomKind::Hallway),
            Some(SizeCategory::Normal),
            9,
        );
        hallways[1].kind = RoomKind::Standard(StandardRoomKind::Hallway);
        hallways[1].size_category = Some(SizeCategory::Normal);
        let door = hallways[0].connection_to(1).unwrap().door.unwrap().point;
        let inside = hallways[0].point_inside(door, 1);
        level.map.set_point(inside, terrain::EMPTY);
        assert!(dispatcher.can_merge(&level, &hallways, 0, 1, door, terrain::EMPTY));
        hallways[1].kind = RoomKind::Connection(ConnectionRoomKind::Tunnel);
        assert!(!dispatcher.can_merge(&level, &hallways, 0, 1, door, terrain::EMPTY));
        hallways[1].kind = RoomKind::Standard(StandardRoomKind::Hallway);

        let mut dispatcher = dispatcher;
        dispatcher.merge(
            &mut level,
            &hallways,
            0,
            1,
            Rect::new(door.x, door.y - 1, door.x + 1, door.y + 2),
            terrain::EMPTY,
        );
        assert_eq!(
            level.map.cells[level.point_to_cell(door)],
            terrain::EMPTY_SP
        );
    }

    #[test]
    fn assembled_city_painter_matches_official_fixture() {
        let mut constructor_rng = RandomStack::with_base_seed(17);
        let mut entrance = Room::standard(StandardRoomKind::Hallway, &mut constructor_rng);
        entrance.kind = RoomKind::Entrance(StandardRoomKind::Hallway);
        entrance.size_category = Some(SizeCategory::Normal);
        entrance.bounds = Rect::new(0, 0, 8, 8);
        let mut middle = Room::standard(StandardRoomKind::LibraryHall, &mut constructor_rng);
        middle.size_category = Some(SizeCategory::Normal);
        middle.bounds = Rect::new(8, 0, 16, 8);
        let mut exit = Room::standard(StandardRoomKind::Statues, &mut constructor_rng);
        exit.kind = RoomKind::Exit(StandardRoomKind::Statues);
        exit.size_category = Some(SizeCategory::Normal);
        exit.bounds = Rect::new(16, 0, 24, 8);
        let mut rooms = vec![entrance, middle, exit];
        connect_unplaced(&mut rooms, 0, 1);
        connect_unplaced(&mut rooms, 1, 2);

        let mut level = Level::new(18, Feeling::None);
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        let mut dispatcher = CityRoomDispatcher::new(FixtureContent);
        let mut painter = CityPainter::new(Feeling::None, 3);
        painter
            .paint(&mut level, &mut rooms, &mut dispatcher, &mut rng)
            .unwrap();
        assert_eq!(level.java_map_hash(), 916_998_761);
        assert_eq!(rng.int(), -224_344_019);
        assert_eq!(level.transitions.len(), 2);
        assert_eq!(level.traps.len(), 3);
        assert_eq!(level.heaps.len(), 0);
        assert_eq!(level.room_order, [2, 0, 1]);
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
