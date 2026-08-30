//! Room graph primitives for Shattered Pixel Dungeon v3.3.8 regular levels.
//!
//! The upstream room graph is identity based (`ArrayList<Room>` and
//! `LinkedHashMap<Room, Door>`).  [`RoomId`] is the corresponding identity in
//! this data-oriented port, while `Vec` preserves both `ArrayList` and
//! `LinkedHashMap` insertion order.  Room rectangles are inclusive on their
//! right and bottom edges even though [`Rect`] itself is not; consequently the
//! room width and height helpers add one.
//!
//! Painting is deliberately outside this module.  The graph, dimensions,
//! point-specific connection rules, and door selection are complete enough
//! for a later painter to consume without changing RNG order.

// Public constructors enforce Java class invariants with assertions; callers
// use the typed factories below rather than supplying unchecked game data.
#![allow(clippy::missing_panics_doc)]

use crate::geometry::{Point, Rect};
use crate::java_math::{div_i32, rem_i32};
use crate::rng::RandomStack;

/// Stable identity of a room while a graph is being built.
pub type RoomId = usize;

/// `Room.ALL`, `LEFT`, `TOP`, `RIGHT`, and `BOTTOM` in declaration order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum Direction {
    All = 0,
    Left = 1,
    Top = 2,
    Right = 3,
    Bottom = 4,
}

/// `StandardRoom.SizeCategory` in Java declaration order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum SizeCategory {
    Normal = 0,
    Large = 1,
    Giant = 2,
}

impl SizeCategory {
    #[must_use]
    pub const fn min_dimension(self) -> i32 {
        match self {
            Self::Normal => 4,
            Self::Large => 10,
            Self::Giant => 14,
        }
    }

    #[must_use]
    pub const fn max_dimension(self) -> i32 {
        match self {
            Self::Normal => 10,
            Self::Large => 14,
            Self::Giant => 18,
        }
    }

    #[must_use]
    pub const fn room_value(self) -> i32 {
        self as i32 + 1
    }

    fn from_ordinal(ordinal: usize) -> Self {
        match ordinal {
            0 => Self::Normal,
            1 => Self::Large,
            2 => Self::Giant,
            _ => panic!("invalid StandardRoom.SizeCategory ordinal: {ordinal}"),
        }
    }
}

/// Standard-room classes selected by all five regular main-path regions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum StandardRoomKind {
    SewerPipe,
    Ring,
    WaterBridge,
    RegionDecoPatch,
    CircleBasin,
    RegionDecoLine,
    Segmented,
    Pillars,
    ChasmBridge,
    CellBlock,
    Cave,
    RegionDecoBridge,
    CavesFissure,
    CirclePit,
    CircleWall,
    Hallway,
    LibraryHall,
    LibraryRing,
    Statues,
    SegmentedLibrary,
    Ruins,
    Chasm,
    Skulls,
    Ritual,
    Plants,
    Aquarium,
    Platform,
    Burned,
    Fissure,
    GrassyGrave,
    Striped,
    Study,
    SuspiciousChest,
    Minefield,
}

/// Concrete connection-room painter classes.  Their spatial behavior is
/// observable before painting, so the class is retained in the graph.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ConnectionRoomKind {
    Tunnel,
    Bridge,
    Perimeter,
    Walkway,
    RingTunnel,
    RingBridge,
    Maze,
}

/// Special rooms available to the regular-level room queue in v3.3.8.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum SpecialRoomKind {
    WeakFloor,
    Crypt,
    Pool,
    Armory,
    Sentry,
    Statue,
    CrystalVault,
    CrystalChoice,
    Sacrifice,
    Runestone,
    Garden,
    Library,
    Storage,
    Treasury,
    MagicWell,
    ToxicGas,
    MagicalFire,
    Traps,
    CrystalPath,
    /// Forced by `Dungeon.labRoomNeeded`; it is not part of the shuffled
    /// nineteen-room run queue.
    Laboratory,
    Pit,
    Shop,
    /// Mandatory single-connection room appended by every regular Halls floor.
    DemonSpawner,
}

/// Secret-room classes in `SecretRoom.ALL_SECRETS` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum SecretRoomKind {
    Garden,
    Laboratory,
    Library,
    Larder,
    Well,
    Runestone,
    Artillery,
    ChestChasm,
    Honeypot,
    Hoard,
    Maze,
    Summoning,
}

/// Quest rooms appended by the Wandmaker, Blacksmith, and Ambitious Imp
/// quest schedulers on regular main-path floors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum QuestRoomKind {
    MassGrave,
    RitualSite,
    RotGarden,
    Blacksmith,
    AmbitiousImp,
}

/// The Java runtime class information which affects room graph generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RoomKind {
    Entrance(StandardRoomKind),
    Exit(StandardRoomKind),
    Standard(StandardRoomKind),
    Connection(ConnectionRoomKind),
    Special(SpecialRoomKind),
    Secret(SecretRoomKind),
    Quest(QuestRoomKind),
}

/// `Room.Door.Type` declaration order is also its priority order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum DoorType {
    Empty,
    Tunnel,
    Water,
    Regular,
    Unlocked,
    Hidden,
    Barricade,
    Locked,
    Crystal,
    Wall,
}

/// A selected door point and its painter-assigned type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Door {
    pub point: Point,
    pub door_type: DoorType,
    pub type_locked: bool,
}

impl Door {
    #[must_use]
    pub const fn new(point: Point) -> Self {
        Self {
            point,
            door_type: DoorType::Empty,
            type_locked: false,
        }
    }

    /// Mirrors `Door.set`: types can only increase unless changes are locked.
    pub fn set_type(&mut self, door_type: DoorType) {
        if !self.type_locked && door_type > self.door_type {
            self.door_type = door_type;
        }
    }

    pub fn lock_type_changes(&mut self, lock: bool) {
        self.type_locked = lock;
    }
}

/// One insertion-ordered entry in `Room.connected`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomConnection {
    pub room: RoomId,
    pub door: Option<Door>,
}

/// Positioned room state used by builders and, later, painters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Room {
    pub bounds: Rect,
    pub kind: RoomKind,
    pub size_category: Option<SizeCategory>,
    /// Cached dynamic minimum used by `ShopRoom.minWidth/minHeight`.
    ///
    /// Ordinary rooms leave this unset.  The regular builder populates it at
    /// the first shop size query, after `findFreeSpace`, so stock generation
    /// consumes the parent RNG at the same point as Java.
    pub minimum_dimension_override: Option<i32>,
    /// Java `ArrayList` order, including no duplicates.
    pub neighbours: Vec<RoomId>,
    /// Java `LinkedHashMap` key order.
    pub connected: Vec<RoomConnection>,
    pub distance: i32,
    pub price: i32,
}

impl Room {
    /// Creates a non-standard room. Standard rooms must use one of the
    /// category-drawing constructors below so their constructor RNG draw is
    /// not accidentally omitted.
    #[must_use]
    pub fn new(kind: RoomKind) -> Self {
        assert!(
            !matches!(
                kind,
                RoomKind::Entrance(_)
                    | RoomKind::Exit(_)
                    | RoomKind::Standard(_)
                    | RoomKind::Quest(QuestRoomKind::RitualSite | QuestRoomKind::Blacksmith)
            ),
            "standard rooms require a size-category draw"
        );
        Self::with_category(kind, None)
    }

