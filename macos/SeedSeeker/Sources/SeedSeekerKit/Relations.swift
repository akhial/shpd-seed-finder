import Foundation

/**
 Pure edits behind the requirement board, ported from the web design's
 `web/src/designs/one/relations.ts` so every platform writes the same
 documents. Every edit returns a new requirement list in the canonical
 encoding, so share links, presets and results files round-trip; the board
 renders the *collapsed* view that ``Swift/Array/boardItems()`` derives from
 the flat list.

 Two ideas cover all three relationship kinds of the model:

 - an *either/or cluster* is several requirements sharing an
   ``ItemRequirement/alternativeGroup``: one slot, any member fills it;
 - a *stack* is a chip (or a whole cluster) asking for more than one item of
   the same kind — the blacksmith's reforge fodder. Its extra copies never
   carry their own constraints. A stack of a concrete item encodes as plain
   repeated requirements; a wildcard or cluster stack encodes as bare copies
   tied to the anchor with an ``ItemRequirement/identityGroup``; a stack with
   a *combined level* encodes as identical members sharing a ``LevelSum``
   (each matched item counts upgrade+1 towards the total, and members are
   optional, so "up to N items reaching T levels").

 Requirements are addressed by their position in the list, as the board's
 chips are; the list's own `key` identifies a row across an edit that moves
 it, which is how the anchor is followed through a rewrite.
 */
public struct BoardItem: Hashable, Identifiable, Sendable {
    /// Stable within one board rendering: the anchor's position, or the
    /// cluster's group.
    public let key: String
    /// Visible requirement indices: one for a chip, all members for a cluster.
    public let members: [Int]
    /// The cluster's alternative group, when this is a cluster.
    public let cluster: Int?
    /// Hidden copy indices behind the stack badge, in requirement order.
    public let extras: [Int]
    /// The stack's combined level, when one is set.
    public let total: Int?

    public var id: String { key }
    /// How many items this asks for: its anchor plus the hidden copies.
    public var stackCount: Int { 1 + extras.count }
    /// The requirement the badges and the editor act on.
    public var anchor: Int { members[0] }

    public init(key: String, members: [Int], cluster: Int? = nil,
                extras: [Int] = [], total: Int? = nil) {
        self.key = key; self.members = members; self.cluster = cluster
        self.extras = extras; self.total = total
    }
}

/// Whether `copy` is the plain repeat of the named `item` its chip carries.
/// A floor limit is a placement bound, not an item property, so a repeat that
/// carries only one still folds into its stack.
private func isPlainItemCopy(_ copy: ItemRequirement, of item: CatalogItem) -> Bool {
    item.kind != .trinket && item.kind != .artifact
        && copy.item?.id == item.id
        && copy.tierMatch == .any
        && copy.upgradeMatch == .any
        && copy.effect == .any
        && !copy.requireUncursed
        && copy.source == nil
        && copy.identityGroup == nil
        && copy.alternativeGroup == nil
        && copy.levelSum == nil
}

/// The bare copy a stack of `anchor`'s kind grows by; it may carry its own
/// floor limit, the one bound that is a placement, not an item property.
///
/// The copy names the broad family rather than the anchor's narrowed weapon
/// kind: the identity label already forces it to be the very item the anchor
/// matched, and only a family-wide copy reads as ``ItemRequirement/isBare``,
/// which is what lets the board fold it back into the anchor's badge.
private func bareCopy(_ anchor: ItemRequirement, identityGroup: Int,
                      key: Int64, maximumDepth: Int?) -> ItemRequirement {
    var copy = anchor
    copy.key = key
    copy.item = nil
    copy.kind = anchor.kind.family
    copy.tier = 0; copy.tierMatch = .any
    copy.upgrade = 0; copy.upgradeMatch = .any
    copy.effect = .any
    copy.requireUncursed = false
    copy.source = nil
    copy.identityGroup = identityGroup
    copy.alternativeGroup = nil
    copy.levelSum = nil
    copy.maximumDepth = maximumDepth
    return copy
}

