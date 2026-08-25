import {
  IDENTITY_GROUP_MAX,
  LEVEL_SUM_GROUP_MAX,
  STACK_MAX,
  isBareRequirement,
  requirementFamily,
} from '../../lib/query'
import type { RequirementState } from '../../lib/wasm/types'

/**
 * Pure edits behind the requirement board. Every edit returns a new
 * requirement list in the canonical document encoding, so share links and
 * results files round-trip; the board itself renders the *collapsed* view
 * that `boardItems` derives from the flat list.
 *
 * Two ideas cover all three relationship kinds of the model:
 *
 * - an *either/or cluster* is several requirements sharing an
 *   `alternativeGroup`: one slot, any member fills it;
 * - a *stack* is a chip (or a whole cluster) asking for more than one item
 *   of the same kind — the blacksmith's reforge fodder. Its extra copies
 *   never carry their own constraints. A stack of a concrete item encodes
 *   as plain repeated requirements; a wildcard or cluster stack encodes as
 *   bare copies tied to the anchor with an `identityGroup`; a stack with a
 *   *combined level* encodes as identical members sharing a `levelSum`
 *   (each matched item counts upgrade+1 towards the total, and members are
 *   optional, so "up to N items reaching T levels").
 */

/** One board entry: a chip, or an either/or cluster of chips. */
export interface BoardItem {
  key: string
  /** Visible requirement indices: one for a chip, all members for a cluster. */
  members: number[]
  /** The cluster's alternative group, when this is a cluster. */
  cluster?: number
  /** Hidden copy indices behind the stack badge, in requirement order. */
  extras: number[]
  /** The stack's combined level, when one is set. */
  total?: number
}

/** How many items a board item asks for: its anchor plus hidden copies. */
export const stackCount = (item: BoardItem): number => 1 + item.extras.length

/** Whether `copy` is the plain repeat of the named item `item` carries.
 * A floor limit is a placement bound, not an item property, so a repeat
 * that carries only one still folds into its stack. */
const isPlainItemCopy = (copy: RequirementState, item: string): boolean =>
  copy.item === item
  && copy.tier.mode === 'any'
  && copy.upgrade.mode === 'any'
  && copy.effect === undefined
  && !copy.uncursed
  && copy.source === undefined
  && copy.identityGroup === undefined
  && copy.alternativeGroup === undefined
  && copy.levelSum === undefined

/**
 * The board's collapsed view of the flat requirement list: clusters group
 * alternatives, and a stack's copies fold into their anchor's badge.
 */
