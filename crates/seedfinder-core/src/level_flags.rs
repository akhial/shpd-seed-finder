//! Data-only port of v3.3.8 `Level.buildFlagMaps()` and `cleanWalls()`.
//!
//! These arrays are the exact spatial predicates consumed by mob placement
//! and `RegularLevel.randomDropCell`. Blob overrides are absent in the
//! canonical freshly generated main dungeon unless a Web blob is explicitly
//! supplied; the current API therefore models the ordinary map plus Sewer's
//! barrel flammability override.

use crate::geometry::{GridMap, terrain};

/// Derived terrain flags for one fully painted level.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelFlags {
    pub passable: Vec<bool>,
    pub los_blocking: Vec<bool>,
    pub flammable: Vec<bool>,
    pub secret: Vec<bool>,
    pub solid: Vec<bool>,
    pub avoid: Vec<bool>,
    pub water: Vec<bool>,
    pub pit: Vec<bool>,
    pub open_space: Vec<bool>,
    pub discoverable: Vec<bool>,
}

impl LevelFlags {
    /// Builds all flag maps in the official operation order, including forced
    /// solid borders, large-mob open-space detection, and `cleanWalls`.
    ///
    /// # Panics
    ///
    /// Panics for maps smaller than 3×3, which cannot be a generated level,
    /// or for invalid terrain identifiers.
    #[must_use]
    pub fn build(map: &GridMap, sewer: bool) -> Self {
        let mut flags = Self::build_for_generation(map);

        for (cell, &tile) in map.cells.iter().enumerate() {
            let terrain_flags = terrain::flags(tile);
            flags.flammable[cell] = terrain_flags & terrain::FLAMABLE != 0;
            flags.secret[cell] = terrain_flags & terrain::SECRET != 0;
            flags.avoid[cell] = terrain_flags & terrain::AVOID != 0;
            flags.water[cell] = terrain_flags & terrain::LIQUID != 0;
            flags.pit[cell] = terrain_flags & terrain::PIT != 0;
        }

        if sewer {
            for (cell, &tile) in map.cells.iter().enumerate() {
                if matches!(tile, terrain::REGION_DECO | terrain::REGION_DECO_ALT) {
                    flags.flammable[cell] = true;
                }
            }
        }

        let length = map.len();
        let width = usize::try_from(map.width).expect("positive map width");
        let last_row = length - width;
        for x in 0..width {
            flags.avoid[x] = false;
            flags.avoid[last_row + x] = false;
        }
        for row_start in (width..last_row).step_by(width) {
            flags.avoid[row_start] = false;
            flags.avoid[row_start + width - 1] = false;
        }

        let width_i32 = map.width;
        let neighbours9 = [
            -width_i32 - 1,
            -width_i32,
            -width_i32 + 1,
            -1,
            0,
            1,
            width_i32 - 1,
            width_i32,
            width_i32 + 1,
        ];
        for cell in 0..length {
            let cell_i32 = i32::try_from(cell).expect("generated map length fits Java int");
            flags.discoverable[cell] = neighbours9.iter().any(|&offset| {
                let neighbour = cell_i32.wrapping_add(offset);
                let Ok(neighbour) = usize::try_from(neighbour) else {
                    return false;
                };
                neighbour < length
                    && !matches!(map.cells[neighbour], terrain::WALL | terrain::WALL_DECO)
            });
        }
        flags
    }

    /// Builds exactly the flag maps generation consumes — `passable`,
    /// `los_blocking`, `solid`, and `open_space` — in the official operation
    /// order. The remaining maps (`flammable`, `secret`, `avoid`, `water`,
    /// `pit`, `discoverable`) are never read during seed generation, so they
    /// stay allocated but all-false; use [`Self::build`] when those matter.
    ///
    /// # Panics
    ///
    /// Panics for maps smaller than 3×3, which cannot be a generated level,
    /// or for invalid terrain identifiers.
    #[must_use]
    pub fn build_for_generation(map: &GridMap) -> Self {
        assert!(map.width >= 3 && map.height >= 3, "level map is too small");
        let length = map.len();
        let mut flags = Self {
            passable: vec![false; length],
            los_blocking: vec![false; length],
            flammable: vec![false; length],
            secret: vec![false; length],
            solid: vec![false; length],
            avoid: vec![false; length],
            water: vec![false; length],
            pit: vec![false; length],
            open_space: vec![false; length],
            discoverable: vec![false; length],
        };

        for (((&tile, passable), los_blocking), solid) in map
            .cells
            .iter()
            .zip(&mut flags.passable)
            .zip(&mut flags.los_blocking)
            .zip(&mut flags.solid)
        {
            let terrain_flags = terrain::flags(tile);
            *passable = terrain_flags & terrain::PASSABLE != 0;
            *los_blocking = terrain_flags & terrain::LOS_BLOCKING != 0;
            *solid = terrain_flags & terrain::SOLID != 0;
        }

        let width = usize::try_from(map.width).expect("positive map width");
        let last_row = length - width;
        for x in 0..width {
            force_solid_border(&mut flags, x);
            force_solid_border(&mut flags, last_row + x);
        }
        for row_start in (width..last_row).step_by(width) {
            force_solid_border(&mut flags, row_start);
            force_solid_border(&mut flags, row_start + width - 1);
        }

        build_open_space(&mut flags, map.width, length, width);
        flags
    }
}

