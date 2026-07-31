import Foundation

/// Navigation through the ordered list of search-result seeds while scouting.
///
/// The seed detail pane can be reached either from a search result or by
/// typing a seed by hand; navigation is only meaningful in the first case, so
/// every helper returns nil when the current seed is not a search result.
public enum ResultNavigation {
    /// 0-based index of `seed` within `seeds`, or nil when it is not one of them.
    public static func position(of seed: String?, in seeds: [String]) -> Int? {
        guard let seed, !seed.isEmpty else { return nil }
        return seeds.firstIndex(of: seed)
    }

    /// The seed `offset` steps away from `seed` in the results, clamped to the
    /// list ends. Nil when `seed` is not a search result or the step would not
    /// move (already at the first or last result).
    public static func seed(from seed: String?, in seeds: [String], offset: Int) -> String? {
        guard let index = position(of: seed, in: seeds) else { return nil }
        let target = min(max(index + offset, 0), seeds.count - 1)
        return target == index ? nil : seeds[target]
    }
}
