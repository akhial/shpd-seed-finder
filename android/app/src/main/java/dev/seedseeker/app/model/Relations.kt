// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

/**
 * Pure edits behind the requirement board, ported from the web design's
 * `relations.ts` so both platforms write the same documents. Every edit
 * returns a new requirement list in the canonical encoding, so share links,
 * presets and results files round-trip; the board renders the *collapsed*
 * view that [boardItems] derives from the flat list.
 *
 * Two ideas cover all three relationship kinds of the model:
 *
 * - an *either/or cluster* is several requirements sharing an
 *   [ItemRequirement.alternativeGroup]: one slot, any member fills it;
 * - a *stack* is a chip (or a whole cluster) asking for more than one item of
 *   the same kind — the blacksmith's reforge fodder. Its extra copies never
 *   carry their own constraints. A stack of a concrete item encodes as plain
 *   repeated requirements; a wildcard or cluster stack encodes as bare copies
 *   tied to the anchor with an [ItemRequirement.identityGroup]; a stack with a
 *   *combined level* encodes as identical members sharing a [LevelSum] (each
 *   matched item counts upgrade+1 towards the total, and members are optional,
 *   so "up to N items reaching T levels").
 */

/** One board entry: a chip, or an either/or cluster of chips. */
data class BoardItem(
    val key: String,
    /** Visible requirement indices: one for a chip, all members for a cluster. */
    val members: List<Int>,
    /** The cluster's alternative group, when this is a cluster. */
    val cluster: Int? = null,
    /** Hidden copy indices behind the stack badge, in requirement order. */
    val extras: List<Int> = emptyList(),
    /** The stack's combined level, when one is set. */
    val total: Int? = null,
) {
    /** How many items this asks for: its anchor plus the hidden copies. */
    val stackCount: Int get() = 1 + extras.size

    /** The requirement the badges and the editor act on. */
    val anchor: Int get() = members.first()
}

/** Whether [copy] is the plain repeat of the named [item]. */
private fun isPlainItemCopy(copy: ItemRequirement, item: CatalogItem): Boolean =
    copy.item?.id == item.id &&
        copy.tierMatch == TierMatch.ANY &&
        copy.upgradeMatch == UpgradeMatch.ANY &&
        copy.effect == EffectFilter.Any &&
        !copy.requireUncursed &&
        copy.source == null &&
        copy.maximumDepth == null &&
        copy.identityGroup == null &&
        copy.alternativeGroup == null &&
        copy.levelSum == null

/**
 * The board's collapsed view of the flat requirement list: clusters group
 * alternatives, and a stack's copies fold into their anchor's badge.
 */
