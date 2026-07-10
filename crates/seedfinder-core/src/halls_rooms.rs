//! Concrete room painting for regular Demon Halls floors (depths 21 through 24).
//!
//! Halls-specific patch rooms, transition subclasses, merge behavior, and
//! decoration are ported directly from Shattered Pixel Dungeon v3.3.8.
//! Region-independent standards, `RegionDecoPatchRoom`, and generic
//! connections reuse [`crate::sewer_rooms::SewerRoomDispatcher`].

#![allow(
    clippy::cast_precision_loss,
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unused_self
)]

use crate::geometry::{PathFinder, Point, Rect, painter as draw, terrain};
use crate::java_math::div_i32;
use crate::level::{Feeling, HeapKind, Level, TransitionKind, TrapKind, TrapSpec};
use crate::painter::{
    PaintError, RegularPainter, RoomPaintDispatch, fill_room, fill_room_margin, generate_patch,
    merge_rooms, set_shared_door_type, standard_can_merge,
};
use crate::rng::RandomStack;
use crate::room::{ConnectionRoomKind, DoorType, Room, RoomId, RoomKind, StandardRoomKind};
use crate::run::GeneratorCategory;
use crate::sewer_rooms::{SewerRoomContent, SewerRoomDispatcher};

/// Exact `HallsLevel` trap classes, constructor flags, and weights.
#[must_use]
pub fn halls_trap_table() -> (Vec<TrapSpec>, Vec<f32>) {
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
            TrapSpec::new(TrapKind::Grim)
                .avoids_hallways()
                .cannot_be_hidden(),
            TrapSpec::new(TrapKind::Pitfall),
            TrapSpec::new(TrapKind::Distortion),
            TrapSpec::new(TrapKind::Gateway).avoids_hallways(),
            TrapSpec::new(TrapKind::Geyser),
        ],
        vec![
            4.0, 4.0, 4.0, 4.0, 4.0, 2.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
            1.0,
        ],
    )
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct HallsRoomPaintState {
    patch: Option<Vec<bool>>,
}

/// Exact non-special Halls room dispatcher.
#[derive(Debug)]
pub struct HallsRoomDispatcher<C> {
    pub common: SewerRoomDispatcher<C>,
    states: Vec<HallsRoomPaintState>,
}

impl<C> HallsRoomDispatcher<C> {
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

    fn ensure_state(&mut self, room: RoomId) -> &mut HallsRoomPaintState {
        if self.states.len() <= room {
            self.states
                .resize_with(room + 1, HallsRoomPaintState::default);
        }
        &mut self.states[room]
    }

    fn patch_at(&self, rooms: &[Room], room: RoomId, point: Point) -> bool {
        self.states[room]
            .patch
            .as_ref()
            .expect("Halls patch is initialized before use")
            [patch_coordinate(&rooms[room], point.x, point.y)]
    }

    /// Concrete `Room.canPlaceItem` for every handled Halls room.
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
        if is_shared_room(rooms[room].kind) {
            self.common.can_place_item(level, rooms, room, point)
        } else {
            true
        }
    }

    /// Concrete `Room.canPlaceCharacter`, including transition exclusions.
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

impl<C> crate::sewer_mob_placement::RoomCharacterRules for HallsRoomDispatcher<C> {
    fn can_place_character(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        point: Point,
    ) -> bool {
        HallsRoomDispatcher::can_place_character(self, level, rooms, room, point)
    }
}

impl<C: SewerRoomContent> RoomPaintDispatch for HallsRoomDispatcher<C> {
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
            RoomKind::Standard(kind) if is_halls_standard(kind) => {
                self.paint_halls_standard(level, rooms, room, kind, rng);
            }
            kind if is_shared_room(kind) || is_halls_connection(kind) => {
                self.common.paint_room(level, rooms, room, rng);
            }
            RoomKind::Special(_) | RoomKind::Secret(_) | RoomKind::Quest(_) => {
                panic!("non-regular Halls room passed to HallsRoomDispatcher")
            }
            RoomKind::Standard(kind) => {
                panic!("non-Halls standard room passed to HallsRoomDispatcher: {kind:?}")
            }
            RoomKind::Connection(kind) => {
                panic!("non-Halls connection room passed to HallsRoomDispatcher: {kind:?}")
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
        halls_can_merge(
            Some(&self.common),
            level,
            rooms,
            room,
            other,
            point,
            merge_terrain,
        )
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
        if is_shared_room(rooms[room].kind) || is_halls_connection(rooms[room].kind) {
            self.common
                .merge(level, rooms, room, other, merge, merge_terrain);
        } else {
            halls_merge(level, rooms, room, other, merge, merge_terrain);
        }
    }

