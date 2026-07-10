//! Exact v3.3.8 painters for the twelve regular `SecretRoom` classes.
//!
//! `SecretLaboratoryRoom` and `SecretLibraryRoom` select from
//! `HashMap<Class, Float>`. Their iteration order depends on JVM class identity
//! hashes, so it is explicit [`SecretRuntimeProfile`] data. The default profile
//! is the pinned Temurin 21.0.11 oracle profile; callers reproducing a different
//! Java/ART runtime can provide that runtime's observed order without changing
//! RNG or painter logic.

#![allow(
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

use std::fmt;

use crate::catalog::Effect;
use crate::generator::{self, BombKind, FoodKind, GeneratedItem, GeneratorError, StoneKind};
use crate::geometry::{GridMap, PathFinder, Point, Rect, painter as draw, terrain};
use crate::java_math::{div_i32, round_f32};
use crate::level::Level;
use crate::model::{Accessibility, ItemSource, WorldItem};
use crate::painter::{generate_patch, set_shared_door_type};
use crate::rng::{FastBound, RandomStack};
use crate::room::{DoorType, Room, RoomId, RoomKind, SecretRoomKind};
use crate::run::{GeneratorCategory, GeneratorState, PotionKind, ScrollKind};
use crate::special_equipment::{SpecialItem, SpecialPrizeContext};

/// Items constructed or generated directly by a secret-room painter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretItem {
    Generated(GeneratedItem),
    EnergyCrystal { quantity: i32 },
    ChargrilledMeat,
    GoldenKey { depth: i32 },
    ShatteredPot,
    Honeypot,
}