    fn with_category(kind: RoomKind, size_category: Option<SizeCategory>) -> Self {
        Self {
            bounds: Rect::default(),
            kind,
            size_category,
            minimum_dimension_override: None,
            neighbours: Vec::new(),
            connected: Vec::new(),
            distance: 0,
            price: 1,
        }
    }

    /// Mirrors construction of a concrete `StandardRoom`: its instance
    /// initializer invokes the virtual `setSizeCat()` exactly once.
    pub fn standard(kind: StandardRoomKind, rng: &mut RandomStack) -> Self {
        let mut room = Self::with_category(RoomKind::Standard(kind), None);
        assert!(room.set_size_category(0, 2, rng));
        room
    }

    fn entrance(kind: StandardRoomKind, rng: &mut RandomStack) -> Self {
        let mut room = Self::with_category(RoomKind::Entrance(kind), None);
        assert!(room.set_size_category(0, 2, rng));
        room
    }

    fn exit(kind: StandardRoomKind, rng: &mut RandomStack) -> Self {
        let mut room = Self::with_category(RoomKind::Exit(kind), None);
        assert!(room.set_size_category(0, 2, rng));
        room
    }

    #[must_use]
    pub fn connection(kind: ConnectionRoomKind) -> Self {
        Self::new(RoomKind::Connection(kind))
    }

    #[must_use]
    pub fn special(kind: SpecialRoomKind) -> Self {
        Self::new(RoomKind::Special(kind))
    }

    #[must_use]
    pub fn secret(kind: SecretRoomKind) -> Self {
        Self::new(RoomKind::Secret(kind))
    }

    /// Constructs one of the quest rooms which `initRooms` can append. The
    /// two `StandardRoom` subclasses perform their constructor category draw;
    /// the three `SpecialRoom` subclasses do not.
    pub fn quest(kind: QuestRoomKind, rng: &mut RandomStack) -> Self {
        let mut room = Self::with_category(RoomKind::Quest(kind), None);
        if matches!(kind, QuestRoomKind::RitualSite | QuestRoomKind::Blacksmith) {
            assert!(room.set_size_category(0, 2, rng));
        }
        room
    }

    #[must_use]
    pub const fn is_entrance(&self) -> bool {
        matches!(self.kind, RoomKind::Entrance(_))
    }

    #[must_use]
    pub const fn is_exit(&self) -> bool {
        matches!(self.kind, RoomKind::Exit(_))
    }

    #[must_use]
    pub const fn is_standard(&self) -> bool {
        matches!(
            self.kind,
            RoomKind::Entrance(_)
                | RoomKind::Exit(_)
                | RoomKind::Standard(_)
                | RoomKind::Quest(QuestRoomKind::RitualSite | QuestRoomKind::Blacksmith)
        )
    }

    #[must_use]
    pub const fn is_secret(&self) -> bool {
        matches!(self.kind, RoomKind::Secret(_))
    }

    #[must_use]
    pub const fn is_connection(&self) -> bool {
        matches!(self.kind, RoomKind::Connection(_))
    }

    #[must_use]
    pub const fn is_shop(&self) -> bool {
        matches!(self.kind, RoomKind::Special(SpecialRoomKind::Shop))
    }

    /// Stores the already-generated `ShopRoom.minWidth/minHeight` result.
    ///
    /// # Panics
    ///
    /// Panics when applied to a non-shop room or to a dimension outside the
    /// room's inherited maximum.
    pub fn set_shop_minimum_dimension(&mut self, dimension: i32) {
        assert!(
            self.is_shop(),
            "dynamic shop sizing belongs only to ShopRoom"
        );
        assert!(
            (7..=10).contains(&dimension),
            "canonical ShopRoom minimum must fit its inherited 7..=10 range"
        );
        self.minimum_dimension_override = Some(dimension);
    }

    /// Inclusive room width (`Room.width()`), unlike `Rect.width()`.
    #[must_use]
    pub const fn width(&self) -> i32 {
        self.bounds.width().wrapping_add(1)
    }

    /// Inclusive room height (`Room.height()`), unlike `Rect.height()`.
    #[must_use]
    pub const fn height(&self) -> i32 {
        self.bounds.height().wrapping_add(1)
    }

    pub fn set_empty(&mut self) {
        self.bounds.set_empty();
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.bounds.set_pos(x, y);
    }

    pub fn shift(&mut self, x: i32, y: i32) {
        self.bounds.shift(x, y);
    }

    fn resize(&mut self, width: i32, height: i32) {
        self.bounds.resize(width, height);
        // CircleBasinRoom cannot roll even inclusive dimensions. `resize` is
        // virtual in Java, so this also applies to size-limit truncation.
        if matches!(
            self.kind,
            RoomKind::Entrance(StandardRoomKind::CircleBasin)
                | RoomKind::Exit(StandardRoomKind::CircleBasin)
                | RoomKind::Standard(StandardRoomKind::CircleBasin)
        ) {
            if rem_i32(self.width(), 2) == 0 {
                self.bounds.right = self.bounds.right.wrapping_sub(1);
            }
            if rem_i32(self.height(), 2) == 0 {
                self.bounds.bottom = self.bounds.bottom.wrapping_sub(1);
            }
        }
        // LibraryRingRoom's giant layout requires even inclusive dimensions.
        if matches!(
            self.kind,
            RoomKind::Entrance(StandardRoomKind::LibraryRing)
                | RoomKind::Exit(StandardRoomKind::LibraryRing)
                | RoomKind::Standard(StandardRoomKind::LibraryRing)
        ) && self.size_category == Some(SizeCategory::Giant)
        {
            if rem_i32(self.width(), 2) == 1 {
                self.bounds.right = self.bounds.right.wrapping_sub(1);
            }
            if rem_i32(self.height(), 2) == 1 {
                self.bounds.bottom = self.bounds.bottom.wrapping_sub(1);
            }
        }
    }

    /// A point strictly inside the one-tile perimeter.
    #[must_use]
    pub const fn inside(&self, point: Point) -> bool {
        point.x > self.bounds.left
            && point.y > self.bounds.top
            && point.x < self.bounds.right
            && point.y < self.bounds.bottom
    }

    #[must_use]
    pub fn point_inside(&self, from: Point, amount: i32) -> Point {
        let mut point = from;
        if from.x == self.bounds.left {
            point.x = point.x.wrapping_add(amount);
        } else if from.x == self.bounds.right {
            point.x = point.x.wrapping_sub(amount);
        } else if from.y == self.bounds.top {
            point.y = point.y.wrapping_add(amount);
        } else if from.y == self.bounds.bottom {
            point.y = point.y.wrapping_sub(amount);
        }
        point
    }

