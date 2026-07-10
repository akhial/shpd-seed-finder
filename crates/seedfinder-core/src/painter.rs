//! Exact data-only port of v3.3.8 regular-level painting.
//!
//! The call order is part of seed compatibility:
//!
//! 1. normalize room bounds and allocate the level map;
//! 2. shuffle the Java room list (stored as [`Level::room_order`]);
//! 3. for each room in that order, place its as-yet-unplaced doors and invoke
//!    [`RoomPaintDispatch::paint_room`];
//! 4. merge rooms and resolve/paint doors;
//! 5. draw one `Random.Long()` from the depth stream, push it as an isolated
//!    child generator, then paint water, grass, traps, and region decoration;
//! 6. pop the child generator.
//!
//! Room subclasses own a large amount of region-specific behavior. Rather
//! than hiding an incomplete match in this generic layer, that behavior is an
//! explicit callback. A complete dispatcher must paint every concrete room,
//! implement its `canMerge`/`merge` overrides, and implement its water, grass,
//! and trap placement predicates. Door changes made by a room must go through
//! [`set_shared_door_type`] so the two Rust connection records preserve Java's
//! shared `Room.Door` object semantics.

// These compatibility helpers intentionally preserve Java's unchecked array
// and graph invariants. Repeating the same invariant-panic section on every
// low-level port obscures the draw-order documentation above.
#![allow(clippy::missing_panics_doc)]

use std::collections::VecDeque;
use std::fmt;

use crate::geometry::{Point, Rect, painter as draw, terrain};
use crate::java_math::round_f32;
use crate::level::{Feeling, Level, PlacedTrap, TrapSpec, sewer_trap_table};
use crate::rng::{FastBound, RandomStack};
use crate::room::{Door, DoorType, Room, RoomId, RoomKind, SizeCategory, place_doors_in_order};

/// A structural painting failure which makes upstream `RegularPainter.paint`
/// return false (or report an impossible builder invariant).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaintError {
    EmptyMapWithoutRooms,
    SpecialRoomWithoutConnections { room: RoomId },
    NoDoorCandidate { room: RoomId, neighbour: RoomId },
}

impl fmt::Display for PaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::EmptyMapWithoutRooms => formatter.write_str("cannot paint an uninitialized map"),
            Self::SpecialRoomWithoutConnections { room } => {
                write!(formatter, "special room {room} has no connections")
            }
            Self::NoDoorCandidate { room, neighbour } => {
                write!(
                    formatter,
                    "rooms {room} and {neighbour} have no valid door point"
                )
            }
        }
    }
}

impl std::error::Error for PaintError {}

/// Region- and class-specific room behavior called by [`RegularPainter`].
pub trait RoomPaintDispatch {
    /// Exact concrete `Room.paint(Level)` implementation. This is called only
    /// after all currently reachable null doors for `room` have been placed.
    fn paint_room(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        room: RoomId,
        rng: &mut RandomStack,
    );

    /// Concrete `Room.canMerge`. Base `Room` returns false; standard room
    /// dispatchers can delegate to [`standard_can_merge`].
    fn can_merge(
        &self,
        _level: &Level,
        _rooms: &[Room],
        _room: RoomId,
        _other: RoomId,
        _point: Point,
        _merge_terrain: i32,
    ) -> bool {
        false
    }

    /// Concrete `Room.merge`. The base implementation fills the selected
    /// rectangle with `merge_terrain`.
    fn merge(
        &mut self,
        level: &mut Level,
        _rooms: &[Room],
        _room: RoomId,
        _other: RoomId,
        merge: Rect,
        merge_terrain: i32,
    ) {
        draw::fill_rect(&mut level.map, merge, merge_terrain);
    }

    fn can_place_water(
        &self,
        _level: &Level,
        _rooms: &[Room],
        _room: RoomId,
        _point: Point,
    ) -> bool {
        true
    }

    fn can_place_grass(
        &self,
        _level: &Level,
        _rooms: &[Room],
        _room: RoomId,
        _point: Point,
    ) -> bool {
        true
    }

    fn can_place_trap(
        &self,
        _level: &Level,
        _rooms: &[Room],
        _room: RoomId,
        _point: Point,
    ) -> bool {
        true
    }
}

/// Default `StandardRoom.canMerge` implementation.
#[must_use]
pub fn standard_can_merge(level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
    let inside = rooms[room].point_inside(point, 1);
    let cell = level.point_to_cell(inside);
    terrain::flags(level.map.cells[cell]) & terrain::SOLID == 0
}

/// Fill an inclusive room rectangle. `Room.width()`/`height()` are one larger
/// than the underlying `Rect` helpers used by generic drawing routines.
pub fn fill_room(level: &mut Level, room: &Room, value: i32) {
    draw::fill(
        &mut level.map,
        room.bounds.left,
        room.bounds.top,
        room.width(),
        room.height(),
        value,
    );
}

/// Fill an inclusive room rectangle after removing an equal margin from all
/// four sides. Java dispatches `Rect.width()` virtually to `Room.width()`.
pub fn fill_room_margin(level: &mut Level, room: &Room, margin: i32, value: i32) {
    draw::fill(
        &mut level.map,
        room.bounds.left.wrapping_add(margin),
        room.bounds.top.wrapping_add(margin),
        room.width().wrapping_sub(margin.wrapping_mul(2)),
        room.height().wrapping_sub(margin.wrapping_mul(2)),
        value,
    );
}

fn two_rooms_mut(rooms: &mut [Room], first: RoomId, second: RoomId) -> (&mut Room, &mut Room) {
    assert_ne!(first, second, "a room cannot connect to itself");
    if first < second {
        let (left, right) = rooms.split_at_mut(second);
        (&mut left[first], &mut right[0])
    } else {
        let (left, right) = rooms.split_at_mut(first);
        (&mut right[0], &mut left[second])
    }
}