    fn can_place_water(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        if is_shared_room(rooms[room].kind) {
            self.common.can_place_water(level, rooms, room, point)
        } else {
            true
        }
    }

    fn can_place_grass(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        if is_shared_room(rooms[room].kind) {
            self.common.can_place_grass(level, rooms, room, point)
        } else {
            true
        }
    }

    fn can_place_trap(&self, level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
        if is_shared_room(rooms[room].kind) {
            self.common.can_place_trap(level, rooms, room, point)
        } else {
            true
        }
    }
}

impl<C: SewerRoomContent> HallsRoomDispatcher<C> {
    fn paint_halls_standard(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        kind: StandardRoomKind,
        rng: &mut RandomStack,
    ) {
        match kind {
            StandardRoomKind::Ruins => self.paint_ruins(level, rooms, room, rng),
            StandardRoomKind::Chasm => self.paint_chasm(level, rooms, room, rng),
            StandardRoomKind::Skulls => self.paint_skulls(level, rooms, room),
            StandardRoomKind::Ritual => self.paint_ritual(level, rooms, room, None, rng),
            _ => unreachable!("caller checks Halls standard kinds"),
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
            StandardRoomKind::RegionDecoPatch => {
                self.common.paint_room(level, rooms, room, rng);
            }
            StandardRoomKind::Ruins => {
                self.paint_ruins(level, rooms, room, rng);
                self.place_patch_transition(level, rooms, room, entrance, PatchObstacle::Wall, rng);
            }
            StandardRoomKind::Chasm => {
                self.paint_chasm(level, rooms, room, rng);
                self.place_patch_transition(
                    level,
                    rooms,
                    room,
                    entrance,
                    PatchObstacle::Chasm,
                    rng,
                );
            }
            StandardRoomKind::Ritual => {
                self.paint_ritual(level, rooms, room, Some(entrance), rng);
            }
            _ => panic!("non-Halls transition room passed to HallsRoomDispatcher: {kind:?}"),
        }
    }

