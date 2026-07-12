//! Exact v3.3.8 painters for the nine equipment-heavy `SpecialRoom` classes.
//!
//! The generic painter deliberately has three explicit integration seams:
//! [`SpecialLevelContext`] owns terrain and occupancy, [`SpecialGeneratorContext`]
//! owns the run-global `Generator` decks, and [`SpecialPrizeContext`] owns the
//! insertion-ordered `Level.itemsToSpawn` list plus trinket state.  This keeps
//! every RNG draw in the room implementation while allowing the composite
//! regular-level dispatcher to decide where side-effect records live.
//!
//! `SentryRoom` and `CrystalChoiceRoom` construct temporary `EmptyRoom`
//! instances during `paint`; those apparently useless constructors consume
//! one and three size-category floats respectively.  Crystal mimics override
//! the ordinary mimic reward generator: their supplied vault prize is merely
//! made non-cursed, and no extra reward draw occurs.

#![allow(clippy::missing_panics_doc)]

use std::fmt;

use crate::catalog::{ArmorEffect, Effect, ItemKind, WeaponEffect, item as catalog_item};
use crate::equipment::{EquipmentRoll, random_armor_glyph, random_weapon_enchantment};
use crate::generator::{self, GeneratedItem, GeneratorError};
use crate::geometry::{GridMap, Point, Rect, painter as draw, terrain};
use crate::java_math::div_i32;
use crate::level::{Level, PaintItem};
use crate::model::{Accessibility, ItemSource, WorldItem};
use crate::painter::set_shared_door_type;
use crate::rng::RandomStack;
use crate::room::{DoorType, Room, RoomId, RoomKind, SpecialRoomKind};
use crate::run::{GeneratorCategory, GeneratorState};

const ARMOR_CURSES: [ArmorEffect; 8] = [
    ArmorEffect::AntiEntropy,
    ArmorEffect::Corrosion,
    ArmorEffect::Displacement,
    ArmorEffect::Metabolism,
    ArmorEffect::Multiplicity,
    ArmorEffect::Stench,
    ArmorEffect::Overgrowth,
    ArmorEffect::Bulk,
];

const WEAPON_CURSES: [WeaponEffect; 8] = [
    WeaponEffect::Annoying,
    WeaponEffect::Displacing,
    WeaponEffect::Dazzling,
    WeaponEffect::Explosive,
    WeaponEffect::Sacrificial,
    WeaponEffect::Wayward,
    WeaponEffect::Polarized,
    WeaponEffect::Friendly,
];

/// Directly constructed items which are not represented by `Generator`.
///
/// Keys and guaranteed potions are kept in this same type because Java adds
/// them to `itemsToSpawn`; a later Pool/Sentry can therefore select one as its
/// prize if room paint order permits it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpecialItem {
    Paint(PaintItem),
    IronKey { depth: i32 },
    CrystalKey { depth: i32 },
    PotionOfInvisibility,
    PotionOfHaste,
}

impl SpecialItem {
    #[must_use]
    pub const fn generated(item: GeneratedItem) -> Self {
        Self::Paint(PaintItem::Generated(item))
    }
}

impl From<PaintItem> for SpecialItem {
    fn from(item: PaintItem) -> Self {
        Self::Paint(item)
    }
}

/// Heap identity assigned by a room painter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SpecialHeapKind {
    Heap,
    Chest,
    Tomb,
    CrystalChest,
}

/// Non-rendering feature records needed to reconstruct special-room state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SpecialFeatureKind {
    HiddenWell,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SpecialBlobKind {
    WeakFloorWell,
    SacrificialFire,
}

/// Generation-visible special-room mob classes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SpecialMobKind {
    Piranha,
    PhantomPiranha,
    Sentry { initial_charge_delay_bits: u32 },
    Statue,
    ArmoredStatue,
    CrystalMimic { stealthy: bool },
}

/// One item together with the branch under which it can be obtained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpecialReward {
    pub item: SpecialItem,
    pub accessibility: Accessibility,
}

/// Ordered, typed side effects emitted while painting one special room.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpecialPaintEvent {
    Drop {
        cell: usize,
        heap: SpecialHeapKind,
        reward: SpecialReward,
    },
    Mob {
        cell: usize,
        kind: SpecialMobKind,
        carried: Vec<SpecialReward>,
    },
    SpawnItem(SpecialItem),
    Blob {
        cell: usize,
        kind: SpecialBlobKind,
        volume: i32,
        prize: Option<SpecialReward>,
    },
    Feature {
        point: Point,
        kind: SpecialFeatureKind,
    },
}

/// Terrain/occupancy sink used by the room painters.
pub trait SpecialLevelContext {
    fn depth(&self) -> u32;
    fn map(&self) -> &GridMap;
    fn map_mut(&mut self) -> &mut GridMap;
    fn has_heap(&self, cell: usize) -> bool;
    fn has_mob(&self, cell: usize) -> bool;
    fn record(&mut self, event: SpecialPaintEvent);
}

/// Adapter for the shared regular-painter [`Level`] plus an external special
/// event stream. Drops and mobs update the existing occupancy bitmaps so later
/// grass/item placement sees the same cells as Java.
pub struct LevelPaintContext<'a> {
    pub level: &'a mut Level,
    pub events: &'a mut Vec<SpecialPaintEvent>,
}

impl<'a> LevelPaintContext<'a> {
    #[must_use]
    pub const fn new(level: &'a mut Level, events: &'a mut Vec<SpecialPaintEvent>) -> Self {
        Self { level, events }
    }
}

impl SpecialLevelContext for LevelPaintContext<'_> {
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

    fn record(&mut self, event: SpecialPaintEvent) {
        match &event {
            SpecialPaintEvent::Drop { cell, .. } => self.level.mark_heap(*cell),
            SpecialPaintEvent::Mob { cell, .. } => self.level.mark_mob(*cell),
            SpecialPaintEvent::SpawnItem(_)
            | SpecialPaintEvent::Blob { .. }
            | SpecialPaintEvent::Feature { .. } => {}
        }
        self.events.push(event);
    }
}

/// A request at one of the exact Java `Generator` call sites.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpecialGeneratorRequest {
    Weapon { floor_set: i32, use_defaults: bool },
    Armor { floor_set: i32 },
    Missile { floor_set: i32, use_defaults: bool },
    Category(GeneratorCategory),
    Bomb,
    Gold,
}

/// Generator-deck context. Implementations must perform the entire requested
/// item constructor/randomization before returning.
pub trait SpecialGeneratorContext {
    /// # Errors
    ///
    /// Returns the underlying version-pinned [`GeneratorError`] when category
    /// state or identity tables violate their canonical invariants.
    fn generate(
        &mut self,
        request: SpecialGeneratorRequest,
        depth: i32,
        random: &mut RandomStack,
    ) -> Result<GeneratedItem, GeneratorError>;
}

impl SpecialGeneratorContext for GeneratorState {
    fn generate(
        &mut self,
        request: SpecialGeneratorRequest,
        depth: i32,
        random: &mut RandomStack,
    ) -> Result<GeneratedItem, GeneratorError> {
        match request {
            SpecialGeneratorRequest::Weapon {
                floor_set,
                use_defaults,
            } => generator::random_weapon(random, self, floor_set, use_defaults)
                .map(GeneratedItem::Equipment),
            SpecialGeneratorRequest::Armor { floor_set } => {
                generator::random_armor(random, floor_set).map(GeneratedItem::Equipment)
            }
            SpecialGeneratorRequest::Missile {
                floor_set,
                use_defaults,
            } => generator::random_missile(random, self, floor_set, use_defaults)
                .map(GeneratedItem::Missile),
            SpecialGeneratorRequest::Category(category) => {
                generator::random_category(random, self, category, depth)
            }
            SpecialGeneratorRequest::Bomb => Ok(generator::random_bomb(random)),
            SpecialGeneratorRequest::Gold => Ok(generator::random_gold(random, depth)),
        }
    }
}

/// `itemsToSpawn`, challenge, trinket, and reward-choice integration.
pub trait SpecialPrizeContext {
    fn take_prize_item(&mut self, random: &mut RandomStack) -> Option<SpecialItem>;
    fn take_trinket_catalyst(&mut self) -> Option<SpecialItem>;
    fn add_item_to_spawn(&mut self, item: SpecialItem);
    fn is_item_blocked(&self, item: &SpecialItem) -> bool;
    fn rat_skull_level(&self) -> i32;
    fn mimic_tooth_level(&self) -> i32;
    fn allocate_choice_group(&mut self) -> u16;
}

/// Canonical insertion-ordered prize pool for the no-challenge profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrizePool {
    pub items_to_spawn: Vec<SpecialItem>,
    pub rat_skull_level: i32,
    pub mimic_tooth_level: i32,
    pub next_choice_group: u16,
}

impl PrizePool {
    #[must_use]
    pub const fn empty(next_choice_group: u16) -> Self {
        Self {
            items_to_spawn: Vec::new(),
            rat_skull_level: -1,
            mimic_tooth_level: -1,
            next_choice_group,
        }
    }

    #[must_use]
    pub fn from_paint_items(items: Vec<PaintItem>, next_choice_group: u16) -> Self {
        Self {
            items_to_spawn: items.into_iter().map(SpecialItem::Paint).collect(),
            ..Self::empty(next_choice_group)
        }
    }
}

impl SpecialPrizeContext for PrizePool {
    fn take_prize_item(&mut self, random: &mut RandomStack) -> Option<SpecialItem> {
        if let Some(index) = self
            .items_to_spawn
            .iter()
            .position(|item| *item == SpecialItem::Paint(PaintItem::TrinketCatalyst))
        {
            return Some(self.items_to_spawn.remove(index));
        }
        if self.items_to_spawn.is_empty() {
            return None;
        }
        let bound =
            i32::try_from(self.items_to_spawn.len()).expect("Level.itemsToSpawn exceeds Java int");
        let index = usize::try_from(random.int_bound(bound)).expect("Random.Int is non-negative");
        Some(self.items_to_spawn.remove(index))
    }