/// Apply `Room.Door.set` to the shared connection object represented by the
/// two directed Rust records.
pub fn set_shared_door_type(
    rooms: &mut [Room],
    first: RoomId,
    second: RoomId,
    door_type: DoorType,
) {
    let (first_room, second_room) = two_rooms_mut(rooms, first, second);
    first_room
        .connection_to_mut(second)
        .expect("forward room connection is missing")
        .door
        .as_mut()
        .expect("door must be placed before room painting")
        .set_type(door_type);
    second_room
        .connection_to_mut(first)
        .expect("reverse room connection is missing")
        .door
        .as_mut()
        .expect("reverse door must be placed before room painting")
        .set_type(door_type);
}

fn force_shared_door_type(rooms: &mut [Room], first: RoomId, second: RoomId, door_type: DoorType) {
    let (first_room, second_room) = two_rooms_mut(rooms, first, second);
    first_room
        .connection_to_mut(second)
        .expect("forward room connection is missing")
        .door
        .as_mut()
        .expect("door must have been placed")
        .door_type = door_type;
    second_room
        .connection_to_mut(first)
        .expect("reverse room connection is missing")
        .door
        .as_mut()
        .expect("reverse door must have been placed")
        .door_type = door_type;
}

fn shared_door(rooms: &[Room], first: RoomId, second: RoomId) -> Door {
    rooms[first]
        .connection_to(second)
        .expect("forward room connection is missing")
        .door
        .expect("door must have been placed")
}

/// Make direct callback mutations converge to the same value on both records.
/// Correct room ports should use [`set_shared_door_type`]; this synchronization
/// is a defensive check around legacy/simple callback implementations.
fn synchronize_room_doors(rooms: &mut [Room], room: RoomId) {
    let neighbours: Vec<RoomId> = rooms[room]
        .connected
        .iter()
        .map(|connection| connection.room)
        .collect();
    for neighbour in neighbours {
        let forward = rooms[room]
            .connection_to(neighbour)
            .and_then(|connection| connection.door);
        let reverse = rooms[neighbour]
            .connection_to(room)
            .and_then(|connection| connection.door);
        let merged = match (forward, reverse) {
            (Some(mut first), Some(second)) => {
                first.door_type = first.door_type.max(second.door_type);
                first.type_locked |= second.type_locked;
                Some(first)
            }
            (Some(door), None) | (None, Some(door)) => Some(door),
            (None, None) => None,
        };
        let (first_room, second_room) = two_rooms_mut(rooms, room, neighbour);
        first_room
            .connection_to_mut(neighbour)
            .expect("forward room connection is missing")
            .door = merged;
        second_room
            .connection_to_mut(room)
            .expect("reverse room connection is missing")
            .door = merged;
    }
}

/// Translate rooms so their padded map starts at zero, then call
/// `Level.setSize`. This must happen before the room-list shuffle.
pub fn normalize_rooms(level: &mut Level, rooms: &mut [Room]) {
    assert!(
        !rooms.is_empty(),
        "regular painting requires at least one room"
    );
    let padding = if level.feeling == Feeling::Chasm {
        2
    } else {
        1
    };
    let mut left_most = i32::MAX;
    let mut top_most = i32::MAX;
    for room in &*rooms {
        left_most = left_most.min(room.bounds.left);
        top_most = top_most.min(room.bounds.top);
    }
    left_most = left_most.wrapping_sub(padding);
    top_most = top_most.wrapping_sub(padding);

    let mut right_most = 0_i32;
    let mut bottom_most = 0_i32;
    for room in rooms.iter_mut() {
        room.shift(left_most.wrapping_neg(), top_most.wrapping_neg());
        right_most = right_most.max(room.bounds.right);
        bottom_most = bottom_most.max(room.bounds.bottom);
    }
    right_most = right_most.wrapping_add(padding);
    bottom_most = bottom_most.wrapping_add(padding);
    level.set_size(right_most.wrapping_add(1), bottom_most.wrapping_add(1));
}