export function boardItems(requirements: readonly RequirementState[]): BoardItem[] {
  const hidden = new Set<number>()

  // Combined-level groups: the first member anchors, the rest fold away.
  const sumAnchors = new Map<number, { anchor: number; extras: number[]; total: number }>()
  requirements.forEach((requirement, index) => {
    const sum = requirement.levelSum
    if (!sum) return
    const existing = sumAnchors.get(sum.group)
    if (existing) existing.extras.push(index)
    else sumAnchors.set(sum.group, { anchor: index, extras: [], total: sum.atLeast })
  })
  for (const group of sumAnchors.values()) for (const index of group.extras) hidden.add(index)

  // Identity stacks: bare copies fold into the constrained unit (or the
  // first member when every member is bare). Groups with two constrained
  // units cannot collapse; validation reports them.
  const identityMembers = new Map<number, number[]>()
  requirements.forEach((requirement, index) => {
    if (requirement.identityGroup !== undefined) {
      identityMembers.set(requirement.identityGroup, [...(identityMembers.get(requirement.identityGroup) ?? []), index])
    }
  })
  /** Copy indices to fold into the item holding the anchor index. */
  const identityExtras = new Map<number, number[]>()
  for (const members of identityMembers.values()) {
    const constrained = members.filter((index) => !isBareRequirement(requirements[index]))
    const units = new Set(constrained.map((index) => requirements[index].alternativeGroup === undefined
      ? `req:${index}` : `alt:${requirements[index].alternativeGroup}`))
    if (units.size > 1) continue
    const anchor = constrained[0] ?? members[0]
    // A cluster anchor labels every member; fold only the lone bare copies.
    const extras = members.filter((index) => index !== anchor
      && requirements[index].alternativeGroup === undefined
      && isBareRequirement(requirements[index]))
    if (extras.length === 0) continue
    identityExtras.set(anchor, extras)
    for (const index of extras) hidden.add(index)
  }

  // Walk the list building chips and clusters, folding plain item repeats
  // into the nearest earlier chip naming the same item.
  const items: BoardItem[] = []
  const clusters = new Map<number, BoardItem>()
  const chipByItem = new Map<string, BoardItem>()
  const attach = (item: BoardItem, anchorIndex: number) => {
    const sum = requirements[anchorIndex].levelSum
    if (sum) {
      const group = sumAnchors.get(sum.group)
      if (group && group.anchor === anchorIndex) {
        item.extras.push(...group.extras)
        item.total = group.total
      }
    }
    const extras = identityExtras.get(anchorIndex)
    if (extras) item.extras.push(...extras)
  }
  requirements.forEach((requirement, index) => {
    if (hidden.has(index)) return
    if (requirement.alternativeGroup !== undefined) {
      const existing = clusters.get(requirement.alternativeGroup)
      if (existing) {
        existing.members.push(index)
        attach(existing, index)
        return
      }
      const item: BoardItem = { key: `alt:${requirement.alternativeGroup}`, members: [index], cluster: requirement.alternativeGroup, extras: [] }
      clusters.set(requirement.alternativeGroup, item)
      attach(item, index)
      items.push(item)
      return
    }
    // A plain repeat of an earlier chip's item folds into that chip.
    if (requirement.item !== undefined && isPlainItemCopy(requirement, requirement.item)) {
      const earlier = chipByItem.get(requirement.item)
      if (earlier && earlier.total === undefined && stackCount(earlier) < STACK_MAX) {
        earlier.extras.push(index)
        return
      }
    }
    const item: BoardItem = { key: `req:${index}`, members: [index], extras: [] }
    attach(item, index)
    if (requirement.item !== undefined && requirement.levelSum === undefined) chipByItem.set(requirement.item, item)
    items.push(item)
  })
  // Single-member clusters render as chips.
  return items.map((item) => (item.cluster !== undefined && item.members.length === 1 ? { ...item, cluster: undefined } : item))
}

/** The number of visible board entries, for the pane's header count. */
export const boardCount = (requirements: readonly RequirementState[]): number => boardItems(requirements).length

const freeGroup = (used: Iterable<number | undefined>, max: number): number | undefined => {
  const taken = new Set(used)
  for (let group = 1; group <= max; group += 1) if (!taken.has(group)) return group
  return undefined
}

const nextAlternativeGroup = (requirements: readonly RequirementState[]): number =>
  requirements.reduce((highest, requirement) => Math.max(highest, requirement.alternativeGroup ?? 0), 0) + 1

const countBy = (requirements: readonly RequirementState[], key: (requirement: RequirementState) => number | undefined) => {
  const counts = new Map<number, number>()
  for (const requirement of requirements) {
    const group = key(requirement)
    if (group !== undefined) counts.set(group, (counts.get(group) ?? 0) + 1)
  }
  return counts
}

/** The bare copy a stack of `anchor`'s kind grows by; it may carry its own
 * floor limit, the one bound that is a placement, not an item property. */
const bareCopy = (anchor: RequirementState, identityGroup: number, maxDepth?: number): RequirementState => ({
  kind: anchor.kind ?? requirementFamily(anchor) as RequirementState['kind'],
  tier: { mode: 'any', value: 3 },
  upgrade: { mode: 'any', value: 1 },
  uncursed: false,
  identityGroup,
  maxDepth,
})

/** The plain repeat a concrete stack of `item` grows by. */
const plainCopy = (anchor: RequirementState, maxDepth?: number): RequirementState => ({
  kind: anchor.kind,
  item: anchor.item,
  tier: { mode: 'any', value: 3 },
  upgrade: { mode: 'any', value: 1 },
  uncursed: false,
  maxDepth,
})

/**
 * Rewrites the list into its canonical stack encoding and drops every
 * group that no longer says anything:
 *
 * - a lone alternative, a lone identity label, and a lone level-sum member
 *   dissolve;
 * - a labelled cluster labels every one of its members;
 * - a stack anchored on a lone concrete chip carries plain repeats, not
 *   identity labels.
 *
 * Every operation funnels through this, so a deleted anchor can never
 * leave stale groups behind.
 */
