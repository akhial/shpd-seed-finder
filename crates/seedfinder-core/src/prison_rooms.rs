//! Concrete room painting for regular Prison floors (depths 6 through 9).
//!
//! This module is intentionally closed over the exact v3.3.8 classes which
//! can be selected by Prison `StandardRoom`, entrance/exit, and
//! connection-room tables. The ten region-independent standard rooms reuse
//! the already parity-pinned common implementations in [`crate::sewer_rooms`];
//! Prison geometry, transitions, cached bridge state, and decoration remain
//! explicit here. Special, secret, and quest rooms are rejected rather than
//! approximated.

#![allow(
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unused_self
)]

use crate::geometry::{Point, Rect, painter as draw, terrain};
use crate::java_math::{div_i32, round_f32};
use crate::level::{Feeling, Level, TransitionKind, TrapKind, TrapSpec};
use crate::painter::{
    PaintError, RegularPainter, RoomPaintDispatch, fill_room, fill_room_margin,
    set_shared_door_type, standard_can_merge,
};
use crate::rng::RandomStack;
use crate::room::{
    ConnectionRoomKind, DoorType, QuestRoomKind, Room, RoomId, RoomKind, StandardRoomKind,
};
use crate::sewer_rooms::SewerRoomDispatcher;
pub use crate::sewer_rooms::{GeneratorRoomContent, SewerRoomContent};

