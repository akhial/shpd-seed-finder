//! Exact v3.3.8 `RegularLevel.createItems()` content and RNG orchestration.
//!
//! Spatial item placement is intentionally delegated to
//! [`RegularItemPlacement`]. Its methods are called at the same points as the
//! Java implementation, and `random_drop_cell` receives the active depth RNG
//! so a concrete room/map implementation can consume the exact placement
//! draws. This module owns item generation, heap/mimic branch draws, mimic
//! prizes, queued-item ordering, and isolated bonus-stream push/pop calls.
//!
//! The canonical profile has no challenges, equipped trinkets, bones, Dried
//! Rose, cached-rations talent, or meta bonus items. All guide/document pages
//! are considered found. Canonically inactive branches still consume their
//! outer `Long()` child seeds; Ebony Mimic and Cracked Spyglass also perform
//! their unconditional first child-stream `Float()` calls.

use std::fmt;

use crate::catalog::{Effect, ItemKind, item as catalog_item};
use crate::challenges::Challenges;
use crate::equipment::EquipmentRoll;
use crate::generator::{
    GeneratedArtifact, GeneratedItem, GeneratorError, MissileKind, random as generator_random,
    random_armor, random_category, random_gold, random_missile, random_weapon,
};
use crate::level_prelude::{Feeling, MandatoryDrops};
use crate::model::{Accessibility, ItemSource, WorldItem};
use crate::rng::RandomStack;
use crate::run::{GeneratorCategory, GeneratorState};

/// Constructor-only items that can be queued in `Level.itemsToSpawn`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QueuedItemKind {
    PotionOfStrength,
    ScrollOfUpgrade,
    Stylus,
    StoneOfEnchantment,
    StoneOfIntuition,
    TrinketCatalyst,
    PotionOfLevitation,
    PotionOfLiquidFlame,
    PotionOfInvisibility,
    PotionOfPurity,
    PotionOfFrost,
    PotionOfHaste,
    IronKey {
        depth: u8,
    },
    CrystalKey {
        depth: u8,
    },
    GoldenKey {
        depth: u8,
    },
    CeremonialCandle,
    Torch,
    /// A non-searchable room-specific constructor with no RNG behavior.
    Other(&'static str),
}

/// One item handled by regular-floor placement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegularItem {
    Generated(GeneratedItem),
    Queued(QueuedItemKind),
}

/// Reconstructs the exact `Level.create()` queue prefix from
/// `PreparedRegularFloor`'s separated generated and mandatory records.
///
/// The first generated item is mandatory food. Fixed mandatory items follow
/// it in source order, and the optional second generated item is LARGE-floor
/// food appended during the later feeling roll.
#[must_use]
pub fn level_create_queue(
    queued_generated_items: &[GeneratedItem],
    mandatory: MandatoryDrops,
) -> Vec<RegularItem> {
    let mut queue = Vec::with_capacity(queued_generated_items.len() + 6);
    if let Some(food) = queued_generated_items.first() {
        queue.push(RegularItem::Generated(*food));
    }
    if mandatory.strength_potion {
        queue.push(RegularItem::Queued(QueuedItemKind::PotionOfStrength));
    }
    if mandatory.upgrade_scroll {
        queue.push(RegularItem::Queued(QueuedItemKind::ScrollOfUpgrade));
    }
    if mandatory.arcane_stylus {
        queue.push(RegularItem::Queued(QueuedItemKind::Stylus));
    }
    if mandatory.enchantment_stone {
        queue.push(RegularItem::Queued(QueuedItemKind::StoneOfEnchantment));
    }
    if mandatory.intuition_stone {
        queue.push(RegularItem::Queued(QueuedItemKind::StoneOfIntuition));
    }
    if mandatory.trinket_catalyst {
        queue.push(RegularItem::Queued(QueuedItemKind::TrinketCatalyst));
    }
    queue.extend(
        queued_generated_items
            .iter()
            .skip(1)
            .copied()
            .map(RegularItem::Generated),
    );
    queue
}

/// Room class requested from `RegularLevel.randomDropCell`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DropCellKind {
    StandardRoom,
    SpecialRoom,
}

/// Heap types assigned by `createItems()` in the canonical profile.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RegularHeapKind {
    Heap,
    Chest,
    LockedChest,
    Skeleton,
}

