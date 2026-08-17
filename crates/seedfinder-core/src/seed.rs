//! Shattered Pixel Dungeon's nine-letter seed-code representation.

use std::fmt;

/// There are exactly 26^9 user-enterable seed codes.
pub const TOTAL_SEEDS: u64 = 5_429_503_678_976;

/// A validated user-enterable dungeon seed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DungeonSeed(u64);

impl DungeonSeed {
    /// The first seed, represented by `AAA-AAA-AAA`.
    pub const MIN: Self = Self(0);

    /// The final seed, represented by `ZZZ-ZZZ-ZZZ`.
    pub const MAX: Self = Self(TOTAL_SEEDS - 1);

    /// Creates a seed from its numeric representation.
    ///
    /// # Errors
    ///
    /// Returns [`SeedError::OutOfRange`] when `value >= 26^9`.
    pub const fn new(value: u64) -> Result<Self, SeedError> {
        if value < TOTAL_SEEDS {
            Ok(Self(value))
        } else {
            Err(SeedError::OutOfRange)
        }
    }

    /// Returns the numeric representation used by the game.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Parses the game's `XXX-XXX-XXX` form.
    ///
    /// Dashes and Unicode whitespace are ignored, matching the Java helper.
    /// Properly dashed eleven-character codes are accepted case-insensitively;
    /// undashed codes must already use uppercase ASCII, matching the upstream
    /// implementation's somewhat unusual behavior.
    ///
    /// # Errors
    ///
    /// Returns [`SeedError::InvalidCode`] unless exactly nine `A..=Z` digits
    /// remain after separators are removed.
    pub fn from_code(input: &str) -> Result<Self, SeedError> {
        let utf16: Vec<u16> = input.encode_utf16().collect();
        let properly_dashed = utf16.len() == 11
            && utf16.get(3) == Some(&u16::from(b'-'))
            && utf16.get(7) == Some(&u16::from(b'-'));

        let normalized = if properly_dashed {
            input.to_uppercase()
        } else {
            input.to_owned()
        };
        let digits: Vec<char> = normalized
            .chars()
            .filter(|character| *character != '-' && !java_regex_whitespace(*character))
            .collect();

        if digits.len() != 9 || digits.iter().any(|digit| !digit.is_ascii_uppercase()) {
            return Err(SeedError::InvalidCode);
        }

        let mut value = 0_u64;
        for digit in digits {
            value = value * 26 + u64::from(u32::from(digit) - u32::from('A'));
        }
        Ok(Self(value))
    }

    /// Converts the seed to the canonical `XXX-XXX-XXX` representation.
    #[must_use]
    pub fn to_code(self) -> String {
        let mut value = self.0;
        let mut raw = [b'A'; 9];
        for digit in raw.iter_mut().rev() {
            *digit += u8::try_from(value % 26).unwrap_or_default();
            value /= 26;
        }

        let mut code = String::with_capacity(11);
        for (index, digit) in raw.into_iter().enumerate() {
            if index == 3 || index == 6 {
                code.push('-');
            }
            code.push(char::from(digit));
        }
        code
    }
}

/// Mirrors `DungeonSeed.convertFromText`, including Java UTF-16 hashing and
/// signed remainder behavior. Unlike [`DungeonSeed`], arbitrary text can map
/// to a negative `long` (notably numeric input such as `-1`).
#[must_use]
pub fn from_text(input: &str) -> i64 {
    if input.is_empty() {
        return -1;
    }
    if let Ok(seed) = DungeonSeed::from_code(input) {
        return i64::try_from(seed.value()).unwrap_or_default();
    }

    let numeric: String = input
        .chars()
        .filter(|character| !java_regex_whitespace(*character))
        .collect();
    if let Ok(value) = numeric.parse::<i64>() {
        return value % i64::try_from(TOTAL_SEEDS).unwrap_or(i64::MAX);
    }

    let mut total = 0_i64;
    for code_unit in input.encode_utf16() {
        total = total.wrapping_mul(31).wrapping_add(i64::from(code_unit));
    }
    if total < 0 {
        total = total.wrapping_add(i64::MAX);
    }
    total % i64::try_from(TOTAL_SEEDS).unwrap_or(i64::MAX)
}

/// Canonicalizes valid seed codes and leaves all other input unchanged.
#[must_use]
pub fn format_text(input: &str) -> String {
    DungeonSeed::from_code(input).map_or_else(|_| input.to_owned(), DungeonSeed::to_code)
}

/// Masks partial, as-you-type seed input into uppercase groups of three.
///
/// Every byte that is not an ASCII letter is dropped, the first nine of the
/// survivors are kept, and only then are they uppercased — so non-ASCII input
/// contributes nothing, whatever case mapping its own alphabet would use. The
/// result is a prefix of a canonical `XXX-XXX-XXX` code, which
/// [`DungeonSeed::from_code`] accepts once nine letters have arrived.
#[must_use]
pub fn format_input(input: &str) -> String {
    let mut output = String::with_capacity(11);
    for (index, byte) in input
        .bytes()
        .filter(u8::is_ascii_alphabetic)
        .take(9)
        .enumerate()
    {
        if index == 3 || index == 6 {
            output.push('-');
        }
        output.push(char::from(byte.to_ascii_uppercase()));
    }
    output
}

