//! Java-compatible spatial primitives used by regular level generation.
//!
//! This module ports the data-oriented parts of Shattered Pixel Dungeon
//! v3.3.8's `Point`, `Rect`, `BArray`, `PathFinder`, `ShadowCaster`, `Terrain`,
//! and the static drawing helpers in `Painter`.  Source files are under:
//!
//! - `SPD-classes/src/main/java/com/watabou/utils/`
//! - `core/src/main/java/com/shatteredpixel/shatteredpixeldungeon/levels/`
//! - `core/src/main/java/com/shatteredpixel/shatteredpixeldungeon/mechanics/`
//!
//! The upstream revision is `7b8b845a76fe76c6b7c031ae9e570852411f56db`
//! (tag `v3.3.8`).  Offset ordering and float operation ordering are part of
//! seed parity and should not be "cleaned up" without an upstream oracle.

// These APIs mirror Java helpers whose contract is to throw when map sizes,
// indices, or paired buffers violate engine invariants. Repeating that same
// panic contract on every low-level drawing/pathfinding method obscures the
// parity notes, so it is documented once for this compatibility module.
#![allow(clippy::missing_panics_doc)]

use std::ops::{Index, IndexMut};

use crate::java_math::{div_i32, f32_to_i32, f64_to_i32, rem_i32, round_f32, round_f64};

/// Port of `com.watabou.utils.Point`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn set(&mut self, x: i32, y: i32) -> &mut Self {
        self.x = x;
        self.y = y;
        self
    }

    pub fn set_point(&mut self, point: Self) -> &mut Self {
        self.set(point.x, point.y)
    }

    /// Mirrors `Point.scale(float)`, including Java's implicit compound cast.
    pub fn scale(&mut self, factor: f32) -> &mut Self {
        #[allow(clippy::cast_precision_loss)]
        let x = self.x as f32;
        #[allow(clippy::cast_precision_loss)]
        let y = self.y as f32;
        self.x = f32_to_i32(x * factor);
        self.y = f32_to_i32(y * factor);
        self
    }

    pub fn offset(&mut self, dx: i32, dy: i32) -> &mut Self {
        self.x = self.x.wrapping_add(dx);
        self.y = self.y.wrapping_add(dy);
        self
    }

    pub fn offset_point(&mut self, delta: Self) -> &mut Self {
        self.offset(delta.x, delta.y)
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.x == 0 && self.y == 0
    }

    /// Mirrors the upstream expression `(float)Math.sqrt(x*x + y*y)`.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // Deliberate Java double-to-float cast.
    pub fn length(self) -> f32 {
        let square = self
            .x
            .wrapping_mul(self.x)
            .wrapping_add(self.y.wrapping_mul(self.y));
        f64::from(square).sqrt() as f32
    }

    /// Mirrors `Point.distance`, including `int` subtraction before the f32s.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // Deliberate Java double-to-float cast.
    pub fn distance(a: Self, b: Self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let dx = a.x.wrapping_sub(b.x) as f32;
        #[allow(clippy::cast_precision_loss)]
        let dy = a.y.wrapping_sub(b.y) as f32;
        let square = dx * dx + dy * dy;
        f64::from(square).sqrt() as f32
    }
}

/// Port of `com.watabou.utils.Rect`.
///
/// `right` and `bottom` are exclusive for `inside`, width, height, and Painter
/// fills.  Upstream room boundaries nevertheless use those coordinates as
/// actual wall cells, and `points()` intentionally iterates them inclusively.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    #[must_use]
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    #[must_use]
    pub const fn width(self) -> i32 {
        self.right.wrapping_sub(self.left)
    }

    #[must_use]
    pub const fn height(self) -> i32 {
        self.bottom.wrapping_sub(self.top)
    }

    #[must_use]
    pub const fn square(self) -> i32 {
        self.width().wrapping_mul(self.height())
    }

    pub fn set(&mut self, left: i32, top: i32, right: i32, bottom: i32) -> &mut Self {
        self.left = left;
        self.top = top;
        self.right = right;
        self.bottom = bottom;
        self
    }

    pub fn set_rect(&mut self, rect: Self) -> &mut Self {
        self.set(rect.left, rect.top, rect.right, rect.bottom)
    }

    pub fn set_pos(&mut self, x: i32, y: i32) -> &mut Self {
        let width = self.width();
        let height = self.height();
        self.set(x, y, x.wrapping_add(width), y.wrapping_add(height))
    }

    pub fn shift(&mut self, x: i32, y: i32) -> &mut Self {
        self.set(
            self.left.wrapping_add(x),
            self.top.wrapping_add(y),
            self.right.wrapping_add(x),
            self.bottom.wrapping_add(y),
        )
    }

    pub fn resize(&mut self, width: i32, height: i32) -> &mut Self {
        self.right = self.left.wrapping_add(width);
        self.bottom = self.top.wrapping_add(height);
        self
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    pub fn set_empty(&mut self) -> &mut Self {
        self.set(0, 0, 0, 0)
    }

    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        Self::new(
            self.left.max(other.left),
            self.top.max(other.top),
            self.right.min(other.right),
            self.bottom.min(other.bottom),
        )
    }

    #[must_use]
    pub fn union_rect(self, other: Self) -> Self {
        Self::new(
            self.left.min(other.left),
            self.top.min(other.top),
            self.right.max(other.right),
            self.bottom.max(other.bottom),
        )
    }

    pub fn union(&mut self, x: i32, y: i32) -> &mut Self {
        if self.is_empty() {
            return self.set(x, y, x.wrapping_add(1), y.wrapping_add(1));
        }

        if x < self.left {
            self.left = x;
        } else if x >= self.right {
            self.right = x.wrapping_add(1);
        }
        if y < self.top {
            self.top = y;
        } else if y >= self.bottom {
            self.bottom = y.wrapping_add(1);
        }
        self
    }

    pub fn union_point(&mut self, point: Point) -> &mut Self {
        self.union(point.x, point.y)
    }

    #[must_use]
    pub const fn inside(self, point: Point) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }

    /// Upstream `Rect.center()`, with random draws supplied by the caller.
    ///
    /// The callback is invoked with bound 2 once per even dimension, in x then
    /// y order.  Supplying the callback avoids coupling geometry to one RNG.
    pub fn center_with<F>(self, mut random_int: F) -> Point
    where
        F: FnMut(i32) -> i32,
    {
        let x_jitter = if rem_i32(self.width(), 2) == 0 {
            random_int(2)
        } else {
            0
        };
        let y_jitter = if rem_i32(self.height(), 2) == 0 {
            random_int(2)
        } else {
            0
        };
        Point::new(
            div_i32(self.left.wrapping_add(self.right), 2).wrapping_add(x_jitter),
            div_i32(self.top.wrapping_add(self.bottom), 2).wrapping_add(y_jitter),
        )
    }

    #[must_use]
    pub const fn shrink(self, distance: i32) -> Self {
        Self::new(
            self.left.wrapping_add(distance),
            self.top.wrapping_add(distance),
            self.right.wrapping_sub(distance),
            self.bottom.wrapping_sub(distance),
        )
    }

    #[must_use]
    pub const fn scale(self, factor: i32) -> Self {
        Self::new(
            self.left.wrapping_mul(factor),
            self.top.wrapping_mul(factor),
            self.right.wrapping_mul(factor),
            self.bottom.wrapping_mul(factor),
        )
    }

    /// Mirrors `Rect.getPoints()`: x is the outer loop and both ends are
    /// inclusive, despite the class's usual exclusive-right convention.
    #[must_use]
    pub fn points(self) -> Vec<Point> {
        let mut points = Vec::new();
        let mut x = self.left;
        while x <= self.right {
            let mut y = self.top;
            while y <= self.bottom {
                points.push(Point::new(x, y));
                if y == self.bottom {
                    break;
                }
                y = y.wrapping_add(1);
            }
            if x == self.right {
                break;
            }
            x = x.wrapping_add(1);
        }
        points
    }
}