/// Exact v3.3.8 `Patch.generate` cellular patch algorithm.
#[must_use]
#[allow(clippy::too_many_lines)] // A direct, auditable port of one Java method.
pub fn generate_patch(
    width: i32,
    height: i32,
    mut fill: f32,
    clustering: i32,
    force_fill_rate: bool,
    rng: &mut RandomStack,
) -> Vec<bool> {
    let length_i32 = width.wrapping_mul(height);
    let length = usize::try_from(length_i32).expect("patch dimensions must be non-negative");
    let mut current = vec![false; length];
    let mut output = vec![false; length];
    #[allow(clippy::cast_precision_loss)]
    let mut fill_difference = round_f32(length_i32 as f32 * fill).wrapping_neg();

    if force_fill_rate && clustering > 0 {
        fill += (0.5_f32 - fill) * 0.5_f32;
    }

    let generator = rng.current_generator();
    fill_difference = fill_difference.wrapping_add(generator.fill_f32_below(&mut output, fill));

    if length > 0 && clustering > 0 {
        let width_usize = usize::try_from(width).expect("patch width is non-negative");
        let height_usize = usize::try_from(height).expect("patch height is non-negative");
        if (2..=64).contains(&width_usize) {
            // Bit-sliced automaton: one u64 row per map line, all interior
            // columns of a row evaluated at once. The board converts to bit
            // rows once for every smoothing pass, then back.
            let mut rows = vec![0_u64; height_usize];
            for (bits, cells) in rows.iter_mut().zip(output.chunks_exact(width_usize)) {
                for (x, &cell) in cells.iter().enumerate() {
                    *bits |= u64::from(cell) << x;
                }
            }
            let mut next_rows = vec![0_u64; height_usize];
            for _ in 0..clustering {
                fill_difference = fill_difference.wrapping_add(clustering_pass_bits(
                    &rows,
                    &mut next_rows,
                    width_usize,
                    height_usize,
                ));
                std::mem::swap(&mut rows, &mut next_rows);
            }
            for (bits, cells) in rows.iter().zip(output.chunks_exact_mut(width_usize)) {
                for (x, cell) in cells.iter_mut().enumerate() {
                    *cell = bits & (1 << x) != 0;
                }
            }
        } else {
            let mut column_counts = vec![0_u8; width_usize];
            for _ in 0..clustering {
                fill_difference = fill_difference.wrapping_add(clustering_pass(
                    &output,
                    &mut current,
                    width_usize,
                    height_usize,
                    &mut column_counts,
                ));
                std::mem::swap(&mut current, &mut output);
            }
        }
    }

    if force_fill_rate && width.min(height) > 2 {
        let offsets = [
            -width - 1,
            -width,
            -width + 1,
            -1,
            0,
            1,
            width - 1,
            width,
            width + 1,
        ];
        let growing = fill_difference < 0;
        let x_bound = FastBound::new(width - 2);
        let y_bound = FastBound::new(height - 2);
        while fill_difference != 0 {
            let mut cell_i32;
            let mut tries = 0_i32;
            loop {
                // Same draws as `rng.int_between(1, width - 1)` and
                // `rng.int_between(1, height - 1)`; the bounds are positive
                // here because both dimensions exceed 2.
                let x = generator.next_i32_fast_bound(&x_bound).wrapping_add(1);
                let y = generator.next_i32_fast_bound(&y_bound).wrapping_add(1);
                cell_i32 = x.wrapping_add(y.wrapping_mul(width));
                tries = tries.wrapping_add(1);
                let cell = usize::try_from(cell_i32).expect("patch correction cell is negative");
                if output[cell] == growing || tries.wrapping_mul(10) >= length_i32 {
                    break;
                }
            }

            for offset in offsets {
                if fill_difference == 0 {
                    break;
                }
                let index = usize::try_from(cell_i32.wrapping_add(offset))
                    .expect("patch neighbour is negative");
                if output[index] != growing {
                    output[index] = growing;
                    fill_difference = fill_difference.wrapping_add(if growing { 1 } else { -1 });
                }
            }
        }
    }
    output
}

/// One smoothing pass of [`generate_patch`]'s cellular automaton over u64
/// bit rows. Per row, the three column sums (0..=3) are carried as two
/// bit-sliced planes, the 3-wide window total (0..=9) as four planes, and the
/// "at least half of the in-bounds 3x3 neighbourhood filled" rule becomes a
/// per-plane comparison against the row-count-dependent threshold. The two
/// edge columns see only a 2-wide window, so they are patched in scalar form.
/// Returns the signed change in filled cells, exactly like [`clustering_pass`].
fn clustering_pass_bits(rows: &[u64], next_rows: &mut [u64], width: usize, height: usize) -> i32 {
    let row_mask = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    // Bits 1..=width-2: the columns with a full 3-wide window.
    let interior_mask = row_mask & !1 & !(1 << (width - 1));
    let mut delta = 0_i32;
    for y in 0..height {
        let above = if y > 0 { rows[y - 1] } else { 0 };
        let mid = rows[y];
        let below = if y + 1 < height { rows[y + 1] } else { 0 };
        let row_count = 1 + u32::from(y > 0) + u32::from(y + 1 < height);

        // Column sums as two planes: sum = 2*column_high + column_low.
        let column_low = above ^ mid ^ below;
        let column_high = (above & mid) | (below & (above ^ mid));

        // Window total = left + centre + right column sums.
        let left_low = column_low << 1;
        let left_high = column_high << 1;
        let right_low = column_low >> 1;
        let right_high = column_high >> 1;
        let sum0 = left_low ^ column_low ^ right_low;
        let carry_low = (left_low & column_low) | (right_low & (left_low ^ column_low));
        let ones = left_high ^ column_high ^ right_high;
        let majority = (left_high & column_high) | (right_high & (left_high ^ column_high));
        let sum1 = ones ^ carry_low;
        let carry_mid = ones & carry_low;
        let sum2 = majority ^ carry_mid;
        let sum3 = majority & carry_mid;

        // 2*count >= row_count*3 for the 3-wide interior window.
        let meets_threshold = match row_count {
            3 => sum3 | (sum2 & (sum1 | sum0)), // count >= 5
            2 => sum3 | sum2 | (sum1 & sum0),   // count >= 3
            _ => sum3 | sum2 | sum1,            // count >= 2
        };
        let mut new_row = meets_threshold & interior_mask;

        // Edge columns have a 2-wide window: 2*count >= row_count*2.
        let edge_count = |x: usize| {
            ((above >> x) & 1) + ((mid >> x) & 1) + ((below >> x) & 1)
        };
        if edge_count(0) + edge_count(1) >= u64::from(row_count) {
            new_row |= 1;
        }
        if edge_count(width - 1) + edge_count(width - 2) >= u64::from(row_count) {
            new_row |= 1 << (width - 1);
        }

        #[allow(clippy::cast_possible_wrap)]
        {
            delta += (new_row & !mid).count_ones() as i32;
            delta -= (mid & !new_row).count_ones() as i32;
        }
        next_rows[y] = new_row;
    }
    delta
}