    fn take_trinket_catalyst(&mut self) -> Option<SpecialItem> {
        self.items_to_spawn
            .iter()
            .position(|item| *item == SpecialItem::Paint(PaintItem::TrinketCatalyst))
            .map(|index| self.items_to_spawn.remove(index))
    }

    fn add_item_to_spawn(&mut self, item: SpecialItem) {
        self.items_to_spawn.push(item);
    }

    fn is_item_blocked(&self, _item: &SpecialItem) -> bool {
        // v3.3.8 challenges block only Dewdrops, which this loot pool cannot hold.
        false
    }

    fn rat_skull_level(&self) -> i32 {
        self.rat_skull_level
    }

    fn mimic_tooth_level(&self) -> i32 {
        self.mimic_tooth_level
    }

    fn allocate_choice_group(&mut self) -> u16 {
        let group = self.next_choice_group;
        self.next_choice_group = self.next_choice_group.wrapping_add(1);
        group
    }
}

/// Search-visible result of painting one handled room.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpecialPaintReport {
    pub searchable_items: Vec<WorldItem>,
}

/// The composite dispatcher can probe this helper without treating unrelated
/// special/secret/quest classes as errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpecialPaintOutcome {
    NotHandled,
    Painted(SpecialPaintReport),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpecialPaintError {
    MissingRoom(RoomId),
    MissingEntrance(RoomId),
    MissingReverseEntrance { room: RoomId, neighbour: RoomId },
    Generator(GeneratorError),
}

impl fmt::Display for SpecialPaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::MissingRoom(room) => write!(formatter, "room {room} does not exist"),
            Self::MissingEntrance(room) => {
                write!(formatter, "special room {room} has no placed entrance")
            }
            Self::MissingReverseEntrance { room, neighbour } => write!(
                formatter,
                "special room {room} entrance has no reverse connection from {neighbour}"
            ),
            Self::Generator(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SpecialPaintError {}

impl From<GeneratorError> for SpecialPaintError {
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
    report: SpecialPaintReport,
}

impl<L, G, P> PaintInputs<'_, L, G, P>
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    fn depth_i32(&self) -> i32 {
        i32::try_from(self.level.depth()).expect("dungeon depth fits Java int")
    }

    fn floor_set(&self) -> i32 {
        self.depth_i32() / 5
    }

    fn generate(
        &mut self,
        request: SpecialGeneratorRequest,
    ) -> Result<SpecialItem, GeneratorError> {
        self.generator
            .generate(request, self.depth_i32(), self.random)
            .map(SpecialItem::generated)
    }

    fn set_door(&mut self, door_type: DoorType) {
        set_shared_door_type(self.rooms, self.room, self.entrance.neighbour, door_type);
    }

    fn spawn(&mut self, item: SpecialItem) {
        self.prizes.add_item_to_spawn(item);
        self.level.record(SpecialPaintEvent::SpawnItem(item));
    }

    fn drop(
        &mut self,
        cell: usize,
        heap: SpecialHeapKind,
        item: SpecialItem,
        source: ItemSource,
        accessibility: Accessibility,
    ) {
        self.record_searchable(item, source, accessibility);
        self.level.record(SpecialPaintEvent::Drop {
            cell,
            heap,
            reward: SpecialReward {
                item,
                accessibility,
            },
        });
    }

    fn mob(
        &mut self,
        cell: usize,
        kind: SpecialMobKind,
        carried: Vec<SpecialReward>,
        source: ItemSource,
    ) {
        for reward in &carried {
            self.record_searchable(reward.item, source, reward.accessibility);
        }
        self.level.record(SpecialPaintEvent::Mob {
            cell,
            kind,
            carried,
        });
    }

    fn blob(
        &mut self,
        cell: usize,
        kind: SpecialBlobKind,
        volume: i32,
        prize: Option<SpecialReward>,
        source: ItemSource,
    ) {
        if let Some(reward) = prize {
            self.record_searchable(reward.item, source, reward.accessibility);
        }
        self.level.record(SpecialPaintEvent::Blob {
            cell,
            kind,
            volume,
            prize,
        });
    }

    fn record_searchable(
        &mut self,
        item: SpecialItem,
        source: ItemSource,
        accessibility: Accessibility,
    ) {
        let depth = u8::try_from(self.level.depth()).expect("dungeon depth fits search model");
        if let Some(world_item) = searchable_world_item(item, depth, source, accessibility) {
            self.report.searchable_items.push(world_item);
        }
    }
}

/// Paints one of the nine `SpecialRoom.EQUIP_SPECIALS` classes.
///
/// Unrelated room kinds, including Laboratory and every consumable-heavy
/// special, return [`SpecialPaintOutcome::NotHandled`] without reading graph,
/// level, generator, prize, or RNG state.
///
/// # Errors
///
/// Returns [`SpecialPaintError`] when a handled room lacks its shared entrance
/// door or when its generator context reports an invariant failure.
pub fn paint_equipment_special<L, G, P>(
    level: &mut L,
    rooms: &mut [Room],
    room: RoomId,
    generator: &mut G,
    prizes: &mut P,
    random: &mut RandomStack,
) -> Result<SpecialPaintOutcome, SpecialPaintError>
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    let selected = rooms
        .get(room)
        .ok_or(SpecialPaintError::MissingRoom(room))?;
    let RoomKind::Special(kind) = selected.kind else {
        return Ok(SpecialPaintOutcome::NotHandled);
    };
    if !is_equipment_special(kind) {
        return Ok(SpecialPaintOutcome::NotHandled);
    }

    let connection = selected
        .connected
        .first()
        .ok_or(SpecialPaintError::MissingEntrance(room))?;
    let entrance = Entrance {
        neighbour: connection.room,
        point: connection
            .door
            .ok_or(SpecialPaintError::MissingEntrance(room))?
            .point,
    };
    if rooms
        .get(entrance.neighbour)
        .and_then(|neighbour| neighbour.connection_to(room))
        .and_then(|connection| connection.door)
        .is_none()
    {
        return Err(SpecialPaintError::MissingReverseEntrance {
            room,
            neighbour: entrance.neighbour,
        });
    }

    let bounds = selected.bounds;
    let mut inputs = PaintInputs {
        level,
        generator,
        prizes,
        rooms,
        room,
        bounds,
        entrance,
        random,
        report: SpecialPaintReport::default(),
    };

    match kind {
        SpecialRoomKind::WeakFloor => paint_weak_floor(&mut inputs),
        SpecialRoomKind::Crypt => paint_crypt(&mut inputs)?,
        SpecialRoomKind::Pool => paint_pool(&mut inputs)?,
        SpecialRoomKind::Armory => paint_armory(&mut inputs)?,
        SpecialRoomKind::Sentry => paint_sentry(&mut inputs)?,
        SpecialRoomKind::Statue => paint_statue(&mut inputs)?,
        SpecialRoomKind::CrystalVault => paint_crystal_vault(&mut inputs)?,
        SpecialRoomKind::CrystalChoice => paint_crystal_choice(&mut inputs)?,
        SpecialRoomKind::Sacrifice => paint_sacrifice(&mut inputs)?,
        _ => unreachable!("equipment-special predicate and dispatch disagree"),
    }

    Ok(SpecialPaintOutcome::Painted(inputs.report))
}

#[must_use]
pub const fn is_equipment_special(kind: SpecialRoomKind) -> bool {
    matches!(
        kind,
        SpecialRoomKind::WeakFloor
            | SpecialRoomKind::Crypt
            | SpecialRoomKind::Pool
            | SpecialRoomKind::Armory
            | SpecialRoomKind::Sentry
            | SpecialRoomKind::Statue
            | SpecialRoomKind::CrystalVault
            | SpecialRoomKind::CrystalChoice
            | SpecialRoomKind::Sacrifice
    )
}

fn fill_room<L: SpecialLevelContext>(level: &mut L, bounds: Rect, value: i32) {
    draw::fill(
        level.map_mut(),
        bounds.left,
        bounds.top,
        inclusive_width(bounds),
        inclusive_height(bounds),
        value,
    );
}

fn fill_room_margin<L: SpecialLevelContext>(level: &mut L, bounds: Rect, margin: i32, value: i32) {
    draw::fill(
        level.map_mut(),
        bounds.left.wrapping_add(margin),
        bounds.top.wrapping_add(margin),
        inclusive_width(bounds).wrapping_sub(margin.wrapping_mul(2)),
        inclusive_height(bounds).wrapping_sub(margin.wrapping_mul(2)),
        value,
    );
}

const fn inclusive_width(bounds: Rect) -> i32 {
    bounds.right.wrapping_sub(bounds.left).wrapping_add(1)
}

const fn inclusive_height(bounds: Rect) -> i32 {
    bounds.bottom.wrapping_sub(bounds.top).wrapping_add(1)
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

fn random_room_point(bounds: Rect, margin: i32, random: &mut RandomStack) -> Point {
    Point::new(
        random.int_range(
            bounds.left.wrapping_add(margin),
            bounds.right.wrapping_sub(margin),
        ),
        random.int_range(
            bounds.top.wrapping_add(margin),
            bounds.bottom.wrapping_sub(margin),
        ),
    )
}

fn point_to_cell<L: SpecialLevelContext>(level: &L, point: Point) -> usize {
    level.map().point_to_cell(point)
}

fn empty_room_constructor(random: &mut RandomStack) {
    random
        .chances(&[1.0, 0.0, 0.0])
        .expect("EmptyRoom's NORMAL size category is always selectable");
}

fn select_copy<T: Copy>(random: &mut RandomStack, values: &[T]) -> T {
    let bound = i32::try_from(values.len()).expect("fixed Java array length fits int");
    let index = usize::try_from(random.int_bound(bound)).expect("Random.Int is non-negative");
    values[index]
}

fn equipment_roll_mut(item: &mut SpecialItem) -> Option<&mut EquipmentRoll> {
    match item {
        SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Equipment(equipment))) => {
            Some(&mut equipment.roll)
        }
        SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Missile(missile))) => {
            Some(&mut missile.roll)
        }
        SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Ring(ring))) => Some(&mut ring.roll),
        SpecialItem::Paint(
            PaintItem::Generated(
                GeneratedItem::Artifact(_)
                | GeneratedItem::Food(_)
                | GeneratedItem::Potion { .. }
                | GeneratedItem::Seed(_)
                | GeneratedItem::Scroll { .. }
                | GeneratedItem::Stone(_)
                | GeneratedItem::Gold { .. }
                | GeneratedItem::Trinket(_)
                | GeneratedItem::Bomb(_)
                | GeneratedItem::TippedDart { .. },
            )
            | PaintItem::ArcaneStylus
            | PaintItem::TrinketCatalyst
            | PaintItem::Direct(_),
        )
        | SpecialItem::IronKey { .. }
        | SpecialItem::CrystalKey { .. }
        | SpecialItem::PotionOfInvisibility
        | SpecialItem::PotionOfHaste => None,
    }
}

