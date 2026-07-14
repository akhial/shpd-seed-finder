// SPDX-License-Identifier: GPL-3.0-or-later

//! Small display-formatting helpers.

/// Groups a count into thousands separated by thin non-breaking spaces.
pub fn group_digits(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    let offset = digits.len() % 3;
    for (index, character) in digits.chars().enumerate() {
        if index != 0 && index % 3 == offset {
            output.push('\u{202f}');
        }
        output.push(character);
    }
    output
}

/// Formats an intrinsic match probability as scientific-notation percent.
pub fn probability_percent(probability: Option<f64>) -> String {
    let Some(probability) = probability.filter(|value| *value > 0.0 && value.is_finite()) else {
        return "estimating…".to_owned();
    };
    let percent = probability * 100.0;
    let mut exponent = percent.log10().floor();
    let mut mantissa = percent / 10.0_f64.powf(exponent);
    if mantissa >= 9.95 {
        mantissa = 1.0;
        exponent += 1.0;
    }
    format!("{mantissa:.1}\u{d7}10{}%", superscript(exponent))
}

fn superscript(exponent: f64) -> String {
    #[allow(clippy::cast_possible_truncation)]
    let exponent = exponent as i32;
    exponent
        .to_string()
        .chars()
        .map(|digit| match digit {
            '-' => '\u{207b}',
            '0' => '\u{2070}',
            '1' => '\u{b9}',
            '2' => '\u{b2}',
            '3' => '\u{b3}',
            '4' => '\u{2074}',
            '5' => '\u{2075}',
            '6' => '\u{2076}',
            '7' => '\u{2077}',
            '8' => '\u{2078}',
            other => other,
        })
        .collect()
}

/// Formats an elapsed wall-clock duration compactly, e.g. `3m 12s`.
pub fn duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "\u{2014}".to_owned();
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total = seconds.round() as u64;
    if total < 60 {
        format!("{total}s")
    } else if total < 3_600 {
        format!("{}m {}s", total / 60, total % 60)
    } else {
        format!("{}h {}m", total / 3_600, (total % 3_600) / 60)
    }
}

/// Formats an estimated duration in natural units, e.g. `2.3 minutes`.
pub fn estimate_duration(seconds: Option<f64>) -> String {
    let Some(seconds) = seconds.filter(|value| value.is_finite() && *value >= 0.0) else {
        return "estimating…".to_owned();
    };
    let (value, unit) = if seconds < 60.0 {
        (seconds, "second")
    } else if seconds < 3_600.0 {
        (seconds / 60.0, "minute")
    } else if seconds < 86_400.0 {
        (seconds / 3_600.0, "hour")
    } else {
        (seconds / 86_400.0, "day")
    };
    let suffix = if (0.95..1.05).contains(&value) {
        ""
    } else {
        "s"
    };
    format!("{value:.1} {unit}{suffix}")
}

/// Formats a seeds-per-second rate compactly, e.g. `1.4M`.
pub fn seed_rate(value: f64) -> String {
    if value <= 0.0 || !value.is_finite() {
        return "\u{2014}".to_owned();
    }
    if value >= 1e6 {
        format!("{:.1}M", value / 1e6)
    } else if value >= 1e3 {
        format!("{:.1}k", value / 1e3)
    } else {
        format!("{value:.0}")
    }
}

#[cfg(test)]
mod tests {
    use super::{duration, estimate_duration, group_digits, probability_percent, seed_rate};

    #[test]
    fn groups_thousands() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1_000), "1\u{202f}000");
        assert_eq!(
            group_digits(5_429_503_678_976),
            "5\u{202f}429\u{202f}503\u{202f}678\u{202f}976"
        );
    }

    #[test]
    fn formats_probability_as_scientific_percent() {
        assert_eq!(probability_percent(None), "estimating…");
        assert_eq!(probability_percent(Some(0.0)), "estimating…");
        assert_eq!(
            probability_percent(Some(0.000_012)),
            "1.2\u{d7}10\u{207b}\u{b3}%"
        );
        assert_eq!(probability_percent(Some(0.099_5)), "1.0\u{d7}10\u{b9}%");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(duration(42.4), "42s");
        assert_eq!(duration(192.0), "3m 12s");
        assert_eq!(duration(3_720.0), "1h 2m");
        assert_eq!(duration(f64::NAN), "\u{2014}");
        assert_eq!(estimate_duration(None), "estimating…");
        assert_eq!(estimate_duration(Some(59.0)), "59.0 seconds");
        assert_eq!(estimate_duration(Some(61.0)), "1.0 minute");
        assert_eq!(estimate_duration(Some(9_000.0)), "2.5 hours");
    }

    #[test]
    fn formats_seed_rates() {
        assert_eq!(seed_rate(0.0), "\u{2014}");
        assert_eq!(seed_rate(950.0), "950");
        assert_eq!(seed_rate(1_400.0), "1.4k");
        assert_eq!(seed_rate(2_500_000.0), "2.5M");
    }
}