/// The exact regular-Prison trap table from `PrisonLevel`.
#[must_use]
pub fn prison_trap_table() -> (Vec<TrapSpec>, Vec<f32>) {
    (
        vec![
            TrapSpec::new(TrapKind::Chilling),
            TrapSpec::new(TrapKind::Shocking),
            TrapSpec::new(TrapKind::Toxic),
            TrapSpec::new(TrapKind::Burning),
            TrapSpec::new(TrapKind::PoisonDart)
                .avoids_hallways()
                .cannot_be_hidden(),
            TrapSpec::new(TrapKind::Alarm),
            TrapSpec::new(TrapKind::Ooze),
            TrapSpec::new(TrapKind::Gripping).avoids_hallways(),
            TrapSpec::new(TrapKind::Confusion),
            TrapSpec::new(TrapKind::Flock),
            TrapSpec::new(TrapKind::Summoning),
            TrapSpec::new(TrapKind::Teleportation),
            TrapSpec::new(TrapKind::Gateway).avoids_hallways(),
            TrapSpec::new(TrapKind::Geyser),
        ],
        vec![
            4.0, 4.0, 4.0, 4.0, 4.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
    )
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PrisonRoomPaintState {
    bridge_space: Option<Rect>,
    bridge: Option<Rect>,
}

/// Exact non-special Prison room dispatcher.
///
/// `common` owns the generator-backed content provider and the cached state
/// for the ten region-independent standard classes. It is public so an
/// assembled floor can recover the provider after painting without a second
/// mutable owner.
#[derive(Debug)]
pub struct PrisonRoomDispatcher<C> {
    pub common: SewerRoomDispatcher<C>,
    states: Vec<PrisonRoomPaintState>,
}

impl<C> PrisonRoomDispatcher<C> {
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

    fn ensure_state(&mut self, room: RoomId) -> &mut PrisonRoomPaintState {
        if self.states.len() <= room {
            self.states
                .resize_with(room + 1, PrisonRoomPaintState::default);
        }
        &mut self.states[room]
    }

    fn state(&self, room: RoomId) -> &PrisonRoomPaintState {
        &self.states[room]
    }

    /// Cached `ChasmBridgeRoom.spaceRect`, used by later item and mob
    /// placement predicates and by entrance/exit placement.
    #[must_use]
    pub fn bridge_space(&self, room: RoomId) -> Option<Rect> {
        self.states.get(room).and_then(|state| state.bridge_space)
    }

    /// Cached `ChasmBridgeRoom.bridgeRect`.
    #[must_use]
    pub fn bridge_rect(&self, room: RoomId) -> Option<Rect> {
        self.states.get(room).and_then(|state| state.bridge)
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
            RoomKind::Standard(StandardRoomKind::ChasmBridge)
            | RoomKind::Entrance(StandardRoomKind::ChasmBridge)
            | RoomKind::Exit(StandardRoomKind::ChasmBridge) => self
                .bridge_space(room)
                .is_none_or(|space| !space.inside(point)),
            kind if is_common_standard(kind) => {
                self.common.can_place_item(level, rooms, room, point)
            }
            _ => true,
        }
    }

    /// Concrete `Room.canPlaceCharacter`, including bridge and exit
    /// exclusions.
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

impl<C> crate::sewer_mob_placement::RoomCharacterRules for PrisonRoomDispatcher<C> {
    fn can_place_character(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        point: Point,
    ) -> bool {
        PrisonRoomDispatcher::can_place_character(self, level, rooms, room, point)
    }
}

impl<C: SewerRoomContent> RoomPaintDispatch for PrisonRoomDispatcher<C> {
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
            RoomKind::Standard(kind) if is_prison_standard(kind) => {
                self.paint_prison_standard(level, rooms, room, kind, rng);
            }
            kind if is_common_standard(kind) => {
                self.common.paint_room(level, rooms, room, rng);
            }
            RoomKind::Connection(ConnectionRoomKind::Perimeter) => {
                self.paint_perimeter_room(level, rooms, room);
            }
            RoomKind::Connection(ConnectionRoomKind::Walkway) => {
                self.paint_walkway(level, rooms, room);
            }
            // RegularBuilder injects this directly for secret-room branches;
            // it is not part of ConnectionRoom.createRoom's Prison weights.
            RoomKind::Connection(ConnectionRoomKind::Maze) => {
                self.common.paint_room(level, rooms, room, rng);
            }
            RoomKind::Connection(_) => {
                panic!("non-Prison connection room passed to PrisonRoomDispatcher")
            }
            RoomKind::Special(_) | RoomKind::Secret(_) | RoomKind::Quest(_) => {
                panic!("non-regular Prison room passed to PrisonRoomDispatcher")
            }
            RoomKind::Standard(kind) => {
                panic!("non-Prison standard room passed to PrisonRoomDispatcher: {kind:?}")
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
            RoomKind::Standard(StandardRoomKind::ChasmBridge)
            | RoomKind::Entrance(StandardRoomKind::ChasmBridge)
            | RoomKind::Exit(StandardRoomKind::ChasmBridge) => {
                let cell = level.point_to_cell(rooms[room].point_inside(point, 1));
                level.map.cells[cell] != terrain::CHASM
            }
            RoomKind::Standard(kind) if is_prison_standard(kind) => {
                standard_can_merge(level, rooms, room, point)
            }
            RoomKind::Entrance(kind) | RoomKind::Exit(kind) if is_prison_standard(kind) => {
                standard_can_merge(level, rooms, room, point)
            }
            kind if is_common_standard(kind) => {
                self.common
                    .can_merge(level, rooms, room, other, point, merge_terrain)
            }
            RoomKind::Connection(ConnectionRoomKind::Walkway) => merge_terrain == terrain::CHASM,
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
        if is_common_standard(rooms[room].kind) {
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

impl<C: SewerRoomContent> PrisonRoomDispatcher<C> {
    fn paint_prison_standard(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        kind: StandardRoomKind,
        rng: &mut RandomStack,
    ) {
        match kind {
            StandardRoomKind::RegionDecoLine => {
                self.paint_region_deco_line(level, rooms, room, rng);
            }
            StandardRoomKind::Segmented => self.paint_segmented(level, rooms, room, rng),
            StandardRoomKind::Pillars => self.paint_pillars(level, rooms, room, rng),
            StandardRoomKind::ChasmBridge => self.paint_chasm_bridge(level, rooms, room, rng),
            StandardRoomKind::CellBlock => self.paint_cell_block(level, rooms, room, rng),
            _ => unreachable!("caller checks Prison standard kinds"),
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
            StandardRoomKind::RegionDecoLine => {
                self.paint_region_deco_line(level, rooms, room, rng);
                self.place_line_transition(level, rooms, room, entrance, rng);
            }
            StandardRoomKind::ChasmBridge => {
                self.paint_chasm_bridge(level, rooms, room, rng);
                self.place_chasm_bridge_transition(level, rooms, room, entrance, rng);
            }
            StandardRoomKind::Pillars => {
                self.paint_pillars(level, rooms, room, rng);
                self.place_pillars_transition(level, rooms, room, entrance, rng);
            }
            StandardRoomKind::CellBlock => {
                self.paint_cell_block(level, rooms, room, rng);
                self.place_cell_block_transition(level, rooms, room, entrance, rng);
            }
            _ => panic!("non-Prison transition room passed to PrisonRoomDispatcher: {kind:?}"),
        }
    }

    fn paint_region_deco_line(
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
        let mut preferences = [1.0_f32; 4]; // north, east, south, west
        for (_, door) in &doors {
            if door.y == bounds.top {
                preferences[0] -= 2.0;
            }
            if door.y == bounds.top + 1 {
                preferences[0] -= 1.0;
            }
            if door.y == bounds.bottom {
                preferences[2] -= 2.0;
            }
            if door.y == bounds.bottom - 1 {
                preferences[2] -= 1.0;
            }
            if door.x == bounds.left {
                preferences[3] -= 2.0;
            }
            if door.x == bounds.left + 1 {
                preferences[3] -= 1.0;
            }
            if door.x == bounds.right {
                preferences[1] -= 2.0;
            }
            if door.x == bounds.right - 1 {
                preferences[1] -= 1.0;
            }
        }
        set_all_doors(rooms, room, DoorType::Regular);

        let side = loop {
            if let Some(side) = rng.chances(&preferences) {
                break side;
            }
            for preference in &mut preferences {
                *preference += 1.0;
            }
        };
        let (from, to) = match side {
            0 => (
                Point::new(bounds.left + 1, bounds.top + 1),
                Point::new(bounds.right - 1, bounds.top + 1),
            ),
            1 => (
                Point::new(bounds.right - 1, bounds.top + 1),
                Point::new(bounds.right - 1, bounds.bottom - 1),
            ),
            2 => (
                Point::new(bounds.left + 1, bounds.bottom - 1),
                Point::new(bounds.right - 1, bounds.bottom - 1),
            ),
            3 => (
                Point::new(bounds.left + 1, bounds.top + 1),
                Point::new(bounds.left + 1, bounds.bottom - 1),
            ),
            _ => unreachable!(),
        };
        draw::draw_line(&mut level.map, from, to, terrain::REGION_DECO);
        for (_, door) in doors {
            draw::draw_inside(&mut level.map, bounds, door, 1, terrain::EMPTY);
        }
    }

    fn paint_segmented(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        let doors = room_doors(rooms, room);
        set_all_doors(rooms, room, DoorType::Regular);
        for (_, door) in doors {
            level.map.set_point(door, terrain::EMPTY);
        }
        let bounds = rooms[room].bounds;
        create_segment_walls(
            level,
            Rect::new(
                bounds.left + 1,
                bounds.top + 1,
                bounds.right - 1,
                bounds.bottom - 1,
            ),
            rng,
        );
    }

    fn paint_pillars(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        set_all_doors(rooms, room, DoorType::Regular);

        let current = &rooms[room];
        let bounds = current.bounds;
        let minimum = current.width().min(current.height());
        if minimum == 7
            || (current.size_category == Some(crate::room::SizeCategory::Normal)
                && rng.int_bound(2) == 0)
        {
            let inset = if minimum >= 11 { 2 } else { 1 };
            let size = div_i32(minimum - 3, 2) - inset;
            let (mut x, mut y) = if rng.int_bound(2) == 0 {
                (
                    rng.int_range(bounds.left + 1 + inset, bounds.right - size - inset),
                    bounds.top + 1 + inset,
                )
            } else {
                (
                    bounds.left + 1 + inset,
                    rng.int_range(bounds.top + 1 + inset, bounds.bottom - size - inset),
                )
            };
            draw::fill(&mut level.map, x, y, size, size, terrain::WALL);
            x = bounds.right - (x - bounds.left + size - 1);
            y = bounds.bottom - (y - bounds.top + size - 1);
            draw::fill(&mut level.map, x, y, size, size, terrain::WALL);
        } else {
            let inset = if minimum >= 12 { 2 } else { 1 };
            let size = div_i32(minimum - 6, inset + 1);
            #[allow(clippy::cast_precision_loss)]
            let x_spaces = (current.width() - 2 * inset - size - 2) as f32;
            #[allow(clippy::cast_precision_loss)]
            let y_spaces = (current.height() - 2 * inset - size - 2) as f32;
            let minimum_spaces = x_spaces.min(y_spaces);
            #[allow(clippy::cast_precision_loss)]
            let skew = round_f32(rng.float() * minimum_spaces) as f32 / minimum_spaces;
            let x_skew = round_f32(skew * x_spaces);
            let y_skew = round_f32(skew * y_spaces);
            for (x, y) in [
                (bounds.left + 1 + inset + x_skew, bounds.top + 1 + inset),
                (bounds.right - size - inset, bounds.top + 1 + inset + y_skew),
                (
                    bounds.right - size - inset - x_skew,
                    bounds.bottom - size - inset,
                ),
                (
                    bounds.left + 1 + inset,
                    bounds.bottom - size - inset - y_skew,
                ),
            ] {
                draw::fill(&mut level.map, x, y, size, size, terrain::WALL);
            }
        }
    }

    fn paint_chasm_bridge(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        let doors = room_doors(rooms, room);
        set_all_doors(rooms, room, DoorType::Regular);

        let current = &rooms[room];
        let bounds = current.bounds;
        let mut doors_xy = 0_i32;
        for (_, door) in &doors {
            if door.x == bounds.left || door.x == bounds.right {
                doors_xy += 1;
            } else {
                doors_xy -= 1;
            }
        }
        doors_xy += div_i32(current.width() - current.height(), 2);

        let (space, bridge) = if doors_xy > 0 || (doors_xy == 0 && rng.int_bound(2) == 0) {
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
            let maximum = if current.width() >= 7 { 2 } else { 1 };
            while end - start > maximum + 1 {
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
            let maximum = if current.height() >= 7 { 2 } else { 1 };
            while end - start > maximum + 1 {
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
        draw::fill_rect(&mut level.map, space, terrain::CHASM);
        draw::fill_rect(&mut level.map, bridge, terrain::EMPTY_SP);
    }

    fn paint_cell_block(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        fill_room_margin(level, &rooms[room], 3, terrain::WALL);

        // Java allocates `new EmptyRoom()` solely to use it as the internal
        // rectangle. StandardRoom's instance initializer still performs its
        // otherwise-discarded `[1, 0, 0]` size-category draw.
        let _discarded_empty_room_category = rng
            .chances(&[1.0, 0.0, 0.0])
            .expect("EmptyRoom always selects NORMAL");
        let bounds = rooms[room].bounds;
        let internal = Rect::new(
            bounds.left + 3,
            bounds.top + 3,
            bounds.right - 3,
            bounds.bottom - 3,
        );
        // `internal` is an EmptyRoom in Java, so width/height are inclusive.
        let internal_width = internal.width() + 1;
        let internal_height = internal.height() + 1;
        let mut rows = div_i32(internal_width - 1, 3);
        let mut columns = div_i32(internal_height - 1, 3);
        if internal_height == 11 {
            columns -= 1;
        }
        if internal_width == 11 {
            rows -= 1;
        }
        let cell_width = div_i32(internal_width - 2 - (rows - 1), rows);
        let cell_height = div_i32(internal_height - 2 - (columns - 1), columns);
        let width_spacing = if rows * cell_width + rows + 1 == internal_width {
            1
        } else {
            2
        };
        let height_spacing = if columns * cell_height + columns + 1 == internal_height {
            1
        } else {
            2
        };

        let mut top_bottom = Some(rows > columns || (rows == columns && rng.int_bound(2) == 0));
        if rows == 1 || columns == 1 {
            top_bottom = top_bottom.map(|value| !value);
        }
        if rows == 1 && columns == 1 {
            top_bottom = None;
        }

        let mut open_rooms = rows * columns;
        if open_rooms == 9 {
            open_rooms -= 1;
        }
        let guarantee_open = rooms[room].is_entrance() || rooms[room].is_exit();
        for x_index in 0..rows {
            for y_index in 0..columns {
                if rows == 3 && columns == 3 && x_index == 1 && y_index == 1 {
                    continue;
                }
                let left = internal.left + 1 + x_index * (cell_width + width_spacing);
                let top = internal.top + 1 + y_index * (cell_height + height_spacing);
                if rng.int_bound(cell_width * cell_height) == 0
                    && (!guarantee_open || open_rooms > 1)
                {
                    draw::fill(
                        &mut level.map,
                        left,
                        top,
                        cell_width,
                        cell_height,
                        terrain::REGION_DECO,
                    );
                    open_rooms -= 1;
                } else {
                    draw::fill(
                        &mut level.map,
                        left,
                        top,
                        cell_width,
                        cell_height,
                        terrain::EMPTY_SP,
                    );
                }

                match top_bottom {
                    None => match rng.int_bound(4) {
                        0 => level.map.set(
                            internal.left,
                            internal.top + div_i32(internal_height, 2),
                            terrain::DOOR,
                        ),
                        1 => level.map.set(
                            internal.left + div_i32(internal_width, 2),
                            internal.top,
                            terrain::DOOR,
                        ),
                        2 => level.map.set(
                            internal.right,
                            internal.top + div_i32(internal_height, 2),
                            terrain::DOOR,
                        ),
                        _ => level.map.set(
                            internal.left + div_i32(internal_width, 2),
                            internal.bottom,
                            terrain::DOOR,
                        ),
                    },
                    Some(true) => {
                        if y_index == 0 {
                            level
                                .map
                                .set(left + div_i32(cell_width, 2), top - 1, terrain::DOOR);
                        } else if y_index == columns - 1 {
                            level.map.set(
                                left + div_i32(cell_width, 2) - 1,
                                top + cell_height,
                                terrain::DOOR,
                            );
                        } else if x_index == 0 {
                            level.map.set(
                                left - 1,
                                top + div_i32(cell_height, 2) - 1,
                                terrain::DOOR,
                            );
                        } else if x_index == rows - 1 {
                            level.map.set(
                                left + cell_width,
                                top + div_i32(cell_height, 2),
                                terrain::DOOR,
                            );
                        }
                    }
                    Some(false) => {
                        if x_index == 0 {
                            level.map.set(
                                left - 1,
                                top + div_i32(cell_height, 2) - 1,
                                terrain::DOOR,
                            );
                        } else if x_index == rows - 1 {
                            level.map.set(
                                left + cell_width,
                                top + div_i32(cell_height, 2),
                                terrain::DOOR,
                            );
                        } else if y_index == 0 {
                            level
                                .map
                                .set(left + div_i32(cell_width, 2), top - 1, terrain::DOOR);
                        } else if y_index == columns - 1 {
                            level.map.set(
                                left + div_i32(cell_width, 2) - 1,
                                top + cell_height,
                                terrain::DOOR,
                            );
                        }
                    }
                }
            }
        }
        set_all_doors(rooms, room, DoorType::Regular);
    }

    fn place_line_transition(
        &self,
        level: &mut Level,
        rooms: &[Room],
        room: RoomId,
        entrance: bool,
        rng: &mut RandomStack,
    ) {
        let cell = loop {
            let cell = level.point_to_cell(random_room_point(&rooms[room], 3, rng));
            if !level.mob_cells[cell] {
                break cell;
            }
        };
        level.map.cells[cell] = if entrance {
            terrain::ENTRANCE
        } else {
            terrain::EXIT
        };
        add_regular_transition(level, cell, entrance);
    }

    fn place_chasm_bridge_transition(
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
            .expect("ChasmBridgeRoom caches spaceRect during paint");
        let cell = loop {
            let point = random_room_point(&rooms[room], 2, rng);
            let cell = level.point_to_cell(point);
            if !space.inside(point) && !level.mob_cells[cell] {
                break cell;
            }
        };
        clear_neighbours8(level, cell, terrain::EMPTY);
        level.map.cells[cell] = if entrance {
            terrain::ENTRANCE
        } else {
            terrain::EXIT
        };
        add_regular_transition(level, cell, entrance);
    }

    fn place_pillars_transition(
        &self,
        level: &mut Level,
        rooms: &[Room],
        room: RoomId,
        entrance: bool,
        rng: &mut RandomStack,
    ) {
        let width = level.width();
        let cell = loop {
            let cell = level.point_to_cell(random_room_point(&rooms[room], 2, rng));
            let cell_i32 = i32::try_from(cell).expect("map exceeds Java int");
            let valid = [1, width, -1].into_iter().all(|offset| {
                let neighbour =
                    usize::try_from(cell_i32 + offset).expect("transition lies inside padded room");
                level.map.cells[neighbour] != terrain::WALL
            });
            if !level.mob_cells[cell] && level.map.cells[cell] != terrain::WALL && valid {
                break cell;
            }
        };
        level.map.cells[cell] = if entrance {
            terrain::ENTRANCE
        } else {
            terrain::EXIT
        };
        add_regular_transition(level, cell, entrance);
    }

    fn place_cell_block_transition(
        &self,
        level: &mut Level,
        rooms: &[Room],
        room: RoomId,
        entrance: bool,
        rng: &mut RandomStack,
    ) {
        let neighbours = neighbours8(level.width());
        let cell = loop {
            let point = random_room_point(&rooms[room], 3, rng);
            let cell = level.point_to_cell(point);
            if level.map.cells[cell] != terrain::EMPTY_SP {
                continue;
            }
            let cell_i32 = i32::try_from(cell).expect("map exceeds Java int");
            let has_door = neighbours.iter().any(|offset| {
                let neighbour =
                    usize::try_from(cell_i32 + offset).expect("transition lies inside padded room");
                level.map.cells[neighbour] == terrain::DOOR
            });
            if !has_door {
                break cell;
            }
        };
        level.map.cells[cell] = if entrance {
            terrain::ENTRANCE_SP
        } else {
            terrain::EXIT
        };
        add_regular_transition(level, cell, entrance);
    }

    fn paint_perimeter_room(&self, level: &mut Level, rooms: &mut [Room], room: RoomId) {
        paint_perimeter(level, rooms, room, level.tunnel_tile());
        set_all_doors(rooms, room, DoorType::Tunnel);
    }

    fn paint_walkway(&self, level: &mut Level, rooms: &mut [Room], room: RoomId) {
        if rooms[room].width().min(rooms[room].height()) > 3 {
            fill_room_margin(level, &rooms[room], 1, terrain::CHASM);
        }
        paint_perimeter(level, rooms, room, level.tunnel_tile());
        set_all_doors(rooms, room, DoorType::Tunnel);
        paint_bridge_neighbour_edges(level, rooms, room);
    }
}

/// Prison-region wrapper supplying v3.3.8 water/grass parameters, trap class
/// tables, and exact `PrisonPainter.decorate` behavior.
#[derive(Clone, Debug, PartialEq)]
pub struct PrisonPainter {
    pub regular: RegularPainter,
}

impl PrisonPainter {
    #[must_use]
    pub fn new(feeling: Feeling, trap_count: i32) -> Self {
        let (classes, chances) = prison_trap_table();
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

    /// Paint a Prison map using the supplied exact room dispatcher.
    ///
    /// # Errors
    ///
    /// Returns a structural [`PaintError`] when a special room is
    /// disconnected or a builder edge has no mutually valid door position.
    pub fn paint<D: RoomPaintDispatch>(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        dispatch: &mut D,
        rng: &mut RandomStack,
    ) -> Result<(), PaintError> {
        self.regular
            .paint_with_decorator(level, rooms, dispatch, rng, decorate_prison)
    }
}

/// Exact v3.3.8 `PrisonPainter.decorate` map mutation and conditional draws.
pub fn decorate_prison(level: &mut Level, rooms: &[Room], order: &[RoomId], rng: &mut RandomStack) {
    let width = level.width();
    let width_usize = usize::try_from(width).expect("level width is negative");
    let length = level.len();

    for cell in width_usize + 1..length - width_usize - 1 {
        if level.map.cells[cell] == terrain::EMPTY {
            let mut chance = 0.05_f32;
            if level.map.cells[cell + 1] == terrain::WALL
                && level.map.cells[cell + width_usize] == terrain::WALL
            {
                chance += 0.2;
            }
            if level.map.cells[cell - 1] == terrain::WALL
                && level.map.cells[cell + width_usize] == terrain::WALL
            {
                chance += 0.2;
            }
            if level.map.cells[cell + 1] == terrain::WALL
                && level.map.cells[cell - width_usize] == terrain::WALL
            {
                chance += 0.2;
            }
            if level.map.cells[cell - 1] == terrain::WALL
                && level.map.cells[cell - width_usize] == terrain::WALL
            {
                chance += 0.2;
            }
            if rng.float() < chance {
                level.map.cells[cell] = terrain::EMPTY_DECO;
            }
        }
    }

    for &room in order {
        if is_java_special_room(rooms[room].kind) {
            continue;
        }
        let chance = match rooms[room].kind {
            RoomKind::Standard(StandardRoomKind::Fissure) => 3,
            RoomKind::Standard(StandardRoomKind::ChasmBridge)
            | RoomKind::Entrance(StandardRoomKind::ChasmBridge)
            | RoomKind::Exit(StandardRoomKind::ChasmBridge) => 5,
            _ => 15,
        };
        let bounds = rooms[room].bounds;
        for y in (bounds.top + 1..bounds.bottom).rev() {
            let row_start = level.point_to_cell(Point::new(bounds.left + 1, y));
            let row_end = level.point_to_cell(Point::new(bounds.right, y));
            for cell in row_start..row_end {
                if level.map.cells[cell] == terrain::CHASM
                    && level.map.cells[cell - width_usize] == terrain::CHASM
                    && rng.int_bound(chance) == 0
                {
                    level.map.cells[cell] = terrain::REGION_DECO_ALT;
                }
            }
        }
    }

    for cell in 0..width_usize {
        if level.map.cells[cell] == terrain::WALL
            && matches!(
                level.map.cells[cell + width_usize],
                terrain::EMPTY | terrain::EMPTY_SP
            )
            && rng.int_bound(6) == 0
        {
            level.map.cells[cell] = terrain::WALL_DECO;
        }
    }
    for cell in width_usize..length - width_usize {
        if level.map.cells[cell] == terrain::WALL
            && level.map.cells[cell - width_usize] == terrain::WALL
            && matches!(
                level.map.cells[cell + width_usize],
                terrain::EMPTY | terrain::EMPTY_SP
            )
            && rng.int_bound(3) == 0
        {
            level.map.cells[cell] = terrain::WALL_DECO;
        }
    }
}

const fn is_prison_standard(kind: StandardRoomKind) -> bool {
    matches!(
        kind,
        StandardRoomKind::RegionDecoLine
            | StandardRoomKind::Segmented
            | StandardRoomKind::Pillars
            | StandardRoomKind::ChasmBridge
            | StandardRoomKind::CellBlock
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

const fn is_java_special_room(kind: RoomKind) -> bool {
    matches!(
        kind,
        RoomKind::Special(_)
            | RoomKind::Secret(_)
            | RoomKind::Quest(
                QuestRoomKind::MassGrave | QuestRoomKind::RotGarden | QuestRoomKind::AmbitiousImp
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

fn create_segment_walls(level: &mut Level, area: Rect, rng: &mut RandomStack) {
    if (area.width() + 1).max(area.height() + 1) < 5
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
                == terrain::WALL
                && level.map.cells[level.point_to_cell(Point::new(split, area.bottom + 1))]
                    == terrain::WALL
            {
                draw::draw_line(
                    &mut level.map,
                    Point::new(split, area.top),
                    Point::new(split, area.bottom),
                    terrain::WALL,
                );
                let opening = rng.int_range(area.top, area.bottom - 1);
                level.map.set(split, opening, terrain::EMPTY);
                level.map.set(split, opening + 1, terrain::EMPTY);
                create_segment_walls(
                    level,
                    Rect::new(area.left, area.top, split - 1, area.bottom),
                    rng,
                );
                create_segment_walls(
                    level,
                    Rect::new(split + 1, area.top, area.right, area.bottom),
                    rng,
                );
                return;
            }
        } else {
            let split = rng.int_range(area.top + 2, area.bottom - 2);
            if level.map.cells[level.point_to_cell(Point::new(area.left - 1, split))]
                == terrain::WALL
                && level.map.cells[level.point_to_cell(Point::new(area.right + 1, split))]
                    == terrain::WALL
            {
                draw::draw_line(
                    &mut level.map,
                    Point::new(area.left, split),
                    Point::new(area.right, split),
                    terrain::WALL,
                );
                let opening = rng.int_range(area.left, area.right - 1);
                level.map.set(opening, split, terrain::EMPTY);
                level.map.set(opening + 1, split, terrain::EMPTY);
                create_segment_walls(
                    level,
                    Rect::new(area.left, area.top, area.right, split - 1),
                    rng,
                );
                create_segment_walls(
                    level,
                    Rect::new(area.left, split + 1, area.right, area.bottom),
                    rng,
                );
                return;
            }
        }
        tries -= 1;
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
    let offsets = neighbours8(level.width());
    let cell = i32::try_from(cell).expect("map exceeds Java int");
    for offset in offsets {
        let neighbour = usize::try_from(cell + offset).expect("cell lies inside padded map");
        level.map.cells[neighbour] = tile;
    }
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
                let distance = perimeter_distance(&rooms[room], from, to);
                if distance < shortest {
                    shortest = distance;
                    selected = (from, to, index);
                }
            }
        }
        fill_perimeter_between(&mut level.map, &rooms[room], selected.0, selected.1, floor);
        points_filled.push(selected.1);
        points_to_fill.remove(selected.2);
    }
}

fn space_between(first: i32, second: i32) -> i32 {
    first.wrapping_sub(second).abs().wrapping_sub(1)
}

fn perimeter_distance(room: &Room, first: Point, second: Point) -> i32 {
    let bounds = room.bounds;
    if ((first.x == bounds.left + 1 || first.x == bounds.right - 1) && first.y == second.y)
        || ((first.y == bounds.top + 1 || first.y == bounds.bottom - 1) && first.x == second.x)
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
    floor: i32,
) {
    let bounds = room.bounds;
    if ((from.x == bounds.left + 1 || from.x == bounds.right - 1) && from.x == to.x)
        || ((from.y == bounds.top + 1 || from.y == bounds.bottom - 1) && from.y == to.y)
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
        Point::new(bounds.left + 1, bounds.top + 1),
        Point::new(bounds.right - 1, bounds.top + 1),
        Point::new(bounds.right - 1, bounds.bottom - 1),
        Point::new(bounds.left + 1, bounds.bottom - 1),
    ];
    for corner in corners {
        if (corner.x == from.x || corner.y == from.y) && (corner.x == to.x || corner.y == to.y) {
            draw::draw_line(map, from, corner, floor);
            draw::draw_line(map, corner, to, floor);
            return;
        }
    }
    let side = if from.y == bounds.top + 1 || from.y == bounds.bottom - 1 {
        if space_between(bounds.left, from.x) + space_between(bounds.left, to.x)
            <= space_between(bounds.right, from.x) + space_between(bounds.right, to.x)
        {
            Point::new(bounds.left + 1, bounds.top + div_i32(room.height(), 2))
        } else {
            Point::new(bounds.right - 1, bounds.top + div_i32(room.height(), 2))
        }
    } else if space_between(bounds.top, from.y) + space_between(bounds.top, to.y)
        <= space_between(bounds.bottom, from.y) + space_between(bounds.bottom, to.y)
    {
        Point::new(bounds.left + div_i32(room.width(), 2), bounds.top + 1)
    } else {
        Point::new(bounds.left + div_i32(room.width(), 2), bounds.bottom - 1)
    };
    fill_perimeter_between(map, room, from, side, floor);
    fill_perimeter_between(map, room, side, to, floor);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::{GeneratedItem, SeedKind};
    use crate::level::{HeapKind, PaintItem};
    use crate::room::{Door, RoomConnection, SizeCategory, SpecialRoomKind};
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
        let mut level = Level::new(8, Feeling::None);
        level.set_size(24, 24);
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        let mut dispatcher = PrisonRoomDispatcher::new(FixtureContent);
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
    fn exact_prison_trap_table_preserves_constructor_flags() {
        let (classes, chances) = prison_trap_table();
        assert_eq!(classes.len(), 14);
        assert_eq!(
            chances,
            [4.0; 5]
                .into_iter()
                .chain([2.0; 3])
                .chain([1.0; 6])
                .collect::<Vec<_>>()
        );
        assert_eq!(classes[4].kind, TrapKind::PoisonDart);
        assert!(classes[4].avoids_hallways);
        assert!(!classes[4].can_be_hidden);
        assert!(classes[7].avoids_hallways);
        assert!(classes[12].avoids_hallways);
        assert!(classes[13].can_be_hidden);
    }

    #[test]
    fn every_reachable_prison_standard_class_matches_official_fixture() {
        let classes = [
            (StandardRoomKind::RegionDecoLine, SizeCategory::Normal, 9),
            (StandardRoomKind::Segmented, SizeCategory::Large, 13),
            (StandardRoomKind::Pillars, SizeCategory::Large, 13),
            (StandardRoomKind::ChasmBridge, SizeCategory::Normal, 9),
            (StandardRoomKind::CellBlock, SizeCategory::Large, 13),
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
        // Captured by tooling/parity/PrisonRoomPaintOracle.java against the
        // official v3.3.8 desktop jar. Plants and GrassyGrave delegate item
        // construction through the fixture provider, so only their official
        // map and event checkpoints are provider-independent.
        let expected = vec![
            (-1_059_723_156, 1_381_175_215, 0),
            (553_060_259, -47_416_568, 0),
            (1_653_026_944, 1_381_175_215, 0),
            (1_464_925_484, 845_149_648, 0),
            (-1_030_875_275, -380_678_566, 0),
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
                "event count differs for class {index}"
            );
            if index != 5 && index != 10 {
                assert_eq!(actual.1, expected.1, "RNG differs for class {index}");
            }
        }
    }

    #[test]
    fn every_reachable_prison_transition_and_connection_matches_official_fixture() {
        let transitions = [
            (
                true,
                StandardRoomKind::RegionDecoLine,
                SizeCategory::Normal,
                9,
            ),
            (
                false,
                StandardRoomKind::RegionDecoLine,
                SizeCategory::Normal,
                9,
            ),
            (true, StandardRoomKind::ChasmBridge, SizeCategory::Normal, 9),
            (
                false,
                StandardRoomKind::ChasmBridge,
                SizeCategory::Normal,
                9,
            ),
            (true, StandardRoomKind::Pillars, SizeCategory::Large, 13),
            (false, StandardRoomKind::Pillars, SizeCategory::Large, 13),
            (true, StandardRoomKind::CellBlock, SizeCategory::Large, 13),
            (false, StandardRoomKind::CellBlock, SizeCategory::Large, 13),
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
            ConnectionRoomKind::Perimeter,
            ConnectionRoomKind::Walkway,
            ConnectionRoomKind::Maze,
        ] {
            actual.push(paint_fixture(RoomKind::Connection(kind), None, 9));
        }
        assert_eq!(
            actual,
            vec![
                (2_064_932_978, 299_112_627, 1),
                (1_154_053_235, 299_112_627, 1),
                (187_571_881, -224_344_019, 1),
                (-217_715_896, -224_344_019, 1),
                (-433_109_882, 845_149_648, 1),
                (1_366_684_295, 845_149_648, 1),
                (-1_611_334_324, -1_079_465_464, 1),
                (1_361_401_327, -1_079_465_464, 1),
                (1_242_550_476, 1_678_708_340, 0),
                (-697_177_396, 1_678_708_340, 0),
                (1_048_108_059, 758_821_230, 0),
            ]
        );
    }

    fn fill_fixture(level: &mut Level, left: i32, top: i32, right: i32, bottom: i32, tile: i32) {
        for y in top..=bottom {
            for x in left..=right {
                level.map.set(x, y, tile);
            }
        }
    }

    #[test]
    fn prison_decoration_matches_official_map_and_rng_fixture() {
        let mut constructor_rng = RandomStack::with_base_seed(17);
        let mut fissure = Room::standard(StandardRoomKind::Fissure, &mut constructor_rng);
        fissure.size_category = Some(SizeCategory::Normal);
        fissure.bounds = Rect::new(2, 2, 10, 10);
        let mut bridge = Room::standard(StandardRoomKind::ChasmBridge, &mut constructor_rng);
        bridge.size_category = Some(SizeCategory::Normal);
        bridge.bounds = Rect::new(11, 2, 20, 10);
        let mut special = Room::special(SpecialRoomKind::Storage);
        special.bounds = Rect::new(2, 11, 10, 20);
        let rooms = vec![fissure, bridge, special];

        let mut level = Level::new(8, Feeling::None);
        level.set_size(24, 24);
        level.map.cells.fill(terrain::WALL);
        fill_fixture(&mut level, 3, 3, 9, 9, terrain::EMPTY);
        fill_fixture(&mut level, 12, 3, 19, 9, terrain::EMPTY_SP);
        fill_fixture(&mut level, 3, 12, 9, 19, terrain::EMPTY);
        fill_fixture(&mut level, 4, 4, 8, 8, terrain::CHASM);
        fill_fixture(&mut level, 13, 4, 18, 8, terrain::CHASM);
        fill_fixture(&mut level, 4, 13, 8, 18, terrain::CHASM);

        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        decorate_prison(&mut level, &rooms, &[0, 1, 2], &mut rng);
        assert_eq!(level.java_map_hash(), -868_832_989);
        assert_eq!(rng.int(), -580_282_782);
    }

    #[test]
    fn assembled_regular_painter_matches_official_prison_fixture() {
        let mut constructor_rng = RandomStack::with_base_seed(17);
        let mut entrance = Room::standard(StandardRoomKind::RegionDecoLine, &mut constructor_rng);
        entrance.kind = RoomKind::Entrance(StandardRoomKind::RegionDecoLine);
        entrance.size_category = Some(SizeCategory::Normal);
        entrance.bounds = Rect::new(0, 0, 8, 8);
        let mut middle = Room::standard(StandardRoomKind::Fissure, &mut constructor_rng);
        middle.size_category = Some(SizeCategory::Normal);
        middle.bounds = Rect::new(8, 0, 16, 8);
        let mut exit = Room::standard(StandardRoomKind::CellBlock, &mut constructor_rng);
        exit.kind = RoomKind::Exit(StandardRoomKind::CellBlock);
        exit.size_category = Some(SizeCategory::Large);
        exit.bounds = Rect::new(16, 0, 28, 12);
        let mut rooms = vec![entrance, middle, exit];
        connect_unplaced(&mut rooms, 0, 1);
        connect_unplaced(&mut rooms, 1, 2);

        let mut level = Level::new(8, Feeling::None);
        let mut random = RandomStack::with_base_seed(0);
        random.push(0x0123_4567_89ab_cdef);
        let mut dispatcher = PrisonRoomDispatcher::new(FixtureContent);
        let mut painter = PrisonPainter {
            regular: RegularPainter::default(),
        };
        painter
            .paint(&mut level, &mut rooms, &mut dispatcher, &mut random)
            .unwrap();

        assert_eq!(level.java_map_hash(), -1_310_124_693);
        assert_eq!(random.int(), -435_441_796);
        assert_eq!(level.transitions.len(), 2);
        assert_eq!(level.room_order, [2, 0, 1]);
    }

    #[test]
    fn bridge_and_exit_placement_rules_preserve_java_exclusions() {
        use crate::sewer_mob_placement::RoomCharacterRules;

        let mut level = Level::new(8, Feeling::None);
        level.set_size(24, 24);
        let mut dispatcher = PrisonRoomDispatcher::new(FixtureContent);
        let bridge_rooms = fixture_room(
            RoomKind::Standard(StandardRoomKind::ChasmBridge),
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

        let exit_rooms = fixture_room(
            RoomKind::Exit(StandardRoomKind::Pillars),
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

    #[test]
    fn fixture_heaps_remain_plain_room_content() {
        let mut rooms = fixture_room(
            RoomKind::Standard(StandardRoomKind::Study),
            Some(SizeCategory::Normal),
            9,
        );
        let mut level = Level::new(8, Feeling::None);
        level.set_size(24, 24);
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        PrisonRoomDispatcher::new(FixtureContent).paint_room(&mut level, &mut rooms, 0, &mut rng);
        assert_eq!(level.heaps.len(), 1);
        assert_eq!(level.heaps[0].kind, HeapKind::Heap);
    }
}