export function normalize(requirements: RequirementState[]): RequirementState[] {
  let next = [...requirements]
  // A cluster that holds an identity label spreads it to all its members.
  const clusterLabel = new Map<number, number>()
  for (const requirement of next) {
    if (requirement.alternativeGroup !== undefined && requirement.identityGroup !== undefined) {
      clusterLabel.set(requirement.alternativeGroup, requirement.identityGroup)
    }
  }
  next = next.map((requirement) => {
    const label = requirement.alternativeGroup === undefined ? undefined : clusterLabel.get(requirement.alternativeGroup)
    return label !== undefined && requirement.identityGroup !== label
      ? { ...requirement, identityGroup: label }
      : requirement
  })
  // A stack anchored on a lone concrete chip encodes as plain repeats.
  const identityMembers = new Map<number, number[]>()
  next.forEach((requirement, index) => {
    if (requirement.identityGroup !== undefined) {
      identityMembers.set(requirement.identityGroup, [...(identityMembers.get(requirement.identityGroup) ?? []), index])
    }
  })
  for (const members of identityMembers.values()) {
    const constrained = members.filter((index) => !isBareRequirement(next[index]))
    if (constrained.length !== 1) continue
    const anchor = next[constrained[0]]
    if (anchor.item === undefined || anchor.alternativeGroup !== undefined) continue
    for (const index of members) {
      next[index] = index === constrained[0]
        ? { ...anchor, identityGroup: undefined }
        : plainCopy(anchor, next[index].maxDepth)
    }
  }
  // Groups of one say nothing.
  const alternatives = countBy(next, (requirement) => requirement.alternativeGroup)
  const identities = countBy(next, (requirement) => requirement.identityGroup)
  const sums = countBy(next, (requirement) => requirement.levelSum?.group)
  return next.map((requirement) => {
    let result = requirement
    if (result.alternativeGroup !== undefined && (alternatives.get(result.alternativeGroup) ?? 0) < 2) {
      result = { ...result, alternativeGroup: undefined }
    }
    if (result.identityGroup !== undefined && (identities.get(result.identityGroup) ?? 0) < 2) {
      result = { ...result, identityGroup: undefined }
    }
    if (result.levelSum && (sums.get(result.levelSum.group) ?? 0) < 2) {
      result = { ...result, levelSum: undefined }
    }
    return result
  })
}

/** Moves the requirement at `from` after the last requirement matching `after`. */
function moveAfter(requirements: RequirementState[], from: number, after: (requirement: RequirementState) => boolean): RequirementState[] {
  const moving = requirements[from]
  const rest = requirements.filter((_, index) => index !== from)
  const last = rest.reduce((found, requirement, index) => (after(requirement) ? index : found), -1)
  return [...rest.slice(0, last + 1), moving, ...rest.slice(last + 1)]
}

/**
 * The chip at `source` becomes an either/or alternative of the chip at
 * `target`. A combined level cannot travel into a cluster and is dropped;
 * a plain-repeat stack keeps its copies by trading them for identity
 * labels, which the cluster's members then share.
 */
export function joinAlternatives(requirements: RequirementState[], source: number, target: number): RequirementState[] {
  if (source === target) return requirements
  const group = requirements[target].alternativeGroup ?? nextAlternativeGroup(requirements)
  if (requirements[source].alternativeGroup === group) return requirements
  let next = [...requirements]
  // A copy has to name the kind it copies, so only a cluster that stays within
  // one category can anchor a stack. When the join would mix categories the
  // repeats simply stay the standalone chips they encode as.
  const clusterMembers = requirements
    .map((requirement, index) => ({ requirement, index }))
    .filter(({ requirement, index }) => index === source || index === target || requirement.alternativeGroup === group)
  const oneCategory = new Set(clusterMembers.map(({ requirement }) => requirementFamily(requirement))).size === 1
  let movedSource = source
  let movedTarget = target
  if (oneCategory) {
    // Trade plain repeats for identity copies so the stack survives the move.
    for (const index of [source, target]) {
      const anchor = next[index]
      if (anchor.item === undefined || anchor.identityGroup !== undefined) continue
      const copies = next
        .map((requirement, i) => ({ requirement, i }))
        .filter(({ requirement, i }) => i !== index && anchor.item !== undefined && isPlainItemCopy(requirement, anchor.item))
        .map(({ i }) => i)
      if (copies.length === 0) continue
      const label = freeGroup(next.map((requirement) => requirement.identityGroup), IDENTITY_GROUP_MAX)
      if (label === undefined) continue
      next[index] = { ...anchor, identityGroup: label }
      for (const i of copies) next[i] = bareCopy(anchor, label, next[i].maxDepth)
    }
  } else {
    // The stacks let go: labelled copies are dropped and plain repeats stay
    // the standalone chips they already encode as. The chip's badge falls
    // back to ×1, which is the visible half of this.
    const labels = new Set(clusterMembers.map(({ requirement }) => requirement.identityGroup))
    const members = new Set(clusterMembers.map(({ index }) => index))
    const labelled = (requirement: RequirementState) => requirement.identityGroup !== undefined && labels.has(requirement.identityGroup)
    const kept = next
      .map((requirement, index) => ({ requirement, index }))
      .filter(({ requirement, index }) => members.has(index) || !labelled(requirement))
    movedSource = kept.findIndex(({ index }) => index === source)
    movedTarget = kept.findIndex(({ index }) => index === target)
    next = kept.map(({ requirement }) => (labelled(requirement) ? { ...requirement, identityGroup: undefined } : requirement))
  }
  next = next.map((requirement, index) => (
    index === movedSource || index === movedTarget
      ? { ...requirement, alternativeGroup: group, levelSum: undefined }
      : requirement
  ))
  return normalize(moveAfter(next, movedSource, (requirement) => requirement.alternativeGroup === group))
}