/// The plain repeat a concrete stack of `anchor`'s item grows by.
private func plainCopy(_ anchor: ItemRequirement, key: Int64, maximumDepth: Int?) -> ItemRequirement {
    var copy = anchor
    copy.key = key
    copy.tier = 0; copy.tierMatch = .any
    copy.upgrade = 0; copy.upgradeMatch = .any
    copy.effect = .any
    copy.requireUncursed = false
    copy.source = nil
    copy.identityGroup = nil
    copy.alternativeGroup = nil
    copy.levelSum = nil
    copy.maximumDepth = maximumDepth
    return copy
}

/// One combined-level group while the board collapses: the member that
/// anchors the chip, the members folding away behind it, and their total.
private struct SumGroup {
    let anchor: Int
    var extras: [Int]
    let total: Int
}

/// A board entry under construction, held by reference so a cluster picks up
/// the members and copies that arrive after it was first seen.
private final class Building {
    let key: String
    var members: [Int]
    let cluster: Int?
    var extras: [Int] = []
    var total: Int?
    init(key: String, member: Int, cluster: Int?) {
        self.key = key; members = [member]; self.cluster = cluster
    }
}

extension Array where Element == ItemRequirement {
    // MARK: - Reading the board

    /// The board's collapsed view of the flat requirement list: clusters group
    /// alternatives, and a stack's copies fold into their anchor's badge.
    public func boardItems() -> [BoardItem] {
        var hidden = Set<Int>()

        // Combined-level groups: the first member anchors, the rest fold away.
        var sumAnchors: [Int: SumGroup] = [:]
        for (index, requirement) in enumerated() {
            guard let sum = requirement.levelSum else { continue }
            if sumAnchors[sum.group] != nil { sumAnchors[sum.group]?.extras.append(index) }
            else { sumAnchors[sum.group] = SumGroup(anchor: index, extras: [], total: sum.atLeast) }
        }
        for group in sumAnchors.values { hidden.formUnion(group.extras) }

        // Identity stacks: bare copies fold into the constrained unit (or the
        // first member when every member is bare). Groups with two constrained
        // units cannot collapse; validation reports them.
        var identityOrder: [Int] = []
        var identityMembers: [Int: [Int]] = [:]
        for (index, requirement) in enumerated() {
            guard let group = requirement.identityGroup else { continue }
            if identityMembers[group] == nil { identityMembers[group] = []; identityOrder.append(group) }
            identityMembers[group]?.append(index)
        }
        /// Copy indices to fold into the item holding the anchor index.
        var identityExtras: [Int: [Int]] = [:]
        for group in identityOrder {
            let members = identityMembers[group] ?? []
            let constrained = members.filter { !self[$0].isBare }
            let units = Set(constrained.map { index -> String in
                if let alternative = self[index].alternativeGroup { return "alt:\(alternative)" }
                return "req:\(index)"
            })
            if units.count > 1 { continue }
            let anchor = constrained.first ?? members[0]
            // A cluster anchor labels every member; fold only the lone bare copies.
            let extras = members.filter {
                $0 != anchor && self[$0].alternativeGroup == nil && self[$0].isBare
            }
            if extras.isEmpty { continue }
            identityExtras[anchor] = extras
            hidden.formUnion(extras)
        }

        // Walk the list building chips and clusters, folding plain item repeats
        // into the nearest earlier chip naming the same item.
        var items: [Building] = []
        var clusters: [Int: Building] = [:]
        var chipByItem: [String: Building] = [:]
        func attach(_ item: Building, _ anchorIndex: Int) {
            if let sum = self[anchorIndex].levelSum, let group = sumAnchors[sum.group],
               group.anchor == anchorIndex {
                item.extras.append(contentsOf: group.extras)
                item.total = group.total
            }
            if let extras = identityExtras[anchorIndex] { item.extras.append(contentsOf: extras) }
        }
        for (index, requirement) in enumerated() {
            if hidden.contains(index) { continue }
            if let group = requirement.alternativeGroup {
                if let existing = clusters[group] {
                    existing.members.append(index)
                    attach(existing, index)
                    continue
                }
                let item = Building(key: "alt:\(group)", member: index, cluster: group)
                clusters[group] = item
                attach(item, index)
                items.append(item)
                continue
            }
            // A plain repeat of an earlier chip's item folds into that chip.
            if let named = requirement.item, isPlainItemCopy(requirement, of: named),
               let earlier = chipByItem[named.id], earlier.total == nil,
               1 + earlier.extras.count < SearchLimits.stackMax {
                earlier.extras.append(index)
                continue
            }
            let item = Building(key: "req:\(index)", member: index, cluster: nil)
            attach(item, index)
            if let named = requirement.item, requirement.levelSum == nil { chipByItem[named.id] = item }
            items.append(item)
        }
        // Single-member clusters render as chips.
        return items.map { building in
            let cluster = building.members.count > 1 ? building.cluster : nil
            return BoardItem(key: cluster == nil ? "req:\(building.members[0])" : building.key,
                             members: building.members, cluster: cluster,
                             extras: building.extras, total: building.total)
        }
    }

