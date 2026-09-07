//! Exact v3.3.8 Sewer `MobSpawner` rotations and constructor-time RNG.
//!
//! Spatial placement is deliberately separate. `RegularLevel.createMobs()`
//! may retain a constructed mob while retrying positions, so callers must ask
//! [`MobQueue::create_mob`] only where Java calls `createMob()` and keep that
//! result across placement failures.

use crate::rng::RandomStack;

/// Every class that can enter a canonical Sewer rotation without trinkets.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum SewerMobKind {
    Rat,
    Snake,
    Gnoll,
    Swarm,
    Crab,
    Slime,
    Albino,
    GnollExile,
    HermitCrab,
    CausticSlime,
    /// The 2.5% depth-four out-of-region rare spawn.
    Thief,
}

/// `Thief` chooses its loot category in its instance initializer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThiefLootCategory {
    Ring,
    Artifact,
}

/// Generation-visible result of reflection construction followed by
/// `ChampionEnemy.rollForChampion` in the no-challenge profile.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstructedSewerMob {
    pub kind: SewerMobKind,
    pub thief_loot: Option<ThiefLootCategory>,
    /// The class ordinal selected by `Random.Int(6)`. No buff is attached when
    /// the Champion Enemies challenge is disabled, but the draw is mandatory.
    pub discarded_champion_roll: u8,
}

/// Persistent `Level.mobsToSpawn` queue for one level instance.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MobQueue {
    remaining: Vec<SewerMobKind>,
}

impl MobQueue {
    #[must_use]
    pub fn remaining(&self) -> &[SewerMobKind] {
        &self.remaining
    }

    /// Mirrors `Level.createMob()`: refill from a freshly shuffled rotation
    /// when empty, remove index zero, construct, then discard a champion roll.
    ///
    /// # Panics
    ///
    /// Panics when called outside Sewer depths 1 through 4.
    pub fn create_mob(&mut self, depth: u32, random: &mut RandomStack) -> ConstructedSewerMob {
        if self.remaining.is_empty() {
            self.remaining = sewer_mob_rotation(depth, random);
        }
        let kind = self.remaining.remove(0);
        let thief_loot = (kind == SewerMobKind::Thief).then(|| {
            if random.int_bound(2) == 0 {
                ThiefLootCategory::Ring
            } else {
                ThiefLootCategory::Artifact
            }
        });
        let discarded_champion_roll = u8::try_from(random.int_bound(6)).unwrap_or_default();
        ConstructedSewerMob {
            kind,
            thief_loot,
            discarded_champion_roll,
        }
    }
}

/// `MobSpawner.getMobRotation` for canonical Sewer depths. This includes the
/// depth-four rare Thief check, one rare-alt Float per entry, and Java's
/// `Collections.shuffle(ArrayList, Random)` ordering.
///
/// # Panics
///
/// Panics when called outside Sewer depths 1 through 4.
#[must_use]
pub fn sewer_mob_rotation(depth: u32, random: &mut RandomStack) -> Vec<SewerMobKind> {
    use SewerMobKind::{Crab, Gnoll, Rat, Slime, Snake, Swarm, Thief};

    let mut rotation = match depth {
        1 => vec![Rat, Rat, Rat, Snake],
        2 => vec![Rat, Rat, Snake, Gnoll, Gnoll],
        3 => vec![Rat, Snake, Gnoll, Gnoll, Gnoll, Swarm, Crab],
        4 => vec![Gnoll, Swarm, Crab, Crab, Slime, Slime],
        _ => panic!("Sewer mob rotations are defined only for depths 1..=4"),
    };

    if depth == 4 && random.float() < 0.025_f32 {
        rotation.push(Thief);
    }

    for kind in &mut rotation {
        if random.float() < 0.02_f32 * random.trinket.exotic_multiplier() {
            *kind = rare_alt(*kind);
        }
    }
    random.shuffle_list(&mut rotation);
    rotation
}

const fn rare_alt(kind: SewerMobKind) -> SewerMobKind {
    match kind {
        SewerMobKind::Rat => SewerMobKind::Albino,
        SewerMobKind::Gnoll => SewerMobKind::GnollExile,
        SewerMobKind::Crab => SewerMobKind::HermitCrab,
        SewerMobKind::Slime => SewerMobKind::CausticSlime,
        _ => kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sequence(depth: u32, seed: i64, count: usize) -> (Vec<ConstructedSewerMob>, i64) {
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed);
        let mut queue = MobQueue::default();
        let mobs = (0..count)
            .map(|_| queue.create_mob(depth, &mut random))
            .collect();
        (mobs, random.long())
    }

    fn short(mob: ConstructedSewerMob) -> String {
        format!(
            "{:?}:{}:{:?}",
            mob.kind, mob.discarded_champion_roll, mob.thief_loot
        )
    }

    #[test]
    fn refill_sequences_match_java_collections_shuffle_oracle() {
        let fixtures = [
            (
                1,
                0,
                2_377_732_757_510_138_102,
                [
                    "Rat:3:None",
                    "Rat:3:None",
                    "Snake:2:None",
                    "Rat:5:None",
                    "Rat:5:None",
                    "Rat:2:None",
                    "Rat:1:None",
                    "Snake:0:None",
                ]
                .as_slice(),
            ),
            (
                3,
                1,
                40_964_018_579_104_698,
                [
                    "Snake:4:None",
                    "Rat:3:None",
                    "Gnoll:3:None",
                    "Swarm:4:None",
                    "Gnoll:0:None",
                    "Gnoll:5:None",
                    "Crab:2:None",
                    "Snake:2:None",
                    "Swarm:2:None",
                    "Rat:2:None",
                    "Gnoll:2:None",
                    "Gnoll:0:None",
                    "Gnoll:2:None",
                    "Crab:3:None",
                ]
                .as_slice(),
            ),
        ];

        for (depth, seed, expected_next, expected) in fixtures {
            let (actual, next) = sequence(depth, seed, expected.len());
            assert_eq!(actual.into_iter().map(short).collect::<Vec<_>>(), expected);
            assert_eq!(next, expected_next);
        }
    }

    #[test]
    fn rare_alt_and_thief_constructor_paths_are_pinned() {
        // These seeds were selected by the Java helper to cover a Rat alt and
        // the depth-four 2.5% Thief insertion respectively.
        let (mobs, next) = sequence(1, 15, 4);
        assert_eq!(
            mobs.into_iter().map(short).collect::<Vec<_>>(),
            ["Albino:1:None", "Rat:0:None", "Snake:1:None", "Rat:5:None"]
        );
        assert_eq!(next, 6_104_042_912_312_235_191);

        let (mobs, next) = sequence(4, 40, 7);
        assert_eq!(
            mobs.into_iter().map(short).collect::<Vec<_>>(),
            [
                "Crab:3:None",
                "Gnoll:2:None",
                "Crab:5:None",
                "Thief:4:Some(Artifact)",
                "Slime:2:None",
                "Swarm:2:None",
                "Slime:3:None",
            ]
        );
        assert_eq!(next, 2_725_982_269_675_418_876);
    }
}
