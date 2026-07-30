import { displayItemName, getItem, kindFamily, sourceLabel, wildcardSpriteForKind, wildcardSprites } from '../../lib/catalog'
import type { ItemCategory, RequirementKind, RequirementState } from '../../lib/wasm/types'

export const categoryLabel: Record<ItemCategory, string> = {
  weapon: 'Weapon',
  armor: 'Armor',
  wand: 'Wand',
  ring: 'Ring',
}

export const categoryPlural: Record<ItemCategory, string> = {
  weapon: 'Weapons',
  armor: 'Armor',
  wand: 'Wands',
  ring: 'Rings',
}

export const categoryTint: Record<ItemCategory, string> = {
  weapon: '#e2a24d',
  armor: '#8fb7e8',
  wand: '#c9a6e8',
  ring: '#e8d05f',
}

export const kindLabel: Record<RequirementKind, string> = {
  ...categoryLabel,
  melee_weapon: 'Melee weapon',
  thrown_weapon: 'Thrown weapon',
}

/** The broad family a requirement belongs to, used for grouping and sprites. */
export function requirementKind(requirement: RequirementState): ItemCategory | undefined {
  if (requirement.kind) return kindFamily(requirement.kind)
  return requirement.item ? getItem(requirement.item)?.type : undefined
}

export function requirementSprite(requirement: RequirementState): number {
  if (requirement.item) {
    const item = getItem(requirement.item)
    if (item) return item.sprite
  }
  if (requirement.kind) return wildcardSpriteForKind(requirement.kind)
  return wildcardSprites[requirementKind(requirement) ?? 'weapon']
}

export function requirementTitle(requirement: RequirementState): string {
  if (requirement.item) return displayItemName(requirement.item)
  const kind = requirement.kind ? kindLabel[requirement.kind].toLowerCase() : 'item'
  const tier = requirement.tier
  if (tier.mode === 'exact') return `Any tier-${tier.value} ${kind}`
  if (tier.mode === 'at_least') return `Any ${kind} · tier ${tier.value}+`
  if (tier.mode === 'at_most') return `Any ${kind} · tier ≤${tier.value}`
  return `Any ${kind}`
}

export function requirementDetails(requirement: RequirementState): string[] {
  const parts: string[] = []
  if (requirement.upgrade.mode === 'exact') parts.push(`exactly +${requirement.upgrade.value}`)
  if (requirement.upgrade.mode === 'at_least') parts.push(`+${requirement.upgrade.value} or higher`)
  if (requirement.effect) parts.push(requirement.effect)
  if (requirement.uncursed) parts.push('uncursed')
  if (requirement.source) parts.push(sourceLabel(requirement.source))
  if (requirement.identityGroup) parts.push(`group ${'ABCD'[requirement.identityGroup - 1] ?? requirement.identityGroup}`)
  if (requirement.maxDepth !== undefined) parts.push(`floors 1–${requirement.maxDepth}`)
  return parts
}
