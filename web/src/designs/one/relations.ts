import { IDENTITY_GROUP_MAX, UPGRADE_SUM_GROUP_MAX, upgradeSumCapacity } from '../../lib/query'
import type { RequirementState } from '../../lib/wasm/types'

/**
 * Pure edits behind the requirement board's direct-manipulation gestures.
 * Every edit returns a new requirement list that the query model already
 * understands; nothing here changes the document format. The model's rules
 * are kept by construction:
 *
 * - an alternative ("either/or") group is several requirements sharing an
 *   `alternativeGroup`; a group of one is meaningless and is collapsed;
 * - a bundle is several requirements sharing an `upgradeSum.group` and one
 *   total; members of an either/or cluster may not carry a sum, so joining
 *   one relationship leaves the other;
 * - a same-item tether is several requirements sharing an `identityGroup`;
 *   it is orthogonal to the two above.
 */

export type Relation = 'alternative' | 'identity' | 'upgradeSum'

/** What the board lays out: a lone chip, an either/or cluster, or a Σ bundle. */
export type BoardItem =
  | { type: 'chip'; index: number }
  | { type: 'alternatives'; group: number; members: number[] }
  | { type: 'bundle'; group: number; atLeast: number; members: number[] }

/** The board's items in requirement order; a cluster or bundle sits where its first member is. */
export function boardItems(requirements: readonly RequirementState[]): BoardItem[] {
  const items: BoardItem[] = []
  const alternatives = new Map<number, BoardItem & { type: 'alternatives' }>()
  const bundles = new Map<number, BoardItem & { type: 'bundle' }>()
  requirements.forEach((requirement, index) => {
    if (requirement.alternativeGroup !== undefined) {
      const existing = alternatives.get(requirement.alternativeGroup)
      if (existing) { existing.members.push(index); return }
      const item = { type: 'alternatives' as const, group: requirement.alternativeGroup, members: [index] }
      alternatives.set(requirement.alternativeGroup, item)
      items.push(item)
      return
    }
    if (requirement.upgradeSum) {
      const existing = bundles.get(requirement.upgradeSum.group)
      if (existing) { existing.members.push(index); return }
      const item = { type: 'bundle' as const, group: requirement.upgradeSum.group, atLeast: requirement.upgradeSum.atLeast, members: [index] }
      bundles.set(requirement.upgradeSum.group, item)
      items.push(item)
      return
    }
    items.push({ type: 'chip', index })
  })
  // Single-member clusters and bundles render as lone chips.
  return items.map((item) => (item.type !== 'chip' && item.members.length === 1 ? { type: 'chip', index: item.members[0] } : item))
}

const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max)

const countBy = (requirements: readonly RequirementState[], key: (requirement: RequirementState) => number | undefined) => {
  const counts = new Map<number, number>()
  for (const requirement of requirements) {
    const group = key(requirement)
    if (group !== undefined) counts.set(group, (counts.get(group) ?? 0) + 1)
  }
  return counts
}

/** Drops every relationship that has a single member left: a group of one says nothing. */
export function collapseLoneGroups(requirements: RequirementState[]): RequirementState[] {
  const alternatives = countBy(requirements, (requirement) => requirement.alternativeGroup)
  const identities = countBy(requirements, (requirement) => requirement.identityGroup)
  const sums = countBy(requirements, (requirement) => requirement.upgradeSum?.group)
  return requirements.map((requirement) => {
    let next = requirement
    if (next.alternativeGroup !== undefined && (alternatives.get(next.alternativeGroup) ?? 0) < 2) {
      const { alternativeGroup: _a, ...rest } = next
      next = rest
    }
    if (next.identityGroup !== undefined && (identities.get(next.identityGroup) ?? 0) < 2) {
      const { identityGroup: _i, ...rest } = next
      next = rest
    }
    if (next.upgradeSum && (sums.get(next.upgradeSum.group) ?? 0) < 2) {
      const { upgradeSum: _s, ...rest } = next
      next = rest
    }
    return next
  })
}

const freeGroup = (used: Iterable<number | undefined>, max: number): number | undefined => {
  const taken = new Set(used)
  for (let group = 1; group <= max; group += 1) if (!taken.has(group)) return group
  return undefined
}

const nextAlternativeGroup = (requirements: readonly RequirementState[]): number =>
  requirements.reduce((highest, requirement) => Math.max(highest, requirement.alternativeGroup ?? 0), 0) + 1