    fn setup_patch(
        &mut self,
        level: &Level,
        rooms: &mut [Room],
        room: RoomId,
        mut fill: f32,
        clustering: i32,
        rng: &mut RandomStack,
    ) {
        let patch_width = rooms[room].width() - 2;
        let patch_height = rooms[room].height() - 2;
        let mut attempts = 0_i32;
        let patch = loop {
            let mut patch = generate_patch(patch_width, patch_height, fill, clustering, true, rng);
            // `PatchRoom.setupPatch` evaluates center before walking doors.
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
                fill -= 0.01;
                attempts = 0;
            }
            if valid {
                break patch;
            }
        };
        let mut patch = patch;
        clean_diagonal_edges(&mut patch, patch_width);
        self.ensure_state(room).patch = Some(patch);
    }

    fn paint_ruins(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        set_all_doors(rooms, room, DoorType::Regular);
        let fill = halls_patch_fill(&rooms[room]);
        self.setup_patch(level, rooms, room, fill, 0, rng);
        let bounds = rooms[room].bounds;
        for y in bounds.top + 1..bounds.bottom {
            for x in bounds.left + 1..bounds.right {
                let point = Point::new(x, y);
                if !self.patch_at(rooms, room, point) {
                    continue;
                }
                let wall = if y > bounds.top + 1
                    && y < bounds.bottom - 1
                    && x > bounds.left + 1
                    && x < bounds.right - 1
                {
                    let adjacent = i32::from(self.patch_at(rooms, room, Point::new(x - 1, y)))
                        + i32::from(self.patch_at(rooms, room, Point::new(x + 1, y)))
                        + i32::from(self.patch_at(rooms, room, Point::new(x, y - 1)))
                        + i32::from(self.patch_at(rooms, room, Point::new(x, y + 1)));
                    rng.int_bound(2) < adjacent
                } else {
                    true
                };
                level.map.set(
                    x,
                    y,
                    if wall {
                        terrain::WALL
                    } else {
                        terrain::REGION_DECO
                    },
                );
            }
        }
    }

    fn paint_chasm(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        set_all_doors(rooms, room, DoorType::Regular);
        let fill = halls_patch_fill(&rooms[room]);
        self.setup_patch(level, rooms, room, fill, 1, rng);
        let bounds = rooms[room].bounds;
        for y in bounds.top + 1..bounds.bottom {
            for x in bounds.left + 1..bounds.right {
                if self.patch_at(rooms, room, Point::new(x, y)) {
                    level.map.set(x, y, terrain::CHASM);
                }
            }
        }
    }

    fn paint_skulls(&self, level: &mut Level, rooms: &mut [Room], room: RoomId) {
        let bounds = rooms[room].bounds;
        let width = rooms[room].width();
        let height = rooms[room].height();
        fill_room(level, &rooms[room], terrain::WALL);
        draw::fill_ellipse(
            &mut level.map,
            bounds.left + 2,
            bounds.top + 2,
            width - 4,
            height - 4,
            terrain::EMPTY,
        );
        for (neighbour, door) in room_doors(rooms, room) {
            set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
            draw::draw_inside(
                &mut level.map,
                bounds,
                door,
                if door.x == bounds.left || door.x == bounds.right {
                    div_i32(width, 2)
                } else {
                    div_i32(height, 2)
                },
                terrain::EMPTY,
            );
        }
        draw::fill_ellipse(
            &mut level.map,
            bounds.left + 4,
            bounds.top + 4,
            width - 8,
            height - 8,
            terrain::STATUE,
        );
        draw::fill_ellipse(
            &mut level.map,
            bounds.left + 6,
            bounds.top + 6,
            width - 12,
            height - 12,
            terrain::WALL,
        );
    }

    fn paint_ritual(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        transition: Option<bool>,
        rng: &mut RandomStack,
    ) {
        fill_room(level, &rooms[room], terrain::WALL);
        fill_room_margin(level, &rooms[room], 1, terrain::EMPTY);
        let center = rooms[room].center(rng);
        let fill = halls_patch_fill(&rooms[room]);
        self.setup_patch(level, rooms, room, fill, 0, rng);
        let bounds = rooms[room].bounds;
        for y in bounds.top + 1..bounds.bottom {
            for x in bounds.left + 1..bounds.right {
                if self.patch_at(rooms, room, Point::new(x, y)) {
                    level.map.set(x, y, terrain::REGION_DECO);
                }
            }
        }
        draw::fill(
            &mut level.map,
            center.x - 3,
            center.y - 3,
            7,
            7,
            terrain::EMPTY,
        );
        for point in [
            Point::new(center.x - 2, center.y - 1),
            Point::new(center.x - 1, center.y - 2),
            Point::new(center.x + 2, center.y - 1),
            Point::new(center.x + 1, center.y - 2),
            Point::new(center.x - 2, center.y + 1),
            Point::new(center.x - 1, center.y + 2),
            Point::new(center.x + 2, center.y + 1),
            Point::new(center.x + 1, center.y + 2),
        ] {
            level.map.set_point(point, terrain::STATUE);
        }
        draw::fill(
            &mut level.map,
            center.x - 1,
            center.y - 1,
            3,
            3,
            terrain::EMBERS,
        );
        level.map.set_point(center, terrain::PEDESTAL);
        let cell = level.point_to_cell(center);
        if let Some(entrance) = transition {
            level.map.cells[cell] = if entrance {
                terrain::ENTRANCE
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
        } else {
            let prize = if rng.int_bound(2) == 0 {
                self.common.content.find_prize_item(rng)
            } else {
                None
            }
            .unwrap_or_else(|| {
                let category = if rng.int_bound(2) == 0 {
                    GeneratorCategory::Potion
                } else {
                    GeneratorCategory::Scroll
                };
                self.common.content.random_category(category, rng)
            });
            level.drop_item(prize, cell, HeapKind::Heap);
        }
        set_all_doors(rooms, room, DoorType::Regular);
    }

    fn place_patch_transition(
        &self,
        level: &mut Level,
        rooms: &[Room],
        room: RoomId,
        entrance: bool,
        obstacle: PatchObstacle,
        rng: &mut RandomStack,
    ) {
        let mut tries = 30_i32;
        let point = loop {
            let point = random_room_point(&rooms[room], 2, rng);
            let cell = level.point_to_cell(point);
            let valid = if tries > 0 {
                !obstacle.blocks(level.map.cells[cell]) && !level.mob_cells[cell]
            } else {
                let cell_i32 = i32::try_from(cell).expect("map exceeds Java int");
                [-level.width(), -1, 1, level.width()]
                    .into_iter()
                    .any(|offset| {
                        let neighbour = usize::try_from(cell_i32 + offset)
                            .expect("transition is within the room perimeter");
                        obstacle.fallback_open(level.map.cells[neighbour])
                    })
                    && !level.mob_cells[cell]
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
                TransitionKind::RegularEntrance
            } else {
                TransitionKind::RegularExit
            },
        );
    }
}

