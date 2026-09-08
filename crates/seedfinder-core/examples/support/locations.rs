use shpd_seedfinder_core::{
    catalog::item,
    generator::{GeneratedEquipment, GeneratedItem},
    level::{Level, PaintItem, PaintMob},
    quest_rooms::QuestPaintEvent,
    regular_items::{RegularItem, RegularItemsResult},
    secret_rooms::{SecretItem, SecretPaintEvent},
    shop::ShopStockItem,
    special_consumable::{ConsumableItem, ConsumablePaintEvent},
    special_equipment::{SpecialItem, SpecialPaintEvent},
    special_forced::{ForcedItem, ForcedPaintEvent},
};

fn equipment(out: &mut String, depth: u32, cell: usize, value: GeneratedEquipment) {
    use std::fmt::Write;
    writeln!(
        out,
        "loc {depth},0,{cell},{},{},{},{}",
        item(value.item).stable_id,
        value.roll.upgrade,
        u8::from(value.roll.cursed),
        value.roll.effect.map_or("-", |e| e.wire_name())
    )
    .unwrap();
}
fn generated(out: &mut String, depth: u32, cell: usize, value: GeneratedItem) {
    if let Some(value) = value.searchable_equipment() {
        equipment(out, depth, cell, value);
    }
}
fn paint(out: &mut String, depth: u32, cell: usize, value: PaintItem) {
    if let PaintItem::Generated(value) = value {
        generated(out, depth, cell, value);
    }
}
fn special(out: &mut String, depth: u32, cell: usize, value: SpecialItem) {
    if let SpecialItem::Paint(value) = value {
        paint(out, depth, cell, value);
    }
}
fn regular(out: &mut String, depth: u32, cell: usize, value: RegularItem) {
    if let RegularItem::Generated(value) = value {
        generated(out, depth, cell, value);
    }
}

// The composites expose separate event streams for each painter family.
// Placement records contain validated, nonnegative cells from randomDropCell.
#[allow(clippy::too_many_arguments, clippy::cast_sign_loss)]
pub fn collect(
    out: &mut String,
    level: &Level,
    regular_items: &RegularItemsResult,
    specials: &[SpecialPaintEvent],
    consumables: &[ConsumablePaintEvent],
    forced: &[ForcedPaintEvent],
    secrets: &[SecretPaintEvent],
    quests: &[QuestPaintEvent],
) {
    let depth = level.depth;
    for heap in &level.heaps {
        for value in &heap.items {
            paint(out, depth, heap.cell, *value);
        }
    }
    for mob in &level.mobs {
        if let PaintMob::Mimic { cell, items } = mob {
            for value in items {
                paint(out, depth, *cell, *value);
            }
        }
    }
    for placement in &regular_items.placements {
        for value in &placement.items {
            regular(out, depth, placement.cell as usize, *value);
        }
    }
    for event in specials {
        match event {
            SpecialPaintEvent::Drop { cell, reward, .. } => special(out, depth, *cell, reward.item),
            SpecialPaintEvent::Mob { cell, carried, .. } => {
                for reward in carried {
                    special(out, depth, *cell, reward.item);
                }
            }
            _ => {}
        }
    }
    for event in consumables {
        if let ConsumablePaintEvent::Drop { cell, reward, .. } = event {
            if let ConsumableItem::Special(value) = reward.item {
                special(out, depth, *cell, value);
            }
        }
        if let ConsumablePaintEvent::Mimic { cell, carried } = event {
            for reward in carried {
                if let ConsumableItem::Special(value) = reward.item {
                    special(out, depth, *cell, value);
                }
            }
        }
    }
    for event in forced {
        if let ForcedPaintEvent::Drop { cell, reward, .. } = event {
            match &reward.item {
                ForcedItem::Regular(value) => regular(out, depth, *cell, *value),
                ForcedItem::Shop(ShopStockItem::Searchable(value)) => equipment(
                    out,
                    depth,
                    *cell,
                    GeneratedEquipment {
                        item: value.item,
                        roll: shpd_seedfinder_core::equipment::EquipmentRoll {
                            upgrade: value.upgrade,
                            cursed: value.cursed,
                            effect: value.effect,
                        },
                    },
                ),
                ForcedItem::Shop(ShopStockItem::Generated(value)) => {
                    generated(out, depth, *cell, *value);
                }
                _ => {}
            }
        }
    }
    for event in secrets {
        if let SecretPaintEvent::Drop { cell, reward, .. } = event {
            if let SecretItem::Generated(value) = reward.item {
                generated(out, depth, *cell, value);
            }
        }
    }
    for event in quests {
        if let QuestPaintEvent::Drop { cell, item, .. } = event {
            regular(out, depth, *cell, *item);
        }
    }
}
