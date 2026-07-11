//! Exact v3.3.8 painting for the three forced/non-queue regular specials.
//!
//! `LaboratoryRoom`, `PitRoom`, and `ShopRoom` do not belong to either of
//! `SpecialRoom`'s shuffled equipment/consumable lists.  They also share two
//! stateful seams which are easy to lose in a seed port:
//!
//! * Laboratory prizes remove the first queued `TrinketCatalyst`, then the
//!   first queued `PotionOfStrength`, before falling back to `Generator`.
//! * Shop stock is generated and cached by the room while its minimum size is
//!   queried, then destructively removed as the painter places it.
//!
//! Pit's cross-floor transition is split exactly as upstream does it.  The
//! weak-floor selection schedules `pitNeededDepth`; painting the next floor's
//! Pit does not create a `LevelTransition`.  Later, `RegularLevel.fallCell`
//! scans the painted Pit rectangle in x-major/y-minor order and performs its
//! own gameplay-time random choice. [`crate::room_decks`] owns the former;
//! this module exposes the landing scan/choice without folding it into paint
//! RNG.

#![allow(clippy::missing_panics_doc)]

use std::fmt;

use crate::generator::{self, GeneratedItem, GeneratorError};
use crate::geometry::{GridMap, Point, Rect, painter as draw, terrain};
use crate::java_math::div_i32;
use crate::level::Level;
use crate::model::{Accessibility, ItemSource, WorldItem};
use crate::painter::set_shared_door_type;
use crate::regular_items::{QueuedItemKind, RegularItem};
use crate::rng::RandomStack;
use crate::room::{DoorType, Room, RoomId, RoomKind, SpecialRoomKind};
use crate::run::{GeneratorCategory, GeneratorState};
use crate::shop::{DirectShopItem, ShopError, ShopRoomCache, ShopRunState, ShopStockItem};

/// `Document.ALCHEMY_GUIDE.pageNames()` insertion order in v3.3.8.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum AlchemyGuidePage {
    Potions,
    Stones,
    EnergyFood,
    ExoticPotions,
    ExoticScrolls,
    Bombs,
    Weapons,
    BrewsElixirs,
    Spells,
}

impl AlchemyGuidePage {
    pub const ALL: [Self; 9] = [
        Self::Potions,
        Self::Stones,
        Self::EnergyFood,
        Self::ExoticPotions,
        Self::ExoticScrolls,
        Self::Bombs,
        Self::Weapons,
        Self::BrewsElixirs,
        Self::Spells,
    ];

    #[must_use]
    pub const fn java_name(self) -> &'static str {
        match self {
            Self::Potions => "Potions",
            Self::Stones => "Stones",
            Self::EnergyFood => "Energy_Food",
            Self::ExoticPotions => "Exotic_Potions",
            Self::ExoticScrolls => "Exotic_Scrolls",
            Self::Bombs => "Bombs",
            Self::Weapons => "Weapons",
            Self::BrewsElixirs => "Brews_Elixirs",
            Self::Spells => "Spells",
        }
    }
}

/// Item identities emitted by the forced-room painters.
///
/// Ordinary generated/queued objects reuse [`RegularItem`], while the three
/// room-only constructors retain their own payloads.  Shop entries are the
/// already-randomized values from [`crate::shop`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForcedItem {
    Regular(RegularItem),
    EnergyCrystal { quantity: i32 },
    AlchemyPage(AlchemyGuidePage),
    Shop(ShopStockItem),
}

impl From<RegularItem> for ForcedItem {
    fn from(item: RegularItem) -> Self {
        Self::Regular(item)
    }
}

/// Final heap type after one Java `Level.drop` call and any chained mutation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ForcedHeapKind {
    Heap,
    Skeleton,
    ForSale,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ForcedBlobKind {
    Alchemy,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ForcedMobKind {
    Shopkeeper,
}

/// One item and the branch under which it can be obtained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForcedReward {
    pub item: ForcedItem,
    pub accessibility: Accessibility,
}

/// Typed, operation-ordered side effects from one forced-room paint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForcedPaintEvent {
    Drop {
        cell: usize,
        heap: ForcedHeapKind,
        /// Heap-wide state immediately after this source operation.
        haunted: bool,
        reward: ForcedReward,
    },
    Mob {
        cell: usize,
        kind: ForcedMobKind,
    },
    SpawnItem(RegularItem),
    Blob {
        cell: usize,
        kind: ForcedBlobKind,
        volume: i32,
    },
    /// `RegularLevel.fallCell(true)` candidates after Pit paint.  Recording
    /// them consumes no RNG; the later selection is an independent event.
    PitFallCandidates(Vec<usize>),
}

/// Terrain/occupancy sink compatible with the other special-room painters.
pub trait ForcedLevelContext {
    fn depth(&self) -> u32;
    fn map(&self) -> &GridMap;
    fn map_mut(&mut self) -> &mut GridMap;
    fn has_heap(&self, cell: usize) -> bool;
    fn has_mob(&self, cell: usize) -> bool;
    fn record(&mut self, event: ForcedPaintEvent);
}

/// Adapter for the shared regular-painter [`Level`].
pub struct ForcedLevelPaintContext<'a> {
    pub level: &'a mut Level,
    pub events: &'a mut Vec<ForcedPaintEvent>,
}

impl<'a> ForcedLevelPaintContext<'a> {
    #[must_use]
    pub const fn new(level: &'a mut Level, events: &'a mut Vec<ForcedPaintEvent>) -> Self {
        Self { level, events }
    }
}

impl ForcedLevelContext for ForcedLevelPaintContext<'_> {
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

    fn record(&mut self, event: ForcedPaintEvent) {
        match &event {
            ForcedPaintEvent::Drop { cell, .. } => self.level.mark_heap(*cell),
            ForcedPaintEvent::Mob { cell, .. } => self.level.mark_mob(*cell),
            ForcedPaintEvent::SpawnItem(_)
            | ForcedPaintEvent::Blob { .. }
            | ForcedPaintEvent::PitFallCandidates(_) => {}
        }
        self.events.push(event);
    }
}

/// Exact queue/document queries made by `LaboratoryRoom` and the challenge
/// predicate queried by `PitRoom`.
pub trait ForcedPrizeContext {
    /// Remove the first queued object whose Java class is `TrinketCatalyst`.
    fn take_trinket_catalyst(&mut self) -> Option<RegularItem>;
    /// Remove the first queued object whose Java class is `PotionOfStrength`.
    fn take_strength_potion(&mut self) -> Option<RegularItem>;
    fn add_item_to_spawn(&mut self, item: RegularItem);
    fn is_item_blocked(&self, item: RegularItem) -> bool;
    fn is_alchemy_page_found(&self, page: AlchemyGuidePage) -> bool;
}

