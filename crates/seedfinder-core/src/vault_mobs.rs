//! Exact v4.0.0 `VaultLevel` mob rotation and constructor-time RNG.
//!
//! `VaultLevel.createMob()` keeps its own `mobsToSpawn` deck: two tier-one
//! classes plus one random duplicate, all three tier-two classes, both
//! tier-three classes plus one random duplicate, shuffled with
//! `Collections.shuffle`. The abstract `VaultElemental` entry resolves to a
//! concrete variant through `VaultElemental.random()` only when it is popped;
//! rooms that hand a class back with `returnMob` insert the *concrete* class,
//! which is constructed later without a second variant draw.
//!
//! Reflection construction itself draws from the depth stream for some
//! classes: `VaultShaman` picks its debuff type, `DM200` and `Golem` pick a
//! loot category, and every `Elemental` rolls its ranged cooldown.

use crate::rng::RandomStack;

/// Every class that can sit in `VaultLevel.mobsToSpawn`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum VaultMobClass {
    Skeleton,
    Dm100,
    Shaman,
    Dm200,
    Ghoul,
    /// The abstract `VaultElemental` deck entry; resolved when popped.
    Elemental,
    Golem,
    FireElemental,
    FrostElemental,
    ShockElemental,
}

/// A concrete mob placed on the Vault level.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum VaultMobKind {
    Skeleton,
    Dm100,
    Shaman,
    Dm200,
    Ghoul,
    Golem,
    FireElemental,
    FrostElemental,
    ShockElemental,
    Sentry,
    Laser,
    TokenDoor,
    Mirror,
}

impl VaultMobKind {
    /// The `Class` object a room sees through `enemy.getClass()`.
    #[must_use]
    pub const fn class(self) -> Option<VaultMobClass> {
        match self {
            Self::Skeleton => Some(VaultMobClass::Skeleton),
            Self::Dm100 => Some(VaultMobClass::Dm100),
            Self::Shaman => Some(VaultMobClass::Shaman),
            Self::Dm200 => Some(VaultMobClass::Dm200),
            Self::Ghoul => Some(VaultMobClass::Ghoul),
            Self::Golem => Some(VaultMobClass::Golem),
            Self::FireElemental => Some(VaultMobClass::FireElemental),
            Self::FrostElemental => Some(VaultMobClass::FrostElemental),
            Self::ShockElemental => Some(VaultMobClass::ShockElemental),
            Self::Sentry | Self::Laser | Self::TokenDoor | Self::Mirror => None,
        }
    }

    /// `Char.hasProp(mob, Char.Property.LARGE)`.
    #[must_use]
    pub const fn is_large(self) -> bool {
        matches!(self, Self::Dm200 | Self::Golem)
    }

    /// Whether the mob's class is one of `VaultLevel.T1Mobs`.
    #[must_use]
    pub const fn is_tier_one(self) -> bool {
        matches!(self, Self::Skeleton | Self::Dm100)
    }

    /// Whether the mob's class is one of `VaultLevel.T2Mobs`.
    #[must_use]
    pub const fn is_tier_two(self) -> bool {
        matches!(self, Self::Shaman | Self::Dm200 | Self::Ghoul)
    }

    /// The loot tier the enemy rooms derive from `T1Mobs`/`T2Mobs`/`T3Mobs`
    /// membership plus the `instanceof Elemental` fallback, starting from
    /// `default_tier`.
    #[must_use]
    pub const fn treasure_tier(self, default_tier: u8) -> u8 {
        match self {
            Self::Skeleton | Self::Dm100 => 1,
            Self::Shaman | Self::Dm200 | Self::Ghoul => 2,
            Self::Golem | Self::FireElemental | Self::FrostElemental | Self::ShockElemental => 3,
            Self::Sentry | Self::Laser | Self::TokenDoor | Self::Mirror => default_tier,
        }
    }
}

/// A placed mob and its cell.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct VaultMob {
    pub kind: VaultMobKind,
    pub cell: usize,
}

/// `Reflection.newInstance(cls)` for a vault enemy class, including the
/// draws its constructor chain makes.
///
/// # Panics
///
/// Panics for the abstract `Elemental` entry, which must be resolved through
/// [`resolve_elemental`] first.
pub fn construct(class: VaultMobClass, random: &mut RandomStack) -> VaultMobKind {
    match class {
        VaultMobClass::Skeleton => VaultMobKind::Skeleton,
        VaultMobClass::Dm100 => VaultMobKind::Dm100,
        VaultMobClass::Shaman => {
            // VaultShaman(): this.type = Random.Int(5)
            let _ = random.int_bound(5);
            VaultMobKind::Shaman
        }
        VaultMobClass::Dm200 => {
            // DM200(): loot = Random.oneOf(WEAPON, ARMOR)
            let _ = random.int_bound(2);
            VaultMobKind::Dm200
        }
        VaultMobClass::Ghoul => VaultMobKind::Ghoul,
        VaultMobClass::Golem => {
            // Golem(): loot = Random.oneOf(WEAPON, ARMOR)
            let _ = random.int_bound(2);
            VaultMobKind::Golem
        }
        VaultMobClass::Elemental => panic!("abstract VaultElemental must be resolved first"),
        VaultMobClass::FireElemental
        | VaultMobClass::FrostElemental
        | VaultMobClass::ShockElemental => {
            // Elemental(): rangedCooldown = Random.NormalIntRange(3, 5)
            let _ = random.normal_int_range(3, 5);
            match class {
                VaultMobClass::FireElemental => VaultMobKind::FireElemental,
                VaultMobClass::FrostElemental => VaultMobKind::FrostElemental,
                _ => VaultMobKind::ShockElemental,
            }
        }
    }
}