/// `Level.buildFlagMaps`'s open-space rule: a non-solid cell is open space
/// when some `PathFinder.CIRCLE8` (cardinal, next diagonal, next cardinal)
/// triple — visited in N/E/S/W order — is entirely non-solid. The early-exit
/// walk over the four triples reduces to an OR of the four all-clear
/// patterns, which the fast path evaluates with one bitmask row per map line.
fn build_open_space(flags: &mut LevelFlags, width_i32: i32, length: usize, width: usize) {
    let height = length / width;
    if width <= 64 {
        let row_mask = if width == 64 {
            u64::MAX
        } else {
            (1_u64 << width) - 1
        };
        let mut clear_rows = vec![0_u64; height];
        for (row, clear) in flags.solid.chunks_exact(width).zip(&mut clear_rows) {
            let mut bits = 0_u64;
            for (x, &solid) in row.iter().enumerate() {
                bits |= u64::from(!solid) << x;
            }
            *clear = bits & row_mask;
        }
        for y in 1..height.saturating_sub(1) {
            let above = clear_rows[y - 1];
            let row = clear_rows[y];
            let below = clear_rows[y + 1];
            let north = above & (above >> 1) & (row >> 1);
            let east = (row >> 1) & (below >> 1) & below;
            let south = below & (below << 1) & (row << 1);
            let west = (row << 1) & (above << 1) & above;
            let open = row & (north | east | south | west) & row_mask;
            let base = y * width;
            for x in 0..width {
                flags.open_space[base + x] = open & (1 << x) != 0;
            }
        }
        return;
    }

    let circle8_triples = [
        (-width_i32, -width_i32 + 1, 1),
        (1, width_i32 + 1, width_i32),
        (width_i32, width_i32 - 1, -1),
        (-1, -width_i32 - 1, -width_i32),
    ];
    for cell in 0..length {
        if flags.solid[cell] {
            continue;
        }
        let cell_i32 = i32::try_from(cell).expect("generated map length fits Java int");
        for (cardinal, diagonal_next, cardinal_next) in circle8_triples {
            let diagonal = offset_index(cell_i32, cardinal);
            if flags.solid[diagonal] {
                flags.open_space[cell] = false;
            } else {
                let first = offset_index(cell_i32, diagonal_next);
                let second = offset_index(cell_i32, cardinal_next);
                if !flags.solid[first] && !flags.solid[second] {
                    flags.open_space[cell] = true;
                    break;
                }
            }
        }
    }
}

fn force_solid_border(flags: &mut LevelFlags, cell: usize) {
    flags.passable[cell] = false;
    flags.avoid[cell] = false;
    flags.los_blocking[cell] = true;
    flags.solid[cell] = true;
}

fn offset_index(cell: i32, offset: i32) -> usize {
    usize::try_from(cell.wrapping_add(offset)).expect("open-space neighbour is in map bounds")
}

#[cfg(test)]
mod tests {
    use crate::geometry::{GridMap, terrain};

    use super::LevelFlags;

    #[test]
    fn borders_open_space_and_sewer_barrels_match_level_rules() {
        let mut map = GridMap::new(5, 5, terrain::EMPTY);
        map.set(2, 2, terrain::REGION_DECO);
        let flags = LevelFlags::build(&map, true);

        for cell in [0, 1, 2, 3, 4, 5, 9, 10, 14, 15, 19, 20, 21, 22, 23, 24] {
            assert!(flags.solid[cell]);
            assert!(flags.los_blocking[cell]);
            assert!(!flags.passable[cell]);
        }
        assert!(flags.flammable[12]);
        assert!(flags.solid[12]);
        assert!(!flags.open_space[12]);
        // The one-cell interior ring has no open 2x2 corner once the center
        // barrel and forced map border are treated as solid.
        assert!(!flags.open_space[6]);
        assert!(flags.discoverable[0]);
    }

    #[test]
    fn clean_walls_marks_only_walls_near_non_wall_terrain() {
        let mut map = GridMap::new(5, 5, terrain::WALL);
        map.set(2, 2, terrain::EMPTY);
        let flags = LevelFlags::build(&map, false);
        for y in 0..5 {
            for x in 0..5 {
                let cell = usize::try_from(x + y * 5).unwrap();
                assert_eq!(
                    flags.discoverable[cell],
                    (1..=3).contains(&x) && (1..=3).contains(&y)
                );
            }
        }
    }
}