/// A reusable row-major map buffer (`cell = x + y * width`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GridMap {
    pub width: i32,
    pub height: i32,
    pub cells: Vec<i32>,
}

impl GridMap {
    #[must_use]
    pub fn new(width: i32, height: i32, value: i32) -> Self {
        assert!(width >= 0 && height >= 0, "negative map dimensions");
        let size = usize::try_from(width)
            .expect("non-negative width")
            .checked_mul(usize::try_from(height).expect("non-negative height"))
            .expect("map dimensions overflow usize");
        Self {
            width,
            height,
            cells: vec![value; size],
        }
    }

    #[must_use]
    pub fn from_cells(width: i32, height: i32, cells: Vec<i32>) -> Self {
        assert!(width >= 0 && height >= 0, "negative map dimensions");
        let expected_size = usize::try_from(width)
            .expect("non-negative width")
            .checked_mul(usize::try_from(height).expect("non-negative height"))
            .expect("map dimensions overflow usize");
        assert_eq!(cells.len(), expected_size, "map buffer has wrong length");
        Self {
            width,
            height,
            cells,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    #[must_use]
    pub const fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.width && y >= 0 && y < self.height
    }

    #[must_use]
    pub fn cell(&self, x: i32, y: i32) -> usize {
        // `Level.pointToCell` and `Painter.set(x,y)` do not validate x/y
        // independently; Java only fails if the resulting flat array index is
        // outside the map. Preserve that occasionally useful row-wrap rule.
        let cell = x.wrapping_add(y.wrapping_mul(self.width));
        let cell = usize::try_from(cell).expect("map index is negative");
        assert!(
            cell < self.cells.len(),
            "map index out of bounds: ({x}, {y})"
        );
        cell
    }

    #[must_use]
    pub fn point_to_cell(&self, point: Point) -> usize {
        self.cell(point.x, point.y)
    }

    #[must_use]
    pub fn cell_to_point(&self, cell: usize) -> Point {
        assert!(cell < self.cells.len(), "map cell out of bounds");
        let cell = i32::try_from(cell).expect("Java map index exceeds i32");
        Point::new(rem_i32(cell, self.width), div_i32(cell, self.width))
    }

    pub fn set(&mut self, x: i32, y: i32, value: i32) {
        let cell = self.cell(x, y);
        self.cells[cell] = value;
    }

    pub fn set_point(&mut self, point: Point, value: i32) {
        self.set(point.x, point.y, value);
    }

    pub fn clear(&mut self, value: i32) {
        self.cells.fill(value);
    }
}

impl Index<usize> for GridMap {
    type Output = i32;

    fn index(&self, index: usize) -> &Self::Output {
        &self.cells[index]
    }
}

impl IndexMut<usize> for GridMap {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.cells[index]
    }
}

/// Result-equivalent ports of `com.watabou.utils.BArray`.
pub mod barray {
    pub fn set_false(values: &mut [bool]) {
        values.fill(false);
    }

    #[must_use]
    pub fn and(a: &[bool], b: &[bool]) -> Vec<bool> {
        assert_eq!(a.len(), b.len(), "BArray operands differ in length");
        a.iter()
            .zip(b)
            .map(|(&left, &right)| left && right)
            .collect()
    }

    pub fn and_into(a: &[bool], b: &[bool], result: &mut [bool]) {
        assert_eq!(a.len(), b.len(), "BArray operands differ in length");
        assert_eq!(a.len(), result.len(), "BArray result differs in length");
        for ((out, &left), &right) in result.iter_mut().zip(a).zip(b) {
            *out = left && right;
        }
    }

    #[must_use]
    pub fn or(a: &[bool], b: &[bool]) -> Vec<bool> {
        assert_eq!(a.len(), b.len(), "BArray operands differ in length");
        a.iter()
            .zip(b)
            .map(|(&left, &right)| left || right)
            .collect()
    }

    pub fn or_into(a: &[bool], b: &[bool], result: &mut [bool]) {
        or_range_into(a, b, 0, a.len(), result);
    }

    /// Mirrors the offset/length overload.  Cells outside the range retain
    /// their old result values, as they do when Java receives a result array.
    pub fn or_range_into(
        a: &[bool],
        b: &[bool],
        offset: usize,
        length: usize,
        result: &mut [bool],
    ) {
        assert_eq!(a.len(), b.len(), "BArray operands differ in length");
        let end = offset.checked_add(length).expect("BArray range overflow");
        assert!(
            end <= a.len() && end <= result.len(),
            "BArray range out of bounds"
        );
        for index in offset..end {
            result[index] = a[index] || b[index];
        }
    }

    #[must_use]
    pub fn not(a: &[bool]) -> Vec<bool> {
        a.iter().map(|value| !value).collect()
    }

    pub fn not_into(a: &[bool], result: &mut [bool]) {
        assert_eq!(a.len(), result.len(), "BArray result differs in length");
        for (out, &value) in result.iter_mut().zip(a) {
            *out = !value;
        }
    }

    #[must_use]
    pub fn is(a: &[i32], value: i32) -> Vec<bool> {
        a.iter().map(|&cell| cell == value).collect()
    }

    #[must_use]
    pub fn is_one_of(a: &[i32], values: &[i32]) -> Vec<bool> {
        a.iter().map(|cell| values.contains(cell)).collect()
    }

    #[must_use]
    pub fn is_not(a: &[i32], value: i32) -> Vec<bool> {
        a.iter().map(|&cell| cell != value).collect()
    }

    #[must_use]
    pub fn is_not_one_of(a: &[i32], values: &[i32]) -> Vec<bool> {
        a.iter().map(|cell| !values.contains(cell)).collect()
    }
}

/// Terrain IDs and bit flags from v3.3.8 `Terrain.java`.
pub mod terrain {
    pub const CHASM: i32 = 0;
    pub const EMPTY: i32 = 1;
    pub const GRASS: i32 = 2;
    pub const EMPTY_WELL: i32 = 3;
    pub const WALL: i32 = 4;
    pub const DOOR: i32 = 5;
    pub const OPEN_DOOR: i32 = 6;
    pub const ENTRANCE: i32 = 7;
    pub const EXIT: i32 = 8;
    pub const EMBERS: i32 = 9;
    pub const LOCKED_DOOR: i32 = 10;
    pub const PEDESTAL: i32 = 11;
    pub const WALL_DECO: i32 = 12;
    pub const BARRICADE: i32 = 13;
    pub const EMPTY_SP: i32 = 14;
    pub const HIGH_GRASS: i32 = 15;
    pub const SECRET_DOOR: i32 = 16;
    pub const SECRET_TRAP: i32 = 17;
    pub const TRAP: i32 = 18;
    pub const INACTIVE_TRAP: i32 = 19;
    pub const EMPTY_DECO: i32 = 20;
    pub const LOCKED_EXIT: i32 = 21;
    pub const UNLOCKED_EXIT: i32 = 22;
    pub const CUSTOM_DECO: i32 = 23;
    pub const WELL: i32 = 24;
    pub const STATUE: i32 = 25;
    pub const STATUE_SP: i32 = 26;
    pub const BOOKSHELF: i32 = 27;
    pub const ALCHEMY: i32 = 28;
    pub const WATER: i32 = 29;
    pub const FURROWED_GRASS: i32 = 30;
    pub const CRYSTAL_DOOR: i32 = 31;
    pub const CUSTOM_DECO_EMPTY: i32 = 32;
    pub const REGION_DECO: i32 = 33;
    pub const REGION_DECO_ALT: i32 = 34;
    pub const MINE_CRYSTAL: i32 = 35;
    pub const MINE_BOULDER: i32 = 36;
    pub const ENTRANCE_SP: i32 = 37;
    pub const HERO_LKD_DR: i32 = 38;

    pub const PASSABLE: i32 = 0x01;
    pub const LOS_BLOCKING: i32 = 0x02;
    pub const FLAMABLE: i32 = 0x04;
    pub const SECRET: i32 = 0x08;
    pub const SOLID: i32 = 0x10;
    pub const AVOID: i32 = 0x20;
    pub const LIQUID: i32 = 0x40;
    pub const PIT: i32 = 0x80;