impl From<GeneratedItem> for SecretItem {
    fn from(item: GeneratedItem) -> Self {
        Self::Generated(item)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SecretHeapKind {
    Heap,
    Chest,
    LockedChest,
    Skeleton,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SecretPlantKind {
    Starflower,
    Seedpod,
    Dewcatcher,
    BlandfruitBush,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SecretBlobKind {
    Foliage,
    Alchemy,
    WaterOfAwareness,
    WaterOfHealth,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SecretTrapKind {
    Rockfall,
    PoisonDart,
    Disintegration,
    Summoning,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SecretMobKind {
    Bee { level: i32, hit_points: i32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecretReward {
    pub item: SecretItem,
    pub accessibility: Accessibility,
}

/// Operation-ordered generation-visible effects emitted by secret painters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretPaintEvent {
    Drop {
        cell: usize,
        heap: SecretHeapKind,
        haunted: bool,
        reward: SecretReward,
    },
    Mob {
        cell: usize,
        kind: SecretMobKind,
    },
    SpawnItem(SecretItem),
    Plant {
        cell: usize,
        kind: SecretPlantKind,
    },
    Blob {
        cell: usize,
        kind: SecretBlobKind,
        volume: i32,
    },
    Trap {
        cell: usize,
        kind: SecretTrapKind,
        visible: bool,
        active: bool,
    },
}

pub trait SecretLevelContext {
    fn depth(&self) -> u32;
    fn map(&self) -> &GridMap;
    fn map_mut(&mut self) -> &mut GridMap;
    fn has_heap(&self, cell: usize) -> bool;
    fn has_mob(&self, cell: usize) -> bool;
    fn has_plant(&self, cell: usize) -> bool;
    fn record(&mut self, event: SecretPaintEvent);
}

/// Adapter for the shared data-only regular [`Level`].
pub struct SecretLevelPaintContext<'a> {
    pub level: &'a mut Level,
    pub events: &'a mut Vec<SecretPaintEvent>,
}

impl<'a> SecretLevelPaintContext<'a> {
    #[must_use]
    pub const fn new(level: &'a mut Level, events: &'a mut Vec<SecretPaintEvent>) -> Self {
        Self { level, events }
    }
}

impl SecretLevelContext for SecretLevelPaintContext<'_> {
    fn depth(&self) -> u32 {
        self.level.depth
    }

    fn map(&self) -> &GridMap {
        &self.level.map
    }

    fn map_mut(&mut self) -> &mut GridMap {
        &mut self.level.map
    }

    fn has_heap(&self, cell: usize) -> bool {
        self.level.heap_cells[cell]
    }

    fn has_mob(&self, cell: usize) -> bool {
        self.level.mob_cells[cell]
    }

    fn has_plant(&self, cell: usize) -> bool {
        self.level.plants.iter().any(|plant| plant.cell == cell)
            || self.events.iter().any(
                |event| matches!(event, SecretPaintEvent::Plant { cell: at, .. } if *at == cell),
            )
    }

    fn record(&mut self, event: SecretPaintEvent) {
        match &event {
            SecretPaintEvent::Drop { cell, .. } => self.level.mark_heap(*cell),
            SecretPaintEvent::Mob { cell, .. } => self.level.mark_mob(*cell),
            SecretPaintEvent::Plant { cell, .. } => {
                if matches!(
                    self.level.map.cells[*cell],
                    terrain::HIGH_GRASS
                        | terrain::FURROWED_GRASS
                        | terrain::EMPTY
                        | terrain::EMBERS
                        | terrain::EMPTY_DECO
                ) {
                    self.level.map.cells[*cell] = terrain::GRASS;
                }
            }
            SecretPaintEvent::SpawnItem(_)
            | SecretPaintEvent::Blob { .. }
            | SecretPaintEvent::Trap { .. } => {}
        }
        self.events.push(event);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretGeneratorRequest {
    Overall,
    DefaultsOverall,
    DefaultsCategory(GeneratorCategory),
    Weapon { floor_set: i32, use_defaults: bool },
    Armor { floor_set: i32 },
    Missile { floor_set: i32, use_defaults: bool },
    Bomb,
    Gold,
}

pub trait SecretGeneratorContext {
    /// # Errors
    ///
    /// Returns a version-pinned generator invariant error.
    fn generate_secret(
        &mut self,
        request: SecretGeneratorRequest,
        depth: i32,
        random: &mut RandomStack,
    ) -> Result<GeneratedItem, GeneratorError>;
}

impl SecretGeneratorContext for GeneratorState {
    fn generate_secret(
        &mut self,
        request: SecretGeneratorRequest,
        depth: i32,
        random: &mut RandomStack,
    ) -> Result<GeneratedItem, GeneratorError> {
        match request {
            SecretGeneratorRequest::Overall => generator::random(random, self, depth),
            SecretGeneratorRequest::DefaultsOverall => {
                generator::random_using_defaults_overall(random, self, depth)
            }
            SecretGeneratorRequest::DefaultsCategory(category) => {
                generator::random_using_defaults(random, self, category, depth)
            }
            SecretGeneratorRequest::Weapon {
                floor_set,
                use_defaults,
            } => generator::random_weapon(random, self, floor_set, use_defaults)
                .map(GeneratedItem::Equipment),
            SecretGeneratorRequest::Armor { floor_set } => {
                generator::random_armor(random, floor_set).map(GeneratedItem::Equipment)
            }
            SecretGeneratorRequest::Missile {
                floor_set,
                use_defaults,
            } => generator::random_missile(random, self, floor_set, use_defaults)
                .map(GeneratedItem::Missile),
            SecretGeneratorRequest::Bomb => Ok(generator::random_bomb(random)),
            SecretGeneratorRequest::Gold => Ok(generator::random_gold(random, depth)),
        }
    }
}

pub trait SecretPrizeContext: SpecialPrizeContext {
    fn exotic_crystals_level(&self) -> i32;
    fn trap_mechanism_level(&self) -> i32;
}

impl SecretPrizeContext for crate::special_equipment::PrizePool {
    fn exotic_crystals_level(&self) -> i32 {
        -1
    }

    fn trap_mechanism_level(&self) -> i32 {
        -1
    }
}

/// JVM-dependent orders used by the two class-keyed weighted maps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecretRuntimeProfile {
    pub laboratory_potions: [PotionKind; 11],
    pub library_scrolls: [ScrollKind; 11],
}

impl SecretRuntimeProfile {
    /// Eclipse Temurin 21.0.11 `HotSpot`, matching `SecretRoomsOracle.java`.
    pub const TEMURIN_21_0_11: Self = Self {
        laboratory_potions: [
            PotionKind::ParalyticGas,
            PotionKind::Haste,
            PotionKind::Purity,
            PotionKind::Frost,
            PotionKind::Levitation,
            PotionKind::MindVision,
            PotionKind::LiquidFlame,
            PotionKind::Experience,
            PotionKind::ToxicGas,
            PotionKind::Healing,
            PotionKind::Invisibility,
        ],
        library_scrolls: [
            ScrollKind::Terror,
            ScrollKind::Transmutation,
            ScrollKind::Lullaby,
            ScrollKind::Rage,
            ScrollKind::Teleportation,
            ScrollKind::Recharging,
            ScrollKind::Retribution,
            ScrollKind::MirrorImage,
            ScrollKind::MagicMapping,
            ScrollKind::Identify,
            ScrollKind::RemoveCurse,
        ],
    };
}

impl Default for SecretRuntimeProfile {
    fn default() -> Self {
        Self::TEMURIN_21_0_11
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SecretPaintReport {
    pub searchable_items: Vec<WorldItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretPaintOutcome {
    NotHandled,
    Painted(SecretPaintReport),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretPaintError {
    MissingRoom(RoomId),
    MissingEntrance(RoomId),
    MissingReverseEntrance { room: RoomId, neighbour: RoomId },
    Generator(GeneratorError),
}

impl fmt::Display for SecretPaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::MissingRoom(room) => write!(formatter, "room {room} does not exist"),
            Self::MissingEntrance(room) => write!(formatter, "secret room {room} has no entrance"),
            Self::MissingReverseEntrance { room, neighbour } => write!(
                formatter,
                "secret room {room} has no reverse entrance from {neighbour}"
            ),
            Self::Generator(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SecretPaintError {}

impl From<GeneratorError> for SecretPaintError {
    fn from(error: GeneratorError) -> Self {
        Self::Generator(error)
    }
}

#[derive(Clone, Copy)]
struct Entrance {
    neighbour: RoomId,
    point: Point,
}

struct PaintInputs<'a, L, G, P> {
    level: &'a mut L,
    generator: &'a mut G,
    prizes: &'a mut P,
    rooms: &'a mut [Room],
    room: RoomId,
    bounds: Rect,
    entrance: Entrance,
    random: &'a mut RandomStack,
    profile: &'a SecretRuntimeProfile,
    report: SecretPaintReport,
}

impl<L, G, P> PaintInputs<'_, L, G, P>
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fn depth_i32(&self) -> i32 {
        i32::try_from(self.level.depth()).expect("dungeon depth fits Java int")
    }

    fn floor_set(&self) -> i32 {
        self.depth_i32() / 5
    }

    fn generate(&mut self, request: SecretGeneratorRequest) -> Result<SecretItem, GeneratorError> {
        self.generator
            .generate_secret(request, self.depth_i32(), self.random)
            .map(SecretItem::Generated)
    }

    fn set_door(&mut self, door_type: DoorType) {
        set_shared_door_type(self.rooms, self.room, self.entrance.neighbour, door_type);
    }

    fn spawn_generated(&mut self, item: GeneratedItem) {
        self.prizes.add_item_to_spawn(SpecialItem::generated(item));
        self.level
            .record(SecretPaintEvent::SpawnItem(SecretItem::Generated(item)));
    }

    fn drop(
        &mut self,
        cell: usize,
        heap: SecretHeapKind,
        item: SecretItem,
        haunted: bool,
        source: ItemSource,
    ) {
        let accessibility = Accessibility::Independent;
        self.record_searchable(item, source, accessibility);
        self.level.record(SecretPaintEvent::Drop {
            cell,
            heap,
            haunted,
            reward: SecretReward {
                item,
                accessibility,
            },
        });
    }

    fn record_searchable(
        &mut self,
        item: SecretItem,
        source: ItemSource,
        accessibility: Accessibility,
    ) {
        let depth = u8::try_from(self.level.depth()).expect("dungeon depth fits search model");
        if let Some(item) = searchable_world_item(item, depth, source, accessibility) {
            self.report.searchable_items.push(item);
        }
    }
}

/// Paint one regular secret room with the pinned Temurin 21 runtime profile.
///
/// # Errors
///
/// Returns a graph or generator invariant error for a handled secret room.
pub fn paint_secret_room<L, G, P>(
    level: &mut L,
    rooms: &mut [Room],
    room: RoomId,
    generator: &mut G,
    prizes: &mut P,
    random: &mut RandomStack,
) -> Result<SecretPaintOutcome, SecretPaintError>
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    paint_secret_room_with_profile(
        level,
        rooms,
        room,
        generator,
        prizes,
        random,
        &SecretRuntimeProfile::TEMURIN_21_0_11,
    )
}

/// Paint one regular secret room using an explicit JVM class-map profile.
///
/// # Errors
///
/// Returns a graph or generator invariant error for a handled secret room.
pub fn paint_secret_room_with_profile<L, G, P>(
    level: &mut L,
    rooms: &mut [Room],
    room: RoomId,
    generator: &mut G,
    prizes: &mut P,
    random: &mut RandomStack,
    profile: &SecretRuntimeProfile,
) -> Result<SecretPaintOutcome, SecretPaintError>
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    let selected = rooms.get(room).ok_or(SecretPaintError::MissingRoom(room))?;
    let RoomKind::Secret(kind) = selected.kind else {
        return Ok(SecretPaintOutcome::NotHandled);
    };
    let bounds = selected.bounds;
    let connection = selected
        .connected
        .first()
        .ok_or(SecretPaintError::MissingEntrance(room))?;
    let entrance = Entrance {
        neighbour: connection.room,
        point: connection
            .door
            .ok_or(SecretPaintError::MissingEntrance(room))?
            .point,
    };
    if rooms
        .get(entrance.neighbour)
        .and_then(|neighbour| neighbour.connection_to(room))
        .and_then(|connection| connection.door)
        .is_none()
    {
        return Err(SecretPaintError::MissingReverseEntrance {
            room,
            neighbour: entrance.neighbour,
        });
    }

    let mut inputs = PaintInputs {
        level,
        generator,
        prizes,
        rooms,
        room,
        bounds,
        entrance,
        random,
        profile,
        report: SecretPaintReport::default(),
    };
    match kind {
        SecretRoomKind::Garden => paint_garden(&mut inputs),
        SecretRoomKind::Laboratory => paint_laboratory(&mut inputs),
        SecretRoomKind::Library => paint_library(&mut inputs),
        SecretRoomKind::Larder => paint_larder(&mut inputs),
        SecretRoomKind::Well => paint_well(&mut inputs),
        SecretRoomKind::Runestone => paint_runestone(&mut inputs)?,
        SecretRoomKind::Artillery => paint_artillery(&mut inputs)?,
        SecretRoomKind::ChestChasm => paint_chest_chasm(&mut inputs)?,
        SecretRoomKind::Honeypot => paint_honeypot(&mut inputs)?,
        SecretRoomKind::Hoard => paint_hoard(&mut inputs)?,
        SecretRoomKind::Maze => paint_maze(&mut inputs)?,
        SecretRoomKind::Summoning => paint_summoning(&mut inputs)?,
    }
    Ok(SecretPaintOutcome::Painted(inputs.report))
}

fn inclusive_width(bounds: Rect) -> i32 {
    bounds.right.wrapping_sub(bounds.left).wrapping_add(1)
}

fn inclusive_height(bounds: Rect) -> i32 {
    bounds.bottom.wrapping_sub(bounds.top).wrapping_add(1)
}

fn fill_room<L: SecretLevelContext>(level: &mut L, bounds: Rect, value: i32) {
    draw::fill(
        level.map_mut(),
        bounds.left,
        bounds.top,
        inclusive_width(bounds),
        inclusive_height(bounds),
        value,
    );
}

fn fill_room_margin<L: SecretLevelContext>(level: &mut L, bounds: Rect, margin: i32, value: i32) {
    draw::fill(
        level.map_mut(),
        bounds.left.wrapping_add(margin),
        bounds.top.wrapping_add(margin),
        inclusive_width(bounds).wrapping_sub(margin.wrapping_mul(2)),
        inclusive_height(bounds).wrapping_sub(margin.wrapping_mul(2)),
        value,
    );
}

fn room_center(bounds: Rect, random: &mut RandomStack) -> Point {
    let x_jitter = if bounds.right.wrapping_sub(bounds.left) % 2 == 1 {
        random.int_bound(2)
    } else {
        0
    };
    let y_jitter = if bounds.bottom.wrapping_sub(bounds.top) % 2 == 1 {
        random.int_bound(2)
    } else {
        0
    };
    Point::new(
        div_i32(bounds.left.wrapping_add(bounds.right), 2).wrapping_add(x_jitter),
        div_i32(bounds.top.wrapping_add(bounds.bottom), 2).wrapping_add(y_jitter),
    )
}

fn random_room_point(bounds: Rect, random: &mut RandomStack) -> Point {
    Point::new(
        random.int_range(bounds.left + 1, bounds.right - 1),
        random.int_range(bounds.top + 1, bounds.bottom - 1),
    )
}

fn point_to_cell<L: SecretLevelContext>(level: &L, point: Point) -> usize {
    level.map().point_to_cell(point)
}

fn searchable_world_item(
    item: SecretItem,
    depth: u8,
    source: ItemSource,
    accessibility: Accessibility,
) -> Option<WorldItem> {
    let SecretItem::Generated(item) = item else {
        return None;
    };
    item.searchable_equipment().map(|equipment| {
        WorldItem::from_equipment_roll(equipment.item, equipment.roll, depth, source, accessibility)
    })
}

fn generated_is_cursed(item: GeneratedItem) -> bool {
    match item {
        GeneratedItem::Equipment(item) => item.roll.cursed,
        GeneratedItem::Missile(item) => item.roll.cursed,
        GeneratedItem::Ring(item) => item.roll.cursed,
        GeneratedItem::Artifact(item) => item.cursed,
        GeneratedItem::Food(_)
        | GeneratedItem::Potion { .. }
        | GeneratedItem::Seed(_)
        | GeneratedItem::Scroll { .. }
        | GeneratedItem::Stone(_)
        | GeneratedItem::Gold { .. }
        | GeneratedItem::Trinket(_)
        | GeneratedItem::Bomb(_)
        | GeneratedItem::TippedDart { .. } => false,
    }
}

fn clear_curse_effect_and_mark_clean(item: &mut SecretItem) {
    let SecretItem::Generated(GeneratedItem::Equipment(equipment)) = item else {
        panic!("SecretMazeRoom prize must be melee weapon or armor");
    };
    if equipment.roll.effect.is_some_and(|effect| match effect {
        Effect::Weapon(effect) => effect.is_curse(),
        Effect::Armor(effect) => effect.is_curse(),
    }) {
        equipment.roll.effect = None;
    }
    equipment.roll.cursed = false;
}

fn upgrade_equipment(item: &mut SecretItem) {
    let SecretItem::Generated(GeneratedItem::Equipment(equipment)) = item else {
        panic!("SecretMazeRoom prize must be equipment");
    };
    equipment.roll.upgrade = equipment.roll.upgrade.wrapping_add(1);
}

fn paint_garden<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::GRASS);
    let patch_width = inclusive_width(inputs.bounds) - 2;
    let grass = generate_patch(
        patch_width,
        inclusive_height(inputs.bounds) - 2,
        0.5,
        0,
        true,
        inputs.random,
    );
    for y in inputs.bounds.top + 1..inputs.bounds.bottom {
        for x in inputs.bounds.left + 1..inputs.bounds.right {
            let patch_cell = usize::try_from(
                (x - inputs.bounds.left - 1) + (y - inputs.bounds.top - 1) * patch_width,
            )
            .expect("garden patch coordinate is non-negative");
            if grass[patch_cell] {
                draw::set_xy(inputs.level.map_mut(), x, y, terrain::HIGH_GRASS);
            }
        }
    }
    inputs.set_door(DoorType::Hidden);

    plant_at_random(inputs, SecretPlantKind::Starflower);
    plant_at_random(inputs, SecretPlantKind::Seedpod);
    plant_at_random(inputs, SecretPlantKind::Dewcatcher);
    let last = if inputs.random.int_bound(2) == 0 {
        SecretPlantKind::Seedpod
    } else {
        SecretPlantKind::Dewcatcher
    };
    plant_at_random(inputs, last);

    for y in inputs.bounds.top + 1..inputs.bounds.bottom {
        for x in inputs.bounds.left + 1..inputs.bounds.right {
            let cell = inputs.level.map().cell(x, y);
            inputs.level.record(SecretPaintEvent::Blob {
                cell,
                kind: SecretBlobKind::Foliage,
                volume: 1,
            });
        }
    }
}

fn plant_at_random<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>, kind: SecretPlantKind)
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    let cell = loop {
        let cell = point_to_cell(
            inputs.level,
            random_room_point(inputs.bounds, inputs.random),
        );
        if !inputs.level.has_plant(cell) {
            break cell;
        }
    };
    inputs.level.record(SecretPaintEvent::Plant { cell, kind });
}

fn potion_weight(kind: PotionKind) -> f32 {
    match kind {
        PotionKind::Healing => 1.0,
        PotionKind::MindVision => 2.0,
        PotionKind::Frost | PotionKind::LiquidFlame | PotionKind::ToxicGas => 3.0,
        PotionKind::Haste
        | PotionKind::Invisibility
        | PotionKind::Levitation
        | PotionKind::ParalyticGas
        | PotionKind::Purity => 4.0,
        PotionKind::Experience => 6.0,
        PotionKind::Strength => 0.0,
    }
}

fn scroll_weight(kind: ScrollKind) -> f32 {
    match kind {
        ScrollKind::Identify => 1.0,
        ScrollKind::RemoveCurse => 2.0,
        ScrollKind::MirrorImage | ScrollKind::Recharging | ScrollKind::Teleportation => 3.0,
        ScrollKind::Lullaby
        | ScrollKind::MagicMapping
        | ScrollKind::Rage
        | ScrollKind::Retribution
        | ScrollKind::Terror => 4.0,
        ScrollKind::Transmutation => 6.0,
        ScrollKind::Upgrade => 0.0,
    }
}

fn exotic_consumable_chance(level: i32) -> f32 {
    if level == -1 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            0.125 + 0.125 * level as f32
        }
    }
}

fn paint_laboratory<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY_SP);
    inputs.set_door(DoorType::Hidden);

    let pot = room_center(inputs.bounds, inputs.random);
    draw::set_point(inputs.level.map_mut(), pot, terrain::ALCHEMY);
    inputs.level.record(SecretPaintEvent::Blob {
        cell: point_to_cell(inputs.level, pot),
        kind: SecretBlobKind::Alchemy,
        volume: 1,
    });

    for _ in 0..2 {
        let cell = random_empty_drop_cell(inputs, terrain::EMPTY_SP);
        let quantity = inputs.random.int_range(3, 5);
        inputs.drop(
            cell,
            SecretHeapKind::Heap,
            SecretItem::EnergyCrystal { quantity },
            false,
            ItemSource::Heap,
        );
    }

    let count = inputs.random.int_range(2, 3);
    let mut weights = inputs.profile.laboratory_potions.map(potion_weight);
    for _ in 0..count {
        let cell = random_empty_drop_cell(inputs, terrain::EMPTY_SP);
        let selected = inputs
            .random
            .chances(&weights)
            .expect("laboratory potion map retains positive weights");
        weights[selected] = 0.0;
        let kind = inputs.profile.laboratory_potions[selected];
        let exotic =
            inputs.random.float() < exotic_consumable_chance(inputs.prizes.exotic_crystals_level());
        inputs.drop(
            cell,
            SecretHeapKind::Heap,
            GeneratedItem::Potion { kind, exotic }.into(),
            false,
            ItemSource::Heap,
        );
    }
}