fun List<ItemRequirement>.boardItems(): List<BoardItem> {
    val hidden = mutableSetOf<Int>()

    // Combined-level groups: the first member anchors, the rest fold away.
    class SumGroup(val anchor: Int, val extras: MutableList<Int>, val total: Int)
    val sumAnchors = linkedMapOf<Int, SumGroup>()
    forEachIndexed { index, requirement ->
        val sum = requirement.levelSum ?: return@forEachIndexed
        val existing = sumAnchors[sum.group]
        if (existing == null) {
            sumAnchors[sum.group] = SumGroup(index, mutableListOf(), sum.atLeast)
        } else {
            existing.extras += index
        }
    }
    for (group in sumAnchors.values) hidden += group.extras

    // Identity stacks: bare copies fold into the constrained unit (or the first
    // member when every member is bare). Groups with two constrained units
    // cannot collapse; validation reports them.
    val identityMembers = linkedMapOf<Int, MutableList<Int>>()
    forEachIndexed { index, requirement ->
        requirement.identityGroup?.let { identityMembers.getOrPut(it) { mutableListOf() } += index }
    }
    /** Copy indices to fold into the item holding the anchor index. */
    val identityExtras = mutableMapOf<Int, List<Int>>()
    for (members in identityMembers.values) {
        val constrained = members.filterNot { this[it].isBare }
        val units = constrained.map { index ->
            this[index].alternativeGroup?.let { "alt:$it" } ?: "req:$index"
        }.distinct()
        if (units.size > 1) continue
        val anchor = constrained.firstOrNull() ?: members.first()
        // A cluster anchor labels every member; fold only the lone bare copies.
        val extras = members.filter { it != anchor && this[it].alternativeGroup == null && this[it].isBare }
        if (extras.isEmpty()) continue
        identityExtras[anchor] = extras
        hidden += extras
    }

    // Walk the list building chips and clusters, folding plain item repeats
    // into the nearest earlier chip naming the same item.
    class Building(
        val key: String,
        val members: MutableList<Int>,
        val cluster: Int?,
        val extras: MutableList<Int> = mutableListOf(),
        var total: Int? = null,
    )
    val items = mutableListOf<Building>()
    val clusters = mutableMapOf<Int, Building>()
    val chipByItem = mutableMapOf<String, Building>()
    fun attach(item: Building, anchorIndex: Int) {
        this[anchorIndex].levelSum?.let { sum ->
            val group = sumAnchors[sum.group]
            if (group != null && group.anchor == anchorIndex) {
                item.extras += group.extras
                item.total = group.total
            }
        }
        identityExtras[anchorIndex]?.let { item.extras += it }
    }
    forEachIndexed { index, requirement ->
        if (index in hidden) return@forEachIndexed
        val group = requirement.alternativeGroup
        if (group != null) {
            val existing = clusters[group]
            if (existing != null) {
                existing.members += index
                attach(existing, index)
                return@forEachIndexed
            }
            val item = Building("alt:$group", mutableListOf(index), group)
            clusters[group] = item
            attach(item, index)
            items += item
            return@forEachIndexed
        }
        // A plain repeat of an earlier chip's item folds into that chip.
        val named = requirement.item
        if (named != null && isPlainItemCopy(requirement, named)) {
            val earlier = chipByItem[named.id]
            if (earlier != null && earlier.total == null && 1 + earlier.extras.size < SearchLimits.STACK_MAX) {
                earlier.extras += index
                return@forEachIndexed
            }
        }
        val item = Building("req:$index", mutableListOf(index), null)
        attach(item, index)
        if (named != null && requirement.levelSum == null) chipByItem[named.id] = item
        items += item
    }
    // Single-member clusters render as chips.
    return items.map { item ->
        val cluster = item.cluster?.takeIf { item.members.size > 1 }
        BoardItem(
            key = if (cluster == null) "req:${item.members.first()}" else item.key,
            members = item.members.toList(),
            cluster = cluster,
            extras = item.extras.toList(),
            total = item.total,
        )
    }
}

/** The number of visible board entries, for the pane's header count. */
fun List<ItemRequirement>.boardCount(): Int = boardItems().size

private fun freeGroup(used: Collection<Int?>, max: Int): Int? =
    (1..max).firstOrNull { it !in used }

private fun List<ItemRequirement>.nextAlternativeGroup(): Int =
    (maxOfOrNull { it.alternativeGroup ?: 0 } ?: 0) + 1

private fun List<ItemRequirement>.nextKey(): Long = (maxOfOrNull { it.key } ?: 0L) + 1L

/** The bare copy a stack of [anchor]'s kind grows by. */
private fun bareCopy(anchor: ItemRequirement, identityGroup: Int, key: Long) = ItemRequirement(
    key = key,
    item = null,
    upgrade = 0,
    kind = anchor.kind,
    upgradeMatch = UpgradeMatch.ANY,
    identityGroup = identityGroup,
)

/** The plain repeat a concrete stack of [anchor]'s item grows by. */
private fun plainCopy(anchor: ItemRequirement, key: Long) = ItemRequirement(
    key = key,
    item = anchor.item,
    upgrade = 0,
    kind = anchor.kind,
    upgradeMatch = UpgradeMatch.ANY,
)