fn set_item_cursed(item: &mut SpecialItem, cursed: bool) {
    if let Some(roll) = equipment_roll_mut(item) {
        roll.cursed = cursed;
        return;
    }
    if let SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Artifact(artifact))) = item {
        artifact.cursed = cursed;
    }
}

fn clear_curse_effect(item: &mut SpecialItem) {
    let Some(roll) = equipment_roll_mut(item) else {
        return;
    };
    if roll.effect.is_some_and(|effect| match effect {
        Effect::Weapon(effect) => effect.is_curse(),
        Effect::Armor(effect) => effect.is_curse(),
    }) {
        roll.effect = None;
    }
    roll.cursed = false;
}

fn upgrade_item(item: &mut SpecialItem) {
    if let Some(roll) = equipment_roll_mut(item) {
        roll.upgrade = roll.upgrade.wrapping_add(1);
    }
}

fn force_good_weapon_enchantment(item: &mut SpecialItem, random: &mut RandomStack) {
    let effect = random_weapon_enchantment(random);
    let SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Equipment(equipment))) = item else {
        panic!("Statue.createWeapon must receive a generated melee weapon");
    };
    assert_eq!(catalog_item(equipment.item).kind, ItemKind::Weapon);
    equipment.roll.cursed = false;
    equipment.roll.effect = Some(Effect::Weapon(effect));
}

fn force_good_armor_glyph(item: &mut SpecialItem, random: &mut RandomStack) {
    let effect = random_armor_glyph(random);
    let SpecialItem::Paint(PaintItem::Generated(GeneratedItem::Equipment(equipment))) = item else {
        panic!("ArmoredStatue must receive generated armor");
    };
    assert_eq!(catalog_item(equipment.item).kind, ItemKind::Armor);
    equipment.roll.cursed = false;
    equipment.roll.effect = Some(Effect::Armor(effect));
}

fn searchable_world_item(
    item: SpecialItem,
    depth: u8,
    source: ItemSource,
    accessibility: Accessibility,
) -> Option<WorldItem> {
    let SpecialItem::Paint(PaintItem::Generated(item)) = item else {
        return None;
    };
    item.searchable_equipment().map(|equipment| {
        WorldItem::from_equipment_roll(equipment.item, equipment.roll, depth, source, accessibility)
    })
}

#[allow(clippy::cast_precision_loss)] // Java converts the trinket level to float here.
const fn rat_skull_multiplier(level: i32) -> f32 {
    if level == -1 { 1.0 } else { 2.0 + level as f32 }
}

#[allow(clippy::cast_precision_loss)] // Java converts the trinket level to float here.
const fn mimic_tooth_multiplier(level: i32) -> f32 {
    if level == -1 {
        1.0
    } else {
        1.5 + 0.5 * level as f32
    }
}

#[allow(clippy::manual_midpoint)] // Preserve Java's `(chance + 0.1f) / 2f` order.
fn crystal_mimic_chance<P: SpecialPrizeContext>(prizes: &P) -> f32 {
    let mut chance = 0.1_f32 * rat_skull_multiplier(prizes.rat_skull_level());
    if chance > 0.1_f32 {
        chance = (chance + 0.1_f32) / 2.0_f32;
    }
    chance * mimic_tooth_multiplier(prizes.mimic_tooth_level())
}

fn piranha_alt_chance<P: SpecialPrizeContext>(prizes: &P) -> f32 {
    0.02_f32 * rat_skull_multiplier(prizes.rat_skull_level())
}

#[allow(clippy::manual_midpoint)] // Preserve Java's `(chance + 0.1f) / 2f` order.
fn statue_alt_chance<P: SpecialPrizeContext>(prizes: &P) -> f32 {
    let mut chance = 0.1_f32 * rat_skull_multiplier(prizes.rat_skull_level());
    if chance > 0.1_f32 {
        chance = (chance + 0.1_f32) / 2.0_f32;
    }
    chance
}

fn is_stealthy_mimic<P: SpecialPrizeContext>(prizes: &P) -> bool {
    prizes.mimic_tooth_level() >= 0
}

fn paint_weak_floor<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>)
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::CHASM);
    inputs.set_door(DoorType::Regular);

    let door = inputs.entrance.point;
    let bounds = inputs.bounds;
    let well = if door.x == bounds.left {
        for y in bounds.top.wrapping_add(1)..bounds.bottom {
            draw::draw_inside(
                inputs.level.map_mut(),
                bounds,
                Point::new(bounds.left, y),
                inputs
                    .random
                    .int_range(1, inclusive_width(bounds).wrapping_sub(4)),
                terrain::EMPTY_SP,
            );
        }
        Point::new(
            bounds.right.wrapping_sub(1),
            if inputs.random.int_bound(2) == 0 {
                bounds.top.wrapping_add(2)
            } else {
                bounds.bottom.wrapping_sub(1)
            },
        )
    } else if door.x == bounds.right {
        for y in bounds.top.wrapping_add(1)..bounds.bottom {
            draw::draw_inside(
                inputs.level.map_mut(),
                bounds,
                Point::new(bounds.right, y),
                inputs
                    .random
                    .int_range(1, inclusive_width(bounds).wrapping_sub(4)),
                terrain::EMPTY_SP,
            );
        }
        Point::new(
            bounds.left.wrapping_add(1),
            if inputs.random.int_bound(2) == 0 {
                bounds.top.wrapping_add(2)
            } else {
                bounds.bottom.wrapping_sub(1)
            },
        )
    } else if door.y == bounds.top {
        for x in bounds.left.wrapping_add(1)..bounds.right {
            draw::draw_inside(
                inputs.level.map_mut(),
                bounds,
                Point::new(x, bounds.top),
                inputs
                    .random
                    .int_range(1, inclusive_height(bounds).wrapping_sub(4)),
                terrain::EMPTY_SP,
            );
        }
        Point::new(
            if inputs.random.int_bound(2) == 0 {
                bounds.left.wrapping_add(1)
            } else {
                bounds.right.wrapping_sub(1)
            },
            bounds.bottom.wrapping_sub(1),
        )
    } else {
        for x in bounds.left.wrapping_add(1)..bounds.right {
            draw::draw_inside(
                inputs.level.map_mut(),
                bounds,
                Point::new(x, bounds.bottom),
                inputs
                    .random
                    .int_range(1, inclusive_height(bounds).wrapping_sub(4)),
                terrain::EMPTY_SP,
            );
        }
        Point::new(
            if inputs.random.int_bound(2) == 0 {
                bounds.left.wrapping_add(1)
            } else {
                bounds.right.wrapping_sub(1)
            },
            bounds.top.wrapping_add(2),
        )
    };

    draw::set_point(inputs.level.map_mut(), well, terrain::CHASM);
    inputs.level.record(SpecialPaintEvent::Feature {
        point: well,
        kind: SpecialFeatureKind::HiddenWell,
    });
    let cell = point_to_cell(inputs.level, well);
    inputs.blob(
        cell,
        SpecialBlobKind::WeakFloorWell,
        1,
        None,
        ItemSource::Heap,
    );
}

fn paint_crypt<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);

    let mut center = room_center(inputs.bounds, inputs.random);
    let door = inputs.entrance.point;
    let bounds = inputs.bounds;
    inputs.set_door(DoorType::Locked);
    inputs.spawn(SpecialItem::IronKey {
        depth: inputs.depth_i32(),
    });

    if door.x == bounds.left {
        draw::set_xy(
            inputs.level.map_mut(),
            bounds.right.wrapping_sub(1),
            bounds.top.wrapping_add(1),
            terrain::STATUE,
        );
        draw::set_xy(
            inputs.level.map_mut(),
            bounds.right.wrapping_sub(1),
            bounds.bottom.wrapping_sub(1),
            terrain::STATUE,
        );
        center.x = bounds.right.wrapping_sub(2);
    } else if door.x == bounds.right {
        draw::set_xy(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            terrain::STATUE,
        );
        draw::set_xy(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.bottom.wrapping_sub(1),
            terrain::STATUE,
        );
        center.x = bounds.left.wrapping_add(2);
    } else if door.y == bounds.top {
        draw::set_xy(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.bottom.wrapping_sub(1),
            terrain::STATUE,
        );
        draw::set_xy(
            inputs.level.map_mut(),
            bounds.right.wrapping_sub(1),
            bounds.bottom.wrapping_sub(1),
            terrain::STATUE,
        );
        center.y = bounds.bottom.wrapping_sub(2);
    } else {
        draw::set_xy(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            terrain::STATUE,
        );
        draw::set_xy(
            inputs.level.map_mut(),
            bounds.right.wrapping_sub(1),
            bounds.top.wrapping_add(1),
            terrain::STATUE,
        );
        center.y = bounds.top.wrapping_add(2);
    }

    let floor_set = inputs.floor_set().wrapping_add(1);
    let mut prize = inputs.generate(SpecialGeneratorRequest::Armor { floor_set })?;
    if inputs.prizes.is_item_blocked(&prize) {
        prize = inputs.generate(SpecialGeneratorRequest::Gold)?;
    } else {
        let curse = select_copy(inputs.random, &ARMOR_CURSES);
        let roll = equipment_roll_mut(&mut prize)
            .expect("CryptRoom always generated armor before applying its curse");
        if !roll.cursed {
            roll.upgrade = roll.upgrade.wrapping_add(1);
            let has_good_glyph = matches!(
                roll.effect,
                Some(Effect::Armor(effect)) if !effect.is_curse()
            );
            if !has_good_glyph {
                roll.effect = Some(Effect::Armor(curse));
            }
        }
        roll.cursed = true;
    }
    let cell = point_to_cell(inputs.level, center);
    inputs.drop(
        cell,
        SpecialHeapKind::Tomb,
        prize,
        ItemSource::Tomb,
        Accessibility::Independent,
    );
    Ok(())
}