impl fmt::Display for DungeonSeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_code())
    }
}

/// Validation failures for seed input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeedError {
    /// Seed number is outside `[0, 26^9)`.
    OutOfRange,
    /// Code is not nine uppercase base-26 digits after separators are removed.
    InvalidCode,
}

impl fmt::Display for SeedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange => formatter.write_str("seed must be in the range [0, 26^9)"),
            Self::InvalidCode => formatter.write_str("seed code must contain nine A-Z characters"),
        }
    }
}

impl std::error::Error for SeedError {}

// Java Pattern's default `\\s` is the ASCII set unless UNICODE_CHARACTER_CLASS
// is enabled. This is deliberately not Rust's broader `char::is_whitespace`.
const fn java_regex_whitespace(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | ' '
    )
}

#[cfg(test)]
mod tests {
    use super::{DungeonSeed, SeedError, TOTAL_SEEDS, format_input, format_text, from_text};

    #[test]
    fn known_code_boundaries_match_upstream() {
        assert_eq!(DungeonSeed::MIN.to_code(), "AAA-AAA-AAA");
        assert_eq!(DungeonSeed::MAX.to_code(), "ZZZ-ZZZ-ZZZ");
        assert_eq!(DungeonSeed::from_code("AAA-AAA-AAB").unwrap().value(), 1);
        assert_eq!(
            DungeonSeed::from_code("ZZZ-ZZZ-ZZZ").unwrap().value(),
            TOTAL_SEEDS - 1
        );
    }

    #[test]
    fn code_round_trips_representative_values() {
        for value in [0, 1, 25, 26, 17_576, 123_456_789, TOTAL_SEEDS - 1] {
            let seed = DungeonSeed::new(value).unwrap();
            assert_eq!(DungeonSeed::from_code(&seed.to_code()), Ok(seed));
        }
    }

    #[test]
    fn parsing_matches_upstream_case_and_separator_rules() {
        assert_eq!(DungeonSeed::from_code("aaa-aaa-aab").unwrap().value(), 1);
        assert_eq!(
            DungeonSeed::from_code("A A A-A A A-A A B").unwrap().value(),
            1
        );
        assert_eq!(
            DungeonSeed::from_code("aaaaaaaaa"),
            Err(SeedError::InvalidCode)
        );
        assert_eq!(
            DungeonSeed::from_code("AAA-AAA-AA0"),
            Err(SeedError::InvalidCode)
        );
    }

    #[test]
    fn rejects_numeric_values_outside_user_seed_space() {
        assert_eq!(DungeonSeed::new(TOTAL_SEEDS), Err(SeedError::OutOfRange));
        assert_eq!(DungeonSeed::new(u64::MAX), Err(SeedError::OutOfRange));
    }

    #[test]
    fn arbitrary_text_matches_java_utf16_hash_fixtures() {
        assert_eq!(from_text("abc-def-ghi"), 8_687_205_886);
        assert_eq!(from_text("abcdefghi"), 4_074_933_826_149);
        assert_eq!(from_text("123 456"), 123_456);
        assert_eq!(from_text("-1"), -1);
        assert_eq!(from_text("Shattered Pixel Dungeon"), 2_149_886_743_767);
        assert_eq!(from_text("😀"), 1_772_899);
    }

    #[test]
    fn formatting_only_canonicalizes_codes() {
        assert_eq!(format_text("abc-def-ghi"), "ABC-DEF-GHI");
        assert_eq!(format_text("abcdefghi"), "abcdefghi");
    }

    #[test]
    fn masking_groups_the_first_nine_ascii_letters() {
        assert_eq!(format_input(""), "");
        assert_eq!(format_input("a"), "A");
        assert_eq!(format_input("abcD"), "ABC-D");
        assert_eq!(format_input("abc-def-ghi"), "ABC-DEF-GHI");
        assert_eq!(format_input(" 1a!b@c#d$e%f^g&h*i extra"), "ABC-DEF-GHI");
        // Every masked prefix of nine letters is a parseable canonical code.
        assert_eq!(
            DungeonSeed::from_code(&format_input("aaa aaa aab")).unwrap(),
            DungeonSeed::from_code("AAA-AAA-AAB").unwrap()
        );

        // Filtering happens before uppercasing, so non-ASCII letters are
        // dropped whatever their own case mapping would produce: Turkish
        // dotless i (U+0131) uppercases to ASCII 'I' but never reaches the
        // mask, unlike a port that uppercases the string first.
        assert_eq!(format_input("åa😀b"), "AB");
        assert_eq!(format_input("\u{131}ab"), "AB");
        assert_eq!(format_input("\u{131}"), "");
    }
}