    pub const FLAGS: [i32; 256] = {
        let mut flags = [0; 256];
        flags[CHASM as usize] = AVOID | PIT;
        flags[EMPTY as usize] = PASSABLE;
        flags[GRASS as usize] = PASSABLE | FLAMABLE;
        flags[EMPTY_WELL as usize] = PASSABLE;
        flags[WATER as usize] = PASSABLE | LIQUID;
        flags[WALL as usize] = LOS_BLOCKING | SOLID;
        flags[DOOR as usize] = PASSABLE | LOS_BLOCKING | FLAMABLE | SOLID;
        flags[OPEN_DOOR as usize] = PASSABLE | FLAMABLE;
        flags[ENTRANCE as usize] = PASSABLE;
        flags[ENTRANCE_SP as usize] = flags[ENTRANCE as usize];
        flags[EXIT as usize] = PASSABLE;
        flags[EMBERS as usize] = PASSABLE;
        flags[LOCKED_DOOR as usize] = LOS_BLOCKING | SOLID;
        flags[HERO_LKD_DR as usize] = flags[LOCKED_DOOR as usize];
        flags[CRYSTAL_DOOR as usize] = SOLID;
        flags[PEDESTAL as usize] = PASSABLE;
        flags[WALL_DECO as usize] = flags[WALL as usize];
        flags[BARRICADE as usize] = FLAMABLE | SOLID | LOS_BLOCKING;
        flags[EMPTY_SP as usize] = flags[EMPTY as usize];
        flags[HIGH_GRASS as usize] = PASSABLE | LOS_BLOCKING | FLAMABLE;
        flags[FURROWED_GRASS as usize] = flags[HIGH_GRASS as usize];
        flags[SECRET_DOOR as usize] = flags[WALL as usize] | SECRET;
        flags[SECRET_TRAP as usize] = flags[EMPTY as usize] | SECRET;
        flags[TRAP as usize] = AVOID;
        flags[INACTIVE_TRAP as usize] = flags[EMPTY as usize];
        flags[EMPTY_DECO as usize] = flags[EMPTY as usize];
        flags[LOCKED_EXIT as usize] = SOLID;
        flags[UNLOCKED_EXIT as usize] = PASSABLE;
        flags[WELL as usize] = AVOID;
        flags[BOOKSHELF as usize] = flags[BARRICADE as usize];
        flags[ALCHEMY as usize] = SOLID;
        flags[CUSTOM_DECO_EMPTY as usize] = flags[EMPTY as usize];
        flags[CUSTOM_DECO as usize] = SOLID;
        flags[STATUE as usize] = SOLID;
        flags[STATUE_SP as usize] = flags[STATUE as usize];
        flags[REGION_DECO as usize] = flags[STATUE as usize];
        flags[REGION_DECO_ALT as usize] = flags[STATUE_SP as usize];
        flags[MINE_CRYSTAL as usize] = SOLID;
        flags[MINE_BOULDER as usize] = SOLID;
        flags
    };

    #[must_use]
    pub fn flags(terrain: i32) -> i32 {
        FLAGS[usize::try_from(terrain).expect("negative terrain ID")]
    }

    #[must_use]
    pub const fn discover(terrain: i32) -> i32 {
        match terrain {
            SECRET_DOOR => DOOR,
            SECRET_TRAP => TRAP,
            _ => terrain,
        }
    }
}

/// Static drawing helpers ported from v3.3.8 `Painter.java`.
pub mod painter {
    use super::{GridMap, Point, Rect, div_i32, f64_to_i32, rem_i32, round_f32, round_f64};

    pub fn set(map: &mut GridMap, cell: usize, value: i32) {
        map.cells[cell] = value;
    }

    pub fn set_xy(map: &mut GridMap, x: i32, y: i32, value: i32) {
        map.set(x, y, value);
    }

    pub fn set_point(map: &mut GridMap, point: Point, value: i32) {
        map.set_point(point, value);
    }

    pub fn fill(map: &mut GridMap, x: i32, y: i32, width: i32, height: i32, value: i32) {
        let end_y = y.wrapping_add(height);
        let mut row = y;
        let mut position = y.wrapping_mul(map.width).wrapping_add(x);
        while row < end_y {
            let end = position.wrapping_add(width);
            let start = usize::try_from(position).expect("Painter.fill start is negative");
            let end = usize::try_from(end).expect("Painter.fill end is negative");
            assert!(start <= end, "Painter.fill has a negative width");
            map.cells[start..end].fill(value);
            row = row.wrapping_add(1);
            position = position.wrapping_add(map.width);
        }
    }

    pub fn fill_rect(map: &mut GridMap, rect: Rect, value: i32) {
        fill(map, rect.left, rect.top, rect.width(), rect.height(), value);
    }