    /// The number of visible board entries, for the pane's header count.
    public var boardCount: Int { boardItems().count }

    /// The board item holding the requirement at `index`, if any.
    public func boardItem(holding index: Int) -> BoardItem? {
        boardItems().first { $0.members.contains(index) }
    }

    /// The floor limit the stack's extra copies share (the first copy's, when
    /// a hand-written document gave them different ones).
    public func copyDepth(of item: BoardItem) -> Int? {
        item.extras.first.flatMap { self[$0].maximumDepth }
    }

    // MARK: - Editing the board

    /**
     Rewrites the list into its canonical stack encoding and drops every group
     that no longer says anything:

     - a lone alternative, a lone identity label and a lone level-sum member
       dissolve;
     - a labelled cluster labels every one of its members;
     - a stack anchored on a lone concrete chip carries plain repeats, not
       identity labels.

     Every operation funnels through this, so a deleted anchor can never leave
     stale groups behind.
     */
    public func normalizeRelations() -> [ItemRequirement] {
        var next = self
        // A cluster that holds an identity label spreads it to all its members.
        var clusterLabel: [Int: Int] = [:]
        for requirement in next {
            if let cluster = requirement.alternativeGroup, let label = requirement.identityGroup {
                clusterLabel[cluster] = label
            }
        }
        for index in next.indices {
            guard let cluster = next[index].alternativeGroup, let label = clusterLabel[cluster],
                  next[index].identityGroup != label else { continue }
            next[index].identityGroup = label
        }
        // A stack anchored on a lone concrete chip encodes as plain repeats.
        var identityOrder: [Int] = []
        var identityMembers: [Int: [Int]] = [:]
        for (index, requirement) in next.enumerated() {
            guard let group = requirement.identityGroup else { continue }
            if identityMembers[group] == nil { identityMembers[group] = []; identityOrder.append(group) }
            identityMembers[group]?.append(index)
        }
        for group in identityOrder {
            let members = identityMembers[group] ?? []
            let constrained = members.filter { !next[$0].isBare }
            guard constrained.count == 1 else { continue }
            let anchorIndex = constrained[0]
            let anchor = next[anchorIndex]
            guard anchor.item != nil, anchor.alternativeGroup == nil else { continue }
            for index in members {
                if index == anchorIndex { next[index].identityGroup = nil }
                else { next[index] = plainCopy(anchor, key: next[index].key, maximumDepth: next[index].maximumDepth) }
            }
        }
        // Groups of one say nothing.
        let alternatives = next.tally { $0.alternativeGroup }
        let identities = next.tally { $0.identityGroup }
        let sums = next.tally { $0.levelSum?.group }
        for index in next.indices {
            if let group = next[index].alternativeGroup, (alternatives[group] ?? 0) < 2 {
                next[index].alternativeGroup = nil
            }
            if let group = next[index].identityGroup, (identities[group] ?? 0) < 2 {
                next[index].identityGroup = nil
            }
            if let sum = next[index].levelSum, (sums[sum.group] ?? 0) < 2 {
                next[index].levelSum = nil
            }
        }
        return next
    }