/**
 * Rewrites the list into its canonical stack encoding and drops every group
 * that no longer says anything:
 *
 * - a lone alternative, a lone identity label and a lone level-sum member
 *   dissolve;
 * - a labelled cluster labels every one of its members;
 * - a stack anchored on a lone concrete chip carries plain repeats, not
 *   identity labels.
 *
 * Every operation funnels through this, so a deleted anchor can never leave
 * stale groups behind.
 */
fun List<ItemRequirement>.normalizeRelations(): List<ItemRequirement> {
    var next = toMutableList()
    // A cluster that holds an identity label spreads it to all its members.
    val clusterLabel = mutableMapOf<Int, Int>()
    for (requirement in next) {
        val cluster = requirement.alternativeGroup
        val label = requirement.identityGroup
        if (cluster != null && label != null) clusterLabel[cluster] = label
    }
    next = next.map { requirement ->
        val label = requirement.alternativeGroup?.let { clusterLabel[it] }
        if (label != null && requirement.identityGroup != label) {
            requirement.copy(identityGroup = label)
        } else {
            requirement
        }
    }.toMutableList()
    // A stack anchored on a lone concrete chip encodes as plain repeats.
    val identityMembers = linkedMapOf<Int, MutableList<Int>>()
    next.forEachIndexed { index, requirement ->
        requirement.identityGroup?.let { identityMembers.getOrPut(it) { mutableListOf() } += index }
    }
    for (members in identityMembers.values) {
        val constrained = members.filterNot { next[it].isBare }
        if (constrained.size != 1) continue
        val anchorIndex = constrained.single()
        val anchor = next[anchorIndex]
        if (anchor.item == null || anchor.alternativeGroup != null) continue
        for (index in members) {
            next[index] = if (index == anchorIndex) {
                anchor.copy(identityGroup = null)
            } else {
                plainCopy(anchor, next[index].key)
            }
        }
    }
    // Groups of one say nothing.
    val alternatives = next.mapNotNull { it.alternativeGroup }.groupingBy { it }.eachCount()
    val identities = next.mapNotNull { it.identityGroup }.groupingBy { it }.eachCount()
    val sums = next.mapNotNull { it.levelSum?.group }.groupingBy { it }.eachCount()
    return next.map { requirement ->
        var result = requirement
        if (result.alternativeGroup != null && (alternatives[result.alternativeGroup] ?: 0) < 2) {
            result = result.copy(alternativeGroup = null)
        }
        if (result.identityGroup != null && (identities[result.identityGroup] ?: 0) < 2) {
            result = result.copy(identityGroup = null)
        }
        val sum = result.levelSum
        if (sum != null && (sums[sum.group] ?: 0) < 2) {
            result = result.copy(levelSum = null)
        }
        result
    }
}

/** Moves the requirement at [from] after the last requirement matching [after]. */
private fun List<ItemRequirement>.moveAfter(
    from: Int,
    after: (ItemRequirement) -> Boolean,
): List<ItemRequirement> {
    val moving = this[from]
    val rest = filterIndexed { index, _ -> index != from }
    val last = rest.indexOfLast(after)
    return rest.subList(0, last + 1) + moving + rest.subList(last + 1, rest.size)
}

/**
 * The chip at [source] becomes an either/or alternative of the chip at
 * [target]. A combined level cannot travel into a cluster and is dropped; a
 * plain-repeat stack keeps its copies by trading them for identity labels,
 * which the cluster's members then share.
 */