fn paint_library<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::BOOKSHELF);
    draw::fill_ellipse(
        inputs.level.map_mut(),
        inputs.bounds.left + 2,
        inputs.bounds.top + 2,
        inclusive_width(inputs.bounds) - 4,
        inclusive_height(inputs.bounds) - 4,
        terrain::EMPTY_SP,
    );
    let count = if inputs.entrance.point.x == inputs.bounds.left
        || inputs.entrance.point.x == inputs.bounds.right
    {
        (inclusive_width(inputs.bounds) - 3) / 2
    } else {
        (inclusive_height(inputs.bounds) - 3) / 2
    };
    draw::draw_inside(
        inputs.level.map_mut(),
        inputs.bounds,
        inputs.entrance.point,
        count,
        terrain::EMPTY_SP,
    );
    inputs.set_door(DoorType::Hidden);

    let count = inputs.random.int_range(2, 3);
    let mut weights = inputs.profile.library_scrolls.map(scroll_weight);
    for _ in 0..count {
        let cell = random_empty_drop_cell(inputs, terrain::EMPTY_SP);
        let selected = inputs
            .random
            .chances(&weights)
            .expect("library scroll map retains positive weights");
        weights[selected] = 0.0;
        let kind = inputs.profile.library_scrolls[selected];
        let exotic =
            inputs.random.float() < exotic_consumable_chance(inputs.prizes.exotic_crystals_level());
        inputs.drop(
            cell,
            SecretHeapKind::Heap,
            GeneratedItem::Scroll { kind, exotic }.into(),
            false,
            ItemSource::Heap,
        );
    }
}