    pub fn fill_rect_margin(map: &mut GridMap, rect: Rect, margin: i32, value: i32) {
        fill(
            map,
            rect.left.wrapping_add(margin),
            rect.top.wrapping_add(margin),
            rect.width().wrapping_sub(margin.wrapping_mul(2)),
            rect.height().wrapping_sub(margin.wrapping_mul(2)),
            value,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_rect_margins(
        map: &mut GridMap,
        rect: Rect,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        value: i32,
    ) {
        fill(
            map,
            rect.left.wrapping_add(left),
            rect.top.wrapping_add(top),
            rect.width().wrapping_sub(left.wrapping_add(right)),
            rect.height().wrapping_sub(top.wrapping_add(bottom)),
            value,
        );
    }

    /// DDA line rasterization with Java f32 division and `Math.round(float)`.
    #[allow(clippy::cast_precision_loss, clippy::float_cmp)] // Exact Java DDA operations.
    pub fn draw_line(map: &mut GridMap, from: Point, to: Point, value: i32) {
        #[allow(clippy::cast_precision_loss)]
        let mut x = from.x as f32;
        #[allow(clippy::cast_precision_loss)]
        let mut y = from.y as f32;
        #[allow(clippy::cast_precision_loss)]
        let mut dx = to.x.wrapping_sub(from.x) as f32;
        #[allow(clippy::cast_precision_loss)]
        let mut dy = to.y.wrapping_sub(from.y) as f32;

        let moving_by_x = dx.abs() >= dy.abs();
        if moving_by_x {
            let denominator = dx.abs();
            dy /= denominator;
            dx /= denominator;
        } else {
            let denominator = dy.abs();
            dx /= denominator;
            dy /= denominator;
        }

        set_xy(map, round_f32(x), round_f32(y), value);
        while (moving_by_x && (to.x as f32) != x) || (!moving_by_x && (to.y as f32) != y) {
            x += dx;
            y += dy;
            set_xy(map, round_f32(x), round_f32(y), value);
        }
    }

    pub fn fill_ellipse_rect(map: &mut GridMap, rect: Rect, value: i32) {
        fill_ellipse(map, rect.left, rect.top, rect.width(), rect.height(), value);
    }

    pub fn fill_ellipse_rect_margin(map: &mut GridMap, rect: Rect, margin: i32, value: i32) {
        fill_ellipse(
            map,
            rect.left.wrapping_add(margin),
            rect.top.wrapping_add(margin),
            rect.width().wrapping_sub(margin.wrapping_mul(2)),
            rect.height().wrapping_sub(margin.wrapping_mul(2)),
            value,
        );
    }

    /// Scanline ellipse port preserving the upstream `h / 2f` f32 operation.
    pub fn fill_ellipse(map: &mut GridMap, x: i32, y: i32, width: i32, height: i32, value: i32) {
        #[allow(clippy::cast_precision_loss)]
        let radius_height = f64::from(height as f32 / 2.0_f32);
        #[allow(clippy::cast_precision_loss)]
        let radius_width = f64::from(width as f32 / 2.0_f32);

        let mut row = 0;
        while row < height {
            let row_y = -radius_height + 0.5 + f64::from(row);
            let rad_h_square = radius_height * radius_height;
            let row_width = 2.0
                * ((radius_width * radius_width) * (1.0 - (row_y * row_y) / rad_h_square)).sqrt();

            #[allow(clippy::cast_precision_loss)] // Mirrors Java long promotion to double.
            let adjusted_width = if rem_i32(width, 2) == 0 {
                (round_f64(row_width / 2.0) as f64) * 2.0
            } else {
                (row_width / 2.0).floor() * 2.0 + 1.0
            };
            let adjusted_width = f64_to_i32(adjusted_width);
            let start_x = x.wrapping_add(div_i32(width.wrapping_sub(adjusted_width), 2));
            fill(map, start_x, y.wrapping_add(row), adjusted_width, 1, value);
            row = row.wrapping_add(1);
        }
    }

    pub fn fill_diamond_rect(map: &mut GridMap, rect: Rect, value: i32) {
        fill_diamond(map, rect.left, rect.top, rect.width(), rect.height(), value);
    }

    pub fn fill_diamond_rect_margin(map: &mut GridMap, rect: Rect, margin: i32, value: i32) {
        fill_diamond(
            map,
            rect.left.wrapping_add(margin),
            rect.top.wrapping_add(margin),
            rect.width().wrapping_sub(margin.wrapping_mul(2)),
            rect.height().wrapping_sub(margin.wrapping_mul(2)),
            value,
        );
    }

    pub fn fill_diamond(map: &mut GridMap, x: i32, y: i32, width: i32, height: i32, value: i32) {
        let mut diamond_width =
            width.wrapping_sub(height.wrapping_sub(2).wrapping_sub(rem_i32(height, 2)));
        diamond_width = diamond_width.max(if rem_i32(width, 2) == 0 { 2 } else { 3 });

        let mut row = 0;
        while row <= height {
            fill(
                map,
                x.wrapping_add(div_i32(width.wrapping_sub(diamond_width), 2)),
                y.wrapping_add(row),
                diamond_width,
                height.wrapping_sub(row.wrapping_mul(2)),
                value,
            );
            diamond_width = diamond_width.wrapping_add(2);
            if diamond_width > width {
                break;
            }
            row = row.wrapping_add(1);
        }
    }

    /// Port of `Painter.drawInside`; `room` uses inclusive room boundaries.
    pub fn draw_inside(
        map: &mut GridMap,
        room: Rect,
        from: Point,
        count: i32,
        value: i32,
    ) -> Point {
        let mut step = Point::default();
        if from.x == room.left {
            step.set(1, 0);
        } else if from.x == room.right {
            step.set(-1, 0);
        } else if from.y == room.top {
            step.set(0, 1);
        } else if from.y == room.bottom {
            step.set(0, -1);
        }

        let mut point = from;
        point.offset_point(step);
        let mut index = 0;
        while index < count {
            if value != -1 {
                set_point(map, point, value);
            }
            point.offset_point(step);
            index = index.wrapping_add(1);
        }
        point
    }
}

/// Largest grid height served by the bitboard fast path of
/// [`PathFinder::all_open_cells_connected`]; taller grids fall back to the
/// queue-based search.
const MAX_BITBOARD_ROWS: usize = 64;

/// Reusable port of v3.3.8 `com.watabou.utils.PathFinder`.
///
/// Upstream stores one global work buffer.  A Rust instance keeps the same
/// arrays together without global synchronization, so one finder can be reused
/// per worker thread.  Distances use eight-way movement and `i32::MAX` for
/// unreachable cells.
#[derive(Clone, Debug)]
pub struct PathFinder {
    width: i32,
    size: usize,
    pub distance: Vec<i32>,
    goals: Vec<bool>,
    queue: Vec<usize>,
    queued: Vec<bool>,
    direction: [i32; 8],
    direction_lr: [i32; 8],
    /// Array-access order, exactly matching `PathFinder.NEIGHBOURS4`.
    pub neighbours4: [i32; 4],
    /// Array-access order, exactly matching `PathFinder.NEIGHBOURS8`.
    pub neighbours8: [i32; 8],
    /// Array-access order, exactly matching `PathFinder.NEIGHBOURS9`.
    pub neighbours9: [i32; 9],
    /// Clockwise order, exactly matching `PathFinder.CIRCLE4`.
    pub circle4: [i32; 4],
    /// Clockwise order, exactly matching `PathFinder.CIRCLE8`.
    pub circle8: [i32; 8],
}

impl PathFinder {
    #[must_use]
    pub fn new(width: i32, height: i32) -> Self {
        let mut finder = Self {
            width: 1,
            size: 0,
            distance: Vec::new(),
            goals: Vec::new(),
            queue: Vec::new(),
            queued: Vec::new(),
            direction: [0; 8],
            direction_lr: [0; 8],
            neighbours4: [0; 4],
            neighbours8: [0; 8],
            neighbours9: [0; 9],
            circle4: [0; 4],
            circle8: [0; 8],
        };
        finder.set_map_size(width, height);
        finder
    }

    /// Resizes every work array and regenerates width-dependent offsets.
    pub fn set_map_size(&mut self, width: i32, height: i32) {
        assert!(
            width > 0 && height > 0,
            "PathFinder dimensions must be positive"
        );
        let size_i32 = width
            .checked_mul(height)
            .expect("PathFinder dimensions exceed Java int indexing");
        let size = usize::try_from(size_i32).expect("positive PathFinder size");

        self.width = width;
        self.size = size;
        // Java allocates fresh primitive arrays here, so their observable
        // state immediately after `setMapSize` is all-zero/all-false.
        self.distance.resize(size, 0);
        self.distance.fill(0);
        self.goals.resize(size, false);
        self.goals.fill(false);
        self.queue.resize(size, 0);
        self.queue.fill(0);
        self.queued.resize(size, false);
        self.queued.fill(false);

        self.direction = [
            -1,
            1,
            -width,
            width,
            -width - 1,
            -width + 1,
            width - 1,
            width + 1,
        ];
        self.direction_lr = [
            -1 - width,
            -1,
            -1 + width,
            -width,
            width,
            1 - width,
            1,
            1 + width,
        ];
        self.neighbours4 = [-width, -1, 1, width];
        self.neighbours8 = [
            -width - 1,
            -width,
            -width + 1,
            -1,
            1,
            width - 1,
            width,
            width + 1,
        ];
        self.neighbours9 = [
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
        self.circle4 = [-width, 1, width, -1];
        self.circle8 = [
            -width - 1,
            -width,
            -width + 1,
            1,
            width + 1,
            width,
            width - 1,
            -1,
        ];
    }

    #[must_use]
    pub const fn width(&self) -> i32 {
        self.width
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.size
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.size == 0
    }

    fn check_cell(&self, cell: usize) {
        assert!(cell < self.size, "PathFinder cell out of bounds");
    }

    fn check_map(&self, map: &[bool]) {
        assert_eq!(map.len(), self.size, "PathFinder map has wrong length");
    }

    fn enqueue(&mut self, tail: &mut usize, cell: usize) {
        // Upstream uses `new int[size]`; retaining that bound catches the same
        // malformed-map cases instead of silently allocating during a search.
        assert!(*tail < self.queue.len(), "PathFinder queue overflow");
        self.queue[*tail] = cell;
        *tail += 1;
    }

    fn lr_range(&self, step: usize) -> std::ops::Range<usize> {
        let step = i32::try_from(step).expect("PathFinder cell exceeds Java int");
        // One shared remainder: cells are non-negative, so `step % width == 0`
        // and `(step + 1) % width == 0` are the first and last map columns.
        let column = rem_i32(step, self.width);
        let start = if column == 0 { 3 } else { 0 };
        let end_trim = if column == self.width - 1 { 3 } else { 0 };
        start..(self.direction_lr.len() - end_trim)
    }

    fn offset_i32(step: usize, offset: i32) -> i32 {
        i32::try_from(step)
            .expect("PathFinder cell exceeds Java int")
            .wrapping_add(offset)
    }

    fn checked_offset(&self, step: usize, offset: i32) -> Option<usize> {
        let cell = Self::offset_i32(step, offset);
        if cell < 0 || usize::try_from(cell).ok()? >= self.size {
            None
        } else {
            Some(usize::try_from(cell).expect("checked non-negative cell"))
        }
    }

    /// Direct offset indexing used by upstream's final path selection loops.
    /// Border callers panic just as the Java array access does; generated maps
    /// conventionally keep their border impassable.
    fn direct_offset(&self, step: usize, offset: i32) -> usize {
        self.checked_offset(step, offset)
            .expect("PathFinder direct neighbour is outside map")
    }

    /// Equivalent of private `buildDistanceMap(from, to, passable)`.
    ///
    /// Source and destination passability are intentionally ignored.  If
    /// `from == to`, this returns false without clearing the prior distances,
    /// exactly like upstream.
    pub fn build_distance_map_between(
        &mut self,
        from: usize,
        to: usize,
        passable: &[bool],
    ) -> bool {
        self.check_cell(from);
        self.check_cell(to);
        self.check_map(passable);
        if from == to {
            return false;
        }

        self.distance.fill(i32::MAX);
        let mut head = 0;
        let mut tail = 0;
        self.enqueue(&mut tail, to);
        self.distance[to] = 0;
        let from_i32 = i32::try_from(from).expect("PathFinder cell exceeds Java int");

        while head < tail {
            let step = self.queue[head];
            head += 1;
            if step == from {
                return true;
            }
            let next_distance = self.distance[step].wrapping_add(1);
            let range = self.lr_range(step);
            for index in range {
                let cell_i32 = Self::offset_i32(step, self.direction_lr[index]);
                let is_from = cell_i32 == from_i32;
                let candidate = usize::try_from(cell_i32).ok();
                let should_queue = is_from
                    || candidate.is_some_and(|cell| {
                        cell < self.size && passable[cell] && self.distance[cell] > next_distance
                    });
                if should_queue {
                    let cell = candidate.expect("the valid source cell is non-negative");
                    self.enqueue(&mut tail, cell);
                    self.distance[cell] = next_distance;
                }
            }
        }
        false
    }

    /// Finds the same tie-broken path as `PathFinder.find`.
    ///
    /// The returned path excludes `from` and includes `to`.
    pub fn find(&mut self, from: usize, to: usize, passable: &[bool]) -> Option<Vec<usize>> {
        if !self.build_distance_map_between(from, to, passable) {
            return None;
        }

        let mut result = Vec::new();
        let mut step = from;
        loop {
            let mut minimum_distance = self.distance[step];
            let mut best = step;
            for offset in self.direction {
                let candidate = self.direct_offset(step, offset);
                let candidate_distance = self.distance[candidate];
                if candidate_distance < minimum_distance {
                    minimum_distance = candidate_distance;
                    best = candidate;
                }
            }
            step = best;
            result.push(step);
            if step == to {
                break;
            }
        }
        Some(result)
    }

    /// Returns the first tie-broken step toward `to`.
    pub fn get_step(&mut self, from: usize, to: usize, passable: &[bool]) -> Option<usize> {
        if !self.build_distance_map_between(from, to, passable) {
            return None;
        }

        let mut minimum_distance = self.distance[from];
        let mut best = from;
        for offset in self.direction {
            let candidate = self.direct_offset(from, offset);
            let candidate_distance = self.distance[candidate];
            if candidate_distance < minimum_distance {
                minimum_distance = candidate_distance;
                best = candidate;
            }
        }
        Some(best)
    }

    /// Floods all passable cells from `to` (the target itself is always zero).
    pub fn build_distance_map(&mut self, to: usize, passable: &[bool]) {
        self.build_distance_map_impl(to, passable, None);
    }

    /// Reachability-only equivalent of `build_distance_map` followed by an
    /// "every open cell has `distance != i32::MAX`" scan, for grids whose
    /// distances are never otherwise read. The flood runs on one bitmask row
    /// per grid line, spreading whole rows per step, which is far cheaper
    /// than the queue-based search on the small patch grids room painting
    /// validates. Like the queue search, the start cell reaches its
    /// neighbours even when the start itself is not open.
    ///
    /// # Panics
    ///
    /// Panics if `start` lies outside the grid or the dimensions are not
    /// positive, mirroring `buildDistanceMap`'s array accesses.
    #[must_use]
    pub fn all_open_cells_connected(
        width: i32,
        height: i32,
        start: usize,
        open: impl Fn(usize) -> bool,
    ) -> bool {
        let width_usize = usize::try_from(width).expect("positive grid width");
        let height_usize = usize::try_from(height).expect("positive grid height");
        let length = width_usize
            .checked_mul(height_usize)
            .expect("grid size fits usize");
        assert!(width > 0 && height > 0, "grid dimensions must be positive");
        assert!(start < length, "start cell is outside the grid");

        if width_usize > 64 || height_usize > MAX_BITBOARD_ROWS {
            // Generated rooms never get this large; fall back to the exact
            // queue-based search if some future caller passes a full map.
            let passable: Vec<bool> = (0..length).map(open).collect();
            let mut finder = PathFinder::new(width, height);
            finder.build_distance_map(start, &passable);
            return passable
                .iter()
                .zip(&finder.distance)
                .all(|(&is_open, &distance)| !is_open || distance != i32::MAX);
        }

        let row_mask = if width_usize == 64 {
            u64::MAX
        } else {
            (1_u64 << width_usize) - 1
        };
        let mut open_rows = [0_u64; MAX_BITBOARD_ROWS];
        for (y, open_row) in open_rows[..height_usize].iter_mut().enumerate() {
            let mut row = 0_u64;
            let base = y * width_usize;
            for x in 0..width_usize {
                row |= u64::from(open(base + x)) << x;
            }
            *open_row = row;
        }

        let mut reach = [0_u64; MAX_BITBOARD_ROWS];
        // Seeding outside `open_rows` reproduces the queue search's quirk of
        // relaxing outward from an impassable start cell.
        reach[start / width_usize] = 1_u64 << (start % width_usize);

        let spread = |row: u64| (row | row << 1 | row >> 1) & row_mask;
        loop {
            let mut changed = false;
            for y in 0..height_usize {
                let above = if y > 0 { reach[y - 1] } else { 0 };
                let below = if y + 1 < height_usize { reach[y + 1] } else { 0 };
                let grown = reach[y] | (spread(reach[y] | above | below) & open_rows[y]);
                if grown != reach[y] {
                    reach[y] = grown;
                    changed = true;
                }
            }
            for y in (0..height_usize).rev() {
                let above = if y > 0 { reach[y - 1] } else { 0 };
                let below = if y + 1 < height_usize { reach[y + 1] } else { 0 };
                let grown = reach[y] | (spread(reach[y] | above | below) & open_rows[y]);
                if grown != reach[y] {
                    reach[y] = grown;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        open_rows[..height_usize]
            .iter()
            .zip(&reach[..height_usize])
            .all(|(&open_row, &reached)| open_row & !reached == 0)
    }

    /// Limited form of `buildDistanceMap(to, passable, limit)`.
    pub fn build_distance_map_limited(&mut self, to: usize, passable: &[bool], limit: i32) {
        self.build_distance_map_impl(to, passable, Some(limit));
    }

    fn build_distance_map_impl(&mut self, to: usize, passable: &[bool], limit: Option<i32>) {
        self.check_cell(to);
        self.check_map(passable);
        let mut head = 0;
        let mut tail = 0;

        // `None` behaves like a limit no distance reaches, so hoisting it
        // keeps the per-step check to a single comparison. Re-slicing the
        // maps to the asserted size lets the optimizer drop redundant
        // per-cell bounds checks inside the neighbour loop.
        let limit = limit.unwrap_or(i32::MAX);
        let size = self.size;
        let width = self.width;
        let directions = self.direction_lr;
        let passable = &passable[..size];
        let queue = &mut self.queue[..size];
        let distance = &mut self.distance[..size];

        distance.fill(i32::MAX);
        queue[0] = to;
        tail += 1;
        distance[to] = 0;

        while head < tail {
            let step = queue[head];
            head += 1;
            let next_distance = distance[step].wrapping_add(1);
            if next_distance > limit {
                return;
            }

            let step_i32 = i32::try_from(step).expect("PathFinder cell exceeds Java int");
            let column = rem_i32(step_i32, width);
            let start_index = if column == 0 { 3 } else { 0 };
            let end_index = if column == width - 1 { 5 } else { 8 };
            for &offset in &directions[start_index..end_index] {
                let cell = step_i32.wrapping_add(offset);
                // A single unsigned compare rejects both negative and
                // out-of-map neighbours.
                #[allow(clippy::cast_sign_loss)]
                let cell = cell as u32 as usize;
                if cell < size && passable[cell] && distance[cell] > next_distance {
                    assert!(tail < size, "PathFinder queue overflow");
                    queue[tail] = cell;
                    tail += 1;
                    distance[cell] = next_distance;
                }
            }
        }
    }

    /// Multi-target version used by retreat pathfinding.
    pub fn build_distance_map_to_any(
        &mut self,
        from: usize,
        goals: &[bool],
        passable: &[bool],
    ) -> bool {
        self.check_cell(from);
        self.check_map(goals);
        self.check_map(passable);
        if goals[from] {
            return false;
        }

        self.distance.fill(i32::MAX);
        let head = 0;
        let mut tail = 0;
        for (cell, &is_goal) in goals.iter().enumerate() {
            if is_goal {
                self.enqueue(&mut tail, cell);
                self.distance[cell] = 0;
            }
        }
        self.flood_until(from, passable, head, tail)
    }

    fn flood_until(
        &mut self,
        from: usize,
        passable: &[bool],
        mut head: usize,
        mut tail: usize,
    ) -> bool {
        let from_i32 = i32::try_from(from).expect("PathFinder cell exceeds Java int");
        while head < tail {
            let step = self.queue[head];
            head += 1;
            if step == from {
                return true;
            }
            let next_distance = self.distance[step].wrapping_add(1);
            let range = self.lr_range(step);
            for index in range {
                let cell_i32 = Self::offset_i32(step, self.direction_lr[index]);
                let is_from = cell_i32 == from_i32;
                let candidate = usize::try_from(cell_i32).ok();
                let should_queue = is_from
                    || candidate.is_some_and(|cell| {
                        cell < self.size && passable[cell] && self.distance[cell] > next_distance
                    });
                if should_queue {
                    let cell = candidate.expect("the valid source cell is non-negative");
                    self.enqueue(&mut tail, cell);
                    self.distance[cell] = next_distance;
                }
            }
        }
        false
    }

    fn build_escape_distance_map(
        &mut self,
        current: usize,
        from: usize,
        look_ahead: i32,
        passable: &[bool],
    ) -> i32 {
        self.check_cell(current);
        self.check_cell(from);
        self.check_map(passable);
        self.distance.fill(i32::MAX);
        let mut destination_distance = i32::MAX;
        let mut head = 0;
        let mut tail = 0;
        self.enqueue(&mut tail, from);
        self.distance[from] = 0;
        let mut distance = 0;

        while head < tail {
            let step = self.queue[head];
            head += 1;
            distance = self.distance[step];
            if distance > destination_distance {
                return destination_distance;
            }
            if step == current {
                destination_distance = distance.wrapping_add(look_ahead);
            }
            let next_distance = distance.wrapping_add(1);
            let range = self.lr_range(step);
            for index in range {
                let Some(cell) = self.checked_offset(step, self.direction_lr[index]) else {
                    continue;
                };
                if passable[cell] && self.distance[cell] > next_distance {
                    self.enqueue(&mut tail, cell);
                    self.distance[cell] = next_distance;
                }
            }
        }
        distance
    }

    /// Rust `Option` form of upstream `getStepBack` (`-1` becomes `None`).
    ///
    /// As in Java, setting `can_approach_from_pos` false can clear cells in the
    /// caller's `passable` buffer.
    pub fn get_step_back(
        &mut self,
        current: usize,
        from: usize,
        look_ahead: i32,
        passable: &mut [bool],
        can_approach_from_pos: bool,
    ) -> Option<usize> {
        let mut target_distance =
            self.build_escape_distance_map(current, from, look_ahead, passable);
        if target_distance == 0 {
            return None;
        }

        if !can_approach_from_pos {
            let mut head = 0;
            let mut tail = 0;
            let mut new_distance = self.distance[current];
            self.queued.fill(false);
            self.enqueue(&mut tail, current);
            self.queued[current] = true;

            while head < tail {
                let step = self.queue[head];
                head += 1;
                if self.distance[step] > new_distance {
                    new_distance = self.distance[step];
                }
                let range = self.lr_range(step);
                for index in range {
                    let Some(cell) = self.checked_offset(step, self.direction_lr[index]) else {
                        continue;
                    };
                    if passable[cell] {
                        if self.distance[cell] < self.distance[current] {
                            passable[cell] = false;
                        } else if self.distance[cell] >= self.distance[step] && !self.queued[cell] {
                            self.enqueue(&mut tail, cell);
                            self.queued[cell] = true;
                        }
                    }
                }
            }
            target_distance = target_distance.min(new_distance);
        }

        for (goal, &distance) in self.goals.iter_mut().zip(&self.distance) {
            *goal = distance == target_distance;
        }
        if !self.build_distance_map_to_internal_goals(current, passable) {
            return None;
        }

        let mut minimum_distance = self.distance[current];
        let mut best = current;
        for offset in self.direction {
            let candidate = self.direct_offset(current, offset);
            let candidate_distance = self.distance[candidate];
            if candidate_distance < minimum_distance {
                minimum_distance = candidate_distance;
                best = candidate;
            }
        }
        Some(best)
    }

    fn build_distance_map_to_internal_goals(&mut self, from: usize, passable: &[bool]) -> bool {
        if self.goals[from] {
            return false;
        }
        self.distance.fill(i32::MAX);
        let mut tail = 0;
        // Indexing instead of an iterator keeps immutable borrows short enough
        // to reuse the instance's queue buffer.
        for cell in 0..self.size {
            if self.goals[cell] {
                self.enqueue(&mut tail, cell);
                self.distance[cell] = 0;
            }
        }
        self.flood_until(from, passable, 0, tail)
    }
}

/// Recursive field-of-view implementation from v3.3.8 `ShadowCaster.java`.
pub mod shadow_caster {
    use crate::java_math::{ceil_f64_to_i32, floor_f64_to_i32};

    pub const MAX_DISTANCE: i32 = 20;

    // Exact values produced by the v3.3.8 static initializer on OpenJDK 21.
    // Baking these in avoids a platform libm `asin/cos` result crossing a
    // `Math.round` boundary before a high-volume seed scan begins.
    const ROUNDING_0: &[i32] = &[0];
    const ROUNDING_1: &[i32] = &[0, 1];
    const ROUNDING_2: &[i32] = &[0, 1, 1];
    const ROUNDING_3: &[i32] = &[0, 1, 2, 2];
    const ROUNDING_4: &[i32] = &[0, 1, 2, 3, 2];
    const ROUNDING_5: &[i32] = &[0, 1, 2, 3, 3, 2];
    const ROUNDING_6: &[i32] = &[0, 1, 2, 3, 4, 4, 2];
    const ROUNDING_7: &[i32] = &[0, 1, 2, 3, 4, 5, 4, 3];
    const ROUNDING_8: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 5, 3];
    const ROUNDING_9: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 6, 5, 3];
    const ROUNDING_10: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 6, 5, 3];
    const ROUNDING_11: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 7, 5, 3];
    const ROUNDING_12: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 8, 7, 6, 3];
    const ROUNDING_13: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 9, 8, 6, 4];
    const ROUNDING_14: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 9, 8, 6, 4];
    const ROUNDING_15: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 9, 8, 6, 4];
    const ROUNDING_16: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 11, 10, 8, 7, 4];
    const ROUNDING_17: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 11, 10, 9, 7, 4];
    const ROUNDING_18: &[i32] = &[
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 12, 11, 9, 7, 4,
    ];
    const ROUNDING_19: &[i32] = &[
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 13, 12, 11, 9, 7, 4,
    ];
    const ROUNDING_20: &[i32] = &[
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 14, 13, 11, 10, 8, 4,
    ];

    fn rounding(distance: usize) -> &'static [i32] {
        match distance {
            0 => ROUNDING_0,
            1 => ROUNDING_1,
            2 => ROUNDING_2,
            3 => ROUNDING_3,
            4 => ROUNDING_4,
            5 => ROUNDING_5,
            6 => ROUNDING_6,
            7 => ROUNDING_7,
            8 => ROUNDING_8,
            9 => ROUNDING_9,
            10 => ROUNDING_10,
            11 => ROUNDING_11,
            12 => ROUNDING_12,
            13 => ROUNDING_13,
            14 => ROUNDING_14,
            15 => ROUNDING_15,
            16 => ROUNDING_16,
            17 => ROUNDING_17,
            18 => ROUNDING_18,
            19 => ROUNDING_19,
            20 => ROUNDING_20,
            _ => unreachable!("shadow distance is clamped"),
        }
    }

    /// Casts circular field of view into a caller-owned reusable buffer.
    ///
    /// Octants are scanned clockwise in precisely the upstream call order.  If
    /// a scan leaves the map, Java catches the array exception and clears the
    /// entire result; this port returns the same cleared buffer without using
    /// panics as control flow.
    pub fn cast_shadow(
        x: i32,
        y: i32,
        width: i32,
        field_of_view: &mut [bool],
        blocking: &[bool],
        distance: i32,
    ) {
        assert!(width > 0, "ShadowCaster width must be positive");
        assert_eq!(
            field_of_view.len(),
            blocking.len(),
            "ShadowCaster buffers differ in length"
        );
        field_of_view.fill(false);
        let source = x.wrapping_add(y.wrapping_mul(width));
        let source = usize::try_from(source).expect("ShadowCaster source cell is negative");
        field_of_view[source] = true;

        // A negative index into Java's `rounding` array is caught by the outer
        // try/catch, which clears the source cell again.
        if distance < 0 {
            field_of_view.fill(false);
            return;
        }
        let distance = distance.min(MAX_DISTANCE);
        let octants = [
            (1, -1, false),
            (-1, 1, true),
            (1, 1, true),
            (1, 1, false),
            (-1, 1, false),
            (1, -1, true),
            (-1, -1, true),
            (-1, -1, false),
        ];
        for (mirror_x, mirror_y, mirror_xy) in octants {
            if scan_octant(
                distance,
                field_of_view,
                blocking,
                1,
                x,
                y,
                width,
                0.0,
                1.0,
                mirror_x,
                mirror_y,
                mirror_xy,
            )
            .is_err()
            {
                field_of_view.fill(false);
                return;
            }
        }
    }

    #[allow(clippy::float_cmp, clippy::similar_names, clippy::too_many_arguments)]
    fn scan_octant(
        distance: i32,
        field_of_view: &mut [bool],
        blocking: &[bool],
        mut row: i32,
        x: i32,
        y: i32,
        width: i32,
        mut left_slope: f64,
        right_slope: f64,
        mirror_x: i32,
        mirror_y: i32,
        mirror_xy: bool,
    ) -> Result<(), ()> {
        let rounding_at_distance = rounding(usize::try_from(distance).map_err(|_| ())?);
        let mut in_blocking = false;

        while row <= distance {
            if right_slope < left_slope {
                return Ok(());
            }
            let start = if left_slope == 0.0 {
                0
            } else {
                floor_f64_to_i32((f64::from(row) - 0.5) * left_slope + 0.499)
            };
            let rounded_end = if distance == 2 && row == 2 {
                // Special corner fill from the upstream clone/mutation.
                2
            } else {
                rounding_at_distance[usize::try_from(row).map_err(|_| ())?]
            };
            let end = if right_slope == 1.0 {
                rounded_end
            } else {
                rounded_end.min(ceil_f64_to_i32(
                    (f64::from(row) + 0.5) * right_slope - 0.499,
                ))
            };

            let mut cell = x.wrapping_add(y.wrapping_mul(width));
            if mirror_xy {
                cell = cell
                    .wrapping_add(mirror_x.wrapping_mul(start).wrapping_mul(width))
                    .wrapping_add(mirror_y.wrapping_mul(row));
            } else {
                cell = cell
                    .wrapping_add(mirror_x.wrapping_mul(start))
                    .wrapping_add(mirror_y.wrapping_mul(row).wrapping_mul(width));
            }

            let mut column = start;
            while column <= end {
                if column == end
                    && in_blocking
                    && ceil_f64_to_i32((f64::from(row) - 0.5) * right_slope - 0.499) != end
                {
                    break;
                }

                let index = usize::try_from(cell).map_err(|_| ())?;
                let visible = field_of_view.get_mut(index).ok_or(())?;
                *visible = true;
                let blocks = *blocking.get(index).ok_or(())?;
                if blocks {
                    if !in_blocking {
                        in_blocking = true;
                        if column != start {
                            scan_octant(
                                distance,
                                field_of_view,
                                blocking,
                                row.wrapping_add(1),
                                x,
                                y,
                                width,
                                left_slope,
                                (f64::from(column) - 0.5) / (f64::from(row) + 0.5),
                                mirror_x,
                                mirror_y,
                                mirror_xy,
                            )?;
                        }
                    }
                } else if in_blocking {
                    in_blocking = false;
                    left_slope = (f64::from(column) - 0.5) / (f64::from(row) - 0.5);
                }

                if mirror_xy {
                    cell = cell.wrapping_add(mirror_x.wrapping_mul(width));
                } else {
                    cell = cell.wrapping_add(mirror_x);
                }
                column = column.wrapping_add(1);
            }
            if in_blocking {
                return Ok(());
            }
            row = row.wrapping_add(1);
        }
        Ok(())
    }
}

