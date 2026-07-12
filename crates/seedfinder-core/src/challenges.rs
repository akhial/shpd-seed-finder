//! Shattered Pixel Dungeon v3.3.8 challenge bitmask.

use std::fmt;

/// Validated subset of `Challenges` enabled for one dungeon run.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Challenges(u16);

impl Challenges {
    pub const NONE: Self = Self(0);
    pub const NO_FOOD: Self = Self(1);
    pub const NO_ARMOR: Self = Self(2);
    pub const NO_HEALING: Self = Self(4);
    pub const NO_HERBALISM: Self = Self(8);
    pub const SWARM_INTELLIGENCE: Self = Self(16);
    pub const DARKNESS: Self = Self(32);
    pub const NO_SCROLLS: Self = Self(64);
    pub const CHAMPION_ENEMIES: Self = Self(128);
    pub const STRONGER_BOSSES: Self = Self(256);
    pub const MAX_VALUE: u16 = 511;

    /// Validates an upstream challenge mask.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidChallenges`] when `bits` exceeds [`Self::MAX_VALUE`].
    pub const fn new(bits: u16) -> Result<Self, InvalidChallenges> {
        if bits <= Self::MAX_VALUE {
            Ok(Self(bits))
        } else {
            Err(InvalidChallenges(bits))
        }
    }

    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, challenge: Self) -> bool {
        self.0 & challenge.0 == challenge.0
    }
}

impl std::ops::BitOr for Challenges {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for Challenges {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidChallenges(pub u16);

impl fmt::Display for InvalidChallenges {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "challenge mask must be in 0..=511, got {}",
            self.0
        )
    }
}

impl std::error::Error for InvalidChallenges {}

#[cfg(test)]
mod tests {
    use super::{Challenges, InvalidChallenges};

    #[test]
    fn accepts_only_upstream_mask_bits() {
        assert_eq!(Challenges::new(511).unwrap().bits(), 511);
        assert_eq!(Challenges::new(512), Err(InvalidChallenges(512)));
    }
}