/// Mimic classes created by ordinary regular-floor item placement.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RegularMimicKind {
    Mimic,
    GoldenMimic,
}

/// Spatial operations needed by [`create_regular_items`].
///
/// `drop_item` mirrors `Level.drop` and returns a handle to its resulting heap.
/// The primary-heap query exists because Java only converts an upgrade prize
/// to a locked chest and queues a key when `heaps.get(cell) == dropped`.
pub trait RegularItemPlacement {
    type HeapHandle;

    fn random_drop_cell(&mut self, random: &mut RandomStack, kind: DropCellKind) -> i32;
    fn clear_grass_if_needed(&mut self, cell: i32);
    fn has_mob(&mut self, cell: i32) -> bool;
    fn drop_item(&mut self, cell: i32, item: RegularItem) -> Self::HeapHandle;
    fn is_primary_heap(&self, cell: i32, heap: &Self::HeapHandle) -> bool;
    fn set_heap_type(&mut self, heap: &Self::HeapHandle, kind: RegularHeapKind);
    fn set_haunted_if_cursed(&mut self, heap: &Self::HeapHandle);
    fn spawn_mimic(&mut self, cell: i32, kind: RegularMimicKind, items: &[RegularItem]);
}

/// Final placement of one heap or mimic group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegularItemPlacementRecord {
    pub cell: i32,
    pub destination: RegularItemDestination,
    pub items: Vec<RegularItem>,
}

/// Search-relevant destination selected for a placed item group.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RegularItemDestination {
    Heap(RegularHeapKind),
    /// The locked-candidate drop was not the heap at its requested cell, so
    /// upstream did not convert it or queue a key.
    NonPrimaryDrop,
    Mimic(RegularMimicKind),
}

/// One isolated child generator consumed after queued items are placed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IsolatedItemStream {
    pub kind: IsolatedItemStreamKind,
    pub seed: i64,
}

/// Source order of the eight unconditional `pushGenerator(Random.Long())`
/// blocks at the end of `RegularLevel.createItems()`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IsolatedItemStreamKind {
    DarknessTorches,
    Bones,
    DriedRosePetals,
    CachedRations,
    GuidePages,
    LorePages,
    EbonyMimic,
    CrackedSpyglass,
}

/// Complete content result of one canonical `RegularLevel.createItems()` call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegularItemsResult {
    pub generated_item_count: u8,
    /// Input queue plus dynamically appended Golden Keys, in Java iteration order.
    pub final_queue: Vec<RegularItem>,
    pub placements: Vec<RegularItemPlacementRecord>,
    pub world_items: Vec<WorldItem>,
    pub isolated_streams: [IsolatedItemStream; 8],
}

/// Version-pinned generator invariant failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegularItemsError(pub GeneratorError);

impl fmt::Display for RegularItemsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for RegularItemsError {}

impl From<GeneratorError> for RegularItemsError {
    fn from(error: GeneratorError) -> Self {
        Self(error)
    }
}

/// Mirrors canonical `RegularLevel.createItems()`.
///
/// The caller must have already run floor preparation, graph construction,
/// painting, and mob creation on the same active depth generator. `queue`
/// should contain the exact `Level.itemsToSpawn` order after room painting.
///
/// # Errors
///
/// Returns an error only if the supplied `GeneratorState` violates a pinned
/// category/deck invariant.
pub fn create_regular_items<P: RegularItemPlacement>(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    depth: u8,
    feeling: Feeling,
    challenges: Challenges,
    mut queue: Vec<RegularItem>,
    placement: &mut P,
) -> Result<RegularItemsResult, RegularItemsError> {
    let depth_i32 = i32::from(depth);
    let chance_index = random.chances(&[6.0, 3.0, 1.0]).unwrap_or_default();
    let mut generated_item_count = 3_u8 + u8::try_from(chance_index).unwrap_or_default();
    if feeling == Feeling::Large {
        generated_item_count += 2;
    }

    let mut placements = Vec::new();
    let mut world_items = Vec::new();
    for _ in 0..generated_item_count {
        let item = RegularItem::Generated(generator_random(random, generator, depth_i32)?);
        place_generated_item(
            random,
            generator,
            depth,
            item,
            &mut queue,
            placement,
            &mut placements,
            &mut world_items,
        )?;
    }

    // Java's enhanced-for observes keys appended during the loop above.
    for item in queue.iter().copied() {
        place_queued_item(
            depth,
            random,
            placement,
            item,
            &mut placements,
            &mut world_items,
        );
    }

    let isolated_streams = consume_isolated_streams(
        random,
        challenges,
        feeling,
        depth,
        placement,
        &mut placements,
        &mut world_items,
    );
    Ok(RegularItemsResult {
        generated_item_count,
        final_queue: queue,
        placements,
        world_items,
        isolated_streams,
    })
}