/// Canonical no-challenge queue plus explicit Alchemy Guide discovery state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForcedPrizePool {
    pub items_to_spawn: Vec<RegularItem>,
    alchemy_pages_found: u16,
}

impl ForcedPrizePool {
    const ALL_ALCHEMY_PAGES: u16 = (1_u16 << AlchemyGuidePage::ALL.len()) - 1;

    /// Canonical seed-finder profile: all journal pages have already been
    /// found, so guide drops do not perturb level RNG.
    #[must_use]
    pub const fn all_pages_found(items_to_spawn: Vec<RegularItem>) -> Self {
        Self {
            items_to_spawn,
            alchemy_pages_found: Self::ALL_ALCHEMY_PAGES,
        }
    }

    /// Profile with no Alchemy Guide pages found.
    #[must_use]
    pub const fn no_pages_found(items_to_spawn: Vec<RegularItem>) -> Self {
        Self {
            items_to_spawn,
            alchemy_pages_found: 0,
        }
    }

    pub fn set_alchemy_page_found(&mut self, page: AlchemyGuidePage, found: bool) {
        let bit = 1_u16 << page as u8;
        if found {
            self.alchemy_pages_found |= bit;
        } else {
            self.alchemy_pages_found &= !bit;
        }
    }
}

impl Default for ForcedPrizePool {
    fn default() -> Self {
        Self::all_pages_found(Vec::new())
    }
}

impl ForcedPrizeContext for ForcedPrizePool {
    fn take_trinket_catalyst(&mut self) -> Option<RegularItem> {
        take_first_queued(&mut self.items_to_spawn, QueuedItemKind::TrinketCatalyst)
    }

    fn take_strength_potion(&mut self) -> Option<RegularItem> {
        self.items_to_spawn
            .iter()
            .position(|item| {
                matches!(
                    item,
                    RegularItem::Queued(QueuedItemKind::PotionOfStrength)
                        | RegularItem::Generated(GeneratedItem::Potion {
                            kind: crate::run::PotionKind::Strength,
                            exotic: false,
                        })
                )
            })
            .map(|index| self.items_to_spawn.remove(index))
    }

    fn add_item_to_spawn(&mut self, item: RegularItem) {
        self.items_to_spawn.push(item);
    }

    fn is_item_blocked(&self, _item: RegularItem) -> bool {
        false
    }

    fn is_alchemy_page_found(&self, page: AlchemyGuidePage) -> bool {
        self.alchemy_pages_found & (1_u16 << page as u8) != 0
    }
}

fn take_first_queued(
    items: &mut Vec<RegularItem>,
    expected: QueuedItemKind,
) -> Option<RegularItem> {
    items
        .iter()
        .position(|item| *item == RegularItem::Queued(expected))
        .map(|index| items.remove(index))
}

/// Java `ShopRoom.itemsToSpawn`: cached generation plus destructive paint
/// progress.  The inner [`ShopRoomCache`] remains the single implementation of
/// stock generation and child-stream shuffling.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ForcedShopRoomState {
    pub cache: ShopRoomCache,
    items_to_spawn: Option<Vec<ShopStockItem>>,
}

impl ForcedShopRoomState {
    /// Build a room around an already-populated Java `itemsToSpawn` snapshot.
    /// This is useful for restored/instrumented rooms and geometry parity
    /// fixtures; ordinary generation should use [`Self::default`].
    #[must_use]
    pub fn with_items_to_spawn(items: Vec<ShopStockItem>) -> Self {
        Self {
            cache: ShopRoomCache::default(),
            items_to_spawn: Some(items),
        }
    }

    fn ensure_items(
        &mut self,
        random: &mut RandomStack,
        generator: &mut GeneratorState,
        depth: i32,
        run_state: &mut ShopRunState,
    ) -> Result<&mut Vec<ShopStockItem>, ShopError> {
        if self.items_to_spawn.is_none() {
            self.items_to_spawn = Some(
                self.cache
                    .items_to_spawn(random, generator, depth, run_state)?
                    .items
                    .clone(),
            );
        }
        self.items_to_spawn
            .as_mut()
            .ok_or(ShopError::CacheInvariant)
    }

    /// Exact `ShopRoom.spacesNeeded()`, including the four-slot sandbag
    /// allowance and the shopkeeper slot.
    ///
    /// # Errors
    ///
    /// Returns the underlying stock-generation error for unsupported depths
    /// or corrupted version-pinned `Generator` state.
    pub fn spaces_needed(
        &mut self,
        random: &mut RandomStack,
        generator: &mut GeneratorState,
        depth: i32,
        run_state: &mut ShopRunState,
    ) -> Result<i32, ShopError> {
        let items = self.ensure_items(random, generator, depth, run_state)?;
        let sandbags = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    ShopStockItem::Direct(DirectShopItem::HourglassSandBag)
                )
            })
            .count();
        let non_sandbags = items.len().saturating_sub(sandbags);
        let needed = non_sandbags.saturating_add(5);
        Ok(i32::try_from(needed).expect("shop stock fits Java int"))
    }

    /// Shared result of `ShopRoom.minWidth()` and `minHeight()`.
    ///
    /// # Errors
    ///
    /// Returns the same stock-generation errors as [`Self::spaces_needed`].
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub fn minimum_dimension(
        &mut self,
        random: &mut RandomStack,
        generator: &mut GeneratorState,
        depth: i32,
        run_state: &mut ShopRunState,
    ) -> Result<i32, ShopError> {
        let spaces = self.spaces_needed(random, generator, depth, run_state)?;
        Ok(((f64::from(spaces)).sqrt() as i32).wrapping_add(3).max(7))
    }

    #[must_use]
    pub fn remaining_items(&self) -> Option<&[ShopStockItem]> {
        self.items_to_spawn.as_deref()
    }
}