pub use shadow_caster::cast_shadow;

#[cfg(test)]
mod tests {
    use super::{GridMap, PathFinder, Point, Rect, barray, cast_shadow, painter, terrain};

    /// Direct parity cases for v3.3.8 `Point.java`.  The overflow cases matter
    /// because Java evaluates the integer expressions before float promotion.
    #[test]
    #[allow(clippy::float_cmp)] // Exact Java oracle value, not an approximation.
    fn point_preserves_java_compound_casts_and_integer_intermediates() {
        let mut point = Point::new(-3, 3);
        point.scale(0.5);
        assert_eq!(point, Point::new(-1, 1));
        assert_eq!(Point::new(3, 4).length().to_bits(), 5.0_f32.to_bits());
        assert!(Point::new(46_341, 0).length().is_nan());
        assert_eq!(
            Point::distance(Point::new(i32::MAX, 0), Point::new(i32::MIN, 0)),
            1.0
        );
    }

    /// v3.3.8 `Rect.center()` consumes x then y and only consumes RNG for an
    /// even-sized dimension. `getPoints()` is column-major and inclusive.
    #[test]
    fn rect_center_and_point_iteration_match_upstream_order() {
        let mut draws = [1, 0].into_iter();
        let center = Rect::new(2, 4, 8, 10).center_with(|bound| {
            assert_eq!(bound, 2);
            draws.next().expect("two even dimensions")
        });
        assert_eq!(center, Point::new(6, 7));
        assert_eq!(draws.next(), None);

        let mut called = false;
        let odd_center = Rect::new(1, 2, 6, 9).center_with(|_| {
            called = true;
            0
        });
        assert_eq!(odd_center, Point::new(3, 5));
        assert!(!called);

        assert_eq!(
            Rect::new(1, 2, 2, 3).points(),
            [
                Point::new(1, 2),
                Point::new(1, 3),
                Point::new(2, 2),
                Point::new(2, 3),
            ]
        );
    }

