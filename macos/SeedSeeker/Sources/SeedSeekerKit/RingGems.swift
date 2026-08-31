import Foundation

/// The gems one run gives the twelve ring classes.
///
/// Shattered Pixel Dungeon shuffles `Ring.gems` once in `Dungeon.init()` and
/// hands each ring class the gem at its own index, so which gem — and therefore
/// which colour — a ring shows is fixed by the seed, before a single floor is
/// generated and before any challenge is read. `items.png` lays the ring block
/// out in `Ring.gems` order, so a ring's cell is
/// ``SpriteSheet/ringSpriteBase`` plus its gem, *not* plus its class.
///
/// The engine publishes the table inside the scout packet, beside the run's
/// manifest; this is the Swift twin of its `RingGems`, and the Android client's
/// `ringGems` and the web scout document's `ringGems` are the same twelve
/// numbers.
public struct RingGems: Equatable, Hashable, Sendable {
    /// Ring classes, and gems, are twelve.
    public static let count = 12

    /// Gem ordinals indexed by ring class — the index a ring's
    /// ``CatalogItem/typeIconIndex`` names, which is also the order the catalog
    /// lists rings in.
    public let ordinals: [Int]

    /// `Ring.gems` before any run shuffles it: every class wears its own gem,
    /// so every ring lands on the cell the catalog gives it.
    ///
    /// This is what a surface with no run to ask must draw — the requirement
    /// board and its editor — since there the cell stands for the ring class
    /// itself rather than for anything a seed holds.
    public static let catalogDefault = RingGems(ordinals: Array(0..<RingGems.count))!

    /// Fails unless `ordinals` is a permutation of `0..<12`: a run's shuffle
    /// gives every class a distinct gem, so anything else is a corrupt table
    /// rather than an unusual run.
    public init?(ordinals: [Int]) {
        guard ordinals.count == Self.count, Set(ordinals) == Set(0..<Self.count) else { return nil }
        self.ordinals = ordinals
    }

    /// The gem this run gave the ring class `typeIconIndex` names, or nil when
    /// that is not a ring class at all.
    public func gem(forRingClass typeIconIndex: Int) -> Int? {
        ordinals.indices.contains(typeIconIndex) ? ordinals[typeIconIndex] : nil
    }
}

extension CatalogItem {
    /// The `items.png` cell this item is drawn in during a run whose ring gems
    /// are `gems`: ``spriteIndex`` for everything but a ring, whose cell is the
    /// run's gem for its class rather than the class's own.
    ///
    /// Pass nil where there is no run. The catalog cell is the ring class's
    /// identity, so a seedless surface keeps drawing it; every surface showing
    /// an item that belongs to a *particular* seed must pass that seed's table,
    /// or every seed renders the same twelve ring colours.
    public func spriteIndex(in gems: RingGems?) -> Int {
        guard let gems, let ringClass = typeIconIndex,
              let gem = gems.gem(forRingClass: ringClass) else { return spriteIndex }
        return SpriteSheet.ringSpriteBase + gem
    }
}