    /**
     The chip at `source` becomes an either/or alternative of the chip at
     `target`. A combined level cannot travel into a cluster and is dropped; a
     plain-repeat stack keeps its copies by trading them for identity labels,
     which the cluster's members then share — but only while the cluster stays
     within one category. Across categories the stacks let go instead.
     */
    public func joinAlternatives(source: Int, target: Int) -> [ItemRequirement] {
        guard source != target, indices.contains(source), indices.contains(target) else { return self }
        let group = self[target].alternativeGroup ?? nextAlternativeGroup()
        if self[source].alternativeGroup == group { return self }
        let sourceKey = self[source].key
        let targetKey = self[target].key
        // A copy has to name the kind it copies, and a cluster spanning
        // categories names none — "weapon or wand" is not a kind anything can
        // be a copy of. So a stack follows its chip into a cluster only while
        // the cluster stays within one category.
        let clusterMembers = indices.filter {
            $0 == source || $0 == target || self[$0].alternativeGroup == group
        }
        let oneCategory = Set(clusterMembers.map { self[$0].kind.family }).count == 1
        var next: [ItemRequirement]
        if oneCategory {
            next = self
            // Trade plain repeats for identity copies so the stack survives the move.
            for index in [source, target] {
                let anchor = next[index]
                guard let named = anchor.item, anchor.identityGroup == nil else { continue }
                let copies = next.indices.filter { $0 != index && isPlainItemCopy(next[$0], of: named) }
                if copies.isEmpty { continue }
                guard let label = freeGroup(next.map(\.identityGroup), upTo: SearchLimits.identityGroupMax)
                else { continue }
                next[index].identityGroup = label
                for copy in copies {
                    next[copy] = bareCopy(anchor, identityGroup: label, key: next[copy].key,
                                          maximumDepth: next[copy].maximumDepth)
                }
            }
        } else {
            // The stacks let go: labelled copies are dropped and plain repeats
            // stay the standalone chips they already encode as. The chip's
            // badge falls back to ×1, which is the visible half of this.
            let labels = Set(clusterMembers.compactMap { self[$0].identityGroup })
            let clusterKeys = Set(clusterMembers.map { self[$0].key })
            next = filter { requirement in
                guard let label = requirement.identityGroup, labels.contains(label) else { return true }
                return clusterKeys.contains(requirement.key)
            }
            for index in next.indices {
                guard let label = next[index].identityGroup, labels.contains(label) else { continue }
                next[index].identityGroup = nil
            }
        }
        // Dropping the copies renumbers the list, so both ends of the join are
        // found again by key.
        let movedSource = next.firstIndex { $0.key == sourceKey } ?? source
        let movedTarget = next.firstIndex { $0.key == targetKey } ?? target
        for index in [movedSource, movedTarget] {
            next[index].alternativeGroup = group
            next[index].levelSum = nil
        }
        return next.moveAfter(from: movedSource) { $0.alternativeGroup == group }.normalizeRelations()
    }

    /// Whether the board item can carry a stack. A copy has to name the kind
    /// it copies, and a cluster spanning two categories — "spear or wand" —
    /// names none, so such a cluster is offered no stack and cannot grow one.
    public func canStack(_ item: BoardItem) -> Bool {
        guard let anchor = item.members.first, indices.contains(anchor) else { return false }
        let family = self[anchor].kind.family
        guard family != .trinket && family != .artifact else { return false }
        return item.members.allSatisfy { indices.contains($0) && self[$0].kind.family == family }
    }

    /// Pulls the chip at `index` out of its cluster; it leaves its stack behind.
    public func detach(_ index: Int) -> [ItemRequirement] {
        guard indices.contains(index) else { return self }
        var next = self
        next[index].alternativeGroup = nil
        next[index].identityGroup = nil
        return next.normalizeRelations()
    }