    /// Mirrors `Room.center()`, including its conditional x-then-y draws.
    pub fn center(&self, rng: &mut RandomStack) -> Point {
        let x_jitter = if rem_i32(self.bounds.right.wrapping_sub(self.bounds.left), 2) == 1 {
            rng.int_bound(2)
        } else {
            0
        };
        let y_jitter = if rem_i32(self.bounds.bottom.wrapping_sub(self.bounds.top), 2) == 1 {
            rng.int_bound(2)
        } else {
            0
        };
        Point::new(
            div_i32(self.bounds.left.wrapping_add(self.bounds.right), 2).wrapping_add(x_jitter),
            div_i32(self.bounds.top.wrapping_add(self.bounds.bottom), 2).wrapping_add(y_jitter),
        )
    }

    /// `StandardRoom.setSizeCat(minOrdinal, maxOrdinal)`.
    pub fn set_size_category(
        &mut self,
        min_ordinal: usize,
        max_ordinal: usize,
        rng: &mut RandomStack,
    ) -> bool {
        if !self.is_standard() || min_ordinal > max_ordinal || min_ordinal >= 3 {
            return false;
        }
        let max_ordinal = max_ordinal.min(2);
        let mut probabilities = self.size_category_probabilities();
        for probability in &mut probabilities[..min_ordinal] {
            *probability = 0.0;
        }
        for probability in &mut probabilities[max_ordinal + 1..] {
            *probability = 0.0;
        }
        let Some(ordinal) = rng.chances(&probabilities) else {
            return false;
        };
        self.size_category = Some(SizeCategory::from_ordinal(ordinal));
        true
    }

    /// The overload which assumes `roomValue == ordinal + 1`.
    pub fn set_size_category_for_value(
        &mut self,
        max_room_value: i32,
        rng: &mut RandomStack,
    ) -> bool {
        if max_room_value <= 0 {
            return false;
        }
        self.set_size_category(
            0,
            usize::try_from(max_room_value - 1).expect("positive room value"),
            rng,
        )
    }

    #[must_use]
    pub fn size_category_probabilities(&self) -> [f32; 3] {
        let standard_kind = match self.kind {
            RoomKind::Entrance(kind) | RoomKind::Exit(kind) | RoomKind::Standard(kind) => kind,
            RoomKind::Quest(QuestRoomKind::RitualSite | QuestRoomKind::Blacksmith) => {
                return [1.0, 0.0, 0.0];
            }
            _ => return [0.0; 3],
        };

        if matches!(self.kind, RoomKind::Entrance(_) | RoomKind::Exit(_)) {
            match standard_kind {
                StandardRoomKind::Ring
                | StandardRoomKind::CircleBasin
                | StandardRoomKind::CellBlock
                | StandardRoomKind::CircleWall
                | StandardRoomKind::LibraryRing
                | StandardRoomKind::Ritual => return [0.0, 1.0, 0.0],
                StandardRoomKind::Cave | StandardRoomKind::Chasm | StandardRoomKind::Ruins => {
                    return [2.0, 1.0, 0.0];
                }
                StandardRoomKind::CavesFissure
                | StandardRoomKind::Pillars
                | StandardRoomKind::Statues => return [3.0, 1.0, 0.0],
                _ => {}
            }
        }

        match standard_kind {
            StandardRoomKind::SewerPipe => [3.0, 2.0, 1.0],
            StandardRoomKind::Ring
            | StandardRoomKind::Segmented
            | StandardRoomKind::Pillars
            | StandardRoomKind::CavesFissure
            | StandardRoomKind::Statues => [9.0, 3.0, 1.0],
            StandardRoomKind::CircleBasin
            | StandardRoomKind::CellBlock
            | StandardRoomKind::CircleWall
            | StandardRoomKind::SegmentedLibrary
            | StandardRoomKind::Skulls => [0.0, 3.0, 1.0],
            StandardRoomKind::Cave
            | StandardRoomKind::CirclePit
            | StandardRoomKind::LibraryRing
            | StandardRoomKind::Ruins
            | StandardRoomKind::Chasm => [4.0, 2.0, 1.0],
            StandardRoomKind::RegionDecoBridge
            | StandardRoomKind::LibraryHall
            | StandardRoomKind::Striped
            | StandardRoomKind::Study => [2.0, 1.0, 0.0],
            StandardRoomKind::Plants | StandardRoomKind::Aquarium => [3.0, 1.0, 0.0],
            StandardRoomKind::Ritual | StandardRoomKind::Platform | StandardRoomKind::Fissure => {
                [6.0, 3.0, 1.0]
            }
            StandardRoomKind::Burned | StandardRoomKind::Minefield => [4.0, 1.0, 0.0],
            StandardRoomKind::WaterBridge
            | StandardRoomKind::RegionDecoPatch
            | StandardRoomKind::RegionDecoLine
            | StandardRoomKind::ChasmBridge
            | StandardRoomKind::Hallway
            | StandardRoomKind::GrassyGrave
            | StandardRoomKind::SuspiciousChest => [1.0, 0.0, 0.0],
        }
    }

    #[must_use]
    pub fn size_factor(&self) -> i32 {
        self.size_category.map_or(1, SizeCategory::room_value)
    }

    #[must_use]
    pub fn connection_weight(&self) -> i32 {
        self.size_factor().wrapping_mul(self.size_factor())
    }