#[allow(clippy::too_many_arguments)]
fn place_generated_item<P: RegularItemPlacement>(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    depth: u8,
    item: RegularItem,
    queue: &mut Vec<RegularItem>,
    placement: &mut P,
    records: &mut Vec<RegularItemPlacementRecord>,
    world_items: &mut Vec<WorldItem>,
) -> Result<(), RegularItemsError> {
    let cell = placement.random_drop_cell(random, DropCellKind::StandardRoom);
    placement.clear_grass_if_needed(cell);

    let heap_kind = match random.int_bound(20) {
        0 => RegularHeapKind::Skeleton,
        1..=4 => {
            // Mimic Tooth multiplier is one: the comparison threshold is zero,
            // but Java still evaluates this Float before short-circuiting.
            let _unused_mimic_roll = random.float();
            RegularHeapKind::Chest
        }
        5 => {
            if depth > 1 && !placement.has_mob(cell) {
                spawn_regular_mimic(
                    random,
                    generator,
                    depth,
                    cell,
                    item,
                    placement,
                    records,
                    world_items,
                )?;
                return Ok(());
            }
            RegularHeapKind::Chest
        }
        _ => RegularHeapKind::Heap,
    };

    if is_locked_chest_candidate(random, item) {
        let spawn_golden = if depth > 1 {
            random.float() < 0.1 && !placement.has_mob(cell)
        } else {
            false
        };
        if spawn_golden {
            spawn_golden_mimic(
                random,
                generator,
                depth,
                cell,
                item,
                placement,
                records,
                world_items,
            )?;
            return Ok(());
        }

        let heap = placement.drop_item(cell, item);
        if placement.is_primary_heap(cell, &heap) {
            placement.set_heap_type(&heap, RegularHeapKind::LockedChest);
            queue.push(RegularItem::Queued(QueuedItemKind::GoldenKey { depth }));
            record_heap(
                cell,
                RegularHeapKind::LockedChest,
                item,
                depth,
                records,
                world_items,
            );
        } else {
            records.push(RegularItemPlacementRecord {
                cell,
                destination: RegularItemDestination::NonPrimaryDrop,
                items: vec![item],
            });
            append_world_item(item, depth, ItemSource::Heap, world_items);
        }
    } else {
        let heap = placement.drop_item(cell, item);
        placement.set_heap_type(&heap, heap_kind);
        if heap_kind == RegularHeapKind::Skeleton {
            placement.set_haunted_if_cursed(&heap);
        }
        record_heap(cell, heap_kind, item, depth, records, world_items);
    }
    Ok(())
}

fn place_queued_item<P: RegularItemPlacement>(
    depth: u8,
    random: &mut RandomStack,
    placement: &mut P,
    item: RegularItem,
    records: &mut Vec<RegularItemPlacementRecord>,
    world_items: &mut Vec<WorldItem>,
) {
    let cell = placement.random_drop_cell(random, DropCellKind::StandardRoom);
    if item == RegularItem::Queued(QueuedItemKind::TrinketCatalyst) {
        let heap = placement.drop_item(cell, item);
        placement.set_heap_type(&heap, RegularHeapKind::LockedChest);
        record_heap(
            cell,
            RegularHeapKind::LockedChest,
            item,
            depth,
            records,
            world_items,
        );

        let key_cell = placement.random_drop_cell(random, DropCellKind::StandardRoom);
        let key = RegularItem::Queued(QueuedItemKind::GoldenKey { depth });
        let key_heap = placement.drop_item(key_cell, key);
        placement.set_heap_type(&key_heap, RegularHeapKind::Heap);
        record_heap(
            key_cell,
            RegularHeapKind::Heap,
            key,
            depth,
            records,
            world_items,
        );
        placement.clear_grass_if_needed(key_cell);
    } else {
        let heap = placement.drop_item(cell, item);
        placement.set_heap_type(&heap, RegularHeapKind::Heap);
        record_heap(
            cell,
            RegularHeapKind::Heap,
            item,
            depth,
            records,
            world_items,
        );
    }
    placement.clear_grass_if_needed(cell);
}