/// Search-visible and transition-visible result of one handled paint.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ForcedPaintReport {
    pub searchable_items: Vec<WorldItem>,
    /// Paint-time Pit candidates. Recompute with [`pit_fall_candidates`] after
    /// mob placement before performing the later gameplay landing choice.
    pub pit_fall_candidates: Vec<usize>,
    /// Canonical sizing leaves this empty. Upstream reports an exception but
    /// continues and retains these objects in `itemsToSpawn`.
    pub shop_unplaced_items: Vec<ShopStockItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForcedPaintOutcome {
    NotHandled,
    Painted(ForcedPaintReport),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForcedPaintError {
    MissingRoom(RoomId),
    MissingEntrance(RoomId),
    MissingReverseEntrance { room: RoomId, neighbour: RoomId },
    InvalidEntrance { room: RoomId, point: Point },
    Generator(GeneratorError),
    Shop(ShopError),
}

impl fmt::Display for ForcedPaintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::MissingRoom(room) => write!(formatter, "room {room} does not exist"),
            Self::MissingEntrance(room) => {
                write!(formatter, "forced room {room} has no placed entrance")
            }
            Self::MissingReverseEntrance { room, neighbour } => write!(
                formatter,
                "forced room {room} entrance has no reverse connection from {neighbour}"
            ),
            Self::InvalidEntrance { room, point } => write!(
                formatter,
                "forced room {room} entrance ({},{}) is not on one edge",
                point.x, point.y
            ),
            Self::Generator(error) => error.fmt(formatter),
            Self::Shop(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ForcedPaintError {}

impl From<GeneratorError> for ForcedPaintError {
    fn from(error: GeneratorError) -> Self {
        Self::Generator(error)
    }
}

impl From<ShopError> for ForcedPaintError {
    fn from(error: ShopError) -> Self {
        Self::Shop(error)
    }
}

#[derive(Clone, Copy)]
struct Entrance {
    neighbour: RoomId,
    point: Point,
}

struct PaintInputs<'a, L, P> {
    level: &'a mut L,
    generator: &'a mut GeneratorState,
    prizes: &'a mut P,
    shop_room: &'a mut ForcedShopRoomState,
    shop_run: &'a mut ShopRunState,
    rooms: &'a mut [Room],
    room: RoomId,
    bounds: Rect,
    entrance: Entrance,
    random: &'a mut RandomStack,
    report: ForcedPaintReport,
}

impl<L, P> PaintInputs<'_, L, P>
where
    L: ForcedLevelContext,
    P: ForcedPrizeContext,
{
    fn depth_i32(&self) -> i32 {
        i32::try_from(self.level.depth()).expect("dungeon depth fits Java int")
    }

    fn depth_u8(&self) -> u8 {
        u8::try_from(self.level.depth()).expect("main dungeon depth fits search model")
    }

    fn set_entrance(&mut self, door_type: DoorType) {
        set_shared_door_type(self.rooms, self.room, self.entrance.neighbour, door_type);
    }

    fn drop(
        &mut self,
        cell: usize,
        heap: ForcedHeapKind,
        haunted: bool,
        item: ForcedItem,
        source: ItemSource,
    ) {
        self.record_searchable(&item, source);
        self.level.record(ForcedPaintEvent::Drop {
            cell,
            heap,
            haunted,
            reward: ForcedReward {
                item,
                accessibility: Accessibility::Independent,
            },
        });
    }

    fn spawn(&mut self, item: RegularItem) {
        self.prizes.add_item_to_spawn(item);
        self.level.record(ForcedPaintEvent::SpawnItem(item));
    }

    fn record_searchable(&mut self, item: &ForcedItem, source: ItemSource) {
        let world = match item {
            ForcedItem::Regular(RegularItem::Generated(generated)) => searchable_generated_item(
                *generated,
                self.depth_u8(),
                source,
                Accessibility::Independent,
            ),
            ForcedItem::Shop(ShopStockItem::Searchable(world)) => Some(world.clone()),
            ForcedItem::Regular(RegularItem::Queued(_))
            | ForcedItem::EnergyCrystal { .. }
            | ForcedItem::AlchemyPage(_)
            | ForcedItem::Shop(ShopStockItem::Generated(_) | ShopStockItem::Direct(_)) => None,
        };
        if let Some(world) = world {
            self.report.searchable_items.push(world);
        }
    }
}

/// Paint one of Laboratory, Pit, or Shop.  Other room classes are a no-op and
/// read none of the graph, queue, Generator, shop, or RNG state.
///
/// # Errors
///
/// Returns an error for a malformed room graph, version-pinned `Generator`
/// invariant failure, or unsupported/corrupted shop stock generation.
#[allow(clippy::too_many_arguments)]
pub fn paint_forced_special<L, P>(
    level: &mut L,
    rooms: &mut [Room],
    room: RoomId,
    generator: &mut GeneratorState,
    prizes: &mut P,
    shop_room: &mut ForcedShopRoomState,
    shop_run: &mut ShopRunState,
    random: &mut RandomStack,
) -> Result<ForcedPaintOutcome, ForcedPaintError>
where
    L: ForcedLevelContext,
    P: ForcedPrizeContext,
{
    let selected = rooms.get(room).ok_or(ForcedPaintError::MissingRoom(room))?;
    let RoomKind::Special(kind) = selected.kind else {
        return Ok(ForcedPaintOutcome::NotHandled);
    };
    if !is_forced_special(kind) {
        return Ok(ForcedPaintOutcome::NotHandled);
    }

    let connection = selected
        .connected
        .first()
        .ok_or(ForcedPaintError::MissingEntrance(room))?;
    let entrance = Entrance {
        neighbour: connection.room,
        point: connection
            .door
            .ok_or(ForcedPaintError::MissingEntrance(room))?
            .point,
    };
    if rooms
        .get(entrance.neighbour)
        .and_then(|neighbour| neighbour.connection_to(room))
        .and_then(|connection| connection.door)
        .is_none()
    {
        return Err(ForcedPaintError::MissingReverseEntrance {
            room,
            neighbour: entrance.neighbour,
        });
    }

    let bounds = selected.bounds;
    if !on_exactly_one_edge(bounds, entrance.point) {
        return Err(ForcedPaintError::InvalidEntrance {
            room,
            point: entrance.point,
        });
    }
    let mut inputs = PaintInputs {
        level,
        generator,
        prizes,
        shop_room,
        shop_run,
        rooms,
        room,
        bounds,
        entrance,
        random,
        report: ForcedPaintReport::default(),
    };

    match kind {
        SpecialRoomKind::Laboratory => paint_laboratory(&mut inputs)?,
        SpecialRoomKind::Pit => paint_pit(&mut inputs)?,
        SpecialRoomKind::Shop => paint_shop(&mut inputs)?,
        _ => unreachable!("forced-special predicate and dispatch disagree"),
    }

    Ok(ForcedPaintOutcome::Painted(inputs.report))
}

#[must_use]
pub const fn is_forced_special(kind: SpecialRoomKind) -> bool {
    matches!(
        kind,
        SpecialRoomKind::Laboratory | SpecialRoomKind::Pit | SpecialRoomKind::Shop
    )
}

fn paint_laboratory<L, P>(inputs: &mut PaintInputs<'_, L, P>) -> Result<(), GeneratorError>
where
    L: ForcedLevelContext,
    P: ForcedPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY_SP);

    let pot = opposite_corner(inputs.bounds, inputs.entrance.point, inputs.random)
        .expect("validated entrance lies on exactly one edge");
    draw::set_point(inputs.level.map_mut(), pot, terrain::ALCHEMY);
    let pot_cell = point_to_cell(inputs.level, pot);
    inputs.level.record(ForcedPaintEvent::Blob {
        cell: pot_cell,
        kind: ForcedBlobKind::Alchemy,
        volume: 1,
    });

    let energy_cell = random_empty_lab_cell(inputs.level, inputs.bounds, inputs.random);
    inputs.drop(
        energy_cell,
        ForcedHeapKind::Heap,
        false,
        ForcedItem::EnergyCrystal { quantity: 5 },
        ItemSource::Heap,
    );

    let prize_count = inputs.random.normal_int_range(1, 2);
    for _ in 0..prize_count {
        let cell = random_empty_lab_cell(inputs.level, inputs.bounds, inputs.random);
        let prize = laboratory_prize(inputs)?;
        inputs.drop(
            cell,
            ForcedHeapKind::Heap,
            false,
            ForcedItem::Regular(prize),
            ItemSource::Heap,
        );
    }

    let missing_pages: Vec<AlchemyGuidePage> = AlchemyGuidePage::ALL
        .into_iter()
        .filter(|page| !inputs.prizes.is_alchemy_page_found(*page))
        .collect();
    let chapter = 1_i32.wrapping_add(inputs.depth_i32() / 5);
    let chapter_target = if missing_pages.len() <= 5 { 2 } else { 1 };
    if !missing_pages.is_empty() && chapter >= chapter_target {
        let pages_to_drop = missing_pages
            .len()
            .min(usize::try_from((chapter - chapter_target) + 1).unwrap_or_default());
        for page in missing_pages.into_iter().take(pages_to_drop) {
            let cell = random_empty_lab_cell(inputs.level, inputs.bounds, inputs.random);
            inputs.drop(
                cell,
                ForcedHeapKind::Heap,
                false,
                ForcedItem::AlchemyPage(page),
                ItemSource::Heap,
            );
        }
    }

    inputs.set_entrance(DoorType::Locked);
    inputs.spawn(RegularItem::Queued(QueuedItemKind::IronKey {
        depth: inputs.depth_u8(),
    }));
    Ok(())
}