    #[must_use]
    pub fn min_width(&self) -> i32 {
        match self.kind {
            RoomKind::Entrance(kind) | RoomKind::Exit(kind) | RoomKind::Standard(kind) => {
                let category = self
                    .size_category
                    .expect("a StandardRoom always has a size category");
                let base = category.min_dimension();
                let mut minimum = match kind {
                    StandardRoomKind::SewerPipe
                    | StandardRoomKind::Ring
                    | StandardRoomKind::Segmented
                    | StandardRoomKind::Pillars
                    | StandardRoomKind::CavesFissure
                    | StandardRoomKind::Statues
                    | StandardRoomKind::Skulls
                    | StandardRoomKind::LibraryHall
                    | StandardRoomKind::Aquarium
                    | StandardRoomKind::Study => base.max(7),
                    StandardRoomKind::RegionDecoPatch
                    | StandardRoomKind::WaterBridge
                    | StandardRoomKind::RegionDecoLine
                    | StandardRoomKind::Cave
                    | StandardRoomKind::RegionDecoBridge
                    | StandardRoomKind::ChasmBridge
                    | StandardRoomKind::Hallway
                    | StandardRoomKind::Chasm
                    | StandardRoomKind::Plants
                    | StandardRoomKind::Fissure
                    | StandardRoomKind::SuspiciousChest => base.max(5),
                    StandardRoomKind::CirclePit => base.max(8),
                    StandardRoomKind::LibraryRing | StandardRoomKind::Ritual => base.max(9),
                    StandardRoomKind::Platform => base.max(6),
                    StandardRoomKind::CircleBasin => base.wrapping_add(1),
                    _ => base,
                };
                if matches!(self.kind, RoomKind::Entrance(_) | RoomKind::Exit(_)) {
                    minimum = match kind {
                        StandardRoomKind::WaterBridge
                        | StandardRoomKind::RegionDecoPatch
                        | StandardRoomKind::RegionDecoLine
                        | StandardRoomKind::ChasmBridge
                        | StandardRoomKind::Cave
                        | StandardRoomKind::Chasm
                        | StandardRoomKind::Ruins => minimum.max(7),
                        StandardRoomKind::RegionDecoBridge => minimum.max(8),
                        StandardRoomKind::CircleWall => minimum.max(11),
                        StandardRoomKind::LibraryRing => minimum.max(13),
                        _ => minimum,
                    };
                }
                minimum
            }
            RoomKind::Connection(kind) => match kind {
                ConnectionRoomKind::RingTunnel | ConnectionRoomKind::RingBridge => 5,
                _ => 3,
            },
            RoomKind::Special(kind) => match kind {
                SpecialRoomKind::Pit
                | SpecialRoomKind::Pool
                | SpecialRoomKind::Runestone
                | SpecialRoomKind::Traps => 6,
                SpecialRoomKind::Sentry
                | SpecialRoomKind::CrystalVault
                | SpecialRoomKind::CrystalChoice
                | SpecialRoomKind::Sacrifice
                | SpecialRoomKind::ToxicGas
                | SpecialRoomKind::MagicalFire
                | SpecialRoomKind::CrystalPath => 7,
                SpecialRoomKind::Shop => self.minimum_dimension_override.unwrap_or(7),
                _ => 5,
            },
            RoomKind::Secret(kind) => match kind {
                SecretRoomKind::Library => 7,
                SecretRoomKind::Larder => 6,
                SecretRoomKind::ChestChasm => 8,
                SecretRoomKind::Maze => 14,
                _ => 5,
            },
            RoomKind::Quest(kind) => match kind {
                QuestRoomKind::MassGrave => 11,
                QuestRoomKind::RitualSite => self
                    .size_category
                    .expect("RitualSiteRoom has a size category")
                    .min_dimension()
                    .max(9),
                QuestRoomKind::RotGarden => 10,
                QuestRoomKind::Blacksmith => self
                    .size_category
                    .expect("BlacksmithRoom has a size category")
                    .min_dimension()
                    .max(8),
                QuestRoomKind::AmbitiousImp => 9,
            },
        }
    }

    #[must_use]
    pub fn max_width(&self) -> i32 {
        match self.kind {
            RoomKind::Entrance(_) | RoomKind::Exit(_) | RoomKind::Standard(_) => self
                .size_category
                .expect("a StandardRoom always has a size category")
                .max_dimension(),
            RoomKind::Connection(_) => 10,
            RoomKind::Special(kind) => match kind {
                SpecialRoomKind::Pit => 9,
                SpecialRoomKind::CrystalVault => 7,
                SpecialRoomKind::Traps => 8,
                _ => 10,
            },
            RoomKind::Secret(kind) => match kind {
                SecretRoomKind::ChestChasm => 9,
                SecretRoomKind::Maze => 18,
                SecretRoomKind::Summoning => 8,
                _ => 10,
            },
            RoomKind::Quest(kind) => match kind {
                QuestRoomKind::RitualSite | QuestRoomKind::Blacksmith => self
                    .size_category
                    .expect("quest StandardRoom has a size category")
                    .max_dimension(),
                QuestRoomKind::AmbitiousImp => 9,
                QuestRoomKind::MassGrave => 11,
                QuestRoomKind::RotGarden => 10,
            },
        }
    }

    #[must_use]
    pub fn min_height(&self) -> i32 {
        // Every graph-relevant override is symmetric except the fixed 11x10
        // v4.0.0 MassGraveRoom.
        match self.kind {
            RoomKind::Quest(QuestRoomKind::MassGrave) => 10,
            _ => self.min_width(),
        }
    }

    #[must_use]
    pub fn max_height(&self) -> i32 {
        match self.kind {
            RoomKind::Quest(QuestRoomKind::MassGrave) => 10,
            _ => self.max_width(),
        }
    }

    /// `Room.setSize()`.
    pub fn set_size(&mut self, rng: &mut RandomStack) -> bool {
        self.set_size_in_range(
            self.min_width(),
            self.max_width(),
            self.min_height(),
            self.max_height(),
            rng,
        )
    }

    pub fn force_size(&mut self, width: i32, height: i32, rng: &mut RandomStack) -> bool {
        self.set_size_in_range(width, width, height, height, rng)
    }

    /// `Room.setSizeWithLimit`, including its unconditional two size draws
    /// once the minimum-dimension check succeeds.
    pub fn set_size_with_limit(&mut self, width: i32, height: i32, rng: &mut RandomStack) -> bool {
        if width < self.min_width() || height < self.min_height() {
            return false;
        }
        let result = self.set_size(rng);
        debug_assert!(result);
        if self.width() > width || self.height() > height {
            self.resize(
                self.width().min(width).wrapping_sub(1),
                self.height().min(height).wrapping_sub(1),
            );
        }
        true
    }

    fn set_size_in_range(
        &mut self,
        min_width: i32,
        max_width: i32,
        min_height: i32,
        max_height: i32,
        rng: &mut RandomStack,
    ) -> bool {
        if min_width < self.min_width()
            || max_width > self.max_width()
            || min_height < self.min_height()
            || max_height > self.max_height()
            || min_width > max_width
            || min_height > max_height
        {
            return false;
        }
        self.resize(
            rng.normal_int_range(min_width, max_width).wrapping_sub(1),
            rng.normal_int_range(min_height, max_height).wrapping_sub(1),
        );
        true
    }

    #[must_use]
    pub fn min_connections(&self, direction: Direction) -> i32 {
        match self.kind {
            RoomKind::Connection(_) if direction == Direction::All => 2,
            _ if direction == Direction::All => 1,
            _ => 0,
        }
    }

    #[must_use]
    pub fn max_connections(&self, direction: Direction) -> i32 {
        match self.kind {
            RoomKind::Special(_)
            | RoomKind::Secret(_)
            | RoomKind::Quest(
                QuestRoomKind::MassGrave | QuestRoomKind::RotGarden | QuestRoomKind::AmbitiousImp,
            ) => 1,
            RoomKind::Connection(ConnectionRoomKind::Maze) => 2,
            // v4.0.0 BlacksmithRoom compares the direction ordinal against its
            // `top` coordinate (`direction == this.top`), not against TOP.
            RoomKind::Quest(QuestRoomKind::Blacksmith) if direction as i32 == self.bounds.top => 1,
            _ if direction == Direction::All => 16,
            _ => 4,
        }
    }

