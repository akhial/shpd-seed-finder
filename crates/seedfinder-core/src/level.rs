//! Minimal, data-only level state needed while painting regular levels.
//!
//! This mirrors the generation-visible portion of v3.3.8 `Level`: map sizing,
//! terrain flag scratch space, heap/mob occupancy consulted by grass painting,
//! placed traps, and the shuffled room-list order retained by
//! `RegularPainter`.  Rendering, actors, blobs, and persistence intentionally
//! remain outside this layer.

use crate::generator::{GeneratedItem, SeedKind};
use crate::geometry::{GridMap, Point, terrain};
pub use crate::level_prelude::Feeling;
use crate::room::RoomId;

/// Trap classes selected by the regular Sewer painter.
///
/// The enum carries identity only. Constructor-time properties which affect
/// painting are stored in [`TrapSpec`], so later regions can add their own
/// exact tables without reflection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum TrapKind {
    Burning,
    Explosive,
    WornDart,
    PoisonDart,
    Gripping,
    Geyser,
    Chilling,
    Shocking,
    Toxic,
    Alarm,
    Ooze,
    Confusion,
    Flock,
    Summoning,
    Teleportation,
    Gateway,
    Frost,
    Storm,
    Corrosion,
    Blazing,
    Disintegration,
    Rockfall,
    Flashing,
    Guardian,
    Weakening,
    Disarming,
    Warping,
    Cursing,
    Pitfall,
    Distortion,
    Grim,
}

/// Generation-visible state produced by constructing a Java `Trap` subclass.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TrapSpec {
    pub kind: TrapKind,
    pub avoids_hallways: bool,
    pub can_be_hidden: bool,
}

impl TrapSpec {
    #[must_use]
    pub const fn new(kind: TrapKind) -> Self {
        Self {
            kind,
            avoids_hallways: false,
            can_be_hidden: true,
        }
    }

    #[must_use]
    pub const fn avoids_hallways(mut self) -> Self {
        self.avoids_hallways = true;
        self
    }

    #[must_use]
    pub const fn cannot_be_hidden(mut self) -> Self {
        self.can_be_hidden = false;
        self
    }
}

/// A trap after `Level.setTrap`, including the visibility chosen by the
/// regular painter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlacedTrap {
    pub spec: TrapSpec,
    pub cell: usize,
    pub visible: bool,
    pub active: bool,
}

/// Direct queue identities which can be selected and painted by a later room
/// but have no `Generator` constructor record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectPaintItem {
    IronKey { depth: i32 },
    CrystalKey { depth: i32 },
    GoldenKey { depth: i32 },
    PotionOfInvisibility,
    PotionOfHaste,
    CeremonialCandle,
    Torch,
    Other(&'static str),
}

/// Items which can be placed during room painting. Most are represented by
/// the ordinary generator model; the remaining variants preserve exact
/// `Level.itemsToSpawn` identity across interleaved room painters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaintItem {
    Generated(GeneratedItem),
    ArcaneStylus,
    TrinketCatalyst,
    Direct(DirectPaintItem),
}