#[derive(Clone, Copy)]
enum PatchObstacle {
    Wall,
    Chasm,
}

impl PatchObstacle {
    const fn blocks(self, tile: i32) -> bool {
        match self {
            Self::Wall => tile == terrain::WALL,
            Self::Chasm => tile == terrain::CHASM,
        }
    }

    const fn fallback_open(self, tile: i32) -> bool {
        match self {
            Self::Wall => tile != terrain::WALL && tile != terrain::REGION_DECO,
            Self::Chasm => tile != terrain::CHASM,
        }
    }
}

/// Halls wrapper supplying exact water, grass, traps, and decoration.
#[derive(Clone, Debug, PartialEq)]
pub struct HallsPainter {
    pub regular: RegularPainter,
}

impl HallsPainter {
    #[must_use]
    pub fn new(feeling: Feeling, trap_count: i32) -> Self {
        let (classes, chances) = halls_trap_table();
        Self {
            regular: RegularPainter::default()
                .set_water(
                    if feeling == Feeling::Water {
                        0.70
                    } else {
                        0.15
                    },
                    6,
                )
                .set_grass(
                    if feeling == Feeling::Grass {
                        0.65
                    } else {
                        0.10
                    },
                    3,
                )
                .set_traps(trap_count, classes, chances),
        }
    }

    /// Paint a Halls map using the supplied exact composite dispatcher.
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
            .paint_with_decorator(level, rooms, dispatch, rng, decorate_halls)
    }
}

/// Exact v3.3.8 `HallsPainter.decorate`, including disconnected-neighbour
/// merges and their duplicated Java traversal.
pub fn decorate_halls(level: &mut Level, rooms: &[Room], order: &[RoomId], rng: &mut RandomStack) {
    let width = usize::try_from(level.width()).expect("level width is positive");
    let width_i32 = level.width();
    for cell in width + 1..level.len() - width - 1 {
        if level.map.cells[cell] == terrain::EMPTY {
            let cell_i32 = i32::try_from(cell).expect("map exceeds Java int");
            let mut count = 0_i32;
            for offset in [
                -width_i32 - 1,
                -width_i32,
                -width_i32 + 1,
                -1,
                1,
                width_i32 - 1,
                width_i32,
                width_i32 + 1,
            ] {
                let neighbour =
                    usize::try_from(cell_i32 + offset).expect("decorated cell excludes map edges");
                if terrain::flags(level.map.cells[neighbour]) & terrain::PASSABLE != 0 {
                    count += 1;
                }
            }
            if rng.int_bound(80) < count {
                level.map.cells[cell] = terrain::EMPTY_DECO;
            }
        } else if level.map.cells[cell] == terrain::WALL
            && level.map.cells[cell - 1] != terrain::WALL_DECO
            && level.map.cells[cell - width] != terrain::WALL_DECO
            && rng.int_bound(20) == 0
        {
            level.map.cells[cell] = terrain::WALL_DECO;
        } else if level.map.cells[cell] == terrain::REGION_DECO && rng.int_bound(2) == 0 {
            level.map.cells[cell] = terrain::REGION_DECO_ALT;
        }
    }

    let mut dispatch = HallsDecorationMergeDispatch;
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
                    &mut dispatch,
                    rng,
                );
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct HallsDecorationMergeDispatch;

impl RoomPaintDispatch for HallsDecorationMergeDispatch {
    fn paint_room(
        &mut self,
        _level: &mut Level,
        _rooms: &mut [Room],
        _room: RoomId,
        _rng: &mut RandomStack,
    ) {
        unreachable!("decoration merge dispatcher never paints rooms")
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
        halls_can_merge::<crate::sewer_rooms::GeneratorRoomContent>(
            None,
            level,
            rooms,
            room,
            other,
            point,
            merge_terrain,
        )
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
        halls_merge(level, rooms, room, other, merge, merge_terrain);
    }
}