/// One smoothing pass of [`generate_patch`]'s cellular automaton, written with
/// sliding-window column sums so the per-cell 3x3 neighbourhood scan stays
/// branch-light and bounds-check free. A cell becomes filled when at least
/// half of its in-bounds 3x3 neighbourhood (self included) is filled, exactly
/// as the original per-cell scan computed. Returns the signed change in filled
/// cells so the caller can keep its running fill difference.
fn clustering_pass(
    output: &[bool],
    current: &mut [bool],
    width: usize,
    height: usize,
    column_counts: &mut [u8],
) -> i32 {
    let mut delta = 0_i32;
    for y in 0..height {
        let row_start = y * width;
        let mid = &output[row_start..row_start + width];
        let above = (y > 0).then(|| &output[row_start - width..row_start]);
        let below =
            (y + 1 < height).then(|| &output[row_start + width..row_start + 2 * width]);
        let rows = 1 + u8::from(above.is_some()) + u8::from(below.is_some());

        match (above, below) {
            (Some(above), Some(below)) => {
                for (((column, &m), &a), &b) in column_counts
                    .iter_mut()
                    .zip(mid)
                    .zip(above)
                    .zip(below)
                {
                    *column = u8::from(m) + u8::from(a) + u8::from(b);
                }
            }
            (Some(other), None) | (None, Some(other)) => {
                for ((column, &m), &o) in column_counts.iter_mut().zip(mid).zip(other) {
                    *column = u8::from(m) + u8::from(o);
                }
            }
            (None, None) => {
                for (column, &m) in column_counts.iter_mut().zip(mid) {
                    *column = u8::from(m);
                }
            }
        }

        let row_out = &mut current[row_start..row_start + width];
        if width == 1 {
            let new = column_counts[0] * 2 >= rows;
            delta += i32::from(new) - i32::from(mid[0]);
            row_out[0] = new;
            continue;
        }

        let edge_threshold = rows * 2;
        let first = column_counts[0] + column_counts[1];
        let new = first * 2 >= edge_threshold;
        delta += i32::from(new) - i32::from(mid[0]);
        row_out[0] = new;

        let interior_threshold = rows * 3;
        for ((out, &m), window) in row_out[1..width - 1]
            .iter_mut()
            .zip(&mid[1..width - 1])
            .zip(column_counts.windows(3))
        {
            let count = window[0] + window[1] + window[2];
            let new = count * 2 >= interior_threshold;
            delta += i32::from(new) - i32::from(m);
            *out = new;
        }

        let last = column_counts[width - 2] + column_counts[width - 1];
        let new = last * 2 >= edge_threshold;
        delta += i32::from(new) - i32::from(mid[width - 1]);
        row_out[width - 1] = new;
    }
    delta
}

/// Configurable generic part of v3.3.8 `RegularPainter`.
#[derive(Clone, Debug, PartialEq)]
pub struct RegularPainter {
    pub water_fill: f32,
    pub water_smoothness: i32,
    pub grass_fill: f32,
    pub grass_smoothness: i32,
    pub trap_count: i32,
    pub trap_classes: Vec<TrapSpec>,
    pub trap_chances: Vec<f32>,
    /// `TrapMechanism.revealHiddenTrapChance()`. Canonical no-trinket runs use
    /// zero, but keeping it explicit makes the draw-free behavior auditable.
    pub revealed_trap_chance: f32,
}

impl Default for RegularPainter {
    fn default() -> Self {
        Self {
            water_fill: 0.0,
            water_smoothness: 0,
            grass_fill: 0.0,
            grass_smoothness: 0,
            trap_count: 0,
            trap_classes: Vec::new(),
            trap_chances: Vec::new(),
            revealed_trap_chance: 0.0,
        }
    }
}

impl RegularPainter {
    #[must_use]
    pub fn set_water(mut self, fill: f32, smoothness: i32) -> Self {
        self.water_fill = fill;
        self.water_smoothness = smoothness;
        self
    }

    #[must_use]
    pub fn set_grass(mut self, fill: f32, smoothness: i32) -> Self {
        self.grass_fill = fill;
        self.grass_smoothness = smoothness;
        self
    }

    #[must_use]
    pub fn set_traps(mut self, count: i32, classes: Vec<TrapSpec>, chances: Vec<f32>) -> Self {
        assert_eq!(
            classes.len(),
            chances.len(),
            "trap class/chance lengths differ"
        );
        self.trap_count = count;
        self.trap_classes = classes;
        self.trap_chances = chances;
        self
    }

    #[must_use]
    pub fn set_revealed_trap_chance(mut self, chance: f32) -> Self {
        self.revealed_trap_chance = chance;
        self
    }

    /// Paint with an explicit region decoration callback. The callback runs
    /// inside the isolated child RNG after water, grass, and traps.
    ///
    /// # Errors
    ///
    /// Returns a structural [`PaintError`] when a special room is disconnected
    /// or a builder edge has no mutually valid door position.
    pub fn paint_with_decorator<D, F>(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        dispatch: &mut D,
        rng: &mut RandomStack,
        mut decorate: F,
    ) -> Result<(), PaintError>
    where
        D: RoomPaintDispatch,
        F: FnMut(&mut Level, &[Room], &[RoomId], &mut RandomStack),
    {
        normalize_rooms(level, rooms);

        let mut order: Vec<RoomId> = (0..rooms.len()).collect();
        rng.shuffle_list(&mut order);
        level.room_order.clone_from(&order);

        for &room in &order {
            if rooms[room].connected.is_empty() && matches!(rooms[room].kind, RoomKind::Special(_))
            {
                return Err(PaintError::SpecialRoomWithoutConnections { room });
            }
            place_doors_in_order(rooms, &[room], rng)
                .map_err(|(room, neighbour)| PaintError::NoDoorCandidate { room, neighbour })?;
            dispatch.paint_room(level, rooms, room, rng);
            synchronize_room_doors(rooms, room);
        }

        paint_doors(level, rooms, &order, dispatch, rng);

        let child_seed = rng.long();
        rng.push(child_seed);
        if self.water_fill > 0.0 {
            self.paint_water(level, rooms, &order, dispatch, rng);
        }
        if self.grass_fill > 0.0 {
            self.paint_grass(level, rooms, &order, dispatch, rng);
        }
        if self.trap_count > 0 {
            self.paint_traps(level, rooms, &order, dispatch, rng);
        }
        decorate(level, rooms, &order, rng);
        rng.pop();
        Ok(())
    }