/**
 * Whether the board item can carry a stack. A copy has to name the kind it
 * copies, and a cluster spanning two categories — "spear or wand" — names
 * none, so such a cluster is offered no stack and cannot grow one.
 */
export const canStack = (requirements: readonly RequirementState[], item: BoardItem): boolean => {
  const family = requirementFamily(requirements[item.members[0]])
  return item.members.every((index) => requirementFamily(requirements[index]) === family)
}

/** Pulls the chip at `index` out of its cluster; it leaves its stack behind. */
export function detach(requirements: RequirementState[], index: number): RequirementState[] {
  const { alternativeGroup: _a, identityGroup: _i, ...rest } = requirements[index]
  return normalize(requirements.map((requirement, i) => (i === index ? rest : requirement)))
}

/** Deletes a whole board item: its members and its hidden copies. */
export function removeItem(requirements: RequirementState[], item: BoardItem): RequirementState[] {
  const doomed = new Set([...item.members, ...item.extras])
  return normalize(requirements.filter((_, index) => !doomed.has(index)))
}

/** Deletes one cluster member; the cluster and its stack live on without it. */
export function removeMember(requirements: RequirementState[], index: number): RequirementState[] {
  return normalize(requirements.filter((_, i) => i !== index))
}

/** Sets how many items the board item anchored at `item` asks for. */
export function setStackCount(requirements: RequirementState[], item: BoardItem, count: number): RequirementState[] {
  const wanted = Math.max(1, Math.min(STACK_MAX, count)) - 1
  if (wanted === item.extras.length) return requirements
  if (wanted < item.extras.length) {
    const doomed = new Set(item.extras.slice(wanted))
    return normalize(requirements.filter((_, index) => !doomed.has(index)))
  }
  if (!canStack(requirements, item)) return requirements
  const anchorIndex = item.members[0]
  const anchor = requirements[anchorIndex]
  const added = wanted - item.extras.length
  // New copies keep to the floor limit the existing copies already carry.
  const inherited = item.extras.length > 0 ? requirements[item.extras[0]].maxDepth : undefined
  let copy: RequirementState
  let next = [...requirements]
  if (item.total !== undefined && anchor.levelSum) {
    copy = { ...anchor }
  } else if (item.cluster === undefined && anchor.item !== undefined) {
    copy = plainCopy(anchor, inherited)
  } else {
    const label = anchor.identityGroup
      ?? freeGroup(next.map((requirement) => requirement.identityGroup), IDENTITY_GROUP_MAX)
    if (label === undefined) return requirements
    next = next.map((requirement, index) => (
      item.members.includes(index) ? { ...requirement, identityGroup: label } : requirement
    ))
    copy = bareCopy(anchor, label, inherited)
  }
  const insertAt = Math.max(...item.members, ...item.extras) + 1
  next = [...next.slice(0, insertAt), ...Array.from({ length: added }, () => ({ ...copy })), ...next.slice(insertAt)]
  return normalize(next)
}

/** The floor limit the stack's extra copies share (the first copy's, when
 * a hand-written document gave them different ones). */