    /// Point-specific connection rules. RNG is intentionally explicit:
    /// `SentryRoom.canConnect` calls `center()` in a way that can consume a
    /// draw for the unused coordinate.
    pub fn can_connect_point(&self, point: Point, rng: &mut RandomStack) -> bool {
        // MassGraveRoom (v4.0.0) replaces the edge test entirely and draws the
        // unused `center()` y jitter on every call.
        if matches!(self.kind, RoomKind::Quest(QuestRoomKind::MassGrave)) {
            let center = self.center(rng);
            return point.x.wrapping_sub(center.x).abs() <= 2;
        }
        // BlacksmithRoom (v4.0.0) only admits its two top corners on the top
        // wall and nothing on the row below it.
        if matches!(self.kind, RoomKind::Quest(QuestRoomKind::Blacksmith)) {
            if point.y == self.bounds.top
                && point.x != self.bounds.left.wrapping_add(1)
                && point.x != self.bounds.right.wrapping_sub(1)
            {
                return false;
            }
            if point.y == self.bounds.top.wrapping_add(1) {
                return false;
            }
        }

        let on_vertical = point.x == self.bounds.left || point.x == self.bounds.right;
        let on_horizontal = point.y == self.bounds.top || point.y == self.bounds.bottom;
        if on_vertical == on_horizontal {
            return false;
        }

        match self.kind {
            RoomKind::Standard(StandardRoomKind::SewerPipe)
            | RoomKind::Secret(SecretRoomKind::Well) => {
                (point.x > self.bounds.left.wrapping_add(1)
                    && point.x < self.bounds.right.wrapping_sub(1))
                    || (point.y > self.bounds.top.wrapping_add(1)
                        && point.y < self.bounds.bottom.wrapping_sub(1))
            }
            RoomKind::Special(SpecialRoomKind::Sentry) => {
                if rem_i32(self.width(), 2) == 1 && point.x == self.center(rng).x {
                    return false;
                }
                if rem_i32(self.height(), 2) == 1 && point.y == self.center(rng).y {
                    return false;
                }
                true
            }
            RoomKind::Special(SpecialRoomKind::CrystalPath) => {
                #[allow(clippy::cast_precision_loss)]
                let center_x =
                    self.bounds.right as f32 - (self.width().wrapping_sub(1) as f32) / 2.0;
                #[allow(clippy::cast_precision_loss)]
                let center_y =
                    self.bounds.bottom as f32 - (self.height().wrapping_sub(1) as f32) / 2.0;
                #[allow(clippy::cast_precision_loss)]
                let x_distance = (point.x as f32 - center_x).abs();
                #[allow(clippy::cast_precision_loss)]
                let y_distance = (point.y as f32 - center_y).abs();
                x_distance < 1.0 || y_distance < 1.0
            }
            _ => true,
        }
    }

    #[must_use]
    pub fn connection_to(&self, room: RoomId) -> Option<&RoomConnection> {
        self.connected.iter().find(|entry| entry.room == room)
    }

    pub fn connection_to_mut(&mut self, room: RoomId) -> Option<&mut RoomConnection> {
        self.connected.iter_mut().find(|entry| entry.room == room)
    }
}

const STANDARD_ROOM_CLASSES: [StandardRoomKind; 35] = [
    StandardRoomKind::SewerPipe,
    StandardRoomKind::Ring,
    StandardRoomKind::WaterBridge,
    StandardRoomKind::RegionDecoPatch,
    StandardRoomKind::CircleBasin,
    StandardRoomKind::RegionDecoLine,
    StandardRoomKind::Segmented,
    StandardRoomKind::Pillars,
    StandardRoomKind::ChasmBridge,
    StandardRoomKind::CellBlock,
    StandardRoomKind::Cave,
    StandardRoomKind::RegionDecoBridge,
    StandardRoomKind::CavesFissure,
    StandardRoomKind::CirclePit,
    StandardRoomKind::CircleWall,
    StandardRoomKind::Hallway,
    StandardRoomKind::LibraryHall,
    StandardRoomKind::LibraryRing,
    StandardRoomKind::Statues,
    StandardRoomKind::SegmentedLibrary,
    StandardRoomKind::Ruins,
    StandardRoomKind::RegionDecoPatch,
    StandardRoomKind::Chasm,
    StandardRoomKind::Skulls,
    StandardRoomKind::Ritual,
    StandardRoomKind::Plants,
    StandardRoomKind::Aquarium,
    StandardRoomKind::Platform,
    StandardRoomKind::Burned,
    StandardRoomKind::Fissure,
    StandardRoomKind::GrassyGrave,
    StandardRoomKind::Striped,
    StandardRoomKind::Study,
    StandardRoomKind::SuspiciousChest,
    StandardRoomKind::Minefield,
];