impl From<GeneratedItem> for PaintItem {
    fn from(item: GeneratedItem) -> Self {
        Self::Generated(item)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HeapKind {
    Heap,
    Chest,
    Tomb,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaintHeap {
    pub cell: usize,
    pub kind: HeapKind,
    pub items: Vec<PaintItem>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TransitionKind {
    Surface,
    RegularEntrance,
    RegularExit,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PaintTransition {
    pub cell: usize,
    pub kind: TransitionKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaintMob {
    Piranha { cell: usize, phantom: bool },
    Mimic { cell: usize, items: Vec<PaintItem> },
}

impl PaintMob {
    #[must_use]
    pub const fn cell(&self) -> usize {
        match self {
            Self::Piranha { cell, .. } | Self::Mimic { cell, .. } => *cell,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PaintPlant {
    pub cell: usize,
    pub seed: SeedKind,
}

/// Operation-order trace for generation-visible side effects of room paint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaintEvent {
    Transition(PaintTransition),
    Drop {
        cell: usize,
        kind: HeapKind,
        item: PaintItem,
    },
    Mob(PaintMob),
    Plant(PaintPlant),
    Trap(PlacedTrap),
}

/// Canonical v3.3.8 Sewer trap class table and weights for a main-dungeon
/// floor. The first floor contains only the always-visible worn dart trap.
#[must_use]
pub fn sewer_trap_table(depth: u32) -> (Vec<TrapSpec>, Vec<f32>) {
    if depth == 1 {
        return (
            vec![
                TrapSpec::new(TrapKind::WornDart)
                    .avoids_hallways()
                    .cannot_be_hidden(),
            ],
            vec![1.0],
        );
    }

    (
        vec![
            TrapSpec::new(TrapKind::Chilling),
            TrapSpec::new(TrapKind::Shocking),
            TrapSpec::new(TrapKind::Toxic),
            TrapSpec::new(TrapKind::WornDart)
                .avoids_hallways()
                .cannot_be_hidden(),
            TrapSpec::new(TrapKind::Alarm),
            TrapSpec::new(TrapKind::Ooze),
            TrapSpec::new(TrapKind::Confusion),
            TrapSpec::new(TrapKind::Flock),
            TrapSpec::new(TrapKind::Summoning),
            TrapSpec::new(TrapKind::Teleportation),
            TrapSpec::new(TrapKind::Gateway).avoids_hallways(),
        ],
        vec![4.0, 4.0, 4.0, 4.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    )
}

/// Data-only subset of v3.3.8 `Level` used during regular-level painting.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub depth: u32,
    pub feeling: Feeling,
    pub map: GridMap,
    /// Temporary `Level.passable` buffer populated before trap placement.
    pub passable: Vec<bool>,
    /// Occupancy queried by `RegularPainter.paintGrass`.
    pub heap_cells: Vec<bool>,
    /// Occupancy queried by `RegularPainter.paintGrass`.
    pub mob_cells: Vec<bool>,
    pub traps: Vec<PlacedTrap>,
    pub heaps: Vec<PaintHeap>,
    pub mobs: Vec<PaintMob>,
    pub plants: Vec<PaintPlant>,
    pub transitions: Vec<PaintTransition>,
    pub paint_events: Vec<PaintEvent>,
    /// Java shuffles `RegularLevel.rooms` in place before room painting. Room
    /// IDs cannot be physically shuffled in this port without invalidating
    /// graph edges, so this permutation is the authoritative later traversal
    /// order for mob and item generation.
    pub room_order: Vec<RoomId>,
    /// Canonical seed-finder profile sets `SPDSettings.intro()` to false.
    pub intro: bool,
    /// Canonical debug journal state has the searching guide page unlocked.
    pub guide_searching_found: bool,
    /// `Challenges.NO_HERBALISM`: terrain is still converted to grass, but no
    /// plant object is registered.
    pub plants_enabled: bool,
}

impl Level {
    #[must_use]
    pub fn new(depth: u32, feeling: Feeling) -> Self {
        Self {
            depth,
            feeling,
            map: GridMap::new(0, 0, terrain::WALL),
            passable: Vec::new(),
            heap_cells: Vec::new(),
            mob_cells: Vec::new(),
            traps: Vec::new(),
            heaps: Vec::new(),
            mobs: Vec::new(),
            plants: Vec::new(),
            transitions: Vec::new(),
            paint_events: Vec::new(),
            room_order: Vec::new(),
            intro: false,
            guide_searching_found: true,
            plants_enabled: true,
        }
    }

    /// `Level.setSize`: allocate the map and painter-visible scratch arrays.
    ///
    /// Generation retries in the game recreate heap, mob, and trap
    /// collections immediately before calling `build`. Clearing those data
    /// here gives one Rust call the same retry boundary.
    pub fn set_size(&mut self, width: i32, height: i32) {
        let initial = if self.feeling == Feeling::Chasm {
            terrain::CHASM
        } else {
            terrain::WALL
        };
        self.map = GridMap::new(width, height, initial);
        let length = self.map.len();
        self.passable = vec![false; length];
        self.heap_cells = vec![false; length];
        self.mob_cells = vec![false; length];
        self.traps.clear();
        self.heaps.clear();
        self.mobs.clear();
        self.plants.clear();
        self.transitions.clear();
        self.paint_events.clear();
    }

    #[must_use]
    pub const fn width(&self) -> i32 {
        self.map.width
    }

    #[must_use]
    pub const fn height(&self) -> i32 {
        self.map.height
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    #[must_use]
    pub fn point_to_cell(&self, point: Point) -> usize {
        self.map.point_to_cell(point)
    }

    #[must_use]
    pub fn tunnel_tile(&self) -> i32 {
        if self.feeling == Feeling::Chasm {
            terrain::EMPTY_SP
        } else {
            terrain::EMPTY
        }
    }

    pub fn mark_heap(&mut self, cell: usize) {
        self.heap_cells[cell] = true;
    }

    pub fn mark_mob(&mut self, cell: usize) {
        self.mob_cells[cell] = true;
    }

    #[must_use]
    pub fn is_occupied_for_grass(&self, cell: usize) -> bool {
        self.heap_cells[cell] || self.mob_cells[cell]
    }

    pub fn set_trap(&mut self, trap: PlacedTrap) {
        if let Some(existing) = self
            .traps
            .iter_mut()
            .find(|existing| existing.cell == trap.cell)
        {
            *existing = trap;
        } else {
            self.traps.push(trap);
        }
        self.paint_events.push(PaintEvent::Trap(trap));
    }

    pub fn drop_item(&mut self, item: PaintItem, cell: usize, kind: HeapKind) {
        self.mark_heap(cell);
        let actual_kind = if let Some(heap) = self.heaps.iter_mut().find(|heap| heap.cell == cell) {
            heap.items.push(item);
            if kind != HeapKind::Heap {
                heap.kind = kind;
            }
            heap.kind
        } else {
            self.heaps.push(PaintHeap {
                cell,
                kind,
                items: vec![item],
            });
            kind
        };
        self.paint_events.push(PaintEvent::Drop {
            cell,
            kind: actual_kind,
            item,
        });
    }

    pub fn add_mob(&mut self, mob: PaintMob) {
        self.mark_mob(mob.cell());
        self.paint_events.push(PaintEvent::Mob(mob.clone()));
        self.mobs.push(mob);
    }

    pub fn plant(&mut self, seed: SeedKind, cell: usize) {
        if matches!(
            self.map.cells[cell],
            terrain::HIGH_GRASS
                | terrain::FURROWED_GRASS
                | terrain::EMPTY
                | terrain::EMBERS
                | terrain::EMPTY_DECO
        ) {
            self.map.cells[cell] = terrain::GRASS;
        }
        if !self.plants_enabled {
            return;
        }
        let plant = PaintPlant { cell, seed };
        if let Some(existing) = self.plants.iter_mut().find(|plant| plant.cell == cell) {
            *existing = plant;
        } else {
            self.plants.push(plant);
        }
        self.paint_events.push(PaintEvent::Plant(plant));
    }

    pub fn add_transition(&mut self, cell: usize, kind: TransitionKind) {
        let transition = PaintTransition { cell, kind };
        self.transitions.push(transition);
        self.paint_events.push(PaintEvent::Transition(transition));
    }

    #[must_use]
    pub fn entrance(&self) -> Option<usize> {
        self.transitions.iter().find_map(|transition| {
            matches!(
                transition.kind,
                TransitionKind::Surface | TransitionKind::RegularEntrance
            )
            .then_some(transition.cell)
        })
    }

    #[must_use]
    pub fn exit(&self) -> Option<usize> {
        self.transitions
            .iter()
            .find(|transition| transition.kind == TransitionKind::RegularExit)
            .map(|transition| transition.cell)
    }

    /// Signed `Arrays.hashCode(int[])`, used by the official parity oracle.
    #[must_use]
    pub fn java_map_hash(&self) -> i32 {
        self.map.cells.iter().fold(1_i32, |hash, &cell| {
            hash.wrapping_mul(31).wrapping_add(cell)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_size_uses_chasm_only_for_chasm_feeling() {
        let mut ordinary = Level::new(2, Feeling::None);
        ordinary.set_size(3, 2);
        assert_eq!(ordinary.map.cells, vec![terrain::WALL; 6]);

        let mut chasm = Level::new(2, Feeling::Chasm);
        chasm.set_size(3, 2);
        assert_eq!(chasm.map.cells, vec![terrain::CHASM; 6]);
        assert_eq!(chasm.tunnel_tile(), terrain::EMPTY_SP);
    }

    #[test]
    fn sewer_trap_constructor_properties_match_java_classes() {
        let (floor_one, weights) = sewer_trap_table(1);
        assert_eq!(weights, [1.0]);
        assert!(floor_one[0].avoids_hallways);
        assert!(!floor_one[0].can_be_hidden);

        let (later, weights) = sewer_trap_table(2);
        assert_eq!(later.len(), 11);
        assert_eq!(
            weights,
            [4.0, 4.0, 4.0, 4.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0]
        );
        assert!(later[3].avoids_hallways);
        assert!(later[10].avoids_hallways);
    }

    #[test]
    fn map_hash_matches_java_arrays_hash_code() {
        let mut level = Level::new(1, Feeling::None);
        level.set_size(2, 2);
        level.map.cells.copy_from_slice(&[1, 4, 29, 16]);
        assert_eq!(level.java_map_hash(), 958_071);
    }

    #[test]
    fn barren_land_keeps_grass_conversion_without_registering_a_plant() {
        let mut level = Level::new(1, Feeling::None);
        level.set_size(3, 3);
        let cell = 4;
        level.map.cells[cell] = terrain::EMPTY;
        level.plants_enabled = false;
        level.plant(SeedKind::Sungrass, cell);
        assert_eq!(level.map.cells[cell], terrain::GRASS);
        assert!(level.plants.is_empty());
        assert!(level.paint_events.is_empty());
    }

    #[test]
    fn planting_converts_java_plantable_terrain_to_grass() {
        let mut level = Level::new(1, Feeling::None);
        level.set_size(6, 1);
        level.map.cells.copy_from_slice(&[
            terrain::HIGH_GRASS,
            terrain::FURROWED_GRASS,
            terrain::EMPTY,
            terrain::EMBERS,
            terrain::EMPTY_DECO,
            terrain::WATER,
        ]);
        for cell in 0..6 {
            level.plant(SeedKind::Sungrass, cell);
        }
        assert_eq!(
            level.map.cells,
            [
                terrain::GRASS,
                terrain::GRASS,
                terrain::GRASS,
                terrain::GRASS,
                terrain::GRASS,
                terrain::WATER,
            ]
        );
    }
}