/// `VaultElemental.random()`.
pub fn resolve_elemental(random: &mut RandomStack) -> VaultMobClass {
    let roll = random.float();
    if roll < 0.4 {
        VaultMobClass::FireElemental
    } else if roll < 0.8 {
        VaultMobClass::FrostElemental
    } else {
        VaultMobClass::ShockElemental
    }
}

const T1_MOBS: [VaultMobClass; 2] = [VaultMobClass::Skeleton, VaultMobClass::Dm100];
const T2_MOBS: [VaultMobClass; 3] = [
    VaultMobClass::Shaman,
    VaultMobClass::Dm200,
    VaultMobClass::Ghoul,
];
const T3_MOBS: [VaultMobClass; 2] = [VaultMobClass::Elemental, VaultMobClass::Golem];

/// `VaultLevel.mobsToSpawn` and its `createMob`/`returnMob` operations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VaultMobDeck {
    to_spawn: Vec<VaultMobClass>,
}

impl VaultMobDeck {
    #[must_use]
    pub fn remaining(&self) -> &[VaultMobClass] {
        &self.to_spawn
    }

    /// `VaultLevel.createMob()`.
    pub fn create_mob(&mut self, random: &mut RandomStack) -> VaultMobKind {
        if self.to_spawn.is_empty() {
            self.to_spawn.extend(T1_MOBS);
            self.to_spawn.push(one_of(&T1_MOBS, random));
            self.to_spawn.extend(T2_MOBS);
            self.to_spawn.extend(T3_MOBS);
            self.to_spawn.push(one_of(&T3_MOBS, random));
            random.shuffle_list(&mut self.to_spawn);
        }
        let mut class = self.to_spawn.remove(0);
        if class == VaultMobClass::Elemental {
            class = resolve_elemental(random);
        }
        construct(class, random)
    }

    /// `VaultLevel.returnMob(cls)`: the class goes back to the front.
    pub fn return_mob(&mut self, class: VaultMobClass) {
        self.to_spawn.insert(0, class);
    }
}

/// `Random.oneOf(VaultLevel.T2Mobs)` followed by reflection construction, as
/// used by `VaultSingleEnemyTreasureRoom`.
pub fn random_tier_two_enemy(random: &mut RandomStack) -> VaultMobKind {
    let class = one_of(&T2_MOBS, random);
    construct(class, random)
}

fn one_of(classes: &[VaultMobClass], random: &mut RandomStack) -> VaultMobClass {
    let bound = i32::try_from(classes.len()).expect("mob table fits Java int");
    classes[usize::try_from(random.int_bound(bound)).expect("Random.Int is non-negative")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_refills_with_nine_classes_and_resolves_elementals() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(7);
        let mut deck = VaultMobDeck::default();
        let first = deck.create_mob(&mut random);
        assert_eq!(deck.remaining().len(), 8);
        assert!(!deck.remaining().contains(&VaultMobClass::FireElemental));
        assert!(first.class().is_some());
        let mut kinds = vec![first];
        for _ in 0..8 {
            kinds.push(deck.create_mob(&mut random));
        }
        assert!(deck.remaining().is_empty());
        assert_eq!(kinds.iter().filter(|kind| kind.is_tier_one()).count(), 3);
        assert_eq!(kinds.iter().filter(|kind| kind.is_tier_two()).count(), 3);
        assert_eq!(
            kinds
                .iter()
                .filter(|kind| kind.treasure_tier(1) == 3)
                .count(),
            3
        );
    }

    #[test]
    fn returned_concrete_elementals_are_rebuilt_without_a_variant_draw() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(11);
        let mut deck = VaultMobDeck::default();
        deck.return_mob(VaultMobClass::ShockElemental);
        let before = random.clone();
        assert_eq!(deck.create_mob(&mut random), VaultMobKind::ShockElemental);
        // Only the Elemental constructor's NormalIntRange (two floats).
        let mut expected = before;
        let _ = expected.float();
        let _ = expected.float();
        assert_eq!(random.int(), expected.int());
    }
}