fun List<ItemRequirement>.joinAlternatives(source: Int, target: Int): List<ItemRequirement> {
    if (source == target) return this
    val group = this[target].alternativeGroup ?: nextAlternativeGroup()
    if (this[source].alternativeGroup == group) return this
    val sourceKey = this[source].key
    val targetKey = this[target].key
    // A copy has to name the kind it copies, and a cluster spanning categories
    // names none — "weapon or wand" is not a kind anything can be a copy of. So
    // a stack follows its chip into a cluster only while the cluster stays
    // within one category.
    val clusterMembers = indices.filter {
        it == source || it == target || this[it].alternativeGroup == group
    }
    val oneCategory = clusterMembers.map { this[it].kind.family }.distinct().size == 1
    val next: MutableList<ItemRequirement>
    if (oneCategory) {
        next = toMutableList()
        // Trade plain repeats for identity copies so the stack survives the move.
        for (index in listOf(source, target)) {
            val anchor = next[index]
            val named = anchor.item ?: continue
            if (anchor.identityGroup != null) continue
            val copies = next.indices.filter { it != index && isPlainItemCopy(next[it], named) }
            if (copies.isEmpty()) continue
            val label = freeGroup(next.map { it.identityGroup }, SearchLimits.IDENTITY_GROUP_MAX) ?: continue
            next[index] = anchor.copy(identityGroup = label)
            for (copy in copies) next[copy] = bareCopy(anchor, label, next[copy].key)
        }
    } else {
        // The stacks let go: labelled copies are dropped and plain repeats stay
        // the standalone chips they already encode as. The chip's badge falls
        // back to ×1, which is the visible half of this.
        val labels = clusterMembers.mapNotNull { this[it].identityGroup }.toSet()
        val clusterKeys = clusterMembers.map { this[it].key }.toSet()
        next = filterNot { it.identityGroup in labels && it.key !in clusterKeys }
            .map { if (it.identityGroup in labels) it.copy(identityGroup = null) else it }
            .toMutableList()
    }
    val movedSource = next.indexOfFirst { it.key == sourceKey }
    val movedTarget = next.indexOfFirst { it.key == targetKey }
    val joined = next.mapIndexed { index, requirement ->
        if (index == movedSource || index == movedTarget) {
            requirement.copy(alternativeGroup = group, levelSum = null)
        } else {
            requirement
        }
    }
    return joined.moveAfter(movedSource) { it.alternativeGroup == group }.normalizeRelations()
}

/** Pulls the chip at [index] out of its cluster; it leaves its stack behind. */
fun List<ItemRequirement>.detach(index: Int): List<ItemRequirement> =
    mapIndexed { i, requirement ->
        if (i == index) requirement.copy(alternativeGroup = null, identityGroup = null) else requirement
    }.normalizeRelations()

/** Deletes a whole board item: its members and its hidden copies. */
fun List<ItemRequirement>.removeItem(item: BoardItem): List<ItemRequirement> {
    val doomed = (item.members + item.extras).toSet()
    return filterIndexed { index, _ -> index !in doomed }.normalizeRelations()
}

/** Deletes one cluster member; the cluster and its stack live on without it. */
fun List<ItemRequirement>.removeMember(index: Int): List<ItemRequirement> =
    filterIndexed { i, _ -> i != index }.normalizeRelations()

/** Sets how many items the board item anchored at [item] asks for. */
fun List<ItemRequirement>.setStackCount(item: BoardItem, count: Int): List<ItemRequirement> {
    val wanted = count.coerceIn(1, SearchLimits.STACK_MAX) - 1
    if (wanted == item.extras.size) return this
    if (wanted < item.extras.size) {
        val doomed = item.extras.drop(wanted).toSet()
        return filterIndexed { index, _ -> index !in doomed }.normalizeRelations()
    }
    val anchor = this[item.anchor]
    val added = wanted - item.extras.size
    var next = toMutableList()
    val copy: (Long) -> ItemRequirement
    if (item.total != null && anchor.levelSum != null) {
        copy = { key -> anchor.copy(key = key) }
    } else if (item.cluster == null && anchor.item != null) {
        copy = { key -> plainCopy(anchor, key) }
    } else {
        val label = anchor.identityGroup
            ?: freeGroup(next.map { it.identityGroup }, SearchLimits.IDENTITY_GROUP_MAX)
            ?: return this
        next = next.mapIndexed { index, requirement ->
            if (index in item.members) requirement.copy(identityGroup = label) else requirement
        }.toMutableList()
        copy = { key -> bareCopy(anchor, label, key) }
    }
    val insertAt = (item.members + item.extras).maxOrNull()!! + 1
    var key = next.nextKey()
    val grown = List(added) { copy(key++) }
    return (next.subList(0, insertAt) + grown + next.subList(insertAt, next.size)).normalizeRelations()
}