fn random_empty_drop_cell<L, G, P>(
    inputs: &mut PaintInputs<'_, L, G, P>,
    expected_terrain: i32,
) -> usize
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    loop {
        let cell = point_to_cell(
            inputs.level,
            random_room_point(inputs.bounds, inputs.random),
        );
        if inputs.level.map().cells[cell] == expected_terrain && !inputs.level.has_heap(cell) {
            return cell;
        }
    }
}

fn paint_larder<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY_SP);
    let center = room_center(inputs.bounds, inputs.random);
    draw::fill(
        inputs.level.map_mut(),
        center.x - 1,
        center.y - 1,
        3,
        3,
        terrain::WATER,
    );
    draw::set_point(inputs.level.map_mut(), center, terrain::GRASS);
    inputs.level.record(SecretPaintEvent::Plant {
        cell: point_to_cell(inputs.level, center),
        kind: SecretPlantKind::BlandfruitBush,
    });

    let mut extra_food = 150_i32.wrapping_mul(1 + inputs.depth_i32() / 5);
    while extra_food > 0 {
        let item = if extra_food >= 450 {
            extra_food -= 450;
            SecretItem::Generated(GeneratedItem::Food(FoodKind::Pasty))
        } else {
            extra_food -= 150;
            SecretItem::ChargrilledMeat
        };
        let cell = random_empty_drop_cell(inputs, terrain::EMPTY_SP);
        inputs.drop(cell, SecretHeapKind::Heap, item, false, ItemSource::Heap);
    }
    inputs.set_door(DoorType::Hidden);
}

fn paint_well<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    let door = inputs.entrance.point;
    let well = if door.x == inputs.bounds.left {
        Point::new(inputs.bounds.right - 2, door.y)
    } else if door.x == inputs.bounds.right {
        Point::new(inputs.bounds.left + 2, door.y)
    } else if door.y == inputs.bounds.top {
        Point::new(door.x, inputs.bounds.bottom - 2)
    } else {
        Point::new(door.x, inputs.bounds.top + 2)
    };
    draw::fill(
        inputs.level.map_mut(),
        well.x - 1,
        well.y - 1,
        3,
        3,
        terrain::CHASM,
    );
    draw::draw_line(inputs.level.map_mut(), door, well, terrain::EMPTY);
    draw::set_point(inputs.level.map_mut(), well, terrain::WELL);
    let kind = if inputs.random.int_bound(2) == 0 {
        SecretBlobKind::WaterOfAwareness
    } else {
        SecretBlobKind::WaterOfHealth
    };
    inputs.level.record(SecretPaintEvent::Blob {
        cell: point_to_cell(inputs.level, well),
        kind,
        volume: 1,
    });
    inputs.set_door(DoorType::Hidden);
}

fn paint_runestone<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    let center = room_center(inputs.bounds, inputs.random);
    let entrance = inputs.entrance.point;
    if entrance.x == inputs.bounds.left || entrance.x == inputs.bounds.right {
        draw::draw_line(
            inputs.level.map_mut(),
            Point::new(center.x, inputs.bounds.top + 1),
            Point::new(center.x, inputs.bounds.bottom - 1),
            terrain::BOOKSHELF,
        );
        if entrance.x == inputs.bounds.left {
            draw::fill(
                inputs.level.map_mut(),
                center.x + 1,
                inputs.bounds.top + 1,
                inputs.bounds.right - center.x - 1,
                inclusive_height(inputs.bounds) - 2,
                terrain::EMPTY_SP,
            );
        } else {
            draw::fill(
                inputs.level.map_mut(),
                inputs.bounds.left + 1,
                inputs.bounds.top + 1,
                center.x - inputs.bounds.left - 1,
                inclusive_height(inputs.bounds) - 2,
                terrain::EMPTY_SP,
            );
        }
    } else {
        draw::draw_line(
            inputs.level.map_mut(),
            Point::new(inputs.bounds.left + 1, center.y),
            Point::new(inputs.bounds.right - 1, center.y),
            terrain::BOOKSHELF,
        );
        if entrance.y == inputs.bounds.top {
            draw::fill(
                inputs.level.map_mut(),
                inputs.bounds.left + 1,
                center.y + 1,
                inclusive_width(inputs.bounds) - 2,
                inputs.bounds.bottom - center.y - 1,
                terrain::EMPTY_SP,
            );
        } else {
            draw::fill(
                inputs.level.map_mut(),
                inputs.bounds.left + 1,
                inputs.bounds.top + 1,
                inclusive_width(inputs.bounds) - 2,
                center.y - inputs.bounds.top - 1,
                terrain::EMPTY_SP,
            );
        }
    }

    inputs.spawn_generated(GeneratedItem::Potion {
        kind: PotionKind::LiquidFlame,
        exotic: false,
    });
    for index in 0..2 {
        let cell = loop {
            let cell = point_to_cell(
                inputs.level,
                random_room_point(inputs.bounds, inputs.random),
            );
            if inputs.level.map().cells[cell] == terrain::EMPTY
                && (index == 0 || !inputs.level.has_heap(cell))
            {
                break cell;
            }
        };
        let item = inputs.generate(SecretGeneratorRequest::DefaultsCategory(
            GeneratorCategory::Stone,
        ))?;
        inputs.drop(cell, SecretHeapKind::Heap, item, false, ItemSource::Heap);
    }
    let cell = loop {
        let cell = point_to_cell(
            inputs.level,
            random_room_point(inputs.bounds, inputs.random),
        );
        if inputs.level.map().cells[cell] == terrain::EMPTY_SP {
            break cell;
        }
    };
    inputs.drop(
        cell,
        SecretHeapKind::Heap,
        GeneratedItem::Stone(StoneKind::Enchantment).into(),
        false,
        ItemSource::Heap,
    );
    inputs.set_door(DoorType::Hidden);
    Ok(())
}

fn paint_artillery<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY_SP);
    let center = room_center(inputs.bounds, inputs.random);
    draw::set_point(inputs.level.map_mut(), center, terrain::STATUE_SP);
    for index in 0..3 {
        let cell = random_empty_drop_cell(inputs, terrain::EMPTY_SP);
        let item = if index == 0 {
            SecretItem::Generated(GeneratedItem::Bomb(BombKind::DoubleBomb))
        } else {
            inputs.generate(SecretGeneratorRequest::Missile {
                floor_set: inputs.floor_set(),
                use_defaults: true,
            })?
        };
        inputs.drop(cell, SecretHeapKind::Heap, item, false, ItemSource::Heap);
    }
    inputs.set_door(DoorType::Hidden);
    Ok(())
}

fn paint_chest_chasm<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::CHASM);
    let chest_points = [
        Point::new(inputs.bounds.left + 3, inputs.bounds.top + 3),
        Point::new(inputs.bounds.right - 3, inputs.bounds.top + 3),
        Point::new(inputs.bounds.right - 3, inputs.bounds.bottom - 3),
        Point::new(inputs.bounds.left + 3, inputs.bounds.bottom - 3),
    ];
    let mut chests = 0_i32;
    for point in chest_points {
        draw::set_point(inputs.level.map_mut(), point, terrain::EMPTY_SP);
        let cell = point_to_cell(inputs.level, point);
        let item = inputs.generate(SecretGeneratorRequest::DefaultsOverall)?;
        inputs.drop(
            cell,
            SecretHeapKind::LockedChest,
            item,
            false,
            ItemSource::LockedChest,
        );
        if inputs.level.has_heap(cell) {
            chests += 1;
        }
    }
    let key_points = [
        Point::new(inputs.bounds.left + 1, inputs.bounds.top + 1),
        Point::new(inputs.bounds.right - 1, inputs.bounds.top + 1),
        Point::new(inputs.bounds.right - 1, inputs.bounds.bottom - 1),
        Point::new(inputs.bounds.left + 1, inputs.bounds.bottom - 1),
    ];
    for point in key_points {
        draw::set_point(inputs.level.map_mut(), point, terrain::EMPTY_SP);
        if chests > 0 {
            chests -= 1;
            inputs.drop(
                point_to_cell(inputs.level, point),
                SecretHeapKind::Heap,
                SecretItem::GoldenKey {
                    depth: inputs.depth_i32(),
                },
                false,
                ItemSource::Heap,
            );
        }
    }
    inputs.spawn_generated(GeneratedItem::Potion {
        kind: PotionKind::Levitation,
        exotic: false,
    });
    inputs.set_door(DoorType::Hidden);
    Ok(())
}