    /// Expected raster captured from the arithmetic in v3.3.8 `Painter.java`.
    #[test]
    fn painter_line_and_ellipse_use_java_rounding() {
        let mut line = GridMap::new(8, 5, 0);
        painter::draw_line(&mut line, Point::new(1, 1), Point::new(6, 3), 9);
        let painted: Vec<Point> = line
            .cells
            .iter()
            .enumerate()
            .filter(|&(_, &value)| value == 9)
            .map(|(cell, _)| line.cell_to_point(cell))
            .collect();
        assert_eq!(
            painted,
            [
                Point::new(1, 1),
                Point::new(2, 1),
                Point::new(3, 2),
                Point::new(4, 2),
                Point::new(5, 3),
                Point::new(6, 3),
            ]
        );

        let mut ellipse = GridMap::new(7, 7, 0);
        painter::fill_ellipse(&mut ellipse, 1, 1, 5, 5, 1);
        let row_widths: Vec<usize> = (1..=5)
            .map(|y| {
                let start = ellipse.cell(0, y);
                ellipse.cells[start..start + 7]
                    .iter()
                    .filter(|&&cell| cell == 1)
                    .count()
            })
            .collect();
        assert_eq!(row_widths, [3, 5, 5, 5, 3]);
    }

    #[test]
    fn painter_rect_fill_and_draw_inside_share_row_major_indexing() {
        let mut map = GridMap::new(7, 7, terrain::CHASM);
        painter::fill_rect_margin(&mut map, Rect::new(1, 1, 6, 6), 1, terrain::EMPTY);
        assert_eq!(map[map.cell(2, 2)], terrain::EMPTY);
        assert_eq!(map[map.cell(5, 5)], terrain::CHASM);

        let end = painter::draw_inside(
            &mut map,
            Rect::new(1, 1, 5, 5),
            Point::new(1, 3),
            2,
            terrain::WATER,
        );
        assert_eq!(end, Point::new(4, 3));
        assert_eq!(map[map.cell(2, 3)], terrain::WATER);
        assert_eq!(map[map.cell(3, 3)], terrain::WATER);
    }