fn generated_high_prize<L, G, P>(
    inputs: &mut PaintInputs<'_, L, G, P>,
    prize_chance_bound: i32,
) -> Result<SpecialItem, GeneratorError>
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    if inputs.random.int_bound(prize_chance_bound) == 0 {
        if let Some(prize) = inputs.prizes.take_prize_item(inputs.random) {
            return Ok(prize);
        }
    }

    let floor_set = inputs.floor_set().wrapping_add(1);
    let request = match inputs.random.int_bound(5) {
        0 | 1 => SpecialGeneratorRequest::Weapon {
            floor_set,
            use_defaults: false,
        },
        2 => SpecialGeneratorRequest::Missile {
            floor_set,
            use_defaults: false,
        },
        _ => SpecialGeneratorRequest::Armor { floor_set },
    };
    let mut prize = inputs.generate(request)?;
    clear_curse_effect(&mut prize);
    if inputs.random.int_bound(3) == 0 {
        upgrade_item(&mut prize);
    }
    Ok(prize)
}

fn paint_pool<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::WATER);
    inputs.set_door(DoorType::Regular);

    let bounds = inputs.bounds;
    let door = inputs.entrance.point;
    let prize_point = if door.x == bounds.left {
        draw::fill(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            1,
            inclusive_height(bounds).wrapping_sub(2),
            terrain::EMPTY_SP,
        );
        Point::new(
            bounds.right.wrapping_sub(1),
            bounds.top.wrapping_add(inclusive_height(bounds) / 2),
        )
    } else if door.x == bounds.right {
        draw::fill(
            inputs.level.map_mut(),
            bounds.right.wrapping_sub(1),
            bounds.top.wrapping_add(1),
            1,
            inclusive_height(bounds).wrapping_sub(2),
            terrain::EMPTY_SP,
        );
        Point::new(
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(inclusive_height(bounds) / 2),
        )
    } else if door.y == bounds.top {
        draw::fill(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            inclusive_width(bounds).wrapping_sub(2),
            1,
            terrain::EMPTY_SP,
        );
        Point::new(
            bounds.left.wrapping_add(inclusive_width(bounds) / 2),
            bounds.bottom.wrapping_sub(1),
        )
    } else {
        draw::fill(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.bottom.wrapping_sub(1),
            inclusive_width(bounds).wrapping_sub(2),
            1,
            terrain::EMPTY_SP,
        );
        Point::new(
            bounds.left.wrapping_add(inclusive_width(bounds) / 2),
            bounds.top.wrapping_add(1),
        )
    };

    let cell = point_to_cell(inputs.level, prize_point);
    let prize = generated_high_prize(inputs, 3)?;
    inputs.drop(
        cell,
        SpecialHeapKind::Chest,
        prize,
        ItemSource::Chest,
        Accessibility::Independent,
    );
    draw::set_point(inputs.level.map_mut(), prize_point, terrain::PEDESTAL);
    inputs.spawn(SpecialItem::PotionOfInvisibility);

    for _ in 0..3 {
        let phantom = inputs.random.float() < piranha_alt_chance(inputs.prizes);
        let cell = loop {
            let point = random_room_point(bounds, 1, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if inputs.level.map().cells[cell] == terrain::WATER && !inputs.level.has_mob(cell) {
                break cell;
            }
        };
        inputs.mob(
            cell,
            if phantom {
                SpecialMobKind::PhantomPiranha
            } else {
                SpecialMobKind::Piranha
            },
            Vec::new(),
            ItemSource::Heap,
        );
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // Mirrors one branch-heavy upstream room method directly.
fn paint_armory<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);

    let bounds = inputs.bounds;
    let door = inputs.entrance.point;
    let statue = if door.x == bounds.left {
        Point::new(
            bounds.right.wrapping_sub(1),
            if inputs.random.int_bound(2) == 0 {
                bounds.top.wrapping_add(1)
            } else {
                bounds.bottom.wrapping_sub(1)
            },
        )
    } else if door.x == bounds.right {
        Point::new(
            bounds.left.wrapping_add(1),
            if inputs.random.int_bound(2) == 0 {
                bounds.top.wrapping_add(1)
            } else {
                bounds.bottom.wrapping_sub(1)
            },
        )
    } else if door.y == bounds.top {
        Point::new(
            if inputs.random.int_bound(2) == 0 {
                bounds.left.wrapping_add(1)
            } else {
                bounds.right.wrapping_sub(1)
            },
            bounds.bottom.wrapping_sub(1),
        )
    } else {
        Point::new(
            if inputs.random.int_bound(2) == 0 {
                bounds.left.wrapping_add(1)
            } else {
                bounds.right.wrapping_sub(1)
            },
            bounds.top.wrapping_add(1),
        )
    };
    draw::set_point(inputs.level.map_mut(), statue, terrain::STATUE);

    let count = inputs.random.int_range(2, 3);
    let mut categories = [1.0_f32; 4];
    for _ in 0..count {
        let cell = loop {
            let point = random_room_point(bounds, 1, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if inputs.level.map().cells[cell] == terrain::EMPTY && !inputs.level.has_heap(cell) {
                break cell;
            }
        };
        let category = inputs
            .random
            .chances(&categories)
            .expect("Armory always has an unused prize category");
        categories[category] = 0.0;
        let request = match category {
            0 => SpecialGeneratorRequest::Bomb,
            1 => SpecialGeneratorRequest::Weapon {
                floor_set: inputs.floor_set(),
                use_defaults: false,
            },
            2 => SpecialGeneratorRequest::Armor {
                floor_set: inputs.floor_set(),
            },
            _ => SpecialGeneratorRequest::Missile {
                floor_set: inputs.floor_set(),
                use_defaults: false,
            },
        };
        let prize = inputs.generate(request)?;
        inputs.drop(
            cell,
            SpecialHeapKind::Heap,
            prize,
            ItemSource::Heap,
            Accessibility::Independent,
        );
    }

    if let Some(catalyst) = inputs.prizes.take_trinket_catalyst() {
        let cell = loop {
            let point = random_room_point(bounds, 1, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if inputs.level.map().cells[cell] == terrain::EMPTY && !inputs.level.has_heap(cell) {
                break cell;
            }
        };
        inputs.drop(
            cell,
            SpecialHeapKind::Heap,
            catalyst,
            ItemSource::Heap,
            Accessibility::Independent,
        );
    }

    inputs.set_door(DoorType::Locked);
    inputs.spawn(SpecialItem::IronKey {
        depth: inputs.depth_i32(),
    });
    Ok(())
}

#[allow(clippy::too_many_lines)] // Four entrance orientations have intentionally distinct formulas.
fn paint_sentry<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY_SP);

    let bounds = inputs.bounds;
    let door = inputs.entrance.point;
    let center = loop {
        let center = room_center(bounds, inputs.random);
        if center.x != door.x && center.y != door.y {
            break center;
        }
    };

    let (sentry, treasure, danger_distance) = if door.x == bounds.left {
        draw::fill(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            1,
            inclusive_height(bounds).wrapping_sub(2),
            terrain::EMPTY,
        );
        let treasure = if door.y > center.y {
            draw::fill(
                inputs.level.map_mut(),
                bounds.left.wrapping_add(1),
                bounds.top.wrapping_add(1),
                2,
                center.y.wrapping_sub(bounds.top).wrapping_sub(1),
                terrain::EMPTY,
            );
            Point::new(
                bounds.left.wrapping_add(1),
                div_i32(bounds.top.wrapping_add(1).wrapping_add(center.y), 2),
            )
        } else {
            draw::fill(
                inputs.level.map_mut(),
                bounds.left.wrapping_add(1),
                center.y.wrapping_add(1),
                2,
                bounds.bottom.wrapping_sub(center.y).wrapping_sub(1),
                terrain::EMPTY,
            );
            Point::new(
                bounds.left.wrapping_add(1),
                div_i32(bounds.bottom.wrapping_add(center.y), 2),
            )
        };
        let mut x = bounds.right.wrapping_sub(3);
        while x > bounds.left {
            let point = Point::new(x, center.y);
            let cell = point_to_cell(inputs.level, point);
            let value = if inputs.level.map().cells[cell] == terrain::EMPTY_SP {
                terrain::STATUE_SP
            } else {
                terrain::STATUE
            };
            draw::set_point(inputs.level.map_mut(), point, value);
            x = x.wrapping_sub(1);
        }
        (
            Point::new(bounds.right.wrapping_sub(1), center.y),
            treasure,
            2_i32.wrapping_mul(inclusive_width(bounds).wrapping_sub(5)),
        )
    } else if door.x == bounds.right {
        draw::fill(
            inputs.level.map_mut(),
            bounds.right.wrapping_sub(1),
            bounds.top.wrapping_add(1),
            1,
            inclusive_height(bounds).wrapping_sub(2),
            terrain::EMPTY,
        );
        let treasure = if door.y > center.y {
            draw::fill(
                inputs.level.map_mut(),
                bounds.right.wrapping_sub(2),
                bounds.top.wrapping_add(1),
                2,
                center.y.wrapping_sub(bounds.top).wrapping_sub(1),
                terrain::EMPTY,
            );
            Point::new(
                bounds.right.wrapping_sub(1),
                div_i32(bounds.top.wrapping_add(1).wrapping_add(center.y), 2),
            )
        } else {
            draw::fill(
                inputs.level.map_mut(),
                bounds.right.wrapping_sub(2),
                center.y.wrapping_add(1),
                2,
                bounds.bottom.wrapping_sub(center.y).wrapping_sub(1),
                terrain::EMPTY,
            );
            Point::new(
                bounds.right.wrapping_sub(1),
                div_i32(bounds.bottom.wrapping_add(1).wrapping_add(center.y), 2),
            )
        };
        let mut x = bounds.left.wrapping_add(3);
        while x < bounds.right {
            let point = Point::new(x, center.y);
            let cell = point_to_cell(inputs.level, point);
            let value = if inputs.level.map().cells[cell] == terrain::EMPTY_SP {
                terrain::STATUE_SP
            } else {
                terrain::STATUE
            };
            draw::set_point(inputs.level.map_mut(), point, value);
            x = x.wrapping_add(1);
        }
        (
            Point::new(bounds.left.wrapping_add(1), center.y),
            treasure,
            2_i32.wrapping_mul(inclusive_width(bounds).wrapping_sub(5)),
        )
    } else if door.y == bounds.top {
        draw::fill(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            inclusive_width(bounds).wrapping_sub(2),
            1,
            terrain::EMPTY,
        );
        let treasure = if door.x > center.x {
            draw::fill(
                inputs.level.map_mut(),
                bounds.left.wrapping_add(1),
                bounds.top.wrapping_add(1),
                center.x.wrapping_sub(bounds.left).wrapping_sub(1),
                2,
                terrain::EMPTY,
            );
            Point::new(
                div_i32(bounds.left.wrapping_add(1).wrapping_add(center.x), 2),
                bounds.top.wrapping_add(1),
            )
        } else {
            draw::fill(
                inputs.level.map_mut(),
                center.x.wrapping_add(1),
                bounds.top.wrapping_add(1),
                bounds.right.wrapping_sub(center.x).wrapping_sub(1),
                2,
                terrain::EMPTY,
            );
            Point::new(
                div_i32(bounds.right.wrapping_add(center.x), 2),
                bounds.top.wrapping_add(1),
            )
        };
        let mut y = bounds.bottom.wrapping_sub(3);
        while y > bounds.top {
            let point = Point::new(center.x, y);
            let cell = point_to_cell(inputs.level, point);
            let value = if inputs.level.map().cells[cell] == terrain::EMPTY_SP {
                terrain::STATUE_SP
            } else {
                terrain::STATUE
            };
            draw::set_point(inputs.level.map_mut(), point, value);
            y = y.wrapping_sub(1);
        }
        (
            Point::new(center.x, bounds.bottom.wrapping_sub(1)),
            treasure,
            2_i32.wrapping_mul(inclusive_height(bounds).wrapping_sub(5)),
        )
    } else {
        draw::fill(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.bottom.wrapping_sub(1),
            inclusive_width(bounds).wrapping_sub(2),
            1,
            terrain::EMPTY,
        );
        let treasure = if door.x > center.x {
            draw::fill(
                inputs.level.map_mut(),
                bounds.left.wrapping_add(1),
                bounds.bottom.wrapping_sub(2),
                center.x.wrapping_sub(bounds.left).wrapping_sub(1),
                2,
                terrain::EMPTY,
            );
            Point::new(
                div_i32(bounds.left.wrapping_add(1).wrapping_add(center.x), 2),
                bounds.bottom.wrapping_sub(1),
            )
        } else {
            draw::fill(
                inputs.level.map_mut(),
                center.x.wrapping_add(1),
                bounds.bottom.wrapping_sub(2),
                bounds.right.wrapping_sub(center.x).wrapping_sub(1),
                2,
                terrain::EMPTY,
            );
            Point::new(
                div_i32(bounds.right.wrapping_add(center.x), 2),
                bounds.bottom.wrapping_sub(1),
            )
        };
        let mut y = bounds.top.wrapping_add(3);
        while y < bounds.bottom {
            let point = Point::new(center.x, y);
            let cell = point_to_cell(inputs.level, point);
            let value = if inputs.level.map().cells[cell] == terrain::EMPTY_SP {
                terrain::STATUE_SP
            } else {
                terrain::STATUE
            };
            draw::set_point(inputs.level.map_mut(), point, value);
            y = y.wrapping_add(1);
        }
        (
            Point::new(center.x, bounds.top.wrapping_add(1)),
            treasure,
            2_i32.wrapping_mul(inclusive_height(bounds).wrapping_sub(5)),
        )
    };

    draw::set_point(inputs.level.map_mut(), sentry, terrain::PEDESTAL);
    let sentry_cell = point_to_cell(inputs.level, sentry);
    // `new EmptyRoom()` assigned to Sentry.room consumes this otherwise unused draw.
    empty_room_constructor(inputs.random);
    #[allow(clippy::cast_precision_loss)]
    let charge_delay = danger_distance as f32 / 3.0_f32 + 0.1_f32;
    inputs.mob(
        sentry_cell,
        SpecialMobKind::Sentry {
            initial_charge_delay_bits: charge_delay.to_bits(),
        },
        Vec::new(),
        ItemSource::Heap,
    );

    draw::set_point(inputs.level.map_mut(), treasure, terrain::PEDESTAL);
    let treasure_cell = point_to_cell(inputs.level, treasure);
    let prize = generated_high_prize(inputs, 2)?;
    inputs.drop(
        treasure_cell,
        SpecialHeapKind::Chest,
        prize,
        ItemSource::Chest,
        Accessibility::Independent,
    );
    inputs.spawn(SpecialItem::PotionOfHaste);
    inputs.set_door(DoorType::Regular);
    Ok(())
}