fn is_locked_chest_candidate(random: &mut RandomStack, item: RegularItem) -> bool {
    let RegularItem::Generated(generated) = item else {
        return false;
    };
    if matches!(generated, GeneratedItem::Artifact(_)) {
        return random.int_bound(2) == 0;
    }
    let Some(level) = upgradable_level(generated) else {
        return false;
    };
    random.int_bound(4_i32.wrapping_sub(i32::from(level))) == 0
}

const fn upgradable_level(item: GeneratedItem) -> Option<u8> {
    match item {
        GeneratedItem::Equipment(equipment) => Some(equipment.roll.upgrade),
        GeneratedItem::Missile(missile) if !matches!(missile.kind, MissileKind::Dart) => {
            Some(missile.roll.upgrade)
        }
        GeneratedItem::Ring(ring) => Some(ring.roll.upgrade),
        GeneratedItem::Artifact(_)
        | GeneratedItem::Missile(_)
        | GeneratedItem::Food(_)
        | GeneratedItem::Potion { .. }
        | GeneratedItem::Seed(_)
        | GeneratedItem::Scroll { .. }
        | GeneratedItem::Stone(_)
        | GeneratedItem::Gold { .. }
        | GeneratedItem::Trinket(_)
        | GeneratedItem::Bomb(_)
        | GeneratedItem::TippedDart { .. } => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_regular_mimic<P: RegularItemPlacement>(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    depth: u8,
    cell: i32,
    original: RegularItem,
    placement: &mut P,
    records: &mut Vec<RegularItemPlacementRecord>,
    world_items: &mut Vec<WorldItem>,
) -> Result<(), RegularItemsError> {
    let reward = RegularItem::Generated(generate_mimic_prize(random, generator, i32::from(depth))?);
    let items = vec![original, reward];
    placement.spawn_mimic(cell, RegularMimicKind::Mimic, &items);
    record_mimic(
        cell,
        RegularMimicKind::Mimic,
        items,
        depth,
        records,
        world_items,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn spawn_golden_mimic<P: RegularItemPlacement>(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    depth: u8,
    cell: i32,
    original: RegularItem,
    placement: &mut P,
    records: &mut Vec<RegularItemPlacementRecord>,
    world_items: &mut Vec<WorldItem>,
) -> Result<(), RegularItemsError> {
    let reward = RegularItem::Generated(generate_mimic_prize(random, generator, i32::from(depth))?);
    // GoldenMimic iterates original first, then its newly generated prize.
    let items = vec![
        golden_mimic_item(random, original),
        golden_mimic_item(random, reward),
    ];
    placement.spawn_mimic(cell, RegularMimicKind::GoldenMimic, &items);
    record_mimic(
        cell,
        RegularMimicKind::GoldenMimic,
        items,
        depth,
        records,
        world_items,
    );
    Ok(())
}

fn generate_mimic_prize(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    depth: i32,
) -> Result<GeneratedItem, GeneratorError> {
    let floor_set = depth / 5;
    match random.int_bound(5) {
        0 => Ok(random_gold(random, depth)),
        1 => random_missile(random, generator, floor_set, false).map(GeneratedItem::Missile),
        2 => random_armor(random, floor_set).map(GeneratedItem::Equipment),
        3 => random_weapon(random, generator, floor_set, false).map(GeneratedItem::Equipment),
        _ => random_category(random, generator, GeneratorCategory::Ring, depth),
    }
}

fn golden_mimic_item(random: &mut RandomStack, item: RegularItem) -> RegularItem {
    let RegularItem::Generated(generated) = item else {
        return item;
    };
    let generated = match generated {
        GeneratedItem::Equipment(mut equipment) => {
            let rerolls_curse = catalog_item(equipment.item).kind == ItemKind::Wand;
            equipment.roll = golden_roll(random, equipment.roll, rerolls_curse);
            GeneratedItem::Equipment(equipment)
        }
        GeneratedItem::Missile(mut missile) => {
            missile.roll = golden_roll(random, missile.roll, false);
            GeneratedItem::Missile(missile)
        }
        GeneratedItem::Ring(mut ring) => {
            ring.roll = golden_roll(random, ring.roll, true);
            GeneratedItem::Ring(ring)
        }
        GeneratedItem::Artifact(artifact) => GeneratedItem::Artifact(GeneratedArtifact {
            kind: artifact.kind,
            cursed: false,
            spellbook_scrolls: artifact.spellbook_scrolls,
        }),
        other => other,
    };
    RegularItem::Generated(generated)
}

fn golden_roll(
    random: &mut RandomStack,
    mut roll: EquipmentRoll,
    upgrade_rerolls_curse: bool,
) -> EquipmentRoll {
    roll.cursed = false;
    roll.effect = match roll.effect {
        Some(Effect::Weapon(effect)) if effect.is_curse() => None,
        Some(Effect::Armor(effect)) if effect.is_curse() => None,
        other => other,
    };
    if roll.upgrade == 0 && random.int_bound(2) == 0 {
        roll.upgrade = 1;
        // Ring.upgrade() and Wand.upgrade() always perform their
        // curse-clearing Int(3), even though the prize pass cleared the curse
        // immediately before the call. Weapon/Armor upgrades draw nothing at
        // level zero.
        if upgrade_rerolls_curse {
            random.int_bound(3);
        }
    }
    roll
}

fn record_heap(
    cell: i32,
    kind: RegularHeapKind,
    item: RegularItem,
    depth: u8,
    records: &mut Vec<RegularItemPlacementRecord>,
    world_items: &mut Vec<WorldItem>,
) {
    records.push(RegularItemPlacementRecord {
        cell,
        destination: RegularItemDestination::Heap(kind),
        items: vec![item],
    });
    let source = match kind {
        RegularHeapKind::Heap => ItemSource::Heap,
        RegularHeapKind::Skeleton => ItemSource::Skeleton,
        RegularHeapKind::Chest => ItemSource::Chest,
        RegularHeapKind::LockedChest => ItemSource::LockedChest,
    };
    append_world_item(item, depth, source, world_items);
}

fn record_mimic(
    cell: i32,
    kind: RegularMimicKind,
    items: Vec<RegularItem>,
    depth: u8,
    records: &mut Vec<RegularItemPlacementRecord>,
    world_items: &mut Vec<WorldItem>,
) {
    let source = match kind {
        RegularMimicKind::Mimic => ItemSource::Mimic,
        RegularMimicKind::GoldenMimic => ItemSource::GoldenMimic,
    };
    for item in &items {
        append_world_item(*item, depth, source, world_items);
    }
    records.push(RegularItemPlacementRecord {
        cell,
        destination: RegularItemDestination::Mimic(kind),
        items,
    });
}

fn append_world_item(
    item: RegularItem,
    depth: u8,
    source: ItemSource,
    output: &mut Vec<WorldItem>,
) {
    let RegularItem::Generated(generated) = item else {
        return;
    };
    let equipment = generated.searchable_equipment();
    if let Some(equipment) = equipment {
        output.push(WorldItem::from_equipment_roll(
            equipment.item,
            equipment.roll,
            depth,
            source,
            Accessibility::Independent,
        ));
    }
}

fn consume_isolated_streams<P: RegularItemPlacement>(
    random: &mut RandomStack,
    challenges: Challenges,
    feeling: Feeling,
    depth: u8,
    placement: &mut P,
    records: &mut Vec<RegularItemPlacementRecord>,
    world_items: &mut Vec<WorldItem>,
) -> [IsolatedItemStream; 8] {
    const KINDS: [IsolatedItemStreamKind; 8] = [
        IsolatedItemStreamKind::DarknessTorches,
        IsolatedItemStreamKind::Bones,
        IsolatedItemStreamKind::DriedRosePetals,
        IsolatedItemStreamKind::CachedRations,
        IsolatedItemStreamKind::GuidePages,
        IsolatedItemStreamKind::LorePages,
        IsolatedItemStreamKind::EbonyMimic,
        IsolatedItemStreamKind::CrackedSpyglass,
    ];
    std::array::from_fn(|index| {
        let kind = KINDS[index];
        let seed = random.long();
        random.push(seed);
        match kind {
            IsolatedItemStreamKind::DarknessTorches
                if challenges.contains(Challenges::DARKNESS) =>
            {
                let count = if feeling == Feeling::Large { 2 } else { 1 };
                for _ in 0..count {
                    place_queued_item(
                        depth,
                        random,
                        placement,
                        RegularItem::Queued(QueuedItemKind::Torch),
                        records,
                        world_items,
                    );
                }
            }
            IsolatedItemStreamKind::EbonyMimic => {
                // `Float() < 0` is false, but the left side is unconditional.
                let _unused_ebony_roll = random.float();
            }
            IsolatedItemStreamKind::CrackedSpyglass => {
                // `(int)(Float() + 0)` is always zero, but consumes the Float.
                let _unused_extra_loot_roll = random.float();
            }
            IsolatedItemStreamKind::DarknessTorches
            | IsolatedItemStreamKind::Bones
            | IsolatedItemStreamKind::DriedRosePetals
            | IsolatedItemStreamKind::CachedRations
            | IsolatedItemStreamKind::GuidePages
            | IsolatedItemStreamKind::LorePages => {}
        }
        random.pop();
        IsolatedItemStream { kind, seed }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::{
        DropCellKind, QueuedItemKind, RegularHeapKind, RegularItem, RegularItemDestination,
        RegularItemPlacement, RegularMimicKind, create_regular_items, level_create_queue,
    };
    use crate::catalog::{Effect, ItemId, WeaponEffect};
    use crate::generator::GeneratedItem;
    use crate::level_prelude::{Feeling, MandatoryDrops};
    use crate::model::ItemSource;
    use crate::rng::RandomStack;
    use crate::run::{RingKind, RunState};

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Call {
        Cell(DropCellKind, i32),
        Clear(i32),
        Mob(i32),
        Drop(i32, RegularItem),
        Type(usize, RegularHeapKind),
        Haunted(usize),
        Mimic(i32, RegularMimicKind, Vec<RegularItem>),
    }

    #[derive(Default)]
    struct LinearPlacement {
        next_cell: i32,
        handles: usize,
        mobs: HashSet<i32>,
        primary: HashMap<usize, bool>,
        calls: Vec<Call>,
    }

    impl LinearPlacement {
        fn new() -> Self {
            Self {
                next_cell: 9,
                ..Self::default()
            }
        }
    }

    impl RegularItemPlacement for LinearPlacement {
        type HeapHandle = usize;

        fn random_drop_cell(&mut self, _random: &mut RandomStack, kind: DropCellKind) -> i32 {
            self.next_cell += 1;
            self.calls.push(Call::Cell(kind, self.next_cell));
            self.next_cell
        }

        fn clear_grass_if_needed(&mut self, cell: i32) {
            self.calls.push(Call::Clear(cell));
        }

        fn has_mob(&mut self, cell: i32) -> bool {
            self.calls.push(Call::Mob(cell));
            self.mobs.contains(&cell)
        }

        fn drop_item(&mut self, cell: i32, item: RegularItem) -> Self::HeapHandle {
            let handle = self.handles;
            self.handles += 1;
            self.primary.insert(handle, true);
            self.calls.push(Call::Drop(cell, item));
            handle
        }

        fn is_primary_heap(&self, _cell: i32, heap: &Self::HeapHandle) -> bool {
            self.primary.get(heap).copied().unwrap_or(false)
        }

        fn set_heap_type(&mut self, heap: &Self::HeapHandle, kind: RegularHeapKind) {
            self.calls.push(Call::Type(*heap, kind));
        }

        fn set_haunted_if_cursed(&mut self, heap: &Self::HeapHandle) {
            self.calls.push(Call::Haunted(*heap));
        }

        fn spawn_mimic(&mut self, cell: i32, kind: RegularMimicKind, items: &[RegularItem]) {
            self.mobs.insert(cell);
            self.calls.push(Call::Mimic(cell, kind, items.to_vec()));
        }
    }

    fn run_fixture(
        depth: u8,
        outer_seed: i64,
        feeling: Feeling,
        queue: Vec<RegularItem>,
    ) -> (super::RegularItemsResult, LinearPlacement, i64) {
        run_fixture_with_challenges(
            depth,
            outer_seed,
            feeling,
            crate::challenges::Challenges::NONE,
            queue,
        )
    }

    fn run_fixture_with_challenges(
        depth: u8,
        outer_seed: i64,
        feeling: Feeling,
        challenges: crate::challenges::Challenges,
        queue: Vec<RegularItem>,
    ) -> (super::RegularItemsResult, LinearPlacement, i64) {
        let mut generator = RunState::new(0).generator;
        let mut random = RandomStack::with_base_seed(0);
        random.push(outer_seed);
        let mut placement = LinearPlacement::new();
        let result = create_regular_items(
            &mut random,
            &mut generator,
            depth,
            feeling,
            challenges,
            queue,
            &mut placement,
        )
        .unwrap();
        let next = random.long();
        random.pop();
        (result, placement, next)
    }

    #[test]
    fn official_java_basic_and_large_rng_vectors_match() {
        let (floor_one, _, next_one) = run_fixture(1, 1, Feeling::None, vec![]);
        assert_eq!(floor_one.generated_item_count, 4);
        assert_eq!(next_one, -482_538_180_091_794_246);
        assert_eq!(
            floor_one
                .placements
                .iter()
                .map(|record| record.destination)
                .collect::<Vec<_>>(),
            [
                RegularItemDestination::Heap(RegularHeapKind::Chest),
                RegularItemDestination::Heap(RegularHeapKind::Chest),
                RegularItemDestination::Heap(RegularHeapKind::Heap),
                RegularItemDestination::Heap(RegularHeapKind::Heap),
            ]
        );
        assert_eq!(floor_one.world_items.len(), 1);
        assert_eq!(floor_one.world_items[0].item, ItemId::LeatherArmor);
        assert_eq!(floor_one.world_items[0].upgrade, 1);
        assert_eq!(floor_one.world_items[0].source, ItemSource::Chest);

        let (large, _, next_large) = run_fixture(11, 11, Feeling::Large, vec![]);
        assert_eq!(large.generated_item_count, 5);
        assert_eq!(next_large, 3_326_389_007_034_255_444);
        assert_eq!(large.world_items.len(), 1);
        assert_eq!(large.world_items[0].item, ItemId::Tomahawk);
        assert_eq!(large.world_items[0].upgrade, 1);
    }

    #[test]
    fn official_java_regular_mimic_prize_vector_matches() {
        let (result, placement, next) = run_fixture(6, 10, Feeling::None, vec![]);
        assert_eq!(next, -61_647_254_574_629_115);
        let mimic = result
            .placements
            .iter()
            .find(|record| {
                record.destination == RegularItemDestination::Mimic(RegularMimicKind::Mimic)
            })
            .unwrap();
        assert_eq!(mimic.cell, 13);
        assert!(matches!(
            mimic.items[0],
            RegularItem::Generated(GeneratedItem::Gold { quantity: 166 })
        ));
        assert_eq!(result.world_items.len(), 1);
        let reward = &result.world_items[0];
        assert_eq!(reward.item, ItemId::Sai);
        assert_eq!(reward.upgrade, 1);
        assert_eq!(reward.effect, Some(Effect::Weapon(WeaponEffect::Blocking)));
        assert_eq!(reward.source, ItemSource::Mimic);
        assert!(
            placement
                .calls
                .iter()
                .any(|call| matches!(call, Call::Mimic(13, RegularMimicKind::Mimic, _)))
        );
    }

    #[test]
    fn official_java_golden_mimic_and_dynamic_key_vector_matches() {
        let (result, _, next) = run_fixture(6, 12, Feeling::None, vec![]);
        assert_eq!(next, -6_659_984_145_522_943_146);
        assert_eq!(result.generated_item_count, 5);
        let golden = result
            .placements
            .iter()
            .find(|record| {
                record.destination == RegularItemDestination::Mimic(RegularMimicKind::GoldenMimic)
            })
            .unwrap();
        assert_eq!(golden.cell, 13);
        let golden_world: Vec<_> = result
            .world_items
            .iter()
            .filter(|item| item.source == ItemSource::GoldenMimic)
            .collect();
        assert_eq!(golden_world.len(), 2);
        assert_eq!(golden_world[0].item, ItemId::MailArmor);
        assert_eq!(golden_world[0].upgrade, 1);
        assert!(!golden_world[0].cursed);
        assert_eq!(golden_world[0].effect, None);
        assert_eq!(golden_world[1].item, ItemId::RingTenacity);
        assert_eq!(golden_world[1].upgrade, 1);
        assert!(!golden_world[1].cursed);
        assert!(matches!(
            golden.items[1],
            RegularItem::Generated(GeneratedItem::Ring(crate::generator::GeneratedRing {
                kind: RingKind::Tenacity,
                roll: crate::equipment::EquipmentRoll {
                    upgrade: 1,
                    cursed: false,
                    ..
                }
            }))
        ));
        assert!(
            result
                .final_queue
                .contains(&RegularItem::Queued(QueuedItemKind::GoldenKey { depth: 6 }))
        );
        assert!(result.placements.iter().any(|record| {
            record.destination == RegularItemDestination::Heap(RegularHeapKind::LockedChest)
                && matches!(
                    record.items[0],
                    RegularItem::Generated(GeneratedItem::Equipment(equipment))
                        if equipment.item == ItemId::Whip
                )
        }));
    }

    #[test]
    fn queued_catalyst_places_its_key_before_clearing_original_grass() {
        let queue = vec![
            RegularItem::Queued(QueuedItemKind::PotionOfStrength),
            RegularItem::Queued(QueuedItemKind::ScrollOfUpgrade),
            RegularItem::Queued(QueuedItemKind::Stylus),
            RegularItem::Queued(QueuedItemKind::TrinketCatalyst),
        ];
        let (result, placement, next) = run_fixture(6, 6, Feeling::None, queue);
        assert_eq!(next, 2_252_222_228_507_478_406);
        assert_eq!(result.placements.len(), 9);
        let locked = result
            .placements
            .iter()
            .find(|record| record.cell == 17)
            .unwrap();
        assert_eq!(
            locked.destination,
            RegularItemDestination::Heap(RegularHeapKind::LockedChest)
        );
        assert_eq!(
            &placement.calls[placement.calls.len() - 3..],
            &[
                Call::Type(8, RegularHeapKind::Heap),
                Call::Clear(18),
                Call::Clear(17),
                // No placement callbacks occur in canonical isolated streams.
            ]
        );
    }

    #[test]
    fn prepared_queue_reinserts_mandatory_items_between_two_large_floor_foods() {
        let foods = [
            GeneratedItem::Food(crate::generator::FoodKind::Ration),
            GeneratedItem::Food(crate::generator::FoodKind::Pasty),
        ];
        let mandatory = MandatoryDrops {
            strength_potion: true,
            upgrade_scroll: true,
            arcane_stylus: false,
            enchantment_stone: true,
            intuition_stone: false,
            trinket_catalyst: true,
        };
        assert_eq!(
            level_create_queue(&foods, mandatory),
            [
                RegularItem::Generated(foods[0]),
                RegularItem::Queued(QueuedItemKind::PotionOfStrength),
                RegularItem::Queued(QueuedItemKind::ScrollOfUpgrade),
                RegularItem::Queued(QueuedItemKind::StoneOfEnchantment),
                RegularItem::Queued(QueuedItemKind::TrinketCatalyst),
                RegularItem::Generated(foods[1]),
            ]
        );
    }

    #[test]
    fn all_eight_isolated_outer_longs_are_retained_in_source_order() {
        let (result, _, _) = run_fixture(16, 16, Feeling::None, vec![]);
        assert_eq!(result.isolated_streams.len(), 8);
        assert_eq!(
            result.isolated_streams.map(|stream| stream.kind as u8),
            [0, 1, 2, 3, 4, 5, 6, 7]
        );
    }

    #[test]
    fn darkness_torches_use_only_the_always_consumed_child_stream() {
        let (normal, _, normal_next) = run_fixture(16, 16, Feeling::None, vec![]);
        let (dark, _, dark_next) = run_fixture_with_challenges(
            16,
            16,
            Feeling::None,
            crate::challenges::Challenges::DARKNESS,
            vec![],
        );
        assert_eq!(dark_next, normal_next);
        assert_eq!(dark.isolated_streams, normal.isolated_streams);
        assert_eq!(dark.placements.len(), normal.placements.len() + 1);
        assert_eq!(
            dark.placements.last().unwrap().items,
            [RegularItem::Queued(QueuedItemKind::Torch),]
        );

        let (large, _, _) = run_fixture_with_challenges(
            16,
            16,
            Feeling::Large,
            crate::challenges::Challenges::DARKNESS,
            vec![],
        );
        assert_eq!(
            large
                .placements
                .iter()
                .filter(|record| record.items == [RegularItem::Queued(QueuedItemKind::Torch)])
                .count(),
            2,
        );
    }
}