fn paint_honeypot<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    let mut broken = room_center(inputs.bounds, inputs.random);
    broken.x = div_i32(broken.x + inputs.entrance.point.x, 2);
    broken.y = div_i32(broken.y + inputs.entrance.point.y, 2);
    let cell = point_to_cell(inputs.level, broken);
    inputs.drop(
        cell,
        SecretHeapKind::Heap,
        SecretItem::ShatteredPot,
        false,
        ItemSource::Heap,
    );
    let bee_level = inputs.depth_i32();
    inputs.level.record(SecretPaintEvent::Mob {
        cell,
        kind: SecretMobKind::Bee {
            level: bee_level,
            hit_points: (2 + bee_level) * 4,
        },
    });

    let honeypot_cell = random_free_heap_cell(inputs);
    inputs.drop(
        honeypot_cell,
        SecretHeapKind::Heap,
        SecretItem::Honeypot,
        false,
        ItemSource::Heap,
    );
    let bomb = inputs.generate(SecretGeneratorRequest::Bomb)?;
    let bomb_cell = random_free_heap_cell(inputs);
    inputs.drop(
        bomb_cell,
        SecretHeapKind::Heap,
        bomb,
        false,
        ItemSource::Heap,
    );
    inputs.set_door(DoorType::Hidden);
    Ok(())
}

fn random_free_heap_cell<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> usize
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    loop {
        let cell = point_to_cell(
            inputs.level,
            random_room_point(inputs.bounds, inputs.random),
        );
        if !inputs.level.has_heap(cell) {
            return cell;
        }
    }
}

fn paint_hoard<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    let trap = if inputs.random.int_bound(2) == 0 {
        SecretTrapKind::Rockfall
    } else if inputs.depth_i32() >= 10 {
        SecretTrapKind::Disintegration
    } else {
        SecretTrapKind::PoisonDart
    };
    let total_gold =
        (inclusive_width(inputs.bounds) - 2).wrapping_mul(inclusive_height(inputs.bounds) - 2) / 2;
    #[allow(clippy::cast_precision_loss)]
    let gold_ratio = 8.0_f32 / total_gold as f32;
    for _ in 0..total_gold {
        let cell = random_free_heap_cell(inputs);
        let mut item = inputs.generate(SecretGeneratorRequest::Gold)?;
        let SecretItem::Generated(GeneratedItem::Gold { quantity }) = &mut item else {
            unreachable!("gold request returns Gold")
        };
        #[allow(clippy::cast_precision_loss)]
        let scaled = *quantity as f32 * gold_ratio;
        *quantity = round_f32(scaled);
        inputs.drop(cell, SecretHeapKind::Heap, item, false, ItemSource::Heap);
    }
    for point in inputs.bounds.points() {
        let selected = inputs.random.int_bound(2) == 0;
        let cell = point_to_cell(inputs.level, point);
        if selected && inputs.level.map().cells[cell] == terrain::EMPTY {
            inputs.level.record(SecretPaintEvent::Trap {
                cell,
                kind: trap,
                visible: true,
                active: true,
            });
            draw::set(inputs.level.map_mut(), cell, terrain::TRAP);
        }
    }
    inputs.set_door(DoorType::Hidden);
    Ok(())
}

fn maze_index(height: i32, x: i32, y: i32) -> usize {
    usize::try_from(x.wrapping_mul(height).wrapping_add(y))
        .expect("maze coordinate is non-negative")
}

fn maze_valid_move(
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
    // strided adds probe the same cells `maze_index` would.
    let step_stride = dx.wrapping_mul(height).wrapping_add(dy);
    let side_stride = side_x.wrapping_mul(height).wrapping_add(side_y);
    let mut index = x.wrapping_mul(height).wrapping_add(y);
    x += dx;
    y += dy;
    index = index.wrapping_add(step_stride);
    if x <= 0 || x >= width - 1 || y <= 0 || y >= height - 1 {
        return false;
    }
    if maze[maze_cell(index)]
        || maze[maze_cell(index + side_stride)]
        || maze[maze_cell(index - side_stride)]
    {
        return false;
    }
    x += dx;
    y += dy;
    index = index.wrapping_add(step_stride);
    if x <= 0 || x >= width - 1 || y <= 0 || y >= height - 1 {
        return false;
    }
    !maze[maze_cell(index)]
        && !maze[maze_cell(index + side_stride)]
        && !maze[maze_cell(index - side_stride)]
}

fn maze_cell(index: i32) -> usize {
    usize::try_from(index).expect("maze coordinate is non-negative")
}

fn maze_direction(
    maze: &[bool],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    random: &mut crate::rng::JavaRandom,
) -> Option<(i32, i32)> {
    if random.next_i32_bound(4) == 0 && maze_valid_move(maze, width, height, x, y, 0, -1) {
        return Some((0, -1));
    }
    if random.next_i32_bound(3) == 0 && maze_valid_move(maze, width, height, x, y, 1, 0) {
        return Some((1, 0));
    }
    if random.next_i32_bound(2) == 0 && maze_valid_move(maze, width, height, x, y, 0, 1) {
        return Some((0, 1));
    }
    maze_valid_move(maze, width, height, x, y, -1, 0).then_some((-1, 0))
}

fn generate_maze(bounds: Rect, entrance: Point, random: &mut RandomStack) -> Vec<bool> {
    let width = inclusive_width(bounds);
    let height = inclusive_height(bounds);
    if (crate::maze::MIN_BIT_MAZE_SIDE..=crate::maze::MAX_BIT_MAZE_HEIGHT).contains(&height)
        && width >= crate::maze::MIN_BIT_MAZE_SIDE
    {
        let mut cols = crate::maze::walled_border_cols(width, height);
        let x = usize::try_from(entrance.x - bounds.left).expect("entrance is in the room");
        cols[x] &= !(1_u64 << (entrance.y - bounds.top));
        crate::maze::grow_maze(&mut cols, width, height, random);
        return crate::maze::cols_to_column_major(&cols, height);
    }

    let mut maze = vec![false; usize::try_from(width * height).expect("positive maze size")];
    for x in 0..width {
        for y in 0..height {
            if x == 0 || x == width - 1 || y == 0 || y == height - 1 {
                maze[maze_index(height, x, y)] = true;
            }
        }
    }
    maze[maze_index(height, entrance.x - bounds.left, entrance.y - bounds.top)] = false;

    // Same hoisting as the sewer connection maze: the loop exits only after
    // 2,500 consecutive failed rounds, so the generator and the reciprocal
    // pick bounds are lifted out of the draw loop. The canonical draw
    // sequence is unchanged.
    let generator = random.current_generator();
    let x_bound = FastBound::new(width);
    let y_bound = FastBound::new(height);
    let mut fails = 0;
    while fails < 2_500 {
        let (mut x, mut y) = loop {
            let x = generator.next_i32_fast_bound(&x_bound);
            let y = generator.next_i32_fast_bound(&y_bound);
            if maze[maze_index(height, x, y)] {
                break (x, y);
            }
        };
        let Some((dx, dy)) = maze_direction(&maze, width, height, x, y, generator) else {
            fails += 1;
            continue;
        };
        fails = 0;
        let mut moves = 0;
        loop {
            x += dx;
            y += dy;
            maze[maze_index(height, x, y)] = true;
            moves += 1;
            if generator.next_i32_bound(moves) != 0
                || !maze_valid_move(&maze, width, height, x, y, dx, dy)
            {
                break;
            }
        }
    }
    maze
}

