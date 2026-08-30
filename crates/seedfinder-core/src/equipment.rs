//! Exact v4.0.0 randomization of generated weapons, armor, and wands.

use crate::catalog::{ArmorEffect, Effect, WeaponEffect};
use crate::rng::RandomStack;

pub(crate) const WEAPON_COMMON: [WeaponEffect; 5] = [
    WeaponEffect::Blazing,
    WeaponEffect::Chilling,
    WeaponEffect::Kinetic,
    WeaponEffect::Shocking,
    WeaponEffect::Venomous,
];
pub(crate) const WEAPON_UNCOMMON: [WeaponEffect; 8] = [
    WeaponEffect::Blocking,
    WeaponEffect::Blooming,
    WeaponEffect::Eldritch,
    WeaponEffect::Elastic,
    WeaponEffect::Lucky,
    WeaponEffect::Projecting,
    WeaponEffect::Unstable,
    WeaponEffect::Vorpal,
];
pub(crate) const WEAPON_RARE: [WeaponEffect; 4] = [
    WeaponEffect::Corrupting,
    WeaponEffect::Crystal,
    WeaponEffect::Grim,
    WeaponEffect::Vampiric,
];
pub(crate) const WEAPON_CURSES: [WeaponEffect; 10] = [
    WeaponEffect::Annoying,
    WeaponEffect::Displacing,
    WeaponEffect::Dazzling,
    WeaponEffect::Explosive,
    WeaponEffect::Friendly,
    WeaponEffect::Polarized,
    WeaponEffect::Pressurized,
    WeaponEffect::Sacrificial,
    WeaponEffect::Wayward,
    WeaponEffect::Wondrous,
];

pub(crate) const ARMOR_COMMON: [ArmorEffect; 4] = [
    ArmorEffect::Obfuscation,
    ArmorEffect::Swiftness,
    ArmorEffect::Viscosity,
    ArmorEffect::Potential,
];
pub(crate) const ARMOR_UNCOMMON: [ArmorEffect; 6] = [
    ArmorEffect::Brimstone,
    ArmorEffect::Stone,
    ArmorEffect::Entanglement,
    ArmorEffect::Repulsion,
    ArmorEffect::Camouflage,
    ArmorEffect::Flow,
];
pub(crate) const ARMOR_RARE: [ArmorEffect; 3] = [
    ArmorEffect::Affection,
    ArmorEffect::AntiMagic,
    ArmorEffect::Thorns,
];
pub(crate) const ARMOR_CURSES: [ArmorEffect; 8] = [
    ArmorEffect::AntiEntropy,
    ArmorEffect::Corrosion,
    ArmorEffect::Displacement,
    ArmorEffect::Metabolism,
    ArmorEffect::Multiplicity,
    ArmorEffect::Stench,
    ArmorEffect::Overgrowth,
    ArmorEffect::Bulk,
];

/// Mutable properties assigned by an equipment class's `random()` method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EquipmentRoll {
    pub upgrade: u8,
    pub effect: Option<Effect>,
    pub cursed: bool,
}

/// Mirrors `Weapon.random()` for the canonical profile (no Parchment Scrap).
pub fn roll_weapon(random: &mut RandomStack) -> EquipmentRoll {
    let upgrade = nested_upgrade_roll(random, 4);

    // Upstream deliberately isolates effect variance in a child RNG.
    let effect_seed = random.long();
    random.push(effect_seed);
    let effect_roll = random.float();
    let result = if effect_roll < 0.3 {
        EquipmentRoll {
            upgrade,
            effect: Some(Effect::Weapon(select(random, &WEAPON_CURSES))),
            cursed: true,
        }
    } else if effect_roll >= 0.9 {
        EquipmentRoll {
            upgrade,
            effect: Some(Effect::Weapon(random_weapon_enchantment(random))),
            cursed: false,
        }
    } else {
        EquipmentRoll {
            upgrade,
            effect: None,
            cursed: false,
        }
    };
    random.pop();
    result
}