    /// IDs and flags are copied from v3.3.8 `Terrain.java`; boolean results are
    /// the corresponding v3.3.8 `BArray` operations.
    #[test]
    fn terrain_and_boolean_array_helpers_match_upstream_tables() {
        assert_eq!(
            terrain::flags(terrain::DOOR),
            terrain::PASSABLE | terrain::LOS_BLOCKING | terrain::FLAMABLE | terrain::SOLID
        );
        assert_eq!(
            terrain::flags(terrain::SECRET_DOOR) & terrain::SECRET,
            terrain::SECRET
        );
        assert_eq!(terrain::discover(terrain::SECRET_TRAP), terrain::TRAP);

        let a = [true, false, true, false];
        let b = [true, true, false, false];
        assert_eq!(barray::and(&a, &b), [true, false, false, false]);
        assert_eq!(barray::or(&a, &b), [true, true, true, false]);
        assert_eq!(barray::not(&a), [false, true, false, true]);
        assert_eq!(
            barray::is_one_of(&[1, 2, 3, 2], &[2, 4]),
            [false, true, false, true]
        );
    }

    /// Vector captured by executing v3.3.8 `PathFinder` from the pinned
    /// `SPD-classes-3.3.8.jar` on this 5x5 map.
    #[test]
    fn pathfinder_offsets_distances_and_tie_breaking_match_java_oracle() {
        let mut finder = PathFinder::new(5, 5);
        assert_eq!(finder.neighbours4, [-5, -1, 1, 5]);
        assert_eq!(finder.neighbours8, [-6, -5, -4, -1, 1, 4, 5, 6]);
        assert_eq!(finder.circle8, [-6, -5, -4, 1, 6, 5, 4, -1]);

        let mut passable = vec![false; 25];
        for y in 1..4 {
            for x in 1..4 {
                passable[x + y * 5] = true;
            }
        }
        passable[12] = false;
        finder.build_distance_map(6, &passable);
        assert_eq!(
            finder.distance,
            [
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                0,
                1,
                2,
                i32::MAX,
                i32::MAX,
                1,
                i32::MAX,
                2,
                i32::MAX,
                i32::MAX,
                2,
                2,
                3,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
            ]
        );
        assert_eq!(finder.find(16, 8, &passable), Some(vec![17, 13, 8]));
        assert_eq!(finder.get_step(16, 8, &passable), Some(17));
    }