fn halls_can_merge<C: SewerRoomContent>(
    common: Option<&SewerRoomDispatcher<C>>,
    level: &Level,
    rooms: &[Room],
    room: RoomId,
    other: RoomId,
    point: Point,
    merge_terrain: i32,
) -> bool {
    match rooms[room].kind {
        kind if is_ruins_family(kind) => true,
        kind if common.is_some() && (is_shared_room(kind) || is_halls_connection(kind)) => common
            .expect("checked above")
            .can_merge(level, rooms, room, other, point, merge_terrain),
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

fn halls_merge(
    level: &mut Level,
    rooms: &[Room],
    room: RoomId,
    other: RoomId,
    merge: Rect,
    merge_terrain: i32,
) {
    let room_kind = rooms[room].kind;
    let other_kind = rooms[other].kind;
    let (tile, door_tile) = if is_chasm_family(room_kind)
        && merge_terrain == terrain::EMPTY
        && (is_chasm_family(other_kind)
            || matches!(other_kind, RoomKind::Standard(StandardRoomKind::Platform)))
    {
        (terrain::CHASM, Some(terrain::EMPTY))
    } else if matches!(room_kind, RoomKind::Standard(StandardRoomKind::Platform))
        && merge_terrain != terrain::CHASM
        && rooms[room].connection_to(other).is_some()
        && (is_chasm_family(other_kind)
            || matches!(other_kind, RoomKind::Standard(StandardRoomKind::Platform)))
    {
        (terrain::CHASM, Some(terrain::EMPTY_SP))
    } else if matches!(
        room_kind,
        RoomKind::Standard(StandardRoomKind::Plants | StandardRoomKind::GrassyGrave)
    ) && merge_terrain == terrain::EMPTY
        && matches!(
            other_kind,
            RoomKind::Standard(StandardRoomKind::Plants | StandardRoomKind::GrassyGrave)
        )
    {
        (terrain::GRASS, None)
    } else if matches!(room_kind, RoomKind::Standard(StandardRoomKind::Striped))
        && merge_terrain == terrain::EMPTY
        && matches!(other_kind, RoomKind::Standard(StandardRoomKind::Striped))
    {
        (terrain::EMPTY_SP, None)
    } else {
        (merge_terrain, None)
    };
    draw::fill_rect(&mut level.map, merge, tile);
    if let Some(door_tile) = door_tile {
        let door = rooms[room]
            .connection_to(other)
            .and_then(|connection| connection.door)
            .expect("special merged rooms share a placed door");
        level.map.set_point(door.point, door_tile);
    }
}

fn halls_patch_fill(room: &Room) -> f32 {
    let scale = room.width().wrapping_mul(room.height()).min(18 * 18);
    0.30 + scale as f32 / 1024.0
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

fn clear_neighbours8(level: &mut Level, cell: usize, tile: i32) {
    let width = level.width();
    let cell = i32::try_from(cell).expect("map exceeds Java int");
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
        let neighbour = usize::try_from(cell + offset).expect("cell is within padded room");
        level.map.cells[neighbour] = tile;
    }
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

const fn is_halls_standard(kind: StandardRoomKind) -> bool {
    matches!(
        kind,
        StandardRoomKind::Ruins
            | StandardRoomKind::Chasm
            | StandardRoomKind::Skulls
            | StandardRoomKind::Ritual
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

const fn is_shared_room(kind: RoomKind) -> bool {
    is_common_standard(kind)
        || matches!(
            kind,
            RoomKind::Standard(StandardRoomKind::RegionDecoPatch)
                | RoomKind::Entrance(StandardRoomKind::RegionDecoPatch)
                | RoomKind::Exit(StandardRoomKind::RegionDecoPatch)
        )
}

const fn is_halls_connection(kind: RoomKind) -> bool {
    matches!(
        kind,
        RoomKind::Connection(
            ConnectionRoomKind::Tunnel
                | ConnectionRoomKind::Bridge
                | ConnectionRoomKind::Walkway
                | ConnectionRoomKind::RingTunnel
                | ConnectionRoomKind::RingBridge
                | ConnectionRoomKind::Maze
        )
    )
}

const fn is_ruins_family(kind: RoomKind) -> bool {
    matches!(
        kind,
        RoomKind::Standard(StandardRoomKind::Ruins)
            | RoomKind::Entrance(StandardRoomKind::Ruins)
            | RoomKind::Exit(StandardRoomKind::Ruins)
    )
}

const fn is_chasm_family(kind: RoomKind) -> bool {
    matches!(
        kind,
        RoomKind::Standard(StandardRoomKind::Chasm)
            | RoomKind::Entrance(StandardRoomKind::Chasm)
            | RoomKind::Exit(StandardRoomKind::Chasm)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::{GeneratedItem, SeedKind};
    use crate::level::PaintItem;
    use crate::room::{Door, RoomConnection, SizeCategory};

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
        let mut level = Level::new(23, Feeling::None);
        level.set_size(28, 28);
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        let mut dispatcher = HallsRoomDispatcher::new(FixtureContent);
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
    fn halls_trap_table_and_painter_parameters_are_exact() {
        let (classes, chances) = halls_trap_table();
        assert_eq!(
            chances,
            [
                4.0, 4.0, 4.0, 4.0, 4.0, 2.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
                1.0, 1.0,
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
                TrapSpec::new(TrapKind::Grim)
                    .avoids_hallways()
                    .cannot_be_hidden(),
                TrapSpec::new(TrapKind::Pitfall),
                TrapSpec::new(TrapKind::Distortion),
                TrapSpec::new(TrapKind::Gateway).avoids_hallways(),
                TrapSpec::new(TrapKind::Geyser),
            ]
        );
        let painter = HallsPainter::new(Feeling::None, 4);
        assert_eq!(painter.regular.water_fill.to_bits(), 0.15_f32.to_bits());
        assert_eq!(painter.regular.water_smoothness, 6);
        assert_eq!(painter.regular.grass_fill.to_bits(), 0.10_f32.to_bits());
        assert_eq!(painter.regular.grass_smoothness, 3);
        assert_eq!(
            HallsPainter::new(Feeling::Water, 0)
                .regular
                .water_fill
                .to_bits(),
            0.70_f32.to_bits()
        );
        assert_eq!(
            HallsPainter::new(Feeling::Grass, 0)
                .regular
                .grass_fill
                .to_bits(),
            0.65_f32.to_bits()
        );
    }

    #[test]
    fn every_halls_standard_size_variant_matches_official_fixture() {
        let fixtures = [
            (
                StandardRoomKind::RegionDecoPatch,
                SizeCategory::Normal,
                9,
                (1_636_741_910, -1_279_993_237, 0),
                false,
            ),
            (
                StandardRoomKind::Ruins,
                SizeCategory::Normal,
                9,
                (-107_961_639, -947_007_904, 0),
                false,
            ),
            (
                StandardRoomKind::Ruins,
                SizeCategory::Large,
                13,
                (426_075_957, 1_302_916_845, 0),
                false,
            ),
            (
                StandardRoomKind::Ruins,
                SizeCategory::Giant,
                17,
                (-233_458_687, 1_841_310_684, 0),
                false,
            ),
            (
                StandardRoomKind::Chasm,
                SizeCategory::Normal,
                9,
                (354_466_588, 1_779_264_910, 0),
                false,
            ),
            (
                StandardRoomKind::Chasm,
                SizeCategory::Large,
                13,
                (-50_462_092, 713_544_083, 0),
                false,
            ),
            (
                StandardRoomKind::Chasm,
                SizeCategory::Giant,
                17,
                (1_329_053_367, -71_003_034, 0),
                false,
            ),
            (
                StandardRoomKind::Skulls,
                SizeCategory::Large,
                13,
                (1_174_750_715, 1_678_708_340, 0),
                false,
            ),
            (
                StandardRoomKind::Skulls,
                SizeCategory::Giant,
                17,
                (623_138_523, 1_678_708_340, 0),
                false,
            ),
            (
                StandardRoomKind::Ritual,
                SizeCategory::Normal,
                9,
                (699_246_400, 1_359_837_091, 1),
                true,
            ),
            (
                StandardRoomKind::Ritual,
                SizeCategory::Large,
                13,
                (1_670_865_516, -1_161_916_463, 1),
                true,
            ),
            (
                StandardRoomKind::Ritual,
                SizeCategory::Giant,
                17,
                (-1_233_360_360, 2_078_328_377, 1),
                true,
            ),
        ];
        for (kind, category, size, expected, provider_dependent_rng) in fixtures {
            let actual = paint_fixture(RoomKind::Standard(kind), Some(category), size);
            assert_eq!(
                actual.0, expected.0,
                "map differs for {kind:?} {category:?}"
            );
            assert_eq!(
                actual.2, expected.2,
                "events differ for {kind:?} {category:?}"
            );
            if !provider_dependent_rng {
                assert_eq!(
                    actual.1, expected.1,
                    "RNG differs for {kind:?} {category:?}"
                );
            }
        }
    }

    #[test]
    fn every_reachable_common_standard_size_variant_matches_official_fixture() {
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
            assert_eq!(
                actual.0, expected.0,
                "map differs for {kind:?} {category:?}"
            );
            assert_eq!(
                actual.2, expected.2,
                "events differ for {kind:?} {category:?}"
            );
            if !provider_dependent_rng {
                assert_eq!(
                    actual.1, expected.1,
                    "RNG differs for {kind:?} {category:?}"
                );
            }
        }
    }

    #[test]
    fn every_halls_transition_size_variant_matches_official_fixture() {
        let fixtures = [
            (
                true,
                StandardRoomKind::RegionDecoPatch,
                SizeCategory::Normal,
                9,
                (1_830_000_412, -311_585_061, 1),
            ),
            (
                false,
                StandardRoomKind::RegionDecoPatch,
                SizeCategory::Normal,
                9,
                (430_554_397, -311_585_061, 1),
            ),
            (
                true,
                StandardRoomKind::Ruins,
                SizeCategory::Normal,
                9,
                (-1_547_510_416, 1_779_264_910, 1),
            ),
            (
                true,
                StandardRoomKind::Ruins,
                SizeCategory::Large,
                13,
                (-1_577_675_572, 552_227_391, 1),
            ),
            (
                false,
                StandardRoomKind::Ruins,
                SizeCategory::Normal,
                9,
                (1_520_245_199, 1_779_264_910, 1),
            ),
            (
                false,
                StandardRoomKind::Ruins,
                SizeCategory::Large,
                13,
                (-1_266_284_501, 552_227_391, 1),
            ),
            (
                true,
                StandardRoomKind::Chasm,
                SizeCategory::Normal,
                9,
                (547_725_090, -311_585_061, 1),
            ),
            (
                true,
                StandardRoomKind::Chasm,
                SizeCategory::Large,
                13,
                (443_959_418, 701_708_724, 1),
            ),
            (
                false,
                StandardRoomKind::Chasm,
                SizeCategory::Normal,
                9,
                (-851_720_925, -311_585_061, 1),
            ),
            (
                false,
                StandardRoomKind::Chasm,
                SizeCategory::Large,
                13,
                (-1_621_120_645, 701_708_724, 1),
            ),
            (
                true,
                StandardRoomKind::Ritual,
                SizeCategory::Large,
                13,
                (-1_321_285_016, 1_090_079_848, 1),
            ),
            (
                false,
                StandardRoomKind::Ritual,
                SizeCategory::Large,
                13,
                (1_574_236_265, 1_090_079_848, 1),
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
                "transition differs for {kind:?} {category:?}"
            );
        }
    }

    #[test]
    fn every_halls_connection_including_injected_maze_matches_official_fixture() {
        let fixtures = [
            (
                ConnectionRoomKind::Tunnel,
                (-1_920_351_944, -2_040_237_378, 0),
            ),
            (
                ConnectionRoomKind::Bridge,
                (-758_714_528, -2_040_237_378, 0),
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
    fn halls_decoration_and_disconnected_merge_match_official_fixture() {
        let mut constructor_rng = RandomStack::with_base_seed(17);
        let mut first = Room::standard(StandardRoomKind::Ruins, &mut constructor_rng);
        first.size_category = Some(SizeCategory::Normal);
        first.bounds = Rect::new(1, 1, 9, 9);
        let mut second = Room::standard(StandardRoomKind::Chasm, &mut constructor_rng);
        second.size_category = Some(SizeCategory::Normal);
        second.bounds = Rect::new(9, 1, 17, 9);
        first.neighbours.push(1);
        second.neighbours.push(0);
        let rooms = vec![first, second];
        let mut level = Level::new(23, Feeling::None);
        level.set_size(20, 12);
        fill_room(&mut level, &rooms[0], terrain::WALL);
        fill_room_margin(&mut level, &rooms[0], 1, terrain::EMPTY);
        fill_room(&mut level, &rooms[1], terrain::WALL);
        fill_room_margin(&mut level, &rooms[1], 1, terrain::EMPTY);
        level.map.set(4, 4, terrain::REGION_DECO);
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        decorate_halls(&mut level, &rooms, &[0, 1], &mut rng);
        assert_eq!(level.java_map_hash(), 416_102_347);
        assert_eq!(rng.int(), -1_430_373_623);
    }

    #[test]
    fn placement_and_chasm_platform_merge_rules_preserve_halls_overrides() {
        let mut level = Level::new(23, Feeling::None);
        level.set_size(28, 28);
        let dispatcher = HallsRoomDispatcher::new(FixtureContent);
        let exit_rooms = fixture_room(
            RoomKind::Exit(StandardRoomKind::Ritual),
            Some(SizeCategory::Large),
            13,
        );
        let point = Point::new(10, 10);
        let cell = level.point_to_cell(point);
        level.map.cells[cell] = terrain::EMPTY;
        level.add_transition(cell, TransitionKind::RegularExit);
        assert!(dispatcher.can_place_item(&level, &exit_rooms, 0, point));
        assert!(!dispatcher.can_place_character(&level, &exit_rooms, 0, point));
        assert!(!dispatcher.can_place_item(&level, &exit_rooms, 0, Point::new(5, 5)));

        let mut rooms = fixture_room(
            RoomKind::Standard(StandardRoomKind::Chasm),
            Some(SizeCategory::Normal),
            9,
        );
        rooms[1].kind = RoomKind::Standard(StandardRoomKind::Platform);
        rooms[1].size_category = Some(SizeCategory::Normal);
        let door = rooms[0].connection_to(1).unwrap().door.unwrap().point;
        level
            .map
            .set_point(rooms[0].point_inside(door, 1), terrain::EMPTY);
        level
            .map
            .set_point(rooms[1].point_inside(door, 1), terrain::EMPTY);
        assert!(dispatcher.can_merge(&level, &rooms, 0, 1, door, terrain::EMPTY));
        assert!(dispatcher.can_merge(&level, &rooms, 1, 0, door, terrain::EMPTY));

        let merge = Rect::new(door.x, door.y - 1, door.x + 1, door.y + 2);
        let mut dispatcher = dispatcher;
        dispatcher.merge(&mut level, &rooms, 0, 1, merge, terrain::EMPTY);
        assert_eq!(level.map.cells[level.point_to_cell(door)], terrain::EMPTY);
        assert!(merge.points().into_iter().any(|point| {
            point != door && level.map.cells[level.point_to_cell(point)] == terrain::CHASM
        }));

        dispatcher.merge(&mut level, &rooms, 1, 0, merge, terrain::EMPTY);
        assert_eq!(
            level.map.cells[level.point_to_cell(door)],
            terrain::EMPTY_SP
        );
    }

    #[test]
    fn assembled_halls_painter_matches_official_fixture() {
        let mut constructor_rng = RandomStack::with_base_seed(17);
        let mut entrance = Room::standard(StandardRoomKind::RegionDecoPatch, &mut constructor_rng);
        entrance.kind = RoomKind::Entrance(StandardRoomKind::RegionDecoPatch);
        entrance.size_category = Some(SizeCategory::Normal);
        entrance.bounds = Rect::new(0, 0, 8, 8);
        let mut middle = Room::standard(StandardRoomKind::Chasm, &mut constructor_rng);
        middle.size_category = Some(SizeCategory::Normal);
        middle.bounds = Rect::new(8, 0, 16, 8);
        let mut exit = Room::standard(StandardRoomKind::Ruins, &mut constructor_rng);
        exit.kind = RoomKind::Exit(StandardRoomKind::Ruins);
        exit.size_category = Some(SizeCategory::Normal);
        exit.bounds = Rect::new(16, 0, 24, 8);
        let mut rooms = vec![entrance, middle, exit];
        connect_unplaced(&mut rooms, 0, 1);
        connect_unplaced(&mut rooms, 1, 2);
        let mut level = Level::new(23, Feeling::None);
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        let mut dispatcher = HallsRoomDispatcher::new(FixtureContent);
        let mut painter = HallsPainter::new(Feeling::None, 3);
        painter
            .paint(&mut level, &mut rooms, &mut dispatcher, &mut rng)
            .unwrap();
        assert_eq!(level.java_map_hash(), -1_190_761_107);
        assert_eq!(rng.int(), -91_199_810);
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