fn paint_maze<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    let width = inclusive_width(inputs.bounds);
    let height = inclusive_height(inputs.bounds);
    let maze = generate_maze(inputs.bounds, inputs.entrance.point, inputs.random);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    let mut passable = vec![false; usize::try_from(width * height).expect("positive maze size")];
    for x in 0..width {
        for y in 0..height {
            let filled = maze[maze_index(height, x, y)];
            if filled {
                draw::set_xy(
                    inputs.level.map_mut(),
                    x + inputs.bounds.left,
                    y + inputs.bounds.top,
                    terrain::WALL,
                );
            }
            passable[usize::try_from(x + width * y).expect("maze path index is non-negative")] =
                !filled;
        }
    }
    let entrance = (inputs.entrance.point.x - inputs.bounds.left)
        + width * (inputs.entrance.point.y - inputs.bounds.top);
    let mut finder = PathFinder::new(width, height);
    finder.build_distance_map(
        usize::try_from(entrance).expect("maze entrance index is non-negative"),
        &passable,
    );
    let mut best_distance = 0;
    let mut best = Point::default();
    for (index, distance) in finder.distance.iter().copied().enumerate() {
        if distance != i32::MAX && distance > best_distance {
            best_distance = distance;
            let index = i32::try_from(index).expect("maze index fits Java int");
            best = Point::new(
                index % width + inputs.bounds.left,
                index / width + inputs.bounds.top,
            );
        }
    }
    let floor_set = inputs.floor_set() + 1;
    let mut prize = if inputs.random.int_bound(2) == 0 {
        inputs.generate(SecretGeneratorRequest::Weapon {
            floor_set,
            use_defaults: true,
        })?
    } else {
        inputs.generate(SecretGeneratorRequest::Armor { floor_set })?
    };
    clear_curse_effect_and_mark_clean(&mut prize);
    if inputs.random.int_bound(3) == 0 {
        upgrade_equipment(&mut prize);
    }
    inputs.drop(
        point_to_cell(inputs.level, best),
        SecretHeapKind::Chest,
        prize,
        false,
        ItemSource::Chest,
    );
    inputs.set_door(DoorType::Hidden);
    Ok(())
}

fn trap_reveal_chance(level: i32) -> f32 {
    if level == -1 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            0.1 + 0.1 * level as f32
        }
    }
}

fn paint_summoning<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SecretLevelContext,
    G: SecretGeneratorContext,
    P: SecretPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::SECRET_TRAP);
    let center = room_center(inputs.bounds, inputs.random);
    let prize = inputs.generate(SecretGeneratorRequest::Overall)?;
    let haunted = match prize {
        SecretItem::Generated(item) => generated_is_cursed(item),
        _ => false,
    };
    inputs.drop(
        point_to_cell(inputs.level, center),
        SecretHeapKind::Skeleton,
        prize,
        haunted,
        ItemSource::Skeleton,
    );
    let revealed_chance = trap_reveal_chance(inputs.prizes.trap_mechanism_level());
    let mut reveal_increment = 0.0_f32;
    for point in inputs.bounds.points() {
        let cell = point_to_cell(inputs.level, point);
        if inputs.level.map().cells[cell] == terrain::SECRET_TRAP {
            reveal_increment += revealed_chance;
            let visible = reveal_increment >= 1.0;
            if visible {
                reveal_increment -= 1.0;
                draw::set(inputs.level.map_mut(), cell, terrain::TRAP);
            }
            inputs.level.record(SecretPaintEvent::Trap {
                cell,
                kind: SecretTrapKind::Summoning,
                visible,
                active: true,
            });
        }
    }
    inputs.set_door(DoorType::Hidden);
    Ok(())
}

#[must_use]
pub const fn can_place_water(kind: RoomKind) -> bool {
    !matches!(kind, RoomKind::Secret(SecretRoomKind::Runestone))
}

#[must_use]
pub const fn can_place_grass(kind: RoomKind) -> bool {
    !matches!(kind, RoomKind::Secret(SecretRoomKind::Runestone))
}

#[must_use]
pub const fn can_place_trap(kind: RoomKind) -> bool {
    !matches!(kind, RoomKind::Secret(SecretRoomKind::Hoard))
}