/// Mirrors `Armor.random()` for the canonical profile (no Parchment Scrap).
pub fn roll_armor(random: &mut RandomStack) -> EquipmentRoll {
    let upgrade = nested_upgrade_roll(random, 4);

    let effect_seed = random.long();
    random.push(effect_seed);
    let effect_roll = random.float();
    let result = if effect_roll < 0.3 {
        EquipmentRoll {
            upgrade,
            effect: Some(Effect::Armor(select(random, &ARMOR_CURSES))),
            cursed: true,
        }
    } else if effect_roll >= 0.85 {
        EquipmentRoll {
            upgrade,
            effect: Some(Effect::Armor(random_armor_glyph(random))),
            cursed: false,
        }
    } else {
        EquipmentRoll {
            upgrade,
            effect: None,
            cursed: false,
        }
    };
    random.pop();
    result
}

/// Mirrors `Wand.random()`. Wands have a cursed flag but no enchantment type.
pub fn roll_wand(random: &mut RandomStack) -> EquipmentRoll {
    EquipmentRoll {
        upgrade: nested_upgrade_roll(random, 3),
        effect: None,
        cursed: random.float() < 0.3,
    }
}

fn nested_upgrade_roll(random: &mut RandomStack, first_bound: i32) -> u8 {
    if random.int_bound(first_bound) != 0 {
        return 0;
    }
    if random.int_bound(5) == 0 { 2 } else { 1 }
}

/// Mirrors `Weapon.Enchantment.random()` with no ignored class.
pub fn random_weapon_enchantment(random: &mut RandomStack) -> WeaponEffect {
    random_weapon_enchantment_ignoring(random, None)
}

/// Mirrors `Weapon.Enchantment.random(toIgnore)`, which `Weapon.enchant()`
/// calls with the weapon's current enchantment class (possibly a curse). The
/// rarity is drawn first; the ignored class is then removed from that rarity's
/// list before `Random.element`. An emptied list falls back to `random()`
/// without ignores.
pub fn random_weapon_enchantment_ignoring(
    random: &mut RandomStack,
    to_ignore: Option<WeaponEffect>,
) -> WeaponEffect {
    let selected = match random.chances(&[50.0, 40.0, 10.0]).unwrap_or_default() {
        0 => select_ignoring(random, &WEAPON_COMMON, to_ignore),
        1 => select_ignoring(random, &WEAPON_UNCOMMON, to_ignore),
        _ => select_ignoring(random, &WEAPON_RARE, to_ignore),
    };
    selected.unwrap_or_else(|| random_weapon_enchantment_ignoring(random, None))
}

/// Mirrors `Armor.Glyph.random()` with no ignored class.
pub fn random_armor_glyph(random: &mut RandomStack) -> ArmorEffect {
    random_armor_glyph_ignoring(random, None)
}

/// Mirrors `Armor.Glyph.random(toIgnore)`; see
/// [`random_weapon_enchantment_ignoring`].
pub fn random_armor_glyph_ignoring(
    random: &mut RandomStack,
    to_ignore: Option<ArmorEffect>,
) -> ArmorEffect {
    let selected = match random.chances(&[50.0, 40.0, 10.0]).unwrap_or_default() {
        0 => select_ignoring(random, &ARMOR_COMMON, to_ignore),
        1 => select_ignoring(random, &ARMOR_UNCOMMON, to_ignore),
        _ => select_ignoring(random, &ARMOR_RARE, to_ignore),
    };
    selected.unwrap_or_else(|| random_armor_glyph_ignoring(random, None))
}