/**
 * Sets or clears the stack's combined level. Only a lone concrete chip can
 * count levels; with a total the whole stack becomes identical optional
 * members ("up to N items reaching T levels"), without one it returns to an
 * anchor with plain repeats ("exactly N of the item").
 */
fun List<ItemRequirement>.setStackTotal(item: BoardItem, total: Int?): List<ItemRequirement> {
    val anchor = this[item.anchor]
    if (item.cluster != null || anchor.item == null) return this
    val indices = (listOf(item.anchor) + item.extras).toSet()
    if (total == null) {
        return mapIndexed { index, requirement ->
            when {
                index !in indices -> requirement
                index == item.anchor -> requirement.copy(levelSum = null)
                else -> plainCopy(anchor, requirement.key)
            }
        }.normalizeRelations()
    }
    val group = anchor.levelSum?.group
        ?: freeGroup(map { it.levelSum?.group }, SearchLimits.LEVEL_SUM_GROUP_MAX)
        ?: return this
    return mapIndexed { index, requirement ->
        if (index !in indices) {
            requirement
        } else {
            anchor.copy(
                key = requirement.key,
                upgrade = 0,
                upgradeMatch = UpgradeMatch.ANY,
                identityGroup = null,
                levelSum = LevelSum(group, total),
            )
        }
    }.normalizeRelations()
}

/**
 * Applies the editor's result: the anchor's own fields plus the stack's shape.
 * [index] is the edited anchor, or null for a new chip. Editing a cluster
 * member leaves the stack's count and total to the cluster.
 */
fun List<ItemRequirement>.applyEdit(
    index: Int?,
    requirement: ItemRequirement,
    count: Int,
    total: Int?,
): List<ItemRequirement> {
    val anchorKey: Long
    var next: List<ItemRequirement>
    if (index == null) {
        anchorKey = nextKey()
        next = this + requirement.copy(key = anchorKey)
    } else {
        val current = this[index]
        anchorKey = current.key
        // The copies belonged to the chip as it was, and the edit may have
        // changed the very kind they copy — so the stack comes down here and
        // is rebuilt below from the count and total the editor returned. A
        // cluster member leaves its stack to the cluster and keeps its copies.
        val doomed = if (current.alternativeGroup != null) {
            emptySet()
        } else {
            boardItems().firstOrNull { index in it.members }?.extras?.toSet() ?: emptySet()
        }
        next = filterIndexed { i, _ -> i !in doomed }.map {
            if (it.key == anchorKey) {
                requirement.copy(key = anchorKey, alternativeGroup = current.alternativeGroup)
            } else {
                it
            }
        }
    }
    next = next.normalizeRelations()
    val anchorIndex = next.indexOfFirst { it.key == anchorKey }
    if (anchorIndex < 0 || next[anchorIndex].alternativeGroup != null) return next
    var item = next.boardItems().firstOrNull { anchorIndex in it.members } ?: return next
    if (item.total != null && total == null) {
        next = next.setStackTotal(item, null)
        item = next.boardItems().firstOrNull { anchorIndex in it.members } ?: item
    }
    next = next.setStackCount(item, count)
    if (total != null) {
        next.boardItems().firstOrNull { anchorIndex in it.members }?.let {
            next = next.setStackTotal(it, total)
        }
    }
    return next
}