#[must_use]
pub fn can_place_character(level: &Level, rooms: &[Room], room: RoomId, point: Point) -> bool {
    if !rooms[room].inside(point) {
        return false;
    }
    !matches!(
        rooms[room].kind,
        RoomKind::Secret(SecretRoomKind::Runestone)
    ) || level.map.cells[level.point_to_cell(point)] != terrain::EMPTY_SP
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::catalog::ItemId;
    use crate::equipment::EquipmentRoll;
    use crate::generator::{ArtifactKind, GeneratedEquipment, MissileKind};
    use crate::level::Feeling;
    use crate::room::{ConnectionRoomKind, Door, RoomConnection};
    use crate::run::{RingKind, RunState};
    use crate::special_equipment::PrizePool;

    struct Fixture {
        map_hash: i32,
        door: DoorType,
        next: i64,
        events: Vec<SecretPaintEvent>,
        report: SecretPaintReport,
        generator: GeneratorState,
    }

    fn paint_fixture(kind: SecretRoomKind, size: i32, door: Point) -> Fixture {
        let mut run = RunState::new(0);
        let mut random = RandomStack::with_base_seed(0);
        random.push(0);
        let mut level = Level::new(3, Feeling::None);
        level.set_size(25, 25);
        let mut events = Vec::new();
        let mut prizes = PrizePool::empty(41);
        let mut secret = Room::secret(kind);
        secret.bounds = Rect::new(2, 2, 2 + size - 1, 2 + size - 1);
        secret.connected.push(RoomConnection {
            room: 1,
            door: Some(Door::new(door)),
        });
        let mut neighbour = Room::connection(ConnectionRoomKind::Tunnel);
        neighbour.connected.push(RoomConnection {
            room: 0,
            door: Some(Door::new(door)),
        });
        let mut rooms = vec![secret, neighbour];
        let report = {
            let mut context = SecretLevelPaintContext::new(&mut level, &mut events);
            let SecretPaintOutcome::Painted(report) = paint_secret_room(
                &mut context,
                &mut rooms,
                0,
                &mut run.generator,
                &mut prizes,
                &mut random,
            )
            .unwrap() else {
                panic!("secret room was not handled")
            };
            report
        };
        Fixture {
            map_hash: level.java_map_hash(),
            door: rooms[0].connected[0].door.unwrap().door_type,
            next: random.long(),
            events,
            report,
            generator: run.generator,
        }
    }

    fn potion_name(kind: PotionKind, exotic: bool) -> &'static str {
        if exotic {
            return match kind {
                PotionKind::Strength => "PotionOfMastery",
                PotionKind::Healing => "PotionOfShielding",
                PotionKind::MindVision => "PotionOfMagicalSight",
                PotionKind::Frost => "PotionOfSnapFreeze",
                PotionKind::LiquidFlame => "PotionOfDragonsBreath",
                PotionKind::ToxicGas => "PotionOfCorrosiveGas",
                PotionKind::Haste => "PotionOfStamina",
                PotionKind::Invisibility => "PotionOfShroudingFog",
                PotionKind::Levitation => "PotionOfStormClouds",
                PotionKind::ParalyticGas => "PotionOfEarthenArmor",
                PotionKind::Purity => "PotionOfCleansing",
                PotionKind::Experience => "PotionOfDivineInspiration",
            };
        }
        match kind {
            PotionKind::Strength => "PotionOfStrength",
            PotionKind::Healing => "PotionOfHealing",
            PotionKind::MindVision => "PotionOfMindVision",
            PotionKind::Frost => "PotionOfFrost",
            PotionKind::LiquidFlame => "PotionOfLiquidFlame",
            PotionKind::ToxicGas => "PotionOfToxicGas",
            PotionKind::Haste => "PotionOfHaste",
            PotionKind::Invisibility => "PotionOfInvisibility",
            PotionKind::Levitation => "PotionOfLevitation",
            PotionKind::ParalyticGas => "PotionOfParalyticGas",
            PotionKind::Purity => "PotionOfPurity",
            PotionKind::Experience => "PotionOfExperience",
        }
    }

    fn scroll_name(kind: ScrollKind, exotic: bool) -> &'static str {
        if exotic {
            return match kind {
                ScrollKind::Upgrade => "ScrollOfEnchantment",
                ScrollKind::Identify => "ScrollOfDivination",
                ScrollKind::RemoveCurse => "ScrollOfAntiMagic",
                ScrollKind::MirrorImage => "ScrollOfPrismaticImage",
                ScrollKind::Recharging => "ScrollOfMysticalEnergy",
                ScrollKind::Teleportation => "ScrollOfPassage",
                ScrollKind::Lullaby => "ScrollOfSirensSong",
                ScrollKind::MagicMapping => "ScrollOfForesight",
                ScrollKind::Rage => "ScrollOfChallenge",
                ScrollKind::Retribution => "ScrollOfPsionicBlast",
                ScrollKind::Terror => "ScrollOfDread",
                ScrollKind::Transmutation => "ScrollOfMetamorphosis",
            };
        }
        match kind {
            ScrollKind::Upgrade => "ScrollOfUpgrade",
            ScrollKind::Identify => "ScrollOfIdentify",
            ScrollKind::RemoveCurse => "ScrollOfRemoveCurse",
            ScrollKind::MirrorImage => "ScrollOfMirrorImage",
            ScrollKind::Recharging => "ScrollOfRecharging",
            ScrollKind::Teleportation => "ScrollOfTeleportation",
            ScrollKind::Lullaby => "ScrollOfLullaby",
            ScrollKind::MagicMapping => "ScrollOfMagicMapping",
            ScrollKind::Rage => "ScrollOfRage",
            ScrollKind::Retribution => "ScrollOfRetribution",
            ScrollKind::Terror => "ScrollOfTerror",
            ScrollKind::Transmutation => "ScrollOfTransmutation",
        }
    }

    fn stone_name(kind: StoneKind) -> &'static str {
        match kind {
            StoneKind::Enchantment => "StoneOfEnchantment",
            StoneKind::Intuition => "StoneOfIntuition",
            StoneKind::DetectMagic => "StoneOfDetectMagic",
            StoneKind::Flock => "StoneOfFlock",
            StoneKind::Shock => "StoneOfShock",
            StoneKind::Blink => "StoneOfBlink",
            StoneKind::DeepSleep => "StoneOfDeepSleep",
            StoneKind::Clairvoyance => "StoneOfClairvoyance",
            StoneKind::Aggression => "StoneOfAggression",
            StoneKind::Blast => "StoneOfBlast",
            StoneKind::Fear => "StoneOfFear",
            StoneKind::Augmentation => "StoneOfAugmentation",
        }
    }

    fn missile_name(kind: MissileKind) -> &'static str {
        match kind {
            MissileKind::ThrowingStone => "ThrowingStone",
            MissileKind::ThrowingKnife => "ThrowingKnife",
            MissileKind::ThrowingSpike => "ThrowingSpike",
            MissileKind::Dart => "Dart",
            MissileKind::FishingSpear => "FishingSpear",
            MissileKind::ThrowingClub => "ThrowingClub",
            MissileKind::Shuriken => "Shuriken",
            MissileKind::ThrowingSpear => "ThrowingSpear",
            MissileKind::Kunai => "Kunai",
            MissileKind::Bolas => "Bolas",
            MissileKind::Javelin => "Javelin",
            MissileKind::Tomahawk => "Tomahawk",
            MissileKind::HeavyBoomerang => "HeavyBoomerang",
            MissileKind::Trident => "Trident",
            MissileKind::ThrowingHammer => "ThrowingHammer",
            MissileKind::ForceCube => "ForceCube",
        }
    }

    fn ring_name(kind: RingKind) -> String {
        format!("RingOf{kind:?}")
    }

    fn artifact_name(kind: ArtifactKind) -> String {
        format!("{kind:?}")
    }

    fn item_text(item: SecretItem) -> String {
        let (class, level, cursed, effect, quantity) = match item {
            SecretItem::EnergyCrystal { quantity } => {
                ("EnergyCrystal".into(), 0, false, "none".into(), quantity)
            }
            SecretItem::ChargrilledMeat => ("ChargrilledMeat".into(), 0, false, "none".into(), 1),
            SecretItem::GoldenKey { .. } => ("GoldenKey".into(), 0, false, "none".into(), 1),
            SecretItem::ShatteredPot => ("ShatteredPot".into(), 0, false, "none".into(), 1),
            SecretItem::Honeypot => ("Honeypot".into(), 0, false, "none".into(), 1),
            SecretItem::Generated(item) => match item {
                GeneratedItem::Equipment(item) => (
                    format!("{:?}", item.item),
                    i32::from(item.roll.upgrade),
                    item.roll.cursed,
                    item.roll.effect.map_or_else(
                        || "none".into(),
                        |effect| match effect {
                            Effect::Weapon(effect) => format!("{effect:?}"),
                            Effect::Armor(effect) => format!("{effect:?}"),
                        },
                    ),
                    1,
                ),
                GeneratedItem::Missile(item) => (
                    missile_name(item.kind).into(),
                    i32::from(item.roll.upgrade),
                    item.roll.cursed,
                    item.roll
                        .effect
                        .map_or_else(|| "none".into(), |effect| format!("{effect:?}")),
                    item.quantity,
                ),
                GeneratedItem::Ring(item) => (
                    ring_name(item.kind),
                    i32::from(item.roll.upgrade),
                    item.roll.cursed,
                    "none".into(),
                    1,
                ),
                GeneratedItem::Artifact(item) => {
                    (artifact_name(item.kind), 0, item.cursed, "none".into(), 1)
                }
                GeneratedItem::Food(kind) => (
                    match kind {
                        FoodKind::Ration => "Food",
                        FoodKind::Pasty => "Pasty",
                        FoodKind::MysteryMeat => "MysteryMeat",
                    }
                    .into(),
                    0,
                    false,
                    "none".into(),
                    1,
                ),
                GeneratedItem::Potion { kind, exotic } => {
                    (potion_name(kind, exotic).into(), 0, false, "none".into(), 1)
                }
                GeneratedItem::Scroll { kind, exotic } => {
                    (scroll_name(kind, exotic).into(), 0, false, "none".into(), 1)
                }
                GeneratedItem::Stone(kind) => (stone_name(kind).into(), 0, false, "none".into(), 1),
                GeneratedItem::Gold { quantity } => {
                    ("Gold".into(), 0, false, "none".into(), quantity)
                }
                GeneratedItem::Bomb(kind) => (
                    match kind {
                        BombKind::Bomb => "Bomb",
                        BombKind::DoubleBomb => "DoubleBomb",
                    }
                    .into(),
                    0,
                    false,
                    "none".into(),
                    1,
                ),
                other => panic!("unused secret-room fixture item: {other:?}"),
            },
        };
        format!(
            "{class}:+{level}:{}:{effect}:q{quantity}",
            if cursed { "cursed" } else { "clean" }
        )
    }

    fn java_int_array_hash(values: &[i32]) -> i32 {
        values.iter().fold(1_i32, |hash, value| {
            hash.wrapping_mul(31).wrapping_add(*value)
        })
    }

    fn java_string_hash(value: &str) -> i32 {
        value.encode_utf16().fold(0_i32, |hash, unit| {
            hash.wrapping_mul(31).wrapping_add(i32::from(unit))
        })
    }

    fn event_hash(events: &[SecretPaintEvent], level_len: usize) -> i32 {
        let mut heaps = BTreeMap::<usize, (SecretHeapKind, bool, Vec<SecretItem>)>::new();
        let mut mobs = Vec::new();
        let mut spawns = Vec::new();
        let mut plants = BTreeMap::new();
        let mut traps = BTreeMap::new();
        let mut blobs = BTreeMap::<&str, (i32, Vec<i32>)>::new();
        for event in events {
            match event {
                SecretPaintEvent::Drop {
                    cell,
                    heap,
                    haunted,
                    reward,
                } => {
                    heaps
                        .entry(*cell)
                        .or_insert_with(|| (*heap, *haunted, Vec::new()))
                        .2
                        .insert(0, reward.item);
                }
                SecretPaintEvent::Mob { cell, kind } => match kind {
                    SecretMobKind::Bee { .. } => mobs.push(format!("{cell}:Bee")),
                },
                SecretPaintEvent::SpawnItem(item) => spawns.push(item_text(*item)),
                SecretPaintEvent::Plant { cell, kind } => {
                    plants.insert(
                        *cell,
                        match kind {
                            SecretPlantKind::Starflower => "Starflower",
                            SecretPlantKind::Seedpod => "Seedpod",
                            SecretPlantKind::Dewcatcher => "Dewcatcher",
                            SecretPlantKind::BlandfruitBush => "BlandfruitBush",
                        },
                    );
                }
                SecretPaintEvent::Blob { cell, kind, volume } => {
                    let name = match kind {
                        SecretBlobKind::Foliage => "Foliage",
                        SecretBlobKind::Alchemy => "Alchemy",
                        SecretBlobKind::WaterOfAwareness => "WaterOfAwareness",
                        SecretBlobKind::WaterOfHealth => "WaterOfHealth",
                    };
                    let blob = blobs.entry(name).or_insert_with(|| (0, vec![0; level_len]));
                    blob.0 += *volume;
                    blob.1[*cell] += *volume;
                }
                SecretPaintEvent::Trap {
                    cell,
                    kind,
                    visible,
                    active,
                } => {
                    traps.insert(
                        *cell,
                        (
                            match kind {
                                SecretTrapKind::Rockfall => "RockfallTrap",
                                SecretTrapKind::PoisonDart => "PoisonDartTrap",
                                SecretTrapKind::Disintegration => "DisintegrationTrap",
                                SecretTrapKind::Summoning => "SummoningTrap",
                            },
                            *visible,
                            *active,
                        ),
                    );
                }
            }
        }
        mobs.sort();
        let heap_text = heaps
            .iter()
            .map(|(cell, (kind, haunted, items))| {
                let name = match kind {
                    SecretHeapKind::Heap => "HEAP",
                    SecretHeapKind::Chest => "CHEST",
                    SecretHeapKind::LockedChest => "LOCKED_CHEST",
                    SecretHeapKind::Skeleton => "SKELETON",
                };
                format!(
                    "{cell}:{name}:{haunted}:{}",
                    items
                        .iter()
                        .copied()
                        .map(item_text)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let plant_text = plants
            .iter()
            .map(|(cell, kind)| format!("{cell}:{kind}"))
            .collect::<Vec<_>>()
            .join(";");
        let trap_text = traps
            .iter()
            .map(|(cell, (kind, visible, active))| format!("{cell}:{kind}:{visible}:{active}"))
            .collect::<Vec<_>>()
            .join(";");
        let blob_text = blobs
            .iter()
            .map(|(name, (volume, cells))| {
                format!("{name}:{volume}:{}", java_int_array_hash(cells))
            })
            .collect::<Vec<_>>()
            .join(";");
        java_string_hash(&format!(
            "heaps=[{heap_text}]|mobs=[{}]|spawn=[{}]|plants=[{plant_text}]|traps=[{trap_text}]|blobs=[{blob_text}]",
            mobs.join(";"),
            spawns.join(",")
        ))
    }

    #[test]
    fn every_secret_room_matches_official_map_and_rng_fixtures() {
        let cases = [
            (
                SecretRoomKind::Garden,
                7,
                Point::new(2, 5),
                45_089_127,
                8_987_859_488_885_190_724_i64,
            ),
            (
                SecretRoomKind::Laboratory,
                7,
                Point::new(2, 5),
                441_096_251,
                3_246_199_166_113_899_023,
            ),
            (
                SecretRoomKind::Library,
                7,
                Point::new(2, 5),
                1_962_174_112,
                25_579_809_655_956_232,
            ),
            (
                SecretRoomKind::Library,
                7,
                Point::new(5, 2),
                -1_886_153_568,
                25_579_809_655_956_232,
            ),
            (
                SecretRoomKind::Larder,
                7,
                Point::new(2, 5),
                -7_922_271,
                4_437_113_781_045_784_766,
            ),
            (
                SecretRoomKind::Well,
                7,
                Point::new(2, 5),
                -2_012_151_085,
                -3_109_364_765_729_502_342,
            ),
            (
                SecretRoomKind::Well,
                7,
                Point::new(8, 5),
                -1_421_471_405,
                -3_109_364_765_729_502_342,
            ),
            (
                SecretRoomKind::Well,
                7,
                Point::new(5, 2),
                -471_690_029,
                -3_109_364_765_729_502_342,
            ),
            (
                SecretRoomKind::Well,
                7,
                Point::new(5, 8),
                2_022_768_467,
                -3_109_364_765_729_502_342,
            ),
            (
                SecretRoomKind::Runestone,
                7,
                Point::new(2, 5),
                1_376_975_770,
                -279_624_296_851_435_688,
            ),
            (
                SecretRoomKind::Runestone,
                7,
                Point::new(8, 5),
                -204_671_910,
                -2_228_689_144_322_150_137,
            ),
            (
                SecretRoomKind::Runestone,
                7,
                Point::new(5, 2),
                -386_862_438,
                2_377_732_757_510_138_102,
            ),
            (
                SecretRoomKind::Runestone,
                7,
                Point::new(5, 8),
                -822_985_382,
                -1_083_761_183_081_836_303,
            ),
            (
                SecretRoomKind::Artillery,
                7,
                Point::new(2, 5),
                -791_036_999,
                -1_083_761_183_081_836_303,
            ),
            (
                SecretRoomKind::ChestChasm,
                8,
                Point::new(2, 5),
                -607_063_517,
                -7_138_230_367_502_298_321,
            ),
            (
                SecretRoomKind::Honeypot,
                7,
                Point::new(2, 5),
                -1_402_175_648,
                5_700_976_833_288_827_063,
            ),
            (
                SecretRoomKind::Hoard,
                7,
                Point::new(2, 5),
                732_586_528,
                -2_697_084_138_585_034_005,
            ),
            (
                SecretRoomKind::Maze,
                14,
                Point::new(2, 8),
                2_027_727_174,
                2_686_245_777_611_411_912,
            ),
            (
                SecretRoomKind::Summoning,
                7,
                Point::new(2, 5),
                2_144_920_432,
                -7_261_648_964_369_397_258,
            ),
        ];
        let event_hashes = [
            -763_699_246,
            1_390_569_341,
            -997_272_324,
            -303_809_175,
            2_022_290_428,
            1_716_771_832,
            -2_069_972_212,
            -73_904_037,
            -35_558_166,
            880_027_930,
            1_523_212_119,
            -688_829_602,
            1_741_100_719,
            -375_431_993,
            1_258_320_770,
            -1_265_841_033,
            1_310_759_283,
            -997_668_527,
            1_776_211_056,
        ];
        let baseline = RunState::new(0).generator;
        for ((kind, size, door, map, next), expected_events) in cases.into_iter().zip(event_hashes)
        {
            let fixture = paint_fixture(kind, size, door);
            assert_eq!(fixture.map_hash, map, "{kind:?} map");
            assert_eq!(fixture.door, DoorType::Hidden, "{kind:?} door");
            assert_eq!(fixture.next, next, "{kind:?} RNG");
            assert_eq!(
                event_hash(&fixture.events, 25 * 25),
                expected_events,
                "{kind:?} events"
            );

            let mut expected_generator = baseline.clone();
            if kind == SecretRoomKind::Summoning {
                expected_generator.category_probabilities[GeneratorCategory::Gold as usize] -= 1.0;
            }
            assert_eq!(fixture.generator, expected_generator, "{kind:?} generator");

            match kind {
                SecretRoomKind::Artillery => {
                    assert_eq!(fixture.report.searchable_items.len(), 2);
                    assert_eq!(
                        fixture.report.searchable_items[0].item,
                        ItemId::FishingSpear
                    );
                    assert_eq!(fixture.report.searchable_items[1].item, ItemId::Shuriken);
                    assert!(
                        fixture
                            .report
                            .searchable_items
                            .iter()
                            .all(|item| item.upgrade == 0
                                && item.effect.is_none()
                                && !item.cursed
                                && item.source == ItemSource::Heap
                                && item.accessibility == Accessibility::Independent)
                    );
                }
                SecretRoomKind::ChestChasm => {
                    assert_eq!(fixture.report.searchable_items.len(), 1);
                    let item = &fixture.report.searchable_items[0];
                    assert_eq!(item.item, ItemId::Shuriken);
                    assert_eq!(item.source, ItemSource::LockedChest);
                    assert_eq!(item.upgrade, 0);
                    assert!(!item.cursed);
                }
                SecretRoomKind::Maze => {
                    assert_eq!(fixture.report.searchable_items.len(), 1);
                    let item = &fixture.report.searchable_items[0];
                    assert_eq!(item.item, ItemId::MailArmor);
                    assert_eq!(item.source, ItemSource::Chest);
                    assert_eq!(item.upgrade, 1);
                    assert!(item.effect.is_none());
                    assert!(!item.cursed);
                }
                _ => assert!(
                    fixture.report.searchable_items.is_empty(),
                    "{kind:?} report"
                ),
            }
        }
    }

    #[test]
    fn runtime_profile_is_the_temurin_21_oracle_iteration_order() {
        assert_eq!(
            SecretRuntimeProfile::default(),
            SecretRuntimeProfile::TEMURIN_21_0_11
        );
    }

    #[test]
    fn summoning_searchable_rewards_retain_the_skeleton_source() {
        let item = searchable_world_item(
            SecretItem::Generated(GeneratedItem::Equipment(GeneratedEquipment {
                item: ItemId::WandFrost,
                roll: EquipmentRoll {
                    upgrade: 2,
                    effect: None,
                    cursed: false,
                },
            })),
            3,
            ItemSource::Skeleton,
            Accessibility::Independent,
        )
        .unwrap();
        assert_eq!(item.item, ItemId::WandFrost);
        assert_eq!(item.upgrade, 2);
        assert_eq!(item.source, ItemSource::Skeleton);
    }
}