export const copyDepthOf = (requirements: readonly RequirementState[], item: BoardItem): number | undefined =>
  item.extras.length > 0 ? requirements[item.extras[0]].maxDepth : undefined

/**
 * Sets or clears the floor limit of the stack's extra copies. The anchor
 * keeps its own limit: "the +3 one before floor 4, the rest wherever" and
 * "…the rest before floor 10" are both sayable. A combined-level stack has
 * identical members and no lone copies to bound.
 */
export function setCopyDepth(requirements: RequirementState[], item: BoardItem, maxDepth: number | undefined): RequirementState[] {
  if (item.total !== undefined) return requirements
  const extras = new Set(item.extras)
  return normalize(requirements.map((requirement, index) => (
    extras.has(index) ? { ...requirement, maxDepth } : requirement
  )))
}

/**
 * Sets or clears the stack's combined level. Only a lone concrete chip can
 * count levels; with a total the whole stack becomes identical optional
 * members ("up to N items reaching T levels"), without one it returns to
 * an anchor with plain repeats ("exactly N of the item").
 */
export function setStackTotal(requirements: RequirementState[], item: BoardItem, total: number | undefined): RequirementState[] {
  const anchorIndex = item.members[0]
  const anchor = requirements[anchorIndex]
  if (item.cluster !== undefined || anchor.item === undefined) return requirements
  const indices = [anchorIndex, ...item.extras]
  if (total === undefined) {
    return normalize(requirements.map((requirement, index) => {
      if (!indices.includes(index)) return requirement
      return index === anchorIndex
        ? { ...requirement, levelSum: undefined }
        : plainCopy(anchor)
    }))
  }
  const group = anchor.levelSum?.group
    ?? freeGroup(requirements.map((requirement) => requirement.levelSum?.group), LEVEL_SUM_GROUP_MAX)
  if (group === undefined) return requirements
  const member: RequirementState = {
    ...anchor,
    upgrade: { mode: 'any', value: 1 },
    identityGroup: undefined,
    levelSum: { group, atLeast: total },
  }
  return normalize(requirements.map((requirement, index) => (
    indices.includes(index) ? { ...member } : requirement
  )))
}

/**
 * Applies the editor's result: the anchor's own fields plus the stack's
 * shape. `index` is the edited anchor, or `null` for a new chip. Editing a
 * cluster member leaves the stack's count and total to the cluster.
 */
export function applyEdit(
  requirements: RequirementState[],
  index: number | null,
  requirement: RequirementState,
  count: number,
  total: number | undefined,
  copyDepth?: number,
): RequirementState[] {
  let next: RequirementState[]
  let anchorIndex: number
  if (index === null) {
    next = [...requirements, { ...requirement }]
    anchorIndex = next.length - 1
  } else {
    const current = requirements[index]
    // The copies belonged to the chip as it was, and the edit may have changed
    // the very kind they copy — so the stack comes down here and is rebuilt
    // below from the count and total the editor returned. A cluster member
    // leaves its stack to the cluster and keeps its copies.
    const doomed = current.alternativeGroup !== undefined
      ? new Set<number>()
      : new Set(boardItems(requirements).find((entry) => entry.members.includes(index))?.extras ?? [])
    next = requirements
      .map((existing, i) => (i === index ? { ...requirement, alternativeGroup: current.alternativeGroup } : existing))
      .filter((_, i) => !doomed.has(i))
    // Deleting the copies before the anchor pulls it down the list; normalize
    // rewrites entries but never reorders them, so this stays right.
    anchorIndex = index - [...doomed].filter((i) => i < index).length
  }
  next = normalize(next)
  if (next[anchorIndex]?.alternativeGroup !== undefined) return next
  let item = boardItems(next).find((entry) => entry.members.includes(anchorIndex))
  if (!item) return next
  if (item.total !== undefined && total === undefined) {
    next = setStackTotal(next, item, undefined)
    item = boardItems(next).find((entry) => entry.members.includes(anchorIndex)) ?? item
  }
  next = setStackCount(next, item, count)
  if (total !== undefined) {
    const refreshed = boardItems(next).find((entry) => entry.members.includes(anchorIndex))
    if (refreshed) next = setStackTotal(next, refreshed, total)
  } else {
    const refreshed = boardItems(next).find((entry) => entry.members.includes(anchorIndex))
    if (refreshed) next = setCopyDepth(next, refreshed, copyDepth)
  }
  return next
}