    #[test]
    fn limited_distance_map_stops_after_java_limit() {
        let mut finder = PathFinder::new(5, 5);
        let mut passable = vec![false; 25];
        for y in 1..4 {
            for x in 1..4 {
                passable[x + y * 5] = true;
            }
        }
        finder.build_distance_map_limited(12, &passable, 1);
        assert_eq!(finder.distance[12], 0);
        for &offset in &finder.neighbours8 {
            let cell = usize::try_from(12_i32 + offset).expect("interior neighbour");
            assert_eq!(finder.distance[cell], 1);
        }
        assert_eq!(finder.distance[0], i32::MAX);
    }

    /// Vector captured from v3.3.8 `PathFinder.getStepBack`.  The second call
    /// also verifies the upstream side effect that forbids cells approaching
    /// the position being escaped from.
    #[test]
    fn retreat_path_and_passability_mutation_match_java_oracle() {
        let mut finder = PathFinder::new(7, 7);
        let mut passable = vec![false; 49];
        for y in 1..6 {
            for x in 1..6 {
                passable[x + y * 7] = true;
            }
        }

        let mut unrestricted = passable.clone();
        assert_eq!(
            finder.get_step_back(17, 24, 2, &mut unrestricted, true),
            Some(10)
        );
        assert_eq!(unrestricted, passable);

        assert_eq!(
            finder.get_step_back(17, 24, 2, &mut passable, false),
            Some(10)
        );
        assert!(!passable[24]);
        assert_eq!(passable.iter().filter(|&&cell| cell).count(), 24);
    }

    /// This exact bitmap was captured from v3.3.8 `ShadowCaster.castShadow`
    /// with a three-cell vertical blocker immediately east of the source.
    #[test]
    fn shadow_caster_matches_java_oracle_bitmap() {
        let mut blocking = vec![false; 81];
        blocking[3 * 9 + 5] = true;
        blocking[4 * 9 + 5] = true;
        blocking[5 * 9 + 5] = true;
        let mut field_of_view = vec![true; 81];
        cast_shadow(4, 4, 9, &mut field_of_view, &blocking, 4);

        let rows: Vec<String> = field_of_view
            .chunks_exact(9)
            .map(|row| {
                row.iter()
                    .map(|&visible| if visible { '#' } else { '.' })
                    .collect()
            })
            .collect();
        assert_eq!(
            rows,
            [
                "..#####..",
                ".#####...",
                "######...",
                "######...",
                "######...",
                "######...",
                "######...",
                ".#####...",
                "..#####..",
            ]
        );
    }

    /// Broader hashes captured from the pinned v3.3.8 jar.  They exercise the
    /// distance-two corner rule, recursive blockers, maximum-distance rounding
    /// rows, and the clamp above `MAX_DISTANCE`.
    #[test]
    fn shadow_caster_matches_java_oracle_across_distance_ranges() {
        let width = 45;
        let mut blocking = vec![false; width * width];
        for y in 0..width {
            for x in 0..width {
                blocking[x + y * width] = (x * 17 + y * 31) % 23 == 0;
            }
        }
        blocking[22 + 22 * width] = false;

        for (distance, expected_count, expected_hash) in [
            (2, 24, -10_832_268_i32),
            (7, 164, -1_890_521_276_i32),
            (20, 867, -426_896_318_i32),
            (99, 867, -426_896_318_i32),
        ] {
            let mut field_of_view = vec![false; width * width];
            cast_shadow(
                22,
                22,
                i32::try_from(width).expect("small oracle map"),
                &mut field_of_view,
                &blocking,
                distance,
            );
            assert_eq!(
                field_of_view.iter().filter(|&&visible| visible).count(),
                expected_count
            );
            let java_hash = field_of_view.iter().fold(1_i32, |hash, &visible| {
                hash.wrapping_mul(31)
                    .wrapping_add(if visible { 1231 } else { 1237 })
            });
            assert_eq!(java_hash, expected_hash);
        }
    }
}
