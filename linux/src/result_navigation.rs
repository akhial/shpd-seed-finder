// SPDX-License-Identifier: GPL-3.0-or-later

//! Navigation through the ordered list of search-result seeds while scouting.
//!
//! The seed pane can be reached either from a search result or by typing a
//! seed by hand; navigation is only meaningful in the first case, so every
//! helper returns `None` when the current seed is not a search result.

/// 0-based position of `seed` among the found `seeds`, with the total count.
pub fn position(seeds: &[String], seed: &str) -> Option<(usize, usize)> {
    seeds
        .iter()
        .position(|code| code == seed)
        .map(|index| (index, seeds.len()))
}

/// Index of the result `delta` steps away from `seed`, clamped to the list
/// ends. `None` when `seed` is not a search result or the step would not move
/// (already at the first or last result).
pub fn step(seeds: &[String], seed: &str, delta: i64) -> Option<usize> {
    let (index, total) = position(seeds, seed)?;
    let (index, last) = (i64::try_from(index).ok()?, i64::try_from(total).ok()? - 1);
    let target = (index + delta).clamp(0, last);
    (target != index).then(|| usize::try_from(target).ok())?
}

#[cfg(test)]
mod tests {
    use super::{position, step};

    fn seeds() -> Vec<String> {
        ["AAA-AAA-AAA", "BBB-BBB-BBB", "CCC-CCC-CCC"]
            .map(str::to_owned)
            .to_vec()
    }

    #[test]
    fn position_locates_a_scouted_seed_inside_the_results() {
        assert_eq!(position(&seeds(), "AAA-AAA-AAA"), Some((0, 3)));
        assert_eq!(position(&seeds(), "CCC-CCC-CCC"), Some((2, 3)));
    }

    #[test]
    fn position_is_none_outside_the_results() {
        assert_eq!(position(&seeds(), "ZZZ-ZZZ-ZZZ"), None);
        assert_eq!(position(&seeds(), ""), None);
    }

    #[test]
    fn position_is_dropped_when_a_new_search_clears_the_results() {
        // A scouted seed keeps its manifest, but an emptied results list must
        // invalidate its position.
        assert_eq!(position(&[], "AAA-AAA-AAA"), None);
        assert_eq!(step(&[], "AAA-AAA-AAA", 1), None);
    }

    #[test]
    fn step_moves_forward_and_backward() {
        assert_eq!(step(&seeds(), "AAA-AAA-AAA", 1), Some(1));
        assert_eq!(step(&seeds(), "BBB-BBB-BBB", 1), Some(2));
        assert_eq!(step(&seeds(), "CCC-CCC-CCC", -1), Some(1));
    }

    #[test]
    fn step_does_not_wrap_past_the_ends() {
        assert_eq!(step(&seeds(), "AAA-AAA-AAA", -1), None);
        assert_eq!(step(&seeds(), "CCC-CCC-CCC", 1), None);
    }

    #[test]
    fn step_clamps_larger_jumps_to_the_list_ends() {
        assert_eq!(step(&seeds(), "BBB-BBB-BBB", 5), Some(2));
        assert_eq!(step(&seeds(), "BBB-BBB-BBB", -5), Some(0));
    }

    #[test]
    fn step_is_inert_without_an_anchor_in_the_results() {
        assert_eq!(step(&seeds(), "ZZZ-ZZZ-ZZZ", 1), None);
        assert_eq!(step(&["AAA-AAA-AAA".to_owned()], "AAA-AAA-AAA", 1), None);
    }
}
