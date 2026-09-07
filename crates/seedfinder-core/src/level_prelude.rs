//! Mandatory-drop and level-feeling rolls performed before spatial generation.

use crate::rng::RandomStack;

/// Run-global counters from the relevant portion of `Dungeon.LimitedDrops`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LimitedDrops {
    pub strength_potions: i32,
    pub upgrade_scrolls: i32,
    pub arcane_styli: i32,
    pub enchantment_stone_dropped: bool,
    pub intuition_stone_dropped: bool,
    pub trinket_catalyst_dropped: bool,
    pub laboratory_rooms: i32,
}

/// Mandatory items appended by `Level.create()` for one ordinary floor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // A compact event record, not mutable state flags.
pub struct MandatoryDrops {
    pub strength_potion: bool,
    pub upgrade_scroll: bool,
    pub arcane_stylus: bool,
    pub enchantment_stone: bool,
    pub intuition_stone: bool,
    pub trinket_catalyst: bool,
}

impl LimitedDrops {
    /// Rolls and mutates the six mandatory-drop sources in their exact upstream
    /// order. Caller must skip boss floors and select the mandatory food first.
    pub fn roll_for_floor(&mut self, depth: i32, random: &mut RandomStack) -> MandatoryDrops {
        let strength_potion = self.strength_needed(depth, random);
        if strength_potion {
            self.strength_potions += 1;
        }

        let upgrade_scroll = self.upgrade_needed(depth, random);
        if upgrade_scroll {
            self.upgrade_scrolls += 1;
        }

        let arcane_stylus = self.stylus_needed(depth, random);
        if arcane_stylus {
            self.arcane_styli += 1;
        }

        let enchantment_stone = self.enchantment_stone_needed(depth, random);
        if enchantment_stone {
            self.enchantment_stone_dropped = true;
        }

        let intuition_stone =
            depth < 5 && !self.intuition_stone_dropped && random.int_bound(4 - depth) == 0;
        if intuition_stone {
            self.intuition_stone_dropped = true;
        }

        let trinket_catalyst =
            depth < 5 && !self.trinket_catalyst_dropped && random.int_bound(4 - depth) == 0;
        if trinket_catalyst {
            self.trinket_catalyst_dropped = true;
        }

        MandatoryDrops {
            strength_potion,
            upgrade_scroll,
            arcane_stylus,
            enchantment_stone,
            intuition_stone,
            trinket_catalyst,
        }
    }

    /// Mirrors `Dungeon.labRoomNeeded`; call during regular-room initialization.
    pub fn laboratory_needed(&self, depth: i32, random: &mut RandomStack) -> bool {
        let region = 1 + depth / 5;
        if region > self.laboratory_rooms {
            let floor_in_region = depth % 5;
            return floor_in_region >= 4 || (floor_in_region == 3 && random.int_bound(2) == 0);
        }
        false
    }

    pub fn record_laboratory(&mut self) {
        self.laboratory_rooms += 1;
    }

    fn strength_needed(&self, depth: i32, random: &mut RandomStack) -> bool {
        let left = 2 - (self.strength_potions - (depth / 5) * 2);
        if left <= 0 {
            return false;
        }
        let floor = depth % 5;
        let mut target_left = 2 - floor / 2;
        if floor % 2 == 1 && random.int_bound(2) == 0 {
            target_left -= 1;
        }
        target_left < left
    }

    fn upgrade_needed(&self, depth: i32, random: &mut RandomStack) -> bool {
        let left = 3 - (self.upgrade_scrolls - (depth / 5) * 3);
        left > 0 && random.int_bound(5 - depth % 5) < left
    }

    fn stylus_needed(&self, depth: i32, random: &mut RandomStack) -> bool {
        let left = 1 - (self.arcane_styli - depth / 5);
        left > 0 && random.int_bound(5 - depth % 5) < left
    }

    fn enchantment_stone_needed(&self, depth: i32, random: &mut RandomStack) -> bool {
        if self.enchantment_stone_dropped || depth / 5 < 1 {
            return false;
        }
        let mut floors_visited = depth - 5;
        if floors_visited > 4 {
            floors_visited -= 1;
        }
        random.int_bound(9 - floors_visited) == 0
    }
}

/// Level feeling that changes geometry or content generation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Feeling {
    #[default]
    None,
    Chasm,
    Water,
    Grass,
    Dark,
    Large,
    Traps,
    Secrets,
}

/// Rolls the ordinary level feeling. Floor 1 has no roll. Under the canonical
/// no-trinket profile, default cases still consume both override floats.
pub fn roll_feeling(depth: i32, random: &mut RandomStack) -> Feeling {
    if depth <= 1 {
        return Feeling::None;
    }
    match random.int_bound(14) {
        0 => Feeling::Chasm,
        1 => Feeling::Water,
        2 => Feeling::Grass,
        3 => Feeling::Dark,
        4 => Feeling::Large,
        5 => Feeling::Traps,
        6 => Feeling::Secrets,
        _ => {
            if random.float()
                < if random.trinket.is(crate::catalog::ItemId::MossyClump) {
                    1.0
                } else {
                    0.0
                }
            {
                return random.trinket.next_feeling();
            }
            if random.float()
                < if random.trinket.is(crate::catalog::ItemId::TrapMechanism) {
                    1.0
                } else {
                    0.0
                }
            {
                return random.trinket.next_feeling();
            }
            Feeling::None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rng::{RandomStack, seed_for_depth};

    use super::{Feeling, LimitedDrops, MandatoryDrops, roll_feeling};

    #[test]
    fn seed_zero_floors_one_to_four_match_actual_java_game() {
        let expected = [
            (
                MandatoryDrops::default(),
                Feeling::None,
                -467_247_898_640_065_295,
            ),
            (
                MandatoryDrops {
                    strength_potion: true,
                    upgrade_scroll: true,
                    intuition_stone: true,
                    ..MandatoryDrops::default()
                },
                Feeling::Chasm,
                4_351_266_459_634_888_850,
            ),
            (
                MandatoryDrops {
                    upgrade_scroll: true,
                    trinket_catalyst: true,
                    ..MandatoryDrops::default()
                },
                Feeling::None,
                -8_864_543_161_300_522_318,
            ),
            (
                MandatoryDrops {
                    strength_potion: true,
                    upgrade_scroll: true,
                    arcane_stylus: true,
                    ..MandatoryDrops::default()
                },
                Feeling::Water,
                -3_810_691_843_960_888_060,
            ),
        ];

        let mut limited = LimitedDrops::default();
        for depth in 1..=4 {
            let mut random = RandomStack::with_base_seed(0);
            random.push(seed_for_depth(0, depth, 0));
            let drops = limited.roll_for_floor(i32::try_from(depth).unwrap(), &mut random);
            let feeling = roll_feeling(i32::try_from(depth).unwrap(), &mut random);
            let (expected_drops, expected_feeling, expected_next) = expected[depth as usize - 1];
            assert_eq!(drops, expected_drops);
            assert_eq!(feeling, expected_feeling);
            assert_eq!(random.long(), expected_next);
        }
        assert_eq!(limited.strength_potions, 2);
        assert_eq!(limited.upgrade_scrolls, 3);
        assert_eq!(limited.arcane_styli, 1);
    }
}