/// `Random.element` over `values` minus `to_ignore`; `None` when nothing is
/// left to draw from (Java then falls back to `random()` with no ignores).
fn select_ignoring<T: Copy + PartialEq>(
    random: &mut RandomStack,
    values: &[T],
    to_ignore: Option<T>,
) -> Option<T> {
    let mut remaining = [None; 10];
    let mut count = 0_usize;
    for &value in values {
        if Some(value) != to_ignore {
            remaining[count] = Some(value);
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let bound = i32::try_from(count).expect("effect tables are tiny");
    let index = usize::try_from(random.int_bound(bound)).unwrap_or_default();
    remaining[index]
}

fn select<T: Copy>(random: &mut RandomStack, values: &[T]) -> T {
    let bound = i32::try_from(values.len()).unwrap_or(1);
    let index = usize::try_from(random.int_bound(bound)).unwrap_or_default();
    values[index]
}

#[cfg(test)]
mod tests {
    use crate::catalog::{ALL_ARMOR_EFFECTS, ALL_WEAPON_EFFECTS, ArmorEffect, Effect, WeaponEffect};
    use crate::rng::RandomStack;

    use super::{
        ARMOR_COMMON, ARMOR_CURSES, ARMOR_RARE, ARMOR_UNCOMMON, EquipmentRoll, WEAPON_COMMON,
        WEAPON_CURSES, WEAPON_RARE, WEAPON_UNCOMMON, roll_armor, roll_wand, roll_weapon,
    };

    fn stack(seed: i64) -> RandomStack {
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed);
        random
    }

    /// The catalog enums (and so the effect wire codes) follow the game
    /// journal: enchantments by rarity, then the curses. The rarity lists
    /// here are the source of that order.
    #[test]
    fn catalog_order_is_the_journal_order() {
        let weapon: Vec<WeaponEffect> = WEAPON_COMMON
            .into_iter()
            .chain(WEAPON_UNCOMMON)
            .chain(WEAPON_RARE)
            .chain(WEAPON_CURSES)
            .collect();
        assert_eq!(weapon, ALL_WEAPON_EFFECTS);
        assert!(weapon.windows(2).all(|pair| (pair[0] as u8) < (pair[1] as u8)));

        let armor: Vec<ArmorEffect> = ARMOR_COMMON
            .into_iter()
            .chain(ARMOR_UNCOMMON)
            .chain(ARMOR_RARE)
            .chain(ARMOR_CURSES)
            .collect();
        assert_eq!(armor, ALL_ARMOR_EFFECTS);
        assert!(armor.windows(2).all(|pair| (pair[0] as u8) < (pair[1] as u8)));
    }

    #[test]
    fn actual_java_game_sequence_matches_for_abc_numeric_seed() {
        let mut random = stack(8_687_205_886);
        assert_eq!(
            roll_weapon(&mut random),
            EquipmentRoll {
                upgrade: 2,
                effect: None,
                cursed: false
            }
        );
        assert_eq!(
            roll_armor(&mut random),
            EquipmentRoll {
                upgrade: 0,
                effect: None,
                cursed: false
            }
        );
        assert_eq!(
            roll_wand(&mut random),
            EquipmentRoll {
                upgrade: 0,
                effect: None,
                cursed: false
            }
        );
        assert_eq!(roll_weapon(&mut random).upgrade, 0);
        assert_eq!(roll_armor(&mut random).upgrade, 0);
        assert_eq!(roll_wand(&mut random).upgrade, 0);
    }

    #[test]
    fn actual_java_game_curse_and_enchantment_fixtures_match() {
        let weapon_zero = roll_weapon(&mut stack(0));
        assert_eq!(weapon_zero.upgrade, 0);
        assert_eq!(
            weapon_zero.effect,
            Some(Effect::Weapon(WeaponEffect::Polarized))
        );
        assert!(weapon_zero.cursed);

        let armor_zero = roll_armor(&mut stack(0));
        assert_eq!(
            armor_zero.effect,
            Some(Effect::Armor(ArmorEffect::Overgrowth))
        );
        assert!(armor_zero.cursed);

        let weapon_nine = roll_weapon(&mut stack(9));
        assert_eq!(
            weapon_nine.effect,
            Some(Effect::Weapon(WeaponEffect::Blazing))
        );
        assert!(!weapon_nine.cursed);

        let armor_nine = roll_armor(&mut stack(9));
        assert_eq!(
            armor_nine.effect,
            Some(Effect::Armor(ArmorEffect::Potential))
        );
        assert!(!armor_nine.cursed);
    }
}