fn laboratory_prize<L, P>(inputs: &mut PaintInputs<'_, L, P>) -> Result<RegularItem, GeneratorError>
where
    L: ForcedLevelContext,
    P: ForcedPrizeContext,
{
    if let Some(item) = inputs.prizes.take_trinket_catalyst() {
        return Ok(item);
    }
    if let Some(item) = inputs.prizes.take_strength_potion() {
        return Ok(item);
    }
    let category = if inputs.random.int_bound(2) == 0 {
        GeneratorCategory::Potion
    } else {
        GeneratorCategory::Stone
    };
    generator::random_category(
        inputs.random,
        inputs.generator,
        category,
        inputs.depth_i32(),
    )
    .map(RegularItem::Generated)
}

fn paint_pit<L, P>(inputs: &mut PaintInputs<'_, L, P>) -> Result<(), GeneratorError>
where
    L: ForcedLevelContext,
    P: ForcedPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY);
    inputs.set_entrance(DoorType::Crystal);

    let well = opposite_corner(inputs.bounds, inputs.entrance.point, inputs.random)
        .expect("validated entrance lies on exactly one edge");
    draw::set_point(inputs.level.map_mut(), well, terrain::EMPTY_WELL);

    let remains_point = room_center(inputs.bounds, inputs.random);
    let remains = point_to_cell(inputs.level, remains_point);
    let main = loop {
        let category = match inputs.random.int_bound(3) {
            0 => GeneratorCategory::Ring,
            1 => GeneratorCategory::Artifact,
            _ => select_pit_equipment_category(inputs.random),
        };
        let item = generator::random_category(
            inputs.random,
            inputs.generator,
            category,
            inputs.depth_i32(),
        )?;
        let regular = RegularItem::Generated(item);
        if !inputs.prizes.is_item_blocked(regular) {
            break regular;
        }
    };

    let mut haunted = regular_item_cursed(main);
    inputs.drop(
        remains,
        ForcedHeapKind::Skeleton,
        haunted,
        ForcedItem::Regular(main),
        ItemSource::Skeleton,
    );

    let prize_count = inputs.random.int_range(1, 2);
    for _ in 0..prize_count {
        let category = match inputs.random.int_bound(4) {
            0 => GeneratorCategory::Potion,
            1 => GeneratorCategory::Scroll,
            2 => GeneratorCategory::Food,
            _ => GeneratorCategory::Gold,
        };
        let prize = RegularItem::Generated(generator::random_category(
            inputs.random,
            inputs.generator,
            category,
            inputs.depth_i32(),
        )?);
        haunted |= regular_item_cursed(prize);
        inputs.drop(
            remains,
            ForcedHeapKind::Skeleton,
            haunted,
            ForcedItem::Regular(prize),
            ItemSource::Skeleton,
        );
    }

    let key = RegularItem::Queued(QueuedItemKind::CrystalKey {
        depth: inputs.depth_u8(),
    });
    inputs.drop(
        remains,
        ForcedHeapKind::Skeleton,
        haunted,
        ForcedItem::Regular(key),
        ItemSource::Skeleton,
    );

    let candidates = pit_fall_candidates_in(inputs.level, inputs.bounds);
    inputs
        .level
        .record(ForcedPaintEvent::PitFallCandidates(candidates.clone()));
    inputs.report.pit_fall_candidates = candidates;
    Ok(())
}

fn select_pit_equipment_category(random: &mut RandomStack) -> GeneratorCategory {
    match random.int_bound(5) {
        0 | 1 => GeneratorCategory::Weapon,
        2 => GeneratorCategory::Missile,
        3 | 4 => GeneratorCategory::Armor,
        _ => unreachable!("Random.Int(5) is within 0..5"),
    }
}