    /// Generic painter with no region-specific decoration.
    ///
    /// # Errors
    ///
    /// Returns the same structural failures as [`Self::paint_with_decorator`].
    pub fn paint<D: RoomPaintDispatch>(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        dispatch: &mut D,
        rng: &mut RandomStack,
    ) -> Result<(), PaintError> {
        self.paint_with_decorator(level, rooms, dispatch, rng, |_, _, _, _| {})
    }

    fn paint_water<D: RoomPaintDispatch>(
        &self,
        level: &mut Level,
        rooms: &[Room],
        order: &[RoomId],
        dispatch: &D,
        rng: &mut RandomStack,
    ) {
        let lake = generate_patch(
            level.width(),
            level.height(),
            self.water_fill,
            self.water_smoothness,
            true,
            rng,
        );
        for &room in order {
            for point in rooms[room].bounds.points() {
                if dispatch.can_place_water(level, rooms, room, point) {
                    let cell = level.point_to_cell(point);
                    if lake[cell] && level.map.cells[cell] == terrain::EMPTY {
                        level.map.cells[cell] = terrain::WATER;
                    }
                }
            }
        }
    }

    fn paint_grass<D: RoomPaintDispatch>(
        &self,
        level: &mut Level,
        rooms: &[Room],
        order: &[RoomId],
        dispatch: &D,
        rng: &mut RandomStack,
    ) {
        let grass = generate_patch(
            level.width(),
            level.height(),
            self.grass_fill,
            self.grass_smoothness,
            true,
            rng,
        );
        let mut grass_cells = Vec::new();
        for &room in order {
            for point in rooms[room].bounds.points() {
                if dispatch.can_place_grass(level, rooms, room, point) {
                    let cell = level.point_to_cell(point);
                    if grass[cell] && level.map.cells[cell] == terrain::EMPTY {
                        grass_cells.push(cell);
                    }
                }
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
        for cell in grass_cells {
            if level.is_occupied_for_grass(cell) {
                level.map.cells[cell] = terrain::GRASS;
                continue;
            }
            let cell_i32 = i32::try_from(cell).expect("level map exceeds Java int indexing");
            let mut count = 1_i32;
            for offset in neighbours {
                let neighbour = usize::try_from(cell_i32.wrapping_add(offset))
                    .expect("grass point lies on an unpadded border");
                if grass[neighbour] {
                    count = count.wrapping_add(1);
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let high_grass_chance = count as f32 / 12.0_f32;
            level.map.cells[cell] = if rng.float() < high_grass_chance {
                terrain::HIGH_GRASS
            } else {
                terrain::GRASS
            };
        }
    }

    fn paint_traps<D: RoomPaintDispatch>(
        &mut self,
        level: &mut Level,
        rooms: &[Room],
        order: &[RoomId],
        dispatch: &D,
        rng: &mut RandomStack,
    ) {
        let mut valid_cells = Vec::new();
        for &room in order {
            for point in rooms[room].bounds.points() {
                if dispatch.can_place_trap(level, rooms, room, point) {
                    let cell = level.point_to_cell(point);
                    if level.map.cells[cell] == terrain::EMPTY {
                        valid_cells.push(cell);
                    }
                }
            }
        }
        let cap = i32::try_from(valid_cells.len() / 5).unwrap_or(i32::MAX);
        self.trap_count = self.trap_count.min(cap);

        for (passable, &tile) in level.passable.iter_mut().zip(&level.map.cells) {
            *passable = terrain::flags(tile) & terrain::PASSABLE != 0;
        }
        let width = level.width();
        let circle = [-width, 1, width, -1];
        let mut valid_non_hallways = Vec::new();
        for &cell in &valid_cells {
            let cell = i32::try_from(cell).expect("level map exceeds Java int indexing");
            let at = |offset: i32| {
                let index = usize::try_from(cell.wrapping_add(offset))
                    .expect("trap point lies on an unpadded border");
                level.passable[index]
            };
            if (at(circle[0]) || at(circle[2])) && (at(circle[1]) || at(circle[3])) {
                valid_non_hallways.push(usize::try_from(cell).unwrap());
            }
        }
        self.trap_count = self.trap_count.min(cap);

        let total = if level.feeling == Feeling::Traps {
            self.trap_count.wrapping_mul(5)
        } else {
            self.trap_count
        };
        let mut reveal_increment = 0.0_f32;
        let mut index = 0_i32;
        while index < total {
            let class = rng
                .chances(&self.trap_chances)
                .expect("positive trap weights are required");
            let spec = self.trap_classes[class];
            let cell = if spec.avoids_hallways && !valid_non_hallways.is_empty() {
                random_element(&valid_non_hallways, rng)
            } else {
                random_element(&valid_cells, rng)
            };
            remove_first(&mut valid_cells, cell);
            remove_first(&mut valid_non_hallways, cell);

            reveal_increment += self.revealed_trap_chance;
            let requested_visible = index >= self.trap_count || reveal_increment >= 1.0;
            let visible = if requested_visible {
                reveal_increment -= 1.0;
                true
            } else {
                !spec.can_be_hidden
            };
            level.set_trap(PlacedTrap {
                spec,
                cell,
                visible,
                active: true,
            });
            level.map.cells[cell] = if visible {
                terrain::TRAP
            } else {
                terrain::SECRET_TRAP
            };
            index = index.wrapping_add(1);
        }
    }
}

fn random_element(values: &[usize], rng: &mut RandomStack) -> usize {
    let bound = i32::try_from(values.len()).expect("Java collection size exceeds int");
    values[usize::try_from(rng.int_bound(bound)).expect("Random.Int is non-negative")]
}

fn remove_first(values: &mut Vec<usize>, value: usize) {
    if let Some(index) = values.iter().position(|&candidate| candidate == value) {
        values.remove(index);
    }
}

fn build_room_distance_map(rooms: &mut [Room], focus: RoomId) {
    for room in rooms.iter_mut() {
        room.distance = i32::MAX;
    }
    rooms[focus].distance = 0;
    let mut queue = VecDeque::new();
    queue.push_back(focus);
    while let Some(room) = queue.pop_front() {
        let distance = rooms[room].distance;
        let price = rooms[room].price;
        // Indexed iteration ends each `connected` borrow before the edge
        // relaxation mutates `rooms`, so no per-node edge list is collected.
        for connection_index in 0..rooms[room].connected.len() {
            let connection = &rooms[room].connected[connection_index];
            let Some(door) = connection.door else {
                continue;
            };
            if !matches!(
                door.door_type,
                DoorType::Empty | DoorType::Tunnel | DoorType::Unlocked | DoorType::Regular
            ) {
                continue;
            }
            let edge = connection.room;
            let candidate = distance.wrapping_add(price);
            if rooms[edge].distance > candidate {
                rooms[edge].distance = candidate;
                queue.push_back(edge);
            }
        }
    }
}

fn non_connection_rooms_in_graph(rooms: &[Room]) -> i32 {
    rooms.iter().fold(0_i32, |count, room| {
        if room.distance != i32::MAX && !matches!(room.kind, RoomKind::Connection(_)) {
            count.wrapping_add(1)
        } else {
            count
        }
    })
}

/// `RegularPainter.mergeRooms`, including the otherwise surprising random
/// draws made by `Rect.center()` when `start` is absent.
#[allow(clippy::too_many_arguments)] // Mirrors the complete Java call context.
pub fn merge_rooms<D: RoomPaintDispatch>(
    level: &mut Level,
    rooms: &[Room],
    room: RoomId,
    neighbour: RoomId,
    start: Option<Point>,
    merge_terrain: i32,
    dispatch: &mut D,
    rng: &mut RandomStack,
) -> bool {
    let intersection = rooms[room].bounds.intersect(rooms[neighbour].bounds);
    if intersection.left == intersection.right {
        let center =
            start.unwrap_or_else(|| intersection.center_with(|bound| rng.int_bound(bound)));
        let mut merge = Rect::new(intersection.left, center.y, intersection.left, center.y);
        let mut point = Point::new(merge.left, merge.top);
        while merge.top > intersection.top
            && dispatch.can_merge(level, rooms, neighbour, room, point, merge_terrain)
            && dispatch.can_merge(level, rooms, room, neighbour, point, merge_terrain)
        {
            merge.top = merge.top.wrapping_sub(1);
            point.y = point.y.wrapping_sub(1);
        }
        point.y = merge.bottom;
        while merge.bottom < intersection.bottom
            && dispatch.can_merge(level, rooms, neighbour, room, point, merge_terrain)
            && dispatch.can_merge(level, rooms, room, neighbour, point, merge_terrain)
        {
            merge.bottom = merge.bottom.wrapping_add(1);
            point.y = point.y.wrapping_add(1);
        }
        if merge.height() >= 3 {
            let opening = Rect::new(
                merge.left,
                merge.top.wrapping_add(1),
                merge.left.wrapping_add(1),
                merge.bottom,
            );
            dispatch.merge(level, rooms, room, neighbour, opening, merge_terrain);
            true
        } else {
            false
        }
    } else if intersection.top == intersection.bottom {
        let center =
            start.unwrap_or_else(|| intersection.center_with(|bound| rng.int_bound(bound)));
        let mut merge = Rect::new(center.x, intersection.top, center.x, intersection.top);
        let mut point = Point::new(merge.left, merge.top);
        while merge.left > intersection.left
            && dispatch.can_merge(level, rooms, neighbour, room, point, merge_terrain)
            && dispatch.can_merge(level, rooms, room, neighbour, point, merge_terrain)
        {
            merge.left = merge.left.wrapping_sub(1);
            point.x = point.x.wrapping_sub(1);
        }
        point.x = merge.right;
        while merge.right < intersection.right
            && dispatch.can_merge(level, rooms, neighbour, room, point, merge_terrain)
            && dispatch.can_merge(level, rooms, room, neighbour, point, merge_terrain)
        {
            merge.right = merge.right.wrapping_add(1);
            point.x = point.x.wrapping_add(1);
        }
        if merge.width() >= 3 {
            let opening = Rect::new(
                merge.left.wrapping_add(1),
                merge.top,
                merge.right,
                merge.top.wrapping_add(1),
            );
            dispatch.merge(level, rooms, room, neighbour, opening, merge_terrain);
            true
        } else {
            false
        }
    } else {
        false
    }
}

#[allow(clippy::manual_midpoint)] // Preserve Java's written f32 operation order.
fn paint_doors<D: RoomPaintDispatch>(
    level: &mut Level,
    rooms: &mut [Room],
    order: &[RoomId],
    dispatch: &mut D,
    rng: &mut RandomStack,
) {
    let mut hidden_door_chance = 0.0_f32;
    if level.depth > 1 {
        #[allow(clippy::cast_precision_loss)]
        let depth = level.depth as f32;
        hidden_door_chance = (depth / 20.0_f32).min(1.0);
    }
    if level.feeling == Feeling::Secrets {
        hidden_door_chance = (0.5_f32 + hidden_door_chance) / 2.0_f32;
    }

    let mut room_merges = vec![None; rooms.len()];
    for &room in order {
        let neighbours: Vec<RoomId> = rooms[room]
            .connected
            .iter()
            .map(|connection| connection.room)
            .collect();
        for neighbour in neighbours {
            if room_merges[room] == Some(neighbour) || room_merges[neighbour] == Some(room) {
                continue;
            }
            if room_merges[room].is_none() && room_merges[neighbour].is_none() {
                let start = shared_door(rooms, room, neighbour).point;
                if merge_rooms(
                    level,
                    rooms,
                    room,
                    neighbour,
                    Some(start),
                    terrain::EMPTY,
                    dispatch,
                    rng,
                ) {
                    if rooms[room].size_category == Some(SizeCategory::Normal) {
                        room_merges[room] = Some(neighbour);
                    }
                    if rooms[neighbour].size_category == Some(SizeCategory::Normal) {
                        room_merges[neighbour] = Some(room);
                    }
                    continue;
                }
            }

            if shared_door(rooms, room, neighbour).door_type == DoorType::Regular {
                if rng.float() < hidden_door_chance {
                    force_shared_door_type(rooms, room, neighbour, DoorType::Hidden);
                    if level.feeling == Feeling::Secrets {
                        build_room_distance_map(rooms, room);
                        if non_connection_rooms_in_graph(rooms) < 2 {
                            force_shared_door_type(rooms, room, neighbour, DoorType::Unlocked);
                        } else {
                            build_room_distance_map(rooms, neighbour);
                            if non_connection_rooms_in_graph(rooms) < 2 {
                                force_shared_door_type(rooms, room, neighbour, DoorType::Unlocked);
                            }
                        }
                    } else {
                        build_room_distance_map(rooms, room);
                        if rooms[neighbour].distance == i32::MAX {
                            force_shared_door_type(rooms, room, neighbour, DoorType::Unlocked);
                        }
                    }
                    build_room_distance_map(rooms, room);
                    if level.feeling != Feeling::Secrets && rooms[neighbour].distance == i32::MAX {
                        force_shared_door_type(rooms, room, neighbour, DoorType::Unlocked);
                    }
                } else {
                    force_shared_door_type(rooms, room, neighbour, DoorType::Unlocked);
                }
            }

            if shared_door(rooms, room, neighbour).door_type == DoorType::Unlocked
                && (rooms[room].is_entrance() || rooms[neighbour].is_entrance())
                && ((level.depth == 1 && level.intro)
                    || (level.depth == 2 && !level.guide_searching_found))
            {
                force_shared_door_type(rooms, room, neighbour, DoorType::Hidden);
            }

            let door = shared_door(rooms, room, neighbour);
            let cell = level.point_to_cell(door.point);
            let tile = match door.door_type {
                DoorType::Empty => Some(terrain::EMPTY),
                DoorType::Tunnel => Some(level.tunnel_tile()),
                DoorType::Water => Some(terrain::WATER),
                DoorType::Regular => None,
                DoorType::Unlocked => Some(terrain::DOOR),
                DoorType::Hidden => Some(terrain::SECRET_DOOR),
                DoorType::Barricade => Some(terrain::BARRICADE),
                DoorType::Locked => Some(terrain::LOCKED_DOOR),
                DoorType::Crystal => Some(terrain::CRYSTAL_DOOR),
                DoorType::Wall => Some(terrain::WALL),
            };
            if let Some(tile) = tile {
                level.map.cells[cell] = tile;
            }
        }
    }
}

/// Sewer-region wrapper which supplies v3.3.8 water/grass parameters, trap
/// class tables, and exact `SewerPainter.decorate` behavior.
#[derive(Clone, Debug, PartialEq)]
pub struct SewerPainter {
    pub regular: RegularPainter,
}

impl SewerPainter {
    #[must_use]
    pub fn new(depth: u32, feeling: Feeling, trap_count: i32) -> Self {
        let (classes, chances) = sewer_trap_table(depth);
        Self {
            regular: RegularPainter::default()
                .set_water(
                    if feeling == Feeling::Water {
                        0.85
                    } else {
                        0.30
                    },
                    5,
                )
                .set_grass(
                    if feeling == Feeling::Grass {
                        0.80
                    } else {
                        0.20
                    },
                    4,
                )
                .set_traps(trap_count, classes, chances),
        }
    }

    /// Paint a Sewer map using the supplied exact room dispatcher.
    ///
    /// # Errors
    ///
    /// Returns a structural [`PaintError`] when a special room is disconnected
    /// or a builder edge has no mutually valid door position.
    pub fn paint<D: RoomPaintDispatch>(
        &mut self,
        level: &mut Level,
        rooms: &mut [Room],
        dispatch: &mut D,
        rng: &mut RandomStack,
    ) -> Result<(), PaintError> {
        self.regular
            .paint_with_decorator(level, rooms, dispatch, rng, decorate_sewers)
    }
}

/// `RegularLevel.nTraps`. Call this at the same point Java evaluates the
/// `setTraps(nTraps(), ...)` argument; moving it into `SewerPainter::new` would
/// silently change constructor draw order.
pub fn draw_regular_trap_count(depth: u32, rng: &mut RandomStack) -> i32 {
    let depth_band = i32::try_from(depth / 5).unwrap_or(i32::MAX);
    rng.normal_int_range(2, 3_i32.wrapping_add(depth_band))
}

/// Exact v3.3.8 `SewerPainter.decorate` map mutation and conditional draws.
pub fn decorate_sewers(
    level: &mut Level,
    _rooms: &[Room],
    _order: &[RoomId],
    rng: &mut RandomStack,
) {
    let width = level.width();
    let width_usize = usize::try_from(width).expect("level width is negative");
    let length = level.len();

    for cell in 0..width_usize {
        if level.map.cells[cell] == terrain::WALL
            && level.map.cells[cell + width_usize] == terrain::WATER
            && rng.int_bound(4) == 0
        {
            level.map.cells[cell] = terrain::WALL_DECO;
        }
    }
    for cell in width_usize..length - width_usize {
        if level.map.cells[cell] == terrain::WALL
            && level.map.cells[cell - width_usize] == terrain::WALL
            && level.map.cells[cell + width_usize] == terrain::WATER
            && rng.int_bound(2) == 0
        {
            level.map.cells[cell] = terrain::WALL_DECO;
        }
    }
    for cell in width_usize + 1..length - width_usize - 1 {
        if level.map.cells[cell] == terrain::EMPTY {
            let count = i32::from(level.map.cells[cell + 1] == terrain::WALL)
                + i32::from(level.map.cells[cell - 1] == terrain::WALL)
                + i32::from(level.map.cells[cell + width_usize] == terrain::WALL)
                + i32::from(level.map.cells[cell - width_usize] == terrain::WALL);
            if rng.int_bound(16) < count.wrapping_mul(count) {
                level.map.cells[cell] = terrain::EMPTY_DECO;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room::{ConnectionRoomKind, connect_rooms};

    #[derive(Default)]
    struct BoxRoomDispatch;

    impl RoomPaintDispatch for BoxRoomDispatch {
        fn paint_room(
            &mut self,
            level: &mut Level,
            rooms: &mut [Room],
            room: RoomId,
            _rng: &mut RandomStack,
        ) {
            fill_room(level, &rooms[room], terrain::WALL);
            draw::fill_rect_margin(&mut level.map, rooms[room].bounds, 1, terrain::EMPTY);
            let neighbours: Vec<RoomId> = rooms[room]
                .connected
                .iter()
                .map(|connection| connection.room)
                .collect();
            for neighbour in neighbours {
                set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
            }
        }
    }

    fn two_connected_rooms(rng: &mut RandomStack) -> Vec<Room> {
        let mut rooms = vec![
            Room::connection(ConnectionRoomKind::Tunnel),
            Room::connection(ConnectionRoomKind::Tunnel),
        ];
        assert!(rooms[0].force_size(5, 5, rng));
        assert!(rooms[1].force_size(5, 5, rng));
        rooms[0].set_position(0, 0);
        rooms[1].set_position(4, 0);
        assert!(connect_rooms(&mut rooms, 0, 1, rng));
        rooms
    }

    #[test]
    fn patch_matches_java_reference_vector() {
        // Captured from v3.3.8 Patch.generate after
        // Random.pushGenerator(0x0123456789abcdefL).
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        let patch = generate_patch(9, 7, 0.30, 5, true, &mut rng);
        let bits: String = patch
            .iter()
            .map(|&value| if value { '1' } else { '0' })
            .collect();
        assert_eq!(
            bits,
            "111000011110000001100000100000000111000000111000000111000000000"
        );
        assert_eq!(rng.int(), 1_569_929_500);
    }

    #[test]
    fn patch_halfway_fill_rounding_matches_java_reference() {
        // 25 * 0.10f lands on the case where `-Math.round(x)` must not be
        // rewritten as `Math.round(-x)`. Captured from the same v3.3.8 class.
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        let patch = generate_patch(5, 5, 0.10, 1, true, &mut rng);
        let bits: String = patch
            .iter()
            .map(|&value| if value { '1' } else { '0' })
            .collect();
        assert_eq!(bits, "0000000000000000000001101");
        assert_eq!(rng.int(), -196_853_640);
    }

    #[test]
    fn room_normalization_order_and_regular_door_are_preserved() {
        let mut rng = RandomStack::with_base_seed(0x55aa);
        let mut rooms = two_connected_rooms(&mut rng);
        let mut level = Level::new(1, Feeling::None);
        let mut painter = RegularPainter::default();
        painter
            .paint(&mut level, &mut rooms, &mut BoxRoomDispatch, &mut rng)
            .unwrap();

        assert_eq!((level.width(), level.height()), (11, 7));
        assert_eq!(level.room_order.len(), 2);
        assert!(level.room_order.contains(&0) && level.room_order.contains(&1));
        let door = shared_door(&rooms, 0, 1);
        assert_eq!(door.door_type, DoorType::Unlocked);
        assert_eq!(
            level.map.cells[level.point_to_cell(door.point)],
            terrain::DOOR
        );
    }

    #[test]
    fn isolated_paint_variance_does_not_advance_depth_stream() {
        let mut setup_rng = RandomStack::with_base_seed(77);
        let rooms = two_connected_rooms(&mut setup_rng);
        let mut dry_rooms = rooms.clone();
        let mut wet_rooms = rooms;
        let mut dry_rng = setup_rng.clone();
        let mut wet_rng = setup_rng;
        let mut dry_level = Level::new(2, Feeling::None);
        let mut wet_level = Level::new(2, Feeling::None);
        let mut dry = RegularPainter::default();
        let mut wet = RegularPainter::default().set_water(0.30, 5);

        dry.paint(
            &mut dry_level,
            &mut dry_rooms,
            &mut BoxRoomDispatch,
            &mut dry_rng,
        )
        .unwrap();
        wet.paint(
            &mut wet_level,
            &mut wet_rooms,
            &mut BoxRoomDispatch,
            &mut wet_rng,
        )
        .unwrap();
        assert_eq!(dry_rng.int(), wet_rng.int());
    }

    #[test]
    fn merge_scan_excludes_the_two_endpoint_walls() {
        struct MergeDispatch;
        impl RoomPaintDispatch for MergeDispatch {
            fn paint_room(
                &mut self,
                _level: &mut Level,
                _rooms: &mut [Room],
                _room: RoomId,
                _rng: &mut RandomStack,
            ) {
            }

            fn can_merge(
                &self,
                _level: &Level,
                _rooms: &[Room],
                _room: RoomId,
                _other: RoomId,
                _point: Point,
                _merge_terrain: i32,
            ) -> bool {
                true
            }
        }

        let mut rng = RandomStack::with_base_seed(9);
        let rooms = two_connected_rooms(&mut rng);
        let mut level = Level::new(1, Feeling::None);
        level.set_size(9, 5);
        assert!(merge_rooms(
            &mut level,
            &rooms,
            0,
            1,
            Some(Point::new(4, 2)),
            terrain::EMPTY,
            &mut MergeDispatch,
            &mut rng,
        ));
        assert_eq!(level.map.cells[level.map.cell(4, 0)], terrain::WALL);
        assert_eq!(level.map.cells[level.map.cell(4, 1)], terrain::EMPTY);
        assert_eq!(level.map.cells[level.map.cell(4, 3)], terrain::EMPTY);
        assert_eq!(level.map.cells[level.map.cell(4, 4)], terrain::WALL);
    }
}