/** Moves the requirement at `from` to sit right after the last requirement matching `after`. */
function moveAfter(requirements: RequirementState[], from: number, after: (requirement: RequirementState, index: number) => boolean): RequirementState[] {
  const moving = requirements[from]
  const rest = requirements.filter((_, index) => index !== from)
  const last = rest.reduce((found, requirement, index) => (after(requirement, index) ? index : found), -1)
  return [...rest.slice(0, last + 1), moving, ...rest.slice(last + 1)]
}

/** The chip at `source` becomes an either/or alternative of the chip at `target`. */
export function joinAlternatives(requirements: RequirementState[], source: number, target: number): RequirementState[] {
  if (source === target) return requirements
  const group = requirements[target].alternativeGroup ?? nextAlternativeGroup(requirements)
  if (requirements[source].alternativeGroup === group) return requirements
  const joined = requirements.map((requirement, index) => {
    if (index !== source && index !== target) return requirement
    // Alternatives cannot carry a combined-upgrade total.
    const { upgradeSum: _s, ...rest } = requirement
    return { ...rest, alternativeGroup: group }
  })
  return collapseLoneGroups(moveAfter(joined, source, (requirement) => requirement.alternativeGroup === group))
}

/** The chips at `source` and `target` must be the same kind of item. */
export function linkIdentity(requirements: RequirementState[], source: number, target: number): RequirementState[] | undefined {
  if (source === target) return requirements
  const existing = requirements[target].identityGroup ?? requirements[source].identityGroup
  const group = existing ?? freeGroup(requirements.map((requirement) => requirement.identityGroup), IDENTITY_GROUP_MAX)
  if (group === undefined) return undefined
  if (requirements[source].identityGroup === group && requirements[target].identityGroup === group) return requirements
  return collapseLoneGroups(requirements.map((requirement, index) => (
    index === source || index === target ? { ...requirement, identityGroup: group } : requirement
  )))
}

/** The chip at `source` joins the Σ bundle of the chip at `target`, or forms a new one with it. */
export function joinBundle(requirements: RequirementState[], source: number, target: number): RequirementState[] | undefined {
  if (source === target) return requirements
  const existing = requirements[target].upgradeSum?.group
  const group = existing ?? freeGroup(requirements.map((requirement) => requirement.upgradeSum?.group), UPGRADE_SUM_GROUP_MAX)
  if (group === undefined) return undefined
  if (requirements[source].upgradeSum?.group === group) return requirements
  const members = requirements.filter((requirement, index) => index === source || index === target || requirement.upgradeSum?.group === group)
  const capacity = upgradeSumCapacity(members)
  // A new bundle starts at half its capacity: clearly a shared total, clearly reachable.
  const atLeast = clamp(requirements[target].upgradeSum?.atLeast ?? Math.ceil(capacity / 2), 1, Math.max(1, capacity))
  const joined = requirements.map((requirement, index) => {
    if (index !== source && index !== target) return requirement
    // Bundle members cannot be either/or alternatives.
    const { alternativeGroup: _a, ...rest } = requirement
    return { ...rest, upgradeSum: { group, atLeast } }
  })
  return collapseLoneGroups(moveAfter(joined, source, (requirement) => requirement.upgradeSum?.group === group))
}

/** Sets one bundle's shared total. */
export function setBundleTotal(requirements: RequirementState[], group: number, atLeast: number): RequirementState[] {
  return requirements.map((requirement) => (
    requirement.upgradeSum?.group === group ? { ...requirement, upgradeSum: { group, atLeast } } : requirement
  ))
}

/** Pulls the chip at `index` out of its either/or cluster and Σ bundle (a same-item tether stays). */
export function detach(requirements: RequirementState[], index: number): RequirementState[] {
  const { alternativeGroup: _a, upgradeSum: _s, ...rest } = requirements[index]
  return collapseLoneGroups(requirements.map((requirement, i) => (i === index ? rest : requirement)))
}

/** Cuts the same-item tether of the chip at `index`. */
export function unlinkIdentity(requirements: RequirementState[], index: number): RequirementState[] {
  const { identityGroup: _i, ...rest } = requirements[index]
  return collapseLoneGroups(requirements.map((requirement, i) => (i === index ? rest : requirement)))
}

export function removeAt(requirements: RequirementState[], index: number): RequirementState[] {
  return collapseLoneGroups(requirements.filter((_, i) => i !== index))
}

/** Applies the relation a drop or a "with…" pick asked for. */
export function relate(requirements: RequirementState[], relation: Relation, source: number, target: number): RequirementState[] | undefined {
  if (relation === 'alternative') return joinAlternatives(requirements, source, target)
  if (relation === 'identity') return linkIdentity(requirements, source, target)
  return joinBundle(requirements, source, target)
}