fn paint_shop<L, P>(inputs: &mut PaintInputs<'_, L, P>) -> Result<(), ForcedPaintError>
where
    L: ForcedLevelContext,
    P: ForcedPrizeContext,
{
    fill_room(inputs.level, inputs.bounds, terrain::WALL);
    fill_room_margin(inputs.level, inputs.bounds, 1, terrain::EMPTY_SP);

    // Upstream calls center before its null-cache fallback in placeItems.
    let shopkeeper = room_center(inputs.bounds, inputs.random);
    let shopkeeper_cell = point_to_cell(inputs.level, shopkeeper);
    inputs.level.record(ForcedPaintEvent::Mob {
        cell: shopkeeper_cell,
        kind: ForcedMobKind::Shopkeeper,
    });

    inputs.shop_room.ensure_items(
        inputs.random,
        inputs.generator,
        inputs.depth_i32(),
        inputs.shop_run,
    )?;
    let mut items = inputs
        .shop_room
        .items_to_spawn
        .take()
        .expect("ensure_items populated ShopRoom.itemsToSpawn");

    let mut entry_inset = point_inside(inputs.bounds, inputs.entrance.point, 1);
    let mut current = entry_inset;
    let mut inset = 1_i32;

    while let Some(item) = items.first().cloned() {
        step_shop_clockwise(&mut current, inputs.bounds, inset);
        if current == entry_inset {
            if entry_inset.y == inputs.bounds.top.wrapping_add(inset) {
                entry_inset.y = entry_inset.y.wrapping_add(1);
            } else if entry_inset.y == inputs.bounds.bottom.wrapping_sub(inset) {
                entry_inset.y = entry_inset.y.wrapping_sub(1);
            }
            if entry_inset.x == inputs.bounds.left.wrapping_add(inset) {
                entry_inset.x = entry_inset.x.wrapping_add(1);
            } else if entry_inset.x == inputs.bounds.right.wrapping_sub(inset) {
                entry_inset.x = entry_inset.x.wrapping_sub(1);
            }
            inset = inset.wrapping_add(1);
            if inset > (inclusive_width(inputs.bounds).min(inclusive_height(inputs.bounds)) - 3) / 2
            {
                break;
            }
            current = entry_inset;
            step_shop_clockwise(&mut current, inputs.bounds, inset);
        }

        let cell = point_to_cell(inputs.level, current);
        if inputs.level.map().cells[cell] == terrain::HIGH_GRASS {
            inputs.level.map_mut().cells[cell] = terrain::GRASS;
        }
        inputs.drop(
            cell,
            ForcedHeapKind::ForSale,
            false,
            ForcedItem::Shop(item),
            ItemSource::Shop,
        );
        items.remove(0);
    }

    if !items.is_empty() {
        for point in inputs.bounds.points() {
            let cell = point_to_cell(inputs.level, point);
            let value = inputs.level.map().cells[cell];
            if matches!(value, terrain::EMPTY_SP | terrain::EMPTY)
                && !inputs.level.has_heap(cell)
                && !inputs.level.has_mob(cell)
            {
                let item = items.remove(0);
                inputs.drop(
                    cell,
                    ForcedHeapKind::ForSale,
                    false,
                    ForcedItem::Shop(item),
                    ItemSource::Shop,
                );
            }
            if items.is_empty() {
                break;
            }
        }
    }

    inputs.report.shop_unplaced_items.clone_from(&items);
    inputs.shop_room.items_to_spawn = Some(items);

    let neighbours: Vec<RoomId> = inputs.rooms[inputs.room]
        .connected
        .iter()
        .map(|connection| connection.room)
        .collect();
    for neighbour in neighbours {
        set_shared_door_type(inputs.rooms, inputs.room, neighbour, DoorType::Regular);
    }

    // `ShatteredPixelDungeon.reportException` is non-throwing. Retaining the
    // leftovers in both room state and report mirrors that behavior.
    Ok(())
}

fn step_shop_clockwise(point: &mut Point, bounds: Rect, inset: i32) {
    if point.x == bounds.left.wrapping_add(inset) && point.y != bounds.top.wrapping_add(inset) {
        point.y = point.y.wrapping_sub(1);
    } else if point.y == bounds.top.wrapping_add(inset)
        && point.x != bounds.right.wrapping_sub(inset)
    {
        point.x = point.x.wrapping_add(1);
    } else if point.x == bounds.right.wrapping_sub(inset)
        && point.y != bounds.bottom.wrapping_sub(inset)
    {
        point.y = point.y.wrapping_add(1);
    } else {
        point.x = point.x.wrapping_sub(1);
    }
}

/// Candidate scan used by `RegularLevel.fallCell(true)` for one Pit room.
/// Heaps are intentionally ignored; only passability and mobs are tested.
#[must_use]
pub fn pit_fall_candidates<L: ForcedLevelContext>(level: &L, room: &Room) -> Vec<usize> {
    pit_fall_candidates_in(level, room.bounds)
}

fn pit_fall_candidates_in<L: ForcedLevelContext>(level: &L, bounds: Rect) -> Vec<usize> {
    bounds
        .points()
        .map(|point| point_to_cell(level, point))
        .filter(|cell| {
            terrain::flags(level.map().cells[*cell]) & terrain::PASSABLE != 0
                && !level.has_mob(*cell)
        })
        .collect()
}

/// Select from a precomputed Pit landing list exactly like
/// `Random.element(ArrayList)`. An empty list leaves RNG untouched.
pub fn choose_pit_fall_cell(candidates: &[usize], random: &mut RandomStack) -> Option<usize> {
    if candidates.is_empty() {
        return None;
    }
    let bound = i32::try_from(candidates.len()).expect("Pit candidate count fits Java int");
    let index = usize::try_from(random.int_bound(bound)).expect("Random.Int is non-negative");
    Some(candidates[index])
}

fn random_empty_lab_cell<L: ForcedLevelContext>(
    level: &L,
    bounds: Rect,
    random: &mut RandomStack,
) -> usize {
    loop {
        let point = random_room_point(bounds, 1, random);
        let cell = point_to_cell(level, point);
        if level.map().cells[cell] == terrain::EMPTY_SP && !level.has_heap(cell) {
            return cell;
        }
    }
}

fn opposite_corner(bounds: Rect, entrance: Point, random: &mut RandomStack) -> Option<Point> {
    if entrance.x == bounds.left {
        Some(Point::new(
            bounds.right.wrapping_sub(1),
            if random.int_bound(2) == 0 {
                bounds.top.wrapping_add(1)
            } else {
                bounds.bottom.wrapping_sub(1)
            },
        ))
    } else if entrance.x == bounds.right {
        Some(Point::new(
            bounds.left.wrapping_add(1),
            if random.int_bound(2) == 0 {
                bounds.top.wrapping_add(1)
            } else {
                bounds.bottom.wrapping_sub(1)
            },
        ))
    } else if entrance.y == bounds.top {
        Some(Point::new(
            if random.int_bound(2) == 0 {
                bounds.left.wrapping_add(1)
            } else {
                bounds.right.wrapping_sub(1)
            },
            bounds.bottom.wrapping_sub(1),
        ))
    } else if entrance.y == bounds.bottom {
        Some(Point::new(
            if random.int_bound(2) == 0 {
                bounds.left.wrapping_add(1)
            } else {
                bounds.right.wrapping_sub(1)
            },
            bounds.top.wrapping_add(1),
        ))
    } else {
        None
    }
}

fn regular_item_cursed(item: RegularItem) -> bool {
    let RegularItem::Generated(item) = item else {
        return false;
    };
    match item {
        GeneratedItem::Equipment(equipment) => equipment.roll.cursed,
        GeneratedItem::Missile(missile) => missile.roll.cursed,
        GeneratedItem::Ring(ring) => ring.roll.cursed,
        GeneratedItem::Artifact(artifact) => artifact.cursed,
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

fn searchable_generated_item(
    generated: GeneratedItem,
    depth: u8,
    source: ItemSource,
    accessibility: Accessibility,
) -> Option<WorldItem> {
    let equipment = generated.searchable_equipment()?;
    Some(WorldItem::from_equipment_roll(
        equipment.item,
        equipment.roll,
        depth,
        source,
        accessibility,
    ))
}

fn fill_room<L: ForcedLevelContext>(level: &mut L, bounds: Rect, value: i32) {
    draw::fill(
        level.map_mut(),
        bounds.left,
        bounds.top,
        inclusive_width(bounds),
        inclusive_height(bounds),
        value,
    );
}

fn fill_room_margin<L: ForcedLevelContext>(level: &mut L, bounds: Rect, margin: i32, value: i32) {
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

fn point_to_cell<L: ForcedLevelContext>(level: &L, point: Point) -> usize {
    level.map().point_to_cell(point)
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

fn point_inside(bounds: Rect, from: Point, amount: i32) -> Point {
    let mut point = from;
    if from.x == bounds.left {
        point.x = point.x.wrapping_add(amount);
    } else if from.x == bounds.right {
        point.x = point.x.wrapping_sub(amount);
    } else if from.y == bounds.top {
        point.y = point.y.wrapping_add(amount);
    } else if from.y == bounds.bottom {
        point.y = point.y.wrapping_sub(amount);
    }
    point
}

const fn on_exactly_one_edge(bounds: Rect, point: Point) -> bool {
    let vertical = point.x == bounds.left || point.x == bounds.right;
    let horizontal = point.y == bounds.top || point.y == bounds.bottom;
    vertical != horizontal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ItemId;
    use crate::equipment::EquipmentRoll;
    use crate::generator::{ArtifactKind, BombKind, GeneratedArtifact, GeneratedRing, StoneKind};
    use crate::level::Feeling;
    use crate::room::{ConnectionRoomKind, Door, RoomConnection};
    use crate::run::{PotionKind, RingKind, RunState, ScrollKind};
    use crate::shop::{ShopBagOffer, ShopBagSet};

    fn connected_rooms(kind: SpecialRoomKind, bounds: Rect, entrance: Point) -> Vec<Room> {
        let mut selected = Room::special(kind);
        selected.bounds = bounds;
        selected.connected.push(RoomConnection {
            room: 1,
            door: Some(Door::new(entrance)),
        });

        let mut neighbour = Room::connection(ConnectionRoomKind::Tunnel);
        neighbour.connected.push(RoomConnection {
            room: 0,
            door: Some(Door::new(entrance)),
        });
        vec![selected, neighbour]
    }

    fn door_type(rooms: &[Room]) -> DoorType {
        rooms[0].connected[0]
            .door
            .expect("fixture door is placed")
            .door_type
    }

    fn run_laboratory(
        outer_seed: i64,
        mut prizes: ForcedPrizePool,
    ) -> (
        Level,
        Vec<Room>,
        Vec<ForcedPaintEvent>,
        ForcedPrizePool,
        ForcedPaintReport,
        i64,
    ) {
        let mut run = RunState::new(0);
        let mut level = Level::new(6, Feeling::None);
        level.set_size(13, 13);
        let mut rooms = connected_rooms(
            SpecialRoomKind::Laboratory,
            Rect::new(2, 2, 8, 8),
            Point::new(2, 5),
        );
        let mut events = Vec::new();
        let mut shop_room = ForcedShopRoomState::default();
        let mut shop_run = ShopRunState::default();
        let mut random = RandomStack::with_base_seed(0);
        random.push(outer_seed);
        let outcome = {
            let mut context = ForcedLevelPaintContext::new(&mut level, &mut events);
            paint_forced_special(
                &mut context,
                &mut rooms,
                0,
                &mut run.generator,
                &mut prizes,
                &mut shop_room,
                &mut shop_run,
                &mut random,
            )
            .unwrap()
        };
        let ForcedPaintOutcome::Painted(report) = outcome else {
            panic!("Laboratory must be handled")
        };
        let next = random.long();
        random.pop();
        (level, rooms, events, prizes, report, next)
    }

    #[test]
    fn laboratory_generated_prizes_match_official_v338_fixture() {
        let (level, rooms, events, prizes, report, next) =
            run_laboratory(123, ForcedPrizePool::default());

        assert_eq!(level.java_map_hash(), 251_446_843);
        assert_eq!(door_type(&rooms), DoorType::Locked);
        assert!(report.searchable_items.is_empty());
        assert_eq!(next, 2_394_716_643_788_677_202);
        assert_eq!(
            prizes.items_to_spawn,
            vec![RegularItem::Queued(QueuedItemKind::IronKey { depth: 6 })]
        );
        assert_eq!(
            events,
            vec![
                ForcedPaintEvent::Blob {
                    cell: 98,
                    kind: ForcedBlobKind::Alchemy,
                    volume: 1,
                },
                drop_event(57, ForcedItem::EnergyCrystal { quantity: 5 }),
                drop_event(
                    46,
                    ForcedItem::Regular(RegularItem::Generated(GeneratedItem::Stone(
                        StoneKind::Aggression,
                    ))),
                ),
                drop_event(
                    82,
                    ForcedItem::Regular(RegularItem::Generated(GeneratedItem::Potion {
                        kind: PotionKind::Healing,
                        exotic: false,
                    })),
                ),
                ForcedPaintEvent::SpawnItem(RegularItem::Queued(QueuedItemKind::IronKey {
                    depth: 6,
                })),
            ]
        );
    }

    #[test]
    fn laboratory_queue_matchers_are_class_specific_and_in_source_priority() {
        let queue = vec![
            RegularItem::Queued(QueuedItemKind::TrinketCatalyst),
            RegularItem::Queued(QueuedItemKind::PotionOfStrength),
        ];
        let (level, rooms, events, prizes, _, next) =
            run_laboratory(4, ForcedPrizePool::all_pages_found(queue));

        assert_eq!(level.java_map_hash(), 170_834_747);
        assert_eq!(door_type(&rooms), DoorType::Locked);
        assert_eq!(next, 4_595_608_453_435_589_273);
        assert_eq!(
            prizes.items_to_spawn,
            vec![
                RegularItem::Queued(QueuedItemKind::PotionOfStrength),
                RegularItem::Queued(QueuedItemKind::IronKey { depth: 6 }),
            ]
        );
        assert_eq!(
            events[0],
            ForcedPaintEvent::Blob {
                cell: 46,
                kind: ForcedBlobKind::Alchemy,
                volume: 1,
            }
        );
        assert_eq!(
            events[1],
            drop_event(69, ForcedItem::EnergyCrystal { quantity: 5 })
        );
        assert_eq!(
            events[2],
            drop_event(
                85,
                ForcedItem::Regular(RegularItem::Queued(QueuedItemKind::TrinketCatalyst)),
            )
        );
    }

    #[test]
    fn laboratory_missing_page_order_and_rng_match_official_v338_fixture() {
        let (level, rooms, events, prizes, _, next) =
            run_laboratory(9_876, ForcedPrizePool::no_pages_found(Vec::new()));

        assert_eq!(level.java_map_hash(), 170_834_747);
        assert_eq!(door_type(&rooms), DoorType::Locked);
        assert_eq!(next, 4_269_283_352_452_184_168);
        assert_eq!(
            prizes.items_to_spawn,
            vec![RegularItem::Queued(QueuedItemKind::IronKey { depth: 6 })]
        );
        assert_eq!(
            events[1],
            drop_event(43, ForcedItem::EnergyCrystal { quantity: 5 })
        );
        assert_eq!(
            events[2],
            drop_event(
                68,
                ForcedItem::Regular(RegularItem::Generated(GeneratedItem::Potion {
                    kind: PotionKind::Healing,
                    exotic: false,
                })),
            )
        );
        assert_eq!(
            events[3],
            drop_event(
                45,
                ForcedItem::Regular(RegularItem::Generated(GeneratedItem::Potion {
                    kind: PotionKind::ToxicGas,
                    exotic: false,
                })),
            )
        );
        assert_eq!(
            events[4],
            drop_event(98, ForcedItem::AlchemyPage(AlchemyGuidePage::Potions))
        );
        assert_eq!(
            events[5],
            drop_event(83, ForcedItem::AlchemyPage(AlchemyGuidePage::Stones))
        );
    }

    #[test]
    fn pit_geometry_heap_key_and_later_landing_match_official_v338_fixture() {
        let mut run = RunState::new(0);
        let mut level = Level::new(11, Feeling::None);
        level.set_size(13, 13);
        let mut rooms = connected_rooms(
            SpecialRoomKind::Pit,
            Rect::new(2, 2, 8, 8),
            Point::new(2, 5),
        );
        let mut events = Vec::new();
        let mut prizes = ForcedPrizePool::default();
        let mut shop_room = ForcedShopRoomState::default();
        let mut shop_run = ShopRunState::default();
        let mut random = RandomStack::with_base_seed(0);
        random.push(123);
        let outcome = {
            let mut context = ForcedLevelPaintContext::new(&mut level, &mut events);
            paint_forced_special(
                &mut context,
                &mut rooms,
                0,
                &mut run.generator,
                &mut prizes,
                &mut shop_room,
                &mut shop_run,
                &mut random,
            )
            .unwrap()
        };
        let ForcedPaintOutcome::Painted(report) = outcome else {
            panic!("Pit must be handled")
        };
        let next = random.long();
        random.pop();

        assert_eq!(level.java_map_hash(), -1_919_487_390);
        assert_eq!(level.map.cells[98], terrain::EMPTY_WELL);
        assert_eq!(door_type(&rooms), DoorType::Crystal);
        assert_eq!(next, -7_561_148_479_966_472_625);
        let expected_candidates = vec![
            42, 55, 68, 81, 94, 43, 56, 69, 82, 95, 44, 57, 70, 83, 96, 45, 58, 71, 84, 97, 46, 59,
            72, 85, 98,
        ];
        assert_eq!(report.pit_fall_candidates, expected_candidates);
        assert_eq!(
            report.searchable_items,
            vec![WorldItem {
                item: ItemId::RingTenacity,
                transmuted_item: None,
                upgrade: 1,
                effect: None,
                cursed: false,
                depth: 11,
                source: ItemSource::Skeleton,
                accessibility: Accessibility::Independent,
            }]
        );
        assert_eq!(
            events,
            vec![
                ForcedPaintEvent::Drop {
                    cell: 70,
                    heap: ForcedHeapKind::Skeleton,
                    haunted: false,
                    reward: ForcedReward {
                        item: ForcedItem::Regular(RegularItem::Generated(GeneratedItem::Ring(
                            GeneratedRing {
                                kind: RingKind::Tenacity,
                                roll: EquipmentRoll {
                                    upgrade: 1,
                                    cursed: false,
                                    effect: None,
                                },
                            },
                        ))),
                        accessibility: Accessibility::Independent,
                    },
                },
                skeleton_drop(
                    70,
                    ForcedItem::Regular(RegularItem::Generated(GeneratedItem::Gold {
                        quantity: 205,
                    })),
                ),
                skeleton_drop(
                    70,
                    ForcedItem::Regular(RegularItem::Queued(QueuedItemKind::CrystalKey {
                        depth: 11,
                    })),
                ),
                ForcedPaintEvent::PitFallCandidates(expected_candidates.clone()),
            ]
        );

        let mut fall_random = RandomStack::with_base_seed(0);
        fall_random.push(77);
        assert_eq!(
            choose_pit_fall_cell(&expected_candidates, &mut fall_random),
            Some(68)
        );
        assert_eq!(fall_random.long(), -4_161_391_460_964_920_639);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn prewarmed_shop_layout_shuffle_and_searchables_match_official_v338_fixture() {
        let mut run = RunState::new(0);
        let mut level = Level::new(6, Feeling::None);
        level.set_size(13, 13);
        let mut rooms = connected_rooms(
            SpecialRoomKind::Shop,
            Rect::new(2, 2, 9, 9),
            Point::new(2, 5),
        );
        let mut events = Vec::new();
        let mut prizes = ForcedPrizePool::default();
        let mut shop_room = ForcedShopRoomState::default();
        let mut shop_run = ShopRunState::default();
        let mut random = RandomStack::with_base_seed(0);
        random.push(123);

        assert_eq!(
            shop_room
                .minimum_dimension(&mut random, &mut run.generator, 6, &mut shop_run)
                .unwrap(),
            8
        );
        let outcome = {
            let mut context = ForcedLevelPaintContext::new(&mut level, &mut events);
            paint_forced_special(
                &mut context,
                &mut rooms,
                0,
                &mut run.generator,
                &mut prizes,
                &mut shop_room,
                &mut shop_run,
                &mut random,
            )
            .unwrap()
        };
        let ForcedPaintOutcome::Painted(report) = outcome else {
            panic!("Shop must be handled")
        };
        let next = random.long();
        random.pop();

        assert_eq!(level.java_map_hash(), -2_113_838_301);
        assert_eq!(door_type(&rooms), DoorType::Regular);
        assert_eq!(next, -5_439_280_920_086_173_306);
        assert_eq!(shop_room.remaining_items(), Some([].as_slice()));
        assert_eq!(
            events[0],
            ForcedPaintEvent::Mob {
                cell: 84,
                kind: ForcedMobKind::Shopkeeper,
            }
        );

        let mut drops: Vec<(usize, &'static str)> = events
            .iter()
            .filter_map(|event| match event {
                ForcedPaintEvent::Drop {
                    cell,
                    reward:
                        ForcedReward {
                            item: ForcedItem::Shop(item),
                            ..
                        },
                    ..
                } => Some((*cell, shop_token(item))),
                _ => None,
            })
            .collect();
        drops.sort_unstable_by_key(|(cell, _)| *cell);
        assert_eq!(
            drops,
            vec![
                (42, "alchemize-3"),
                (43, "identify"),
                (44, "healing"),
                (45, "haste"),
                (46, "healing"),
                (47, "healing"),
                (55, "remove-curse"),
                (56, "ration"),
                (60, "ration"),
                (73, "cleansing-dart"),
                (81, "shuriken"),
                (86, "unstable-spellbook"),
                (94, "leather"),
                (99, "augmentation"),
                (107, "ankh"),
                (108, "toxic-gas"),
                (109, "bag"),
                (110, "magic-mapping"),
                (111, "spear"),
                (112, "double-bomb"),
            ]
        );
        assert_eq!(
            report
                .searchable_items
                .iter()
                .map(|world| (world.item, world.source))
                .collect::<Vec<_>>(),
            vec![
                (ItemId::CleansingDart, ItemSource::Shop),
                (ItemId::Spear, ItemSource::Shop),
                (ItemId::LeatherArmor, ItemSource::Shop),
                (ItemId::Shuriken, ItemSource::Shop),
            ]
        );
    }

    #[test]
    fn shop_overflow_uses_rect_x_major_scan_after_both_clockwise_rings() {
        let mut run = RunState::new(0);
        let mut level = Level::new(6, Feeling::None);
        level.set_size(13, 13);
        let mut rooms = connected_rooms(
            SpecialRoomKind::Shop,
            Rect::new(0, 0, 6, 6),
            Point::new(0, 3),
        );
        let mut events = Vec::new();
        let mut prizes = ForcedPrizePool::default();
        let injected = vec![ShopStockItem::Direct(DirectShopItem::Torch); 24];
        let mut shop_room = ForcedShopRoomState::with_items_to_spawn(injected);
        let mut shop_run = ShopRunState::default();
        let mut random = RandomStack::with_base_seed(0);
        random.push(55);
        let outcome = {
            let mut context = ForcedLevelPaintContext::new(&mut level, &mut events);
            paint_forced_special(
                &mut context,
                &mut rooms,
                0,
                &mut run.generator,
                &mut prizes,
                &mut shop_room,
                &mut shop_run,
                &mut random,
            )
            .unwrap()
        };
        let ForcedPaintOutcome::Painted(report) = outcome else {
            panic!("Shop must be handled")
        };

        let drop_cells: Vec<usize> = events
            .iter()
            .filter_map(|event| match event {
                ForcedPaintEvent::Drop { cell, .. } => Some(*cell),
                _ => None,
            })
            .collect();
        assert_eq!(
            drop_cells,
            vec![
                27, 14, 15, 16, 17, 18, 31, 44, 57, 70, 69, 68, 67, 66, 53, 28, 29, 30, 43, 56, 55,
                54, 40, 41,
            ]
        );
        assert_eq!(
            events[0],
            ForcedPaintEvent::Mob {
                cell: 42,
                kind: ForcedMobKind::Shopkeeper,
            }
        );
        assert!(report.shop_unplaced_items.is_empty());
        assert_eq!(shop_room.remaining_items(), Some([].as_slice()));
        assert_eq!(door_type(&rooms), DoorType::Regular);
        assert_eq!(level.java_map_hash(), -1_150_507_603);
        assert_eq!(random.long(), 2_421_656_602_643_609_670);
    }

    fn drop_event(cell: usize, item: ForcedItem) -> ForcedPaintEvent {
        ForcedPaintEvent::Drop {
            cell,
            heap: ForcedHeapKind::Heap,
            haunted: false,
            reward: ForcedReward {
                item,
                accessibility: Accessibility::Independent,
            },
        }
    }

    fn skeleton_drop(cell: usize, item: ForcedItem) -> ForcedPaintEvent {
        ForcedPaintEvent::Drop {
            cell,
            heap: ForcedHeapKind::Skeleton,
            haunted: false,
            reward: ForcedReward {
                item,
                accessibility: Accessibility::Independent,
            },
        }
    }

    fn shop_token(item: &ShopStockItem) -> &'static str {
        match item {
            ShopStockItem::Searchable(world) => match world.item {
                ItemId::LeatherArmor => "leather",
                ItemId::Shuriken => "shuriken",
                ItemId::Spear => "spear",
                ItemId::CleansingDart => "cleansing-dart",
                _ => "other-searchable",
            },
            ShopStockItem::Generated(GeneratedItem::Potion {
                kind: PotionKind::Haste,
                exotic: false,
            }) => "haste",
            ShopStockItem::Generated(GeneratedItem::Potion {
                kind: PotionKind::Healing,
                exotic: false,
            })
            | ShopStockItem::Direct(DirectShopItem::PotionOfHealing) => "healing",
            ShopStockItem::Generated(GeneratedItem::Potion {
                kind: PotionKind::ToxicGas,
                exotic: false,
            }) => "toxic-gas",
            ShopStockItem::Generated(GeneratedItem::Scroll {
                kind: ScrollKind::RemoveCurse,
                exotic: false,
            })
            | ShopStockItem::Direct(DirectShopItem::ScrollOfRemoveCurse) => "remove-curse",
            ShopStockItem::Generated(GeneratedItem::Artifact(GeneratedArtifact {
                kind: ArtifactKind::UnstableSpellbook,
                ..
            })) => "unstable-spellbook",
            ShopStockItem::Generated(GeneratedItem::Bomb(BombKind::DoubleBomb)) => "double-bomb",
            ShopStockItem::Direct(DirectShopItem::Alchemize { quantity: 3 }) => "alchemize-3",
            ShopStockItem::Direct(DirectShopItem::ScrollOfIdentify) => "identify",
            ShopStockItem::Direct(DirectShopItem::SmallRation) => "ration",
            ShopStockItem::Direct(DirectShopItem::StoneOfAugmentation) => "augmentation",
            ShopStockItem::Direct(DirectShopItem::Ankh) => "ankh",
            ShopStockItem::Direct(DirectShopItem::ScrollOfMagicMapping) => "magic-mapping",
            ShopStockItem::Direct(DirectShopItem::Bag(
                ShopBagOffer::Deterministic(_) | ShopBagOffer::RuntimeHashMapTie { .. },
            )) => "bag",
            _ => "other",
        }
    }

    #[test]
    fn bag_fixture_type_can_represent_runtime_hashmap_ties() {
        // Keep the abstract Bag token deliberate: the Java ChooseBag HashMap
        // tie is process identity dependent, not a world-seed property.
        assert_eq!(
            shop_token(&ShopStockItem::Direct(DirectShopItem::Bag(
                ShopBagOffer::RuntimeHashMapTie {
                    candidates: ShopBagSet::ALL,
                }
            ))),
            "bag"
        );
    }
}