fn standard_room_probabilities(depth: u32) -> [f32; 35] {
    assert!((1..=26).contains(&depth), "regular depth must be 1..=26");
    let mut probabilities = [0.0; 35];
    match depth {
        1..=5 => probabilities[..5].copy_from_slice(&[16.0, 8.0, 8.0, 4.0, 4.0]),
        6..=10 => probabilities[5..10].copy_from_slice(&[10.0, 10.0, 10.0, 5.0, 5.0]),
        11..=15 => probabilities[10..15].copy_from_slice(&[16.0, 8.0, 8.0, 4.0, 4.0]),
        16..=20 => probabilities[15..20].copy_from_slice(&[10.0, 10.0, 10.0, 5.0, 5.0]),
        21..=26 => probabilities[20..25].copy_from_slice(&[10.0, 10.0, 10.0, 5.0, 5.0]),
        _ => unreachable!(),
    }
    match depth {
        1 => {
            probabilities[25..]
                .copy_from_slice(&[1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
        }
        2..=4 | 6..=26 => probabilities[25..].fill(1.0),
        5 => probabilities[4] = 0.0,
        _ => unreachable!(),
    }
    probabilities
}

/// Selects and constructs a standard room using the exact v3.3.8 class table
/// for Sewers, Prison, Caves, City, and Halls.
pub fn create_standard_room(depth: u32, rng: &mut RandomStack) -> Room {
    let probabilities = standard_room_probabilities(depth);
    let index = rng
        .chances(&probabilities)
        .expect("standard-room table has positive weight");
    Room::standard(STANDARD_ROOM_CLASSES[index], rng)
}

fn transition_room_classes(depth: u32) -> [StandardRoomKind; 4] {
    match depth {
        1..=5 => [
            StandardRoomKind::WaterBridge,
            StandardRoomKind::RegionDecoPatch,
            StandardRoomKind::Ring,
            StandardRoomKind::CircleBasin,
        ],
        6..=10 => [
            StandardRoomKind::RegionDecoLine,
            StandardRoomKind::ChasmBridge,
            StandardRoomKind::Pillars,
            StandardRoomKind::CellBlock,
        ],
        11..=15 => [
            StandardRoomKind::Cave,
            StandardRoomKind::RegionDecoBridge,
            StandardRoomKind::CavesFissure,
            StandardRoomKind::CircleWall,
        ],
        16..=20 => [
            StandardRoomKind::Hallway,
            StandardRoomKind::Statues,
            StandardRoomKind::LibraryHall,
            StandardRoomKind::LibraryRing,
        ],
        21..=26 => [
            StandardRoomKind::RegionDecoPatch,
            StandardRoomKind::Ruins,
            StandardRoomKind::Chasm,
            StandardRoomKind::Ritual,
        ],
        _ => panic!("regular transition-room table is only defined for depths 1..=26"),
    }
}

/// `EntranceRoom.createEntrance()` across all five regular regions.
pub fn create_entrance_room(depth: u32, rng: &mut RandomStack) -> Room {
    let probabilities: &[f32] = match depth {
        1..=2 => &[4.0, 3.0, 0.0, 0.0],
        3..=26 => &[4.0, 3.0, 2.0, 1.0],
        _ => panic!("regular entrance table is only defined for depths 1..=26"),
    };
    let index = rng
        .chances(probabilities)
        .expect("entrance-room table has positive weight");
    Room::entrance(transition_room_classes(depth)[index], rng)
}

/// `ExitRoom.createExit()` across all five regular regions.
pub fn create_exit_room(depth: u32, rng: &mut RandomStack) -> Room {
    let probabilities: &[f32] = match depth {
        1 => &[4.0, 3.0, 0.0, 0.0],
        2..=26 => &[4.0, 3.0, 2.0, 1.0],
        _ => panic!("regular exit table is only defined for depths 1..=26"),
    };
    let index = rng
        .chances(probabilities)
        .expect("exit-room table has positive weight");
    Room::exit(transition_room_classes(depth)[index], rng)
}

/// `ConnectionRoom.createRoom()` across all five regular regions.
pub fn create_connection_room(depth: u32, rng: &mut RandomStack) -> Room {
    let probabilities: &[f32] = match depth {
        1..=4 => &[20.0, 1.0, 0.0, 2.0, 2.0, 1.0],
        5 | 21 => &[20.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        6..=10 => &[0.0, 0.0, 22.0, 3.0, 0.0, 0.0],
        11..=15 => &[12.0, 0.0, 0.0, 5.0, 5.0, 3.0],
        16..=20 => &[0.0, 0.0, 18.0, 3.0, 3.0, 1.0],
        22..=26 => &[15.0, 4.0, 0.0, 2.0, 3.0, 2.0],
        _ => panic!("regular connection-room table is only defined for depths 1..=26"),
    };
    let index = rng
        .chances(probabilities)
        .expect("connection-room table has positive weight");
    let kind = [
        ConnectionRoomKind::Tunnel,
        ConnectionRoomKind::Bridge,
        ConnectionRoomKind::Perimeter,
        ConnectionRoomKind::Walkway,
        ConnectionRoomKind::RingTunnel,
        ConnectionRoomKind::RingBridge,
    ][index];
    Room::connection(kind)
}

/// Constructs graph metadata for a quest room selected by its quest state.
pub fn create_quest_room(kind: QuestRoomKind, rng: &mut RandomStack) -> Room {
    Room::quest(kind, rng)
}

#[must_use]
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

/// `Room.addNeigbour`, retaining the upstream misspelling only in this note.
pub fn add_neighbour(rooms: &mut [Room], first: RoomId, second: RoomId) -> bool {
    if rooms[first].neighbours.contains(&second) {
        return true;
    }
    let intersection = rooms[first].bounds.intersect(rooms[second].bounds);
    if (intersection.width() == 0 && intersection.height() >= 2)
        || (intersection.height() == 0 && intersection.width() >= 2)
    {
        let (first_room, second_room) = two_rooms_mut(rooms, first, second);
        first_room.neighbours.push(second);
        second_room.neighbours.push(first);
        true
    } else {
        false
    }
}

/// Counts connected rooms on one side using `LinkedHashMap` iteration order.
#[must_use]
pub fn current_connections(rooms: &[Room], room: RoomId, direction: Direction) -> i32 {
    if direction == Direction::All {
        return i32::try_from(rooms[room].connected.len()).unwrap_or(i32::MAX);
    }
    let mut total = 0_i32;
    for connection in &rooms[room].connected {
        let intersection = rooms[room].bounds.intersect(rooms[connection.room].bounds);
        let matches = match direction {
            Direction::Left => {
                intersection.width() == 0 && intersection.left == rooms[room].bounds.left
            }
            Direction::Top => {
                intersection.height() == 0 && intersection.top == rooms[room].bounds.top
            }
            Direction::Right => {
                intersection.width() == 0 && intersection.right == rooms[room].bounds.right
            }
            Direction::Bottom => {
                intersection.height() == 0 && intersection.bottom == rooms[room].bounds.bottom
            }
            Direction::All => unreachable!(),
        };
        if matches {
            total = total.wrapping_add(1);
        }
    }
    total
}

#[must_use]
pub fn remaining_connections(rooms: &[Room], room: RoomId, direction: Direction) -> i32 {
    if current_connections(rooms, room, Direction::All)
        >= rooms[room].max_connections(Direction::All)
    {
        0
    } else {
        rooms[room]
            .max_connections(direction)
            .wrapping_sub(current_connections(rooms, room, direction))
    }
}

/// `Room.canConnect(int direction)`, including the v4.0.0 `MassGraveRoom`
/// override that only admits connections through its bottom wall.
#[must_use]
pub fn can_connect_direction(rooms: &[Room], room: RoomId, direction: Direction) -> bool {
    remaining_connections(rooms, room, direction) > 0
        && (!matches!(rooms[room].kind, RoomKind::Quest(QuestRoomKind::MassGrave))
            || direction == Direction::Bottom)
}

/// `Room.canConnect(Room)`, including candidate point iteration and any draws
/// made by point-specific overrides.
pub fn can_connect_rooms(
    rooms: &[Room],
    first: RoomId,
    second: RoomId,
    rng: &mut RandomStack,
) -> bool {
    if matches!(
        rooms[first].kind,
        RoomKind::Quest(QuestRoomKind::Blacksmith)
    ) && rooms[second].is_exit()
    {
        return false;
    }
    if matches!(
        rooms[first].kind,
        RoomKind::Special(SpecialRoomKind::DemonSpawner)
    ) && rooms[second].is_exit()
    {
        return false;
    }
    if matches!(rooms[first].kind, RoomKind::Quest(QuestRoomKind::MassGrave)) {
        if rooms[second].is_entrance() {
            return false;
        }
        // MassGraveRoom requires at least three intervening rooms. Keep the
        // exact nested LinkedHashMap traversal rather than replacing it with
        // a general graph-distance calculation.
        for first_hop in &rooms[second].connected {
            if rooms[first_hop.room].is_entrance() {
                return false;
            }
            for second_hop in &rooms[first_hop.room].connected {
                if rooms[second_hop.room].is_entrance() {
                    return false;
                }
                for third_hop in &rooms[second_hop.room].connected {
                    if rooms[third_hop.room].is_entrance() {
                        return false;
                    }
                }
            }
        }
    }
    if (rooms[first].is_exit() && rooms[second].is_entrance())
        || (rooms[first].is_entrance() && rooms[second].is_exit())
    {
        return false;
    }

    let intersection = rooms[first].bounds.intersect(rooms[second].bounds);
    let mut found_point = false;
    for point in intersection.points() {
        if rooms[first].can_connect_point(point, rng) && rooms[second].can_connect_point(point, rng)
        {
            found_point = true;
            break;
        }
    }
    if !found_point {
        return false;
    }

    if intersection.width() == 0 && intersection.left == rooms[first].bounds.left {
        can_connect_direction(rooms, first, Direction::Left)
            && can_connect_direction(rooms, second, Direction::Right)
    } else if intersection.height() == 0 && intersection.top == rooms[first].bounds.top {
        can_connect_direction(rooms, first, Direction::Top)
            && can_connect_direction(rooms, second, Direction::Bottom)
    } else if intersection.width() == 0 && intersection.right == rooms[first].bounds.right {
        can_connect_direction(rooms, first, Direction::Right)
            && can_connect_direction(rooms, second, Direction::Left)
    } else if intersection.height() == 0 && intersection.bottom == rooms[first].bounds.bottom {
        can_connect_direction(rooms, first, Direction::Bottom)
            && can_connect_direction(rooms, second, Direction::Top)
    } else {
        false
    }
}

/// `Room.connect`, preserving connection insertion order in both rooms.
pub fn connect_rooms(
    rooms: &mut [Room],
    first: RoomId,
    second: RoomId,
    rng: &mut RandomStack,
) -> bool {
    let neighbours =
        rooms[first].neighbours.contains(&second) || add_neighbour(rooms, first, second);
    if !neighbours
        || rooms[first].connection_to(second).is_some()
        || !can_connect_rooms(rooms, first, second, rng)
    {
        return false;
    }
    let (first_room, second_room) = two_rooms_mut(rooms, first, second);
    first_room.connected.push(RoomConnection {
        room: second,
        door: None,
    });
    second_room.connected.push(RoomConnection {
        room: first,
        door: None,
    });
    true
}

/// `Room.clearConnections`, removing reverse entries without disturbing the
/// relative order of any surviving entries.
pub fn clear_connections(rooms: &mut [Room], room: RoomId) {
    let neighbours = rooms[room].neighbours.clone();
    for neighbour in neighbours {
        rooms[neighbour]
            .neighbours
            .retain(|&candidate| candidate != room);
    }
    rooms[room].neighbours.clear();

    let connected: Vec<RoomId> = rooms[room]
        .connected
        .iter()
        .map(|entry| entry.room)
        .collect();
    for other in connected {
        rooms[other].connected.retain(|entry| entry.room != room);
    }
    rooms[room].connected.clear();
}

/// Clears graph links on an initial room list between builder retries.
pub fn clear_all_connections(rooms: &mut [Room]) {
    for room in rooms {
        room.neighbours.clear();
        room.connected.clear();
    }
}

/// Reproduces `RegularPainter.placeDoors` for an explicit room traversal
/// order. The same door value is written to both sides of each edge.
///
/// # Errors
///
/// Returns the room pair when their intersection contains no mutually valid
/// door point, matching the upstream painter's reported generation failure.
pub fn place_doors_in_order(
    rooms: &mut [Room],
    order: &[RoomId],
    rng: &mut RandomStack,
) -> Result<(), (RoomId, RoomId)> {
    for &room in order {
        let neighbours: Vec<RoomId> = rooms[room]
            .connected
            .iter()
            .map(|entry| entry.room)
            .collect();
        for neighbour in neighbours {
            if rooms[room]
                .connection_to(neighbour)
                .and_then(|entry| entry.door)
                .is_some()
            {
                continue;
            }
            let intersection = rooms[room].bounds.intersect(rooms[neighbour].bounds);
            let mut candidates = Vec::new();
            for point in intersection.points() {
                if rooms[room].can_connect_point(point, rng)
                    && rooms[neighbour].can_connect_point(point, rng)
                {
                    candidates.push(point);
                }
            }
            if candidates.is_empty() {
                return Err((room, neighbour));
            }
            let candidate = usize::try_from(rng.int_bound(
                i32::try_from(candidates.len()).expect("door candidate count exceeds Java int"),
            ))
            .expect("Random.Int is non-negative");
            let door = Door::new(candidates[candidate]);
            let (first_room, second_room) = two_rooms_mut(rooms, room, neighbour);
            first_room
                .connection_to_mut(neighbour)
                .expect("forward connection disappeared")
                .door = Some(door);
            second_room
                .connection_to_mut(room)
                .expect("reverse connection disappeared")
                .door = Some(door);
        }
    }
    Ok(())
}

/// Convenience form for an unshuffled room list.
///
/// # Errors
///
/// Returns the room pair when a connected edge has no valid door point.
pub fn place_doors(rooms: &mut [Room], rng: &mut RandomStack) -> Result<(), (RoomId, RoomId)> {
    let order: Vec<RoomId> = (0..rooms.len()).collect();
    place_doors_in_order(rooms, &order, rng)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_dimensions_are_inclusive_and_connections_are_ordered() {
        let mut rng = RandomStack::with_base_seed(0);
        let mut rooms = vec![
            Room::connection(ConnectionRoomKind::Tunnel),
            Room::connection(ConnectionRoomKind::Tunnel),
            Room::connection(ConnectionRoomKind::Tunnel),
        ];
        for room in &mut rooms {
            assert!(room.force_size(5, 5, &mut rng));
        }
        rooms[0].set_position(0, 0);
        rooms[1].set_position(4, 0);
        rooms[2].set_position(0, 4);
        assert_eq!((rooms[0].width(), rooms[0].height()), (5, 5));

        assert!(connect_rooms(&mut rooms, 0, 2, &mut rng));
        assert!(connect_rooms(&mut rooms, 0, 1, &mut rng));
        assert_eq!(
            rooms[0]
                .connected
                .iter()
                .map(|entry| entry.room)
                .collect::<Vec<_>>(),
            [2, 1]
        );
        assert_eq!(current_connections(&rooms, 0, Direction::Right), 1);
        assert_eq!(current_connections(&rooms, 0, Direction::Bottom), 1);
    }

    #[test]
    fn entrance_and_exit_never_connect_directly() {
        let mut rng = RandomStack::with_base_seed(123);
        let mut rooms = vec![
            create_entrance_room(1, &mut rng),
            create_exit_room(1, &mut rng),
        ];
        assert!(rooms[0].force_size(7, 7, &mut rng));
        assert!(rooms[1].force_size(7, 7, &mut rng));
        rooms[1].set_position(6, 0);
        assert!(!connect_rooms(&mut rooms, 0, 1, &mut rng));
    }

    #[test]
    fn standard_constructor_and_category_redraw_match_java_reference() {
        // Captured from v3.3.8 `Random.pushGenerator(0x123456789abcdefL)`:
        // createRoom type, constructor category, then setSizeCat(3).
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        let mut room = create_standard_room(2, &mut rng);
        assert_eq!(room.kind, RoomKind::Standard(StandardRoomKind::Ring));
        assert_eq!(room.size_category, Some(SizeCategory::Normal));
        assert!(room.set_size_category_for_value(3, &mut rng));
        assert_eq!(room.size_category, Some(SizeCategory::Normal));
    }

    #[test]
    fn door_candidates_follow_rect_x_then_y_order() {
        let mut rng = RandomStack::with_base_seed(99);
        let mut rooms = vec![
            Room::connection(ConnectionRoomKind::Tunnel),
            Room::connection(ConnectionRoomKind::Tunnel),
        ];
        assert!(rooms[0].force_size(5, 5, &mut rng));
        assert!(rooms[1].force_size(5, 5, &mut rng));
        rooms[1].set_position(4, 0);
        assert!(connect_rooms(&mut rooms, 0, 1, &mut rng));
        place_doors(&mut rooms, &mut rng).unwrap();
        let first = rooms[0].connected[0].door.unwrap();
        let reverse = rooms[1].connected[0].door.unwrap();
        assert_eq!(first, reverse);
        assert_eq!(first.point.x, 4);
        assert!((1..=3).contains(&first.point.y));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One table-driven fixture pins every regional Java factory.
    fn regional_factories_match_pinned_java_vectors() {
        let expected = [
            (
                6,
                (StandardRoomKind::Segmented, SizeCategory::Normal, 8, 9),
                (StandardRoomKind::CellBlock, SizeCategory::Large, 10, 13),
                (StandardRoomKind::RegionDecoLine, SizeCategory::Normal, 8, 8),
                (ConnectionRoomKind::Perimeter, 6, 6),
            ),
            (
                11,
                (
                    StandardRoomKind::RegionDecoBridge,
                    SizeCategory::Normal,
                    6,
                    8,
                ),
                (StandardRoomKind::CircleWall, SizeCategory::Large, 11, 13),
                (StandardRoomKind::Cave, SizeCategory::Large, 11, 12),
                (ConnectionRoomKind::Tunnel, 6, 6),
            ),
            (
                16,
                (StandardRoomKind::LibraryHall, SizeCategory::Normal, 8, 9),
                (StandardRoomKind::LibraryRing, SizeCategory::Large, 13, 14),
                (StandardRoomKind::Hallway, SizeCategory::Normal, 7, 7),
                (ConnectionRoomKind::Perimeter, 6, 6),
            ),
            (
                21,
                (
                    StandardRoomKind::RegionDecoPatch,
                    SizeCategory::Normal,
                    6,
                    8,
                ),
                (StandardRoomKind::Ritual, SizeCategory::Large, 10, 13),
                (
                    StandardRoomKind::RegionDecoPatch,
                    SizeCategory::Normal,
                    8,
                    8,
                ),
                (ConnectionRoomKind::Tunnel, 6, 6),
            ),
        ];

        for (depth, standard, entrance, exit, connection) in expected {
            let mut rng = RandomStack::with_base_seed(0);
            rng.push(0x0123_4567_89ab_cdef);
            let mut standard_room = create_standard_room(depth, &mut rng);
            assert!(standard_room.set_size(&mut rng));
            assert_eq!(
                (
                    standard_room.kind,
                    standard_room.size_category,
                    standard_room.width(),
                    standard_room.height()
                ),
                (
                    RoomKind::Standard(standard.0),
                    Some(standard.1),
                    standard.2,
                    standard.3
                )
            );
            let mut entrance_room = create_entrance_room(depth, &mut rng);
            assert!(entrance_room.set_size(&mut rng));
            assert_eq!(
                (
                    entrance_room.kind,
                    entrance_room.size_category,
                    entrance_room.width(),
                    entrance_room.height()
                ),
                (
                    RoomKind::Entrance(entrance.0),
                    Some(entrance.1),
                    entrance.2,
                    entrance.3
                )
            );
            let mut exit_room = create_exit_room(depth, &mut rng);
            assert!(exit_room.set_size(&mut rng));
            assert_eq!(
                (
                    exit_room.kind,
                    exit_room.size_category,
                    exit_room.width(),
                    exit_room.height()
                ),
                (RoomKind::Exit(exit.0), Some(exit.1), exit.2, exit.3)
            );
            let mut connection_room = create_connection_room(depth, &mut rng);
            assert!(connection_room.set_size(&mut rng));
            assert_eq!(
                (
                    connection_room.kind,
                    connection_room.width(),
                    connection_room.height()
                ),
                (
                    RoomKind::Connection(connection.0),
                    connection.1,
                    connection.2
                )
            );
            assert_eq!(rng.int(), 18_341_899);
        }
    }

    #[test]
    fn quest_room_graph_metadata_matches_pinned_java_vector() {
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x0123_4567_89ab_cdef);
        let mut rooms = [
            create_quest_room(QuestRoomKind::MassGrave, &mut rng),
            create_quest_room(QuestRoomKind::RitualSite, &mut rng),
            create_quest_room(QuestRoomKind::RotGarden, &mut rng),
            create_quest_room(QuestRoomKind::Blacksmith, &mut rng),
            create_quest_room(QuestRoomKind::AmbitiousImp, &mut rng),
        ];
        for room in &mut rooms {
            assert!(room.set_size(&mut rng));
        }
        assert_eq!(
            rooms.map(|room| (
                room.kind,
                room.size_category,
                room.width(),
                room.height(),
                room.max_connections(Direction::All)
            )),
            [
                (RoomKind::Quest(QuestRoomKind::MassGrave), None, 11, 10, 1),
                (
                    RoomKind::Quest(QuestRoomKind::RitualSite),
                    Some(SizeCategory::Normal),
                    10,
                    9,
                    16
                ),
                (RoomKind::Quest(QuestRoomKind::RotGarden), None, 10, 10, 1),
                // Unplaced rooms sit at top == 0, so Java's
                // `direction == this.top` quirk reports one connection.
                (
                    RoomKind::Quest(QuestRoomKind::Blacksmith),
                    Some(SizeCategory::Normal),
                    9,
                    9,
                    1
                ),
                (RoomKind::Quest(QuestRoomKind::AmbitiousImp), None, 9, 9, 1),
            ]
        );
        assert_eq!(rng.int(), -504_935_820);
    }

    #[test]
    fn quest_room_connection_overrides_match_java_graph_rules() {
        let mut rng = RandomStack::with_base_seed(7);
        let mut rooms = vec![
            create_entrance_room(6, &mut rng),
            Room::connection(ConnectionRoomKind::Tunnel),
            Room::connection(ConnectionRoomKind::Tunnel),
            Room::connection(ConnectionRoomKind::Tunnel),
            create_quest_room(QuestRoomKind::MassGrave, &mut rng),
            create_exit_room(11, &mut rng),
            create_quest_room(QuestRoomKind::Blacksmith, &mut rng),
        ];
        rooms[3].connected.push(RoomConnection {
            room: 2,
            door: None,
        });
        rooms[2].connected.push(RoomConnection {
            room: 1,
            door: None,
        });
        rooms[1].connected.push(RoomConnection {
            room: 0,
            door: None,
        });
        assert!(!can_connect_rooms(&rooms, 4, 3, &mut rng));
        assert!(!can_connect_rooms(&rooms, 6, 5, &mut rng));

        rooms[1].connected.clear();
        rooms[4].bounds = Rect::new(4, 0, 10, 6);
        // v4.0.0 MassGraveRoom only connects through its bottom wall.
        rooms[3].bounds = Rect::new(0, 0, 4, 4);
        assert!(!can_connect_rooms(&rooms, 4, 3, &mut rng));
        rooms[3].bounds = Rect::new(4, 6, 8, 10);
        assert!(can_connect_rooms(&rooms, 4, 3, &mut rng));
    }
}