fn paint_statue<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);

    let mut center = room_center(inputs.bounds, inputs.random);
    let bounds = inputs.bounds;
    let door = inputs.entrance.point;
    inputs.set_door(DoorType::Locked);
    inputs.spawn(SpecialItem::IronKey {
        depth: inputs.depth_i32(),
    });

    if door.x == bounds.left {
        draw::fill(
            inputs.level.map_mut(),
            bounds.right.wrapping_sub(1),
            bounds.top.wrapping_add(1),
            1,
            inclusive_height(bounds).wrapping_sub(2),
            terrain::STATUE,
        );
        center.x = bounds.right.wrapping_sub(2);
    } else if door.x == bounds.right {
        draw::fill(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            1,
            inclusive_height(bounds).wrapping_sub(2),
            terrain::STATUE,
        );
        center.x = bounds.left.wrapping_add(2);
    } else if door.y == bounds.top {
        draw::fill(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.bottom.wrapping_sub(1),
            inclusive_width(bounds).wrapping_sub(2),
            1,
            terrain::STATUE,
        );
        center.y = bounds.bottom.wrapping_sub(2);
    } else {
        draw::fill(
            inputs.level.map_mut(),
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            inclusive_width(bounds).wrapping_sub(2),
            1,
            terrain::STATUE,
        );
        center.y = bounds.top.wrapping_add(2);
    }

    let armored = inputs.random.float() < statue_alt_chance(inputs.prizes);
    let mut weapon = inputs.generate(SpecialGeneratorRequest::Weapon {
        floor_set: inputs.floor_set(),
        use_defaults: false,
    })?;
    force_good_weapon_enchantment(&mut weapon, inputs.random);

    let source = if armored {
        ItemSource::ArmoredStatue
    } else {
        ItemSource::Statue
    };
    let mut carried = vec![SpecialReward {
        item: weapon,
        accessibility: Accessibility::Independent,
    }];
    if armored {
        let mut armor = inputs.generate(SpecialGeneratorRequest::Armor {
            floor_set: inputs.floor_set(),
        })?;
        force_good_armor_glyph(&mut armor, inputs.random);
        carried.push(SpecialReward {
            item: armor,
            accessibility: Accessibility::Independent,
        });
    }
    let cell = point_to_cell(inputs.level, center);
    inputs.mob(
        cell,
        if armored {
            SpecialMobKind::ArmoredStatue
        } else {
            SpecialMobKind::Statue
        },
        carried,
        source,
    );
    Ok(())
}

fn paint_crystal_vault<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY_SP);
    fill_room_margin(inputs.level, inputs.bounds, 2, terrain::EMPTY);

    let center = room_center(inputs.bounds, inputs.random);
    let center_cell = point_to_cell(inputs.level, center);
    let mut categories = [
        GeneratorCategory::Wand,
        GeneratorCategory::Ring,
        GeneratorCategory::Artifact,
    ];
    inputs.random.shuffle_list(&mut categories);
    let first = inputs.generate(SpecialGeneratorRequest::Category(categories[0]))?;
    let mut second = inputs.generate(SpecialGeneratorRequest::Category(categories[1]))?;

    let door_cell = point_to_cell(inputs.level, inputs.entrance.point);
    let width = inputs.level.map().width;
    let circle = [
        -width - 1,
        -width,
        -width + 1,
        1,
        width + 1,
        width,
        width - 1,
        -1,
    ];
    let (first_cell, second_cell) = loop {
        let index =
            usize::try_from(inputs.random.int_bound(8)).expect("Random.Int is non-negative");
        let first_cell = usize::try_from(
            i32::try_from(center_cell)
                .expect("map cell fits Java int")
                .wrapping_add(circle[index]),
        )
        .expect("vault prize cell is non-negative");
        let second_cell = usize::try_from(
            i32::try_from(center_cell)
                .expect("map cell fits Java int")
                .wrapping_add(circle[(index + 4) % 8]),
        )
        .expect("vault prize cell is non-negative");
        if !adjacent(inputs.level.map(), first_cell, door_cell)
            && !adjacent(inputs.level.map(), second_cell, door_cell)
        {
            break (first_cell, second_cell);
        }
    };

    // Java drops the first chest immediately before this Float. Recording it
    // after the draw is observationally equivalent and lets the typed event
    // carry the final choice accessibility without a mutable backpatch API.
    let mimic = inputs.random.float() < crystal_mimic_chance(inputs.prizes);
    if mimic {
        inputs.drop(
            first_cell,
            SpecialHeapKind::CrystalChest,
            first,
            ItemSource::CrystalChest,
            Accessibility::Independent,
        );
        set_item_cursed(&mut second, false);
        inputs.mob(
            second_cell,
            SpecialMobKind::CrystalMimic {
                stealthy: is_stealthy_mimic(inputs.prizes),
            },
            vec![SpecialReward {
                item: second,
                accessibility: Accessibility::Independent,
            }],
            ItemSource::CrystalMimic,
        );
    } else {
        let group = inputs.prizes.allocate_choice_group();
        inputs.drop(
            first_cell,
            SpecialHeapKind::CrystalChest,
            first,
            ItemSource::CrystalChest,
            Accessibility::Choice { group, option: 0 },
        );
        inputs.drop(
            second_cell,
            SpecialHeapKind::CrystalChest,
            second,
            ItemSource::CrystalChest,
            Accessibility::Choice { group, option: 1 },
        );
    }

    draw::set(inputs.level.map_mut(), first_cell, terrain::PEDESTAL);
    draw::set(inputs.level.map_mut(), second_cell, terrain::PEDESTAL);
    inputs.spawn(SpecialItem::CrystalKey {
        depth: inputs.depth_i32(),
    });
    inputs.set_door(DoorType::Locked);
    inputs.spawn(SpecialItem::IronKey {
        depth: inputs.depth_i32(),
    });
    Ok(())
}