    /// Deletes a whole board item: its members and its hidden copies.
    public func removeItem(_ item: BoardItem) -> [ItemRequirement] {
        let doomed = Set(item.members + item.extras)
        return dropping(doomed).normalizeRelations()
    }

    /// Deletes one cluster member; the cluster and its stack live on without it.
    public func removeMember(_ index: Int) -> [ItemRequirement] {
        dropping([index]).normalizeRelations()
    }

    /// Sets how many items the board item anchored at `item` asks for.
    public func setStackCount(_ item: BoardItem, _ count: Int) -> [ItemRequirement] {
        let wanted = Swift.min(Swift.max(count, 1), SearchLimits.stackMax) - 1
        if wanted == item.extras.count { return self }
        if wanted < item.extras.count {
            return dropping(Set(item.extras.dropFirst(wanted))).normalizeRelations()
        }
        // Shrinking a cluster that spans categories is fine; growing one is not.
        if !canStack(item) { return self }
        let anchor = self[item.anchor]
        let added = wanted - item.extras.count
        // New copies keep to the floor limit the existing copies already carry.
        let inherited = copyDepth(of: item)
        var next = self
        let makeCopy: (Int64) -> ItemRequirement
        if item.total != nil, anchor.levelSum != nil {
            makeCopy = { key in var copy = anchor; copy.key = key; return copy }
        } else if item.cluster == nil, anchor.item != nil {
            makeCopy = { key in plainCopy(anchor, key: key, maximumDepth: inherited) }
        } else {
            guard let label = anchor.identityGroup
                ?? freeGroup(next.map(\.identityGroup), upTo: SearchLimits.identityGroupMax)
            else { return self }
            for index in item.members { next[index].identityGroup = label }
            makeCopy = { key in
                bareCopy(anchor, identityGroup: label, key: key, maximumDepth: inherited)
            }
        }
        let insertAt = (item.members + item.extras).max().map { $0 + 1 } ?? next.count
        next.insert(contentsOf: next.freshKeys(added).map(makeCopy), at: insertAt)
        return next.normalizeRelations()
    }

    /**
     Sets or clears the floor limit of the stack's extra copies. The anchor
     keeps its own limit: "the +3 one before floor 4, the rest wherever" and
     "…the rest before floor 10" are both sayable. A combined-level stack has
     identical members and no lone copies to bound.
     */
    public func setCopyDepth(_ item: BoardItem, _ maximumDepth: Int?) -> [ItemRequirement] {
        if item.total != nil { return self }
        var next = self
        for index in item.extras { next[index].maximumDepth = maximumDepth }
        return next.normalizeRelations()
    }

    /**
     Sets or clears the stack's combined level. Only a lone concrete ring chip
     can count levels; with a total the whole stack becomes identical optional
     members ("up to N items reaching T levels"), without one it returns to an
     anchor with plain repeats ("exactly N of the item").
     */
    public func setStackTotal(_ item: BoardItem, _ total: Int?) -> [ItemRequirement] {
        let anchorIndex = item.anchor
        let anchor = self[anchorIndex]
        guard item.cluster == nil, anchor.item != nil else { return self }
        var next = self
        guard let total else {
            for index in [anchorIndex] + item.extras {
                if index == anchorIndex { next[index].levelSum = nil }
                else { next[index] = plainCopy(anchor, key: next[index].key, maximumDepth: nil) }
            }
            return next.normalizeRelations()
        }
        // Only rings count levels together; clearing a total above never
        // needs this check, so stale non-ring sums can still be dissolved.
        guard anchor.kind.family == .ring else { return self }
        guard let group = anchor.levelSum?.group
            ?? freeGroup(map { $0.levelSum?.group }, upTo: SearchLimits.levelSumGroupMax)
        else { return self }
        for index in [anchorIndex] + item.extras {
            var member = anchor
            member.key = next[index].key
            // The total speaks for the stack, so per-member upgrades reset.
            member.upgrade = 0
            member.upgradeMatch = .any
            member.identityGroup = nil
            member.levelSum = LevelSum(group: group, atLeast: total)
            next[index] = member
        }
        return next.normalizeRelations()
    }

    /**
     Applies the editor's result: the anchor's own fields plus the stack's
     shape. `index` is the edited anchor, or nil for a new chip. Editing a
     cluster member leaves the stack's count and total to the cluster.
     */
    public func applyEdit(index: Int?, requirement: ItemRequirement, count: Int,
                          total: Int?, copyDepth: Int? = nil) -> [ItemRequirement] {
        var next: [ItemRequirement]
        let anchorKey: Int64
        if let index, indices.contains(index) {
            let current = self[index]
            anchorKey = current.key
            // The copies belonged to the chip as it was, and the edit may have
            // changed the very kind they copy — so the stack comes down here
            // and is rebuilt below from the count and total the editor
            // returned. A cluster member leaves its stack to the cluster and
            // keeps its copies.
            let doomed: Set<Int> = current.alternativeGroup != nil
                ? [] : Set(boardItem(holding: index)?.extras ?? [])
            next = dropping(doomed).map { existing in
                guard existing.key == anchorKey else { return existing }
                var edited = requirement
                edited.key = anchorKey
                edited.alternativeGroup = current.alternativeGroup
                return edited
            }
        } else {
            var added = requirement
            if contains(where: { $0.key == added.key }) { added.key = freshKeys(1)[0] }
            anchorKey = added.key
            next = self + [added]
        }
        next = next.normalizeRelations()
        // Every rewrite below reorders and renumbers, so the anchor is found
        // again by key each time rather than tracked as an index.
        func anchored(_ list: [ItemRequirement]) -> BoardItem? {
            guard let index = list.firstIndex(where: { $0.key == anchorKey }),
                  list[index].alternativeGroup == nil else { return nil }
            return list.boardItem(holding: index)
        }
        guard var item = anchored(next) else { return next }
        if item.total != nil, total == nil {
            next = next.setStackTotal(item, nil)
            item = anchored(next) ?? item
        }
        next = next.setStackCount(item, count)
        guard let refreshed = anchored(next) else { return next }
        return total != nil
            ? next.setStackTotal(refreshed, total)
            : next.setCopyDepth(refreshed, copyDepth)
    }

    // MARK: - Small helpers

    /// The list without the requirements at `doomed`.
    private func dropping(_ doomed: Set<Int>) -> [ItemRequirement] {
        doomed.isEmpty ? self : enumerated().filter { !doomed.contains($0.offset) }.map(\.element)
    }

    /// How many requirements each non-nil value of `key` covers.
    private func tally(_ key: (ItemRequirement) -> Int?) -> [Int: Int] {
        var counts: [Int: Int] = [:]
        for requirement in self {
            if let group = key(requirement) { counts[group, default: 0] += 1 }
        }
        return counts
    }

    /// Moves the requirement at `from` after the last requirement matching `after`.
    private func moveAfter(from: Int, after: (ItemRequirement) -> Bool) -> [ItemRequirement] {
        var rest = self
        let moving = rest.remove(at: from)
        let last = rest.lastIndex(where: after).map { $0 + 1 } ?? 0
        rest.insert(moving, at: last)
        return rest
    }

    private func nextAlternativeGroup() -> Int {
        (map { $0.alternativeGroup ?? 0 }.max() ?? 0) + 1
    }

    /// `count` keys no requirement in the list holds. Keys only have to be
    /// distinct within one query, so counting up from one is enough — and
    /// keeps a rewritten list reproducible, which random keys would not.
    private func freshKeys(_ count: Int) -> [Int64] {
        var taken = Set(map(\.key))
        var keys: [Int64] = []
        var candidate: Int64 = 1
        while keys.count < count {
            if !taken.contains(candidate) { keys.append(candidate); taken.insert(candidate) }
            candidate += 1
        }
        return keys
    }
}

/// The lowest group number in `1...max` that nothing in `used` holds.
private func freeGroup(_ used: [Int?], upTo max: Int) -> Int? {
    let taken = Set(used.compactMap { $0 })
    return (1...max).first { !taken.contains($0) }
}