fn adjacent(map: &GridMap, first: usize, second: usize) -> bool {
    let first = map.cell_to_point(first);
    let second = map.cell_to_point(second);
    first.x.abs_diff(second.x).max(first.y.abs_diff(second.y)) == 1
}

#[allow(clippy::too_many_lines)] // Four orientation layouts are ported statement-for-statement.
fn paint_crystal_choice<L, G, P>(
    inputs: &mut PaintInputs<'_, L, G, P>,
) -> Result<(), GeneratorError>
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    let bounds = inputs.bounds;
    let entrance = inputs.entrance.point;

    // `new EmptyRoom()` for entry, room1, and room2.
    empty_room_constructor(inputs.random);
    empty_room_constructor(inputs.random);
    empty_room_constructor(inputs.random);

    let mut entry = Rect::default();
    let mut room1 = Rect::default();
    let mut room2 = Rect::default();
    if entrance.x == bounds.left {
        entry.set(
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            bounds.left.wrapping_add(2),
            bounds.bottom.wrapping_sub(1),
        );
        room1.set(
            entry.right.wrapping_add(2),
            bounds.top.wrapping_add(1),
            bounds.right.wrapping_sub(1),
            room_center(bounds, inputs.random).y.wrapping_sub(1),
        );
        room2.set(
            entry.right.wrapping_add(2),
            room1.bottom.wrapping_add(2),
            bounds.right.wrapping_sub(1),
            bounds.bottom.wrapping_sub(1),
        );
        draw::set_xy(
            inputs.level.map_mut(),
            entry.right.wrapping_add(1),
            div_i32(room1.top.wrapping_add(room1.bottom).wrapping_add(1), 2),
            terrain::CRYSTAL_DOOR,
        );
        draw::set_xy(
            inputs.level.map_mut(),
            entry.right.wrapping_add(1),
            div_i32(room2.top.wrapping_add(room2.bottom), 2),
            terrain::CRYSTAL_DOOR,
        );
    } else if entrance.y == bounds.top {
        entry.set(
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            bounds.right.wrapping_sub(1),
            bounds.top.wrapping_add(2),
        );
        room1.set(
            bounds.left.wrapping_add(1),
            entry.bottom.wrapping_add(2),
            room_center(bounds, inputs.random).x.wrapping_sub(1),
            bounds.bottom.wrapping_sub(1),
        );
        room2.set(
            room1.right.wrapping_add(2),
            entry.bottom.wrapping_add(2),
            bounds.right.wrapping_sub(1),
            bounds.bottom.wrapping_sub(1),
        );
        draw::set_xy(
            inputs.level.map_mut(),
            div_i32(room1.left.wrapping_add(room1.right).wrapping_add(1), 2),
            entry.bottom.wrapping_add(1),
            terrain::CRYSTAL_DOOR,
        );
        draw::set_xy(
            inputs.level.map_mut(),
            div_i32(room2.left.wrapping_add(room2.right), 2),
            entry.bottom.wrapping_add(1),
            terrain::CRYSTAL_DOOR,
        );
    } else if entrance.x == bounds.right {
        entry.set(
            bounds.right.wrapping_sub(2),
            bounds.top.wrapping_add(1),
            bounds.right.wrapping_sub(1),
            bounds.bottom.wrapping_sub(1),
        );
        draw::draw_line(
            inputs.level.map_mut(),
            Point::new(bounds.right.wrapping_sub(1), bounds.top.wrapping_add(1)),
            Point::new(bounds.right.wrapping_sub(1), bounds.bottom.wrapping_sub(1)),
            terrain::EMPTY,
        );
        room1.set(
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            entry.left.wrapping_sub(2),
            room_center(bounds, inputs.random).y.wrapping_sub(1),
        );
        room2.set(
            bounds.left.wrapping_add(1),
            room1.bottom.wrapping_add(2),
            entry.left.wrapping_sub(2),
            bounds.bottom.wrapping_sub(1),
        );
        draw::set_xy(
            inputs.level.map_mut(),
            entry.left.wrapping_sub(1),
            div_i32(room1.top.wrapping_add(room1.bottom).wrapping_add(1), 2),
            terrain::CRYSTAL_DOOR,
        );
        draw::set_xy(
            inputs.level.map_mut(),
            entry.left.wrapping_sub(1),
            div_i32(room2.top.wrapping_add(room2.bottom), 2),
            terrain::CRYSTAL_DOOR,
        );
    } else {
        entry.set(
            bounds.left.wrapping_add(1),
            bounds.bottom.wrapping_sub(2),
            bounds.right.wrapping_sub(1),
            bounds.bottom.wrapping_sub(1),
        );
        room1.set(
            bounds.left.wrapping_add(1),
            bounds.top.wrapping_add(1),
            room_center(bounds, inputs.random).x.wrapping_sub(1),
            entry.top.wrapping_sub(2),
        );
        room2.set(
            room1.right.wrapping_add(2),
            bounds.top.wrapping_add(1),
            bounds.right.wrapping_sub(1),
            entry.top.wrapping_sub(2),
        );
        draw::set_xy(
            inputs.level.map_mut(),
            div_i32(room1.left.wrapping_add(room1.right).wrapping_add(1), 2),
            entry.top.wrapping_sub(1),
            terrain::CRYSTAL_DOOR,
        );
        draw::set_xy(
            inputs.level.map_mut(),
            div_i32(room2.left.wrapping_add(room2.right), 2),
            entry.top.wrapping_sub(1),
            terrain::CRYSTAL_DOOR,
        );
    }

    fill_temporary_room(inputs.level, entry, terrain::EMPTY);
    fill_temporary_room(inputs.level, room1, terrain::EMPTY_SP);
    fill_temporary_room(inputs.level, room2, terrain::EMPTY_SP);

    if inputs.random.int_bound(2) == 0 {
        std::mem::swap(&mut room1, &mut room2);
    }
    let group = inputs.prizes.allocate_choice_group();
    let count = inputs.random.normal_int_range(3, 4);
    for _ in 0..count {
        let category = if inputs.random.int_bound(2) == 0 {
            GeneratorCategory::Potion
        } else {
            GeneratorCategory::Scroll
        };
        let reward = inputs.generate(SpecialGeneratorRequest::Category(category))?;
        let margin = i32::from(temporary_room_square(room1) >= 16);
        let cell = loop {
            let point = random_room_point(room1, margin, inputs.random);
            let cell = point_to_cell(inputs.level, point);
            if !inputs.level.has_heap(cell) {
                break cell;
            }
        };
        inputs.drop(
            cell,
            SpecialHeapKind::Heap,
            reward,
            ItemSource::Heap,
            Accessibility::Choice { group, option: 0 },
        );
    }

    let category = match inputs.random.int_bound(3) {
        0 => GeneratorCategory::Wand,
        1 => GeneratorCategory::Ring,
        _ => GeneratorCategory::Artifact,
    };
    let hidden = inputs.generate(SpecialGeneratorRequest::Category(category))?;
    let hidden_point = temporary_room_center(room2, inputs.random);
    let hidden_cell = point_to_cell(inputs.level, hidden_point);
    inputs.drop(
        hidden_cell,
        SpecialHeapKind::Chest,
        hidden,
        ItemSource::Chest,
        Accessibility::Choice { group, option: 1 },
    );

    inputs.spawn(SpecialItem::CrystalKey {
        depth: inputs.depth_i32(),
    });
    inputs.set_door(DoorType::Locked);
    inputs.spawn(SpecialItem::IronKey {
        depth: inputs.depth_i32(),
    });
    Ok(())
}

fn fill_temporary_room<L: SpecialLevelContext>(level: &mut L, room: Rect, value: i32) {
    draw::fill(
        level.map_mut(),
        room.left,
        room.top,
        inclusive_width(room),
        inclusive_height(room),
        value,
    );
}

const fn temporary_room_square(room: Rect) -> i32 {
    inclusive_width(room).wrapping_mul(inclusive_height(room))
}

fn temporary_room_center(room: Rect, random: &mut RandomStack) -> Point {
    room_center(room, random)
}

#[allow(clippy::too_many_lines)] // Path, cover, fire, and prize logic form one upstream method.
fn paint_sacrifice<L, G, P>(inputs: &mut PaintInputs<'_, L, G, P>) -> Result<(), GeneratorError>
where
    L: SpecialLevelContext,
    G: SpecialGeneratorContext,
    P: SpecialPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::CHASM);

    let bounds = inputs.bounds;
    let door = inputs.entrance.point;
    let mut center = room_center(bounds, inputs.random);
    if door.x == bounds.left || door.x == bounds.right {
        if door.y == center.y {
            center.y = center.y.wrapping_add(if inputs.random.int_bound(2) == 0 {
                -1
            } else {
                1
            });
        }
        let mut point = draw::draw_inside(
            inputs.level.map_mut(),
            bounds,
            door,
            door.x.wrapping_sub(center.x).wrapping_abs().wrapping_sub(2),
            terrain::EMPTY_SP,
        );
        while point.y != center.y {
            draw::set_point(inputs.level.map_mut(), point, terrain::EMPTY_SP);
            point.y = point
                .y
                .wrapping_add(if point.y < center.y { 1 } else { -1 });
        }
    } else {
        if door.x == center.x {
            center.x = center.x.wrapping_add(if inputs.random.int_bound(2) == 0 {
                -1
            } else {
                1
            });
        }
        let mut point = draw::draw_inside(
            inputs.level.map_mut(),
            bounds,
            door,
            door.y.wrapping_sub(center.y).wrapping_abs().wrapping_sub(2),
            terrain::EMPTY_SP,
        );
        while point.x != center.x {
            draw::set_point(inputs.level.map_mut(), point, terrain::EMPTY_SP);
            point.x = point
                .x
                .wrapping_add(if point.x < center.x { 1 } else { -1 });
        }
    }

    let mut statue = center;
    statue.x = statue.x.wrapping_sub(2);
    if statue.x > bounds.left {
        draw::set_point(inputs.level.map_mut(), statue, terrain::STATUE);
    }
    statue.x = statue.x.wrapping_add(2);
    statue.y = statue.y.wrapping_sub(2);
    if statue.y > bounds.top {
        draw::set_point(inputs.level.map_mut(), statue, terrain::STATUE);
    }
    statue.y = statue.y.wrapping_add(2);
    statue.x = statue.x.wrapping_add(2);
    if statue.x < bounds.right {
        draw::set_point(inputs.level.map_mut(), statue, terrain::STATUE);
    }
    statue.x = statue.x.wrapping_sub(2);
    statue.y = statue.y.wrapping_add(2);
    if statue.y < bounds.bottom {
        draw::set_point(inputs.level.map_mut(), statue, terrain::STATUE);
    }

    draw::fill(
        inputs.level.map_mut(),
        center.x.wrapping_sub(1),
        center.y.wrapping_sub(1),
        3,
        3,
        terrain::EMBERS,
    );
    draw::set_point(inputs.level.map_mut(), center, terrain::PEDESTAL);

    let floor_set = inputs.floor_set().wrapping_add(1);
    let mut prize = inputs.generate(SpecialGeneratorRequest::Weapon {
        floor_set,
        use_defaults: false,
    })?;
    if inputs.prizes.is_item_blocked(&prize) {
        prize = inputs.generate(SpecialGeneratorRequest::Gold)?;
    } else {
        let curse = select_copy(inputs.random, &WEAPON_CURSES);
        let roll = equipment_roll_mut(&mut prize)
            .expect("SacrificeRoom always generated a melee weapon before cursing it");
        if !roll.cursed {
            roll.upgrade = roll.upgrade.wrapping_add(1);
            let has_good_enchantment = matches!(
                roll.effect,
                Some(Effect::Weapon(effect)) if !effect.is_curse()
            );
            if !has_good_enchantment {
                roll.effect = Some(Effect::Weapon(curse));
            }
        }
        roll.cursed = true;
    }
    let cell = point_to_cell(inputs.level, center);
    inputs.blob(
        cell,
        SpecialBlobKind::SacrificialFire,
        6_i32.wrapping_add(inputs.depth_i32().wrapping_mul(4)),
        Some(SpecialReward {
            item: prize,
            accessibility: Accessibility::Independent,
        }),
        ItemSource::SacrificialFire,
    );
    inputs.set_door(DoorType::Empty);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ItemId;
    use crate::generator::BombKind;
    use crate::level::Feeling;
    use crate::room::{ConnectionRoomKind, Door, RoomConnection};
    use crate::run::RunState;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct EventShape {
        drops: Vec<(usize, SpecialHeapKind)>,
        mobs: Vec<(usize, SpecialMobKind)>,
        spawns: Vec<SpecialItem>,
        blobs: Vec<(usize, SpecialBlobKind, i32, bool)>,
        features: Vec<(Point, SpecialFeatureKind)>,
    }

    struct FixtureResult {
        map_hash: i32,
        door_type: DoorType,
        next: i64,
        report: SpecialPaintReport,
        events: Vec<SpecialPaintEvent>,
        pool: PrizePool,
    }

    fn paint_fixture(kind: SpecialRoomKind, outer_seed: i64) -> FixtureResult {
        let mut run = RunState::new(0);
        let mut random = RandomStack::with_base_seed(0);
        random.push(outer_seed);
        let mut level = Level::new(11, Feeling::None);
        level.set_size(11, 11);
        let mut events = Vec::new();
        let mut pool = PrizePool::empty(41);

        let mut special = Room::special(kind);
        special.bounds = Rect::new(2, 2, 8, 8);
        special.connected.push(RoomConnection {
            room: 1,
            door: Some(Door::new(Point::new(2, 4))),
        });
        let mut neighbour = Room::connection(ConnectionRoomKind::Tunnel);
        neighbour.connected.push(RoomConnection {
            room: 0,
            door: Some(Door::new(Point::new(2, 4))),
        });
        let mut rooms = vec![special, neighbour];

        let outcome = {
            let mut context = LevelPaintContext::new(&mut level, &mut events);
            paint_equipment_special(
                &mut context,
                &mut rooms,
                0,
                &mut run.generator,
                &mut pool,
                &mut random,
            )
            .unwrap()
        };
        let SpecialPaintOutcome::Painted(report) = outcome else {
            panic!("requested equipment room was not handled");
        };
        FixtureResult {
            map_hash: level.java_map_hash(),
            door_type: rooms[0].connected[0].door.unwrap().door_type,
            next: random.long(),
            report,
            events,
            pool,
        }
    }

    fn shape(events: &[SpecialPaintEvent]) -> EventShape {
        let mut result = EventShape {
            drops: Vec::new(),
            mobs: Vec::new(),
            spawns: Vec::new(),
            blobs: Vec::new(),
            features: Vec::new(),
        };
        for event in events {
            match event {
                SpecialPaintEvent::Drop { cell, heap, .. } => {
                    result.drops.push((*cell, *heap));
                }
                SpecialPaintEvent::Mob { cell, kind, .. } => {
                    result.mobs.push((*cell, *kind));
                }
                SpecialPaintEvent::SpawnItem(item) => result.spawns.push(*item),
                SpecialPaintEvent::Blob {
                    cell,
                    kind,
                    volume,
                    prize,
                } => result.blobs.push((*cell, *kind, *volume, prize.is_some())),
                SpecialPaintEvent::Feature { point, kind } => {
                    result.features.push((*point, *kind));
                }
            }
        }
        result.drops.sort_unstable_by_key(|drop| drop.0);
        result.mobs.sort_unstable_by_key(|mob| mob.0);
        result
    }

    fn world_item(
        item: ItemId,
        upgrade: u8,
        effect: Option<Effect>,
        cursed: bool,
        source: ItemSource,
        accessibility: Accessibility,
    ) -> WorldItem {
        WorldItem {
            item,
            transmuted_item: None,
            upgrade,
            effect,
            cursed,
            depth: 11,
            source,
            accessibility,
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One table pins all nine official Java painters.
    fn all_equipment_specials_match_official_java_fixtures() {
        let cases = [
            (
                SpecialRoomKind::WeakFloor,
                0,
                1_571_022_701,
                DoorType::Regular,
                -8_292_973_307_042_192_125,
            ),
            (
                SpecialRoomKind::Crypt,
                0,
                -536_192_048,
                DoorType::Locked,
                5_700_976_833_288_827_063,
            ),
            (
                SpecialRoomKind::Pool,
                0,
                -1_006_474_149,
                DoorType::Regular,
                -1_083_761_183_081_836_303,
            ),
            (
                SpecialRoomKind::Armory,
                0,
                -727_037_000,
                DoorType::Locked,
                2_377_732_757_510_138_102,
            ),
            (
                SpecialRoomKind::Sentry,
                0,
                -1_429_008_949,
                DoorType::Regular,
                -7_423_979_211_207_825_555,
            ),
            (
                SpecialRoomKind::Statue,
                0,
                824_035_768,
                DoorType::Locked,
                -7_423_979_211_207_825_555,
            ),
            (
                SpecialRoomKind::CrystalVault,
                2,
                -1_253_026_420,
                DoorType::Locked,
                1_020_582_816_453_570_829,
            ),
            (
                SpecialRoomKind::CrystalChoice,
                1,
                1_683_799_629,
                DoorType::Locked,
                4_274_990_748_576_512_957,
            ),
            (
                SpecialRoomKind::Sacrifice,
                0,
                1_664_818_880,
                DoorType::Empty,
                5_700_976_833_288_827_063,
            ),
        ];

        for (kind, outer_seed, map_hash, door_type, next) in cases {
            let result = paint_fixture(kind, outer_seed);
            assert_eq!(result.map_hash, map_hash, "{kind:?} map");
            assert_eq!(result.door_type, door_type, "{kind:?} door");
            assert_eq!(result.next, next, "{kind:?} RNG checkpoint");

            let expected_items = expected_items(kind);
            assert_eq!(
                result.report.searchable_items, expected_items,
                "{kind:?} searchable items"
            );
            assert_eq!(
                shape(&result.events),
                expected_shape(kind),
                "{kind:?} events"
            );
        }
    }

    fn expected_items(kind: SpecialRoomKind) -> Vec<WorldItem> {
        match kind {
            SpecialRoomKind::WeakFloor => Vec::new(),
            SpecialRoomKind::Crypt => vec![world_item(
                ItemId::PlateArmor,
                1,
                Some(Effect::Armor(ArmorEffect::Stench)),
                true,
                ItemSource::Tomb,
                Accessibility::Independent,
            )],
            SpecialRoomKind::Pool => vec![world_item(
                ItemId::ScaleArmor,
                0,
                None,
                false,
                ItemSource::Chest,
                Accessibility::Independent,
            )],
            SpecialRoomKind::Armory => vec![
                world_item(
                    ItemId::MailArmor,
                    0,
                    None,
                    false,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
                world_item(
                    ItemId::Greatshield,
                    0,
                    None,
                    false,
                    ItemSource::Heap,
                    Accessibility::Independent,
                ),
            ],
            SpecialRoomKind::Sentry => vec![world_item(
                ItemId::PlateArmor,
                1,
                None,
                false,
                ItemSource::Chest,
                Accessibility::Independent,
            )],
            SpecialRoomKind::Statue => vec![world_item(
                ItemId::Crossbow,
                1,
                Some(Effect::Weapon(WeaponEffect::Lucky)),
                false,
                ItemSource::Statue,
                Accessibility::Independent,
            )],
            SpecialRoomKind::CrystalVault => vec![world_item(
                ItemId::WandFrost,
                0,
                None,
                true,
                ItemSource::CrystalChest,
                Accessibility::Choice {
                    group: 41,
                    option: 1,
                },
            )],
            SpecialRoomKind::CrystalChoice => vec![world_item(
                ItemId::WandFrost,
                1,
                None,
                false,
                ItemSource::Chest,
                Accessibility::Choice {
                    group: 41,
                    option: 1,
                },
            )],
            SpecialRoomKind::Sacrifice => vec![world_item(
                ItemId::Greatshield,
                1,
                Some(Effect::Weapon(WeaponEffect::Wayward)),
                true,
                ItemSource::SacrificialFire,
                Accessibility::Independent,
            )],
            _ => unreachable!(),
        }
    }

    fn expected_shape(kind: SpecialRoomKind) -> EventShape {
        match kind {
            SpecialRoomKind::WeakFloor => EventShape {
                drops: vec![],
                mobs: vec![],
                spawns: vec![],
                blobs: vec![(51, SpecialBlobKind::WeakFloorWell, 1, false)],
                features: vec![(Point::new(7, 4), SpecialFeatureKind::HiddenWell)],
            },
            SpecialRoomKind::Crypt => EventShape {
                drops: vec![(61, SpecialHeapKind::Tomb)],
                mobs: vec![],
                spawns: vec![SpecialItem::IronKey { depth: 11 }],
                blobs: vec![],
                features: vec![],
            },
            SpecialRoomKind::Pool => EventShape {
                drops: vec![(62, SpecialHeapKind::Chest)],
                mobs: vec![
                    (40, SpecialMobKind::Piranha),
                    (71, SpecialMobKind::Piranha),
                    (84, SpecialMobKind::Piranha),
                ],
                spawns: vec![SpecialItem::PotionOfInvisibility],
                blobs: vec![],
                features: vec![],
            },
            SpecialRoomKind::Armory => EventShape {
                drops: vec![
                    (37, SpecialHeapKind::Heap),
                    (62, SpecialHeapKind::Heap),
                    (71, SpecialHeapKind::Heap),
                ],
                mobs: vec![],
                spawns: vec![SpecialItem::IronKey { depth: 11 }],
                blobs: vec![],
                features: vec![],
            },
            SpecialRoomKind::Sentry => EventShape {
                drops: vec![(69, SpecialHeapKind::Chest)],
                mobs: vec![(
                    62,
                    SpecialMobKind::Sentry {
                        initial_charge_delay_bits: (4.0_f32 / 3.0_f32 + 0.1_f32).to_bits(),
                    },
                )],
                spawns: vec![SpecialItem::PotionOfHaste],
                blobs: vec![],
                features: vec![],
            },
            SpecialRoomKind::Statue => EventShape {
                drops: vec![],
                mobs: vec![(61, SpecialMobKind::Statue)],
                spawns: vec![SpecialItem::IronKey { depth: 11 }],
                blobs: vec![],
                features: vec![],
            },
            SpecialRoomKind::CrystalVault => EventShape {
                drops: vec![
                    (59, SpecialHeapKind::CrystalChest),
                    (61, SpecialHeapKind::CrystalChest),
                ],
                mobs: vec![],
                spawns: vec![
                    SpecialItem::CrystalKey { depth: 11 },
                    SpecialItem::IronKey { depth: 11 },
                ],
                blobs: vec![],
                features: vec![],
            },
            SpecialRoomKind::CrystalChoice => EventShape {
                drops: vec![
                    (40, SpecialHeapKind::Chest),
                    (72, SpecialHeapKind::Heap),
                    (83, SpecialHeapKind::Heap),
                    (84, SpecialHeapKind::Heap),
                ],
                mobs: vec![],
                spawns: vec![
                    SpecialItem::CrystalKey { depth: 11 },
                    SpecialItem::IronKey { depth: 11 },
                ],
                blobs: vec![],
                features: vec![],
            },
            SpecialRoomKind::Sacrifice => EventShape {
                drops: vec![],
                mobs: vec![],
                spawns: vec![],
                blobs: vec![(60, SpecialBlobKind::SacrificialFire, 50, true)],
                features: vec![],
            },
            _ => unreachable!(),
        }
    }

    #[test]
    fn crystal_mimic_prize_is_clean_and_independently_accessible() {
        let result = paint_fixture(SpecialRoomKind::CrystalVault, 0);
        assert_eq!(result.map_hash, -1_253_026_420);
        assert_eq!(result.next, -1_083_761_183_081_836_303);
        assert_eq!(result.pool.next_choice_group, 41);

        let [
            SpecialPaintEvent::Drop {
                cell: 59,
                reward: first,
                ..
            },
            SpecialPaintEvent::Mob {
                cell: 61,
                kind: SpecialMobKind::CrystalMimic { stealthy: false },
                carried,
            },
            SpecialPaintEvent::SpawnItem(SpecialItem::CrystalKey { depth: 11 }),
            SpecialPaintEvent::SpawnItem(SpecialItem::IronKey { depth: 11 }),
        ] = result.events.as_slice()
        else {
            panic!("seed-zero vault event shape changed");
        };
        assert_eq!(first.accessibility, Accessibility::Independent);
        assert_eq!(carried.len(), 1);
        assert_eq!(carried[0].accessibility, Accessibility::Independent);
        let mut carried_item = carried[0].item;
        assert!(!equipment_roll_mut(&mut carried_item).unwrap().cursed);
    }

    #[test]
    fn armored_statue_and_armory_missile_paths_match_java() {
        let statue = paint_fixture(SpecialRoomKind::Statue, 11);
        assert_eq!(statue.map_hash, 824_035_768);
        assert_eq!(statue.next, -1_218_090_000_690_447_877);
        assert_eq!(
            statue.report.searchable_items,
            [
                world_item(
                    ItemId::Crossbow,
                    0,
                    Some(Effect::Weapon(WeaponEffect::Grim)),
                    false,
                    ItemSource::ArmoredStatue,
                    Accessibility::Independent,
                ),
                world_item(
                    ItemId::MailArmor,
                    0,
                    Some(Effect::Armor(ArmorEffect::Stone)),
                    false,
                    ItemSource::ArmoredStatue,
                    Accessibility::Independent,
                ),
            ]
        );
        assert!(matches!(
            statue.events.as_slice(),
            [
                SpecialPaintEvent::SpawnItem(SpecialItem::IronKey { depth: 11 }),
                SpecialPaintEvent::Mob {
                    cell: 61,
                    kind: SpecialMobKind::ArmoredStatue,
                    carried,
                },
            ] if carried.len() == 2
        ));

        let armory = paint_fixture(SpecialRoomKind::Armory, 1);
        assert_eq!(armory.map_hash, -727_037_000);
        assert_eq!(armory.next, -1_272_370_737_395_698_989);
        let mut actual = armory.report.searchable_items;
        actual.sort_unstable_by_key(|item| item.item);
        let mut expected = vec![
            world_item(
                ItemId::Tomahawk,
                0,
                Some(Effect::Weapon(WeaponEffect::Shocking)),
                false,
                ItemSource::Heap,
                Accessibility::Independent,
            ),
            world_item(
                ItemId::MailArmor,
                0,
                None,
                false,
                ItemSource::Heap,
                Accessibility::Independent,
            ),
            world_item(
                ItemId::Sai,
                0,
                Some(Effect::Weapon(WeaponEffect::Explosive)),
                true,
                ItemSource::Heap,
                Accessibility::Independent,
            ),
        ];
        expected.sort_unstable_by_key(|item| item.item);
        assert_eq!(actual, expected);
    }

    #[test]
    fn unhandled_specials_leave_all_context_and_rng_state_untouched() {
        for kind in [
            SpecialRoomKind::Laboratory,
            SpecialRoomKind::DemonSpawner,
            SpecialRoomKind::Pit,
        ] {
            let mut run = RunState::new(0);
            let expected_generator = run.generator.clone();
            let mut level = Level::new(11, Feeling::None);
            level.set_size(11, 11);
            let expected_level = level.clone();
            let mut events = Vec::new();
            let mut pool = PrizePool::empty(9);
            let expected_pool = pool.clone();
            let mut rooms = vec![Room::special(kind)];
            let mut random = RandomStack::with_base_seed(0);
            random.push(17);
            let mut expected_random = random.clone();

            let outcome = {
                let mut context = LevelPaintContext::new(&mut level, &mut events);
                paint_equipment_special(
                    &mut context,
                    &mut rooms,
                    0,
                    &mut run.generator,
                    &mut pool,
                    &mut random,
                )
                .unwrap()
            };
            assert_eq!(outcome, SpecialPaintOutcome::NotHandled);
            assert_eq!(level, expected_level);
            assert_eq!(run.generator, expected_generator);
            assert_eq!(pool, expected_pool);
            assert!(events.is_empty());
            assert_eq!(random.long(), expected_random.long());
        }
    }

    #[test]
    fn prize_pool_prioritizes_catalyst_and_retains_spawned_keys() {
        let mut pool = PrizePool::from_paint_items(
            vec![
                PaintItem::Generated(GeneratedItem::Bomb(BombKind::Bomb)),
                PaintItem::TrinketCatalyst,
            ],
            5,
        );
        let mut random = RandomStack::with_base_seed(0);
        random.push(0);
        let mut untouched = random.clone();
        assert_eq!(
            pool.take_prize_item(&mut random),
            Some(SpecialItem::Paint(PaintItem::TrinketCatalyst))
        );
        assert_eq!(random.long(), untouched.long());

        pool.add_item_to_spawn(SpecialItem::IronKey { depth: 11 });
        assert_eq!(pool.items_to_spawn.len(), 2);
        assert!(pool.take_trinket_catalyst().is_none());
    }
}
