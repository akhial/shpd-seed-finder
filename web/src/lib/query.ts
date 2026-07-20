import { getItem, isCurseForCategory } from './catalog'
import type {
  QueryDocument,
  QueryState,
  RequirementDocument,
  RequirementState,
  TierFilter,
  UpgradeFilter,
} from './wasm/types'

export const defaultTier = (): TierFilter => ({ mode: 'any', value: 3 })
export const defaultUpgrade = (): UpgradeFilter => ({ mode: 'any', value: 1 })

export const emptyRequirement = (kind?: RequirementState['kind']): RequirementState => ({
  kind,
  tier: defaultTier(),
  upgrade: defaultUpgrade(),
  uncursed: false,
})

export const defaultQueryState = (): QueryState => ({
  requirements: [],
  maxDepth: 24,
  requireBlacksmith: false,
  excludeBlacksmithRewards: false,
  fastMode: false,
  challenges: [],
})

function requirementToDocument(requirement: RequirementState): RequirementDocument {
  const output: RequirementDocument = {}
  if (requirement.kind) output.kind = requirement.kind
  if (requirement.item) output.item = requirement.item
  if (requirement.tier.mode !== 'any') {
    output.tier = { [requirement.tier.mode]: requirement.tier.value } as NonNullable<RequirementDocument['tier']>
  }
  if (requirement.upgrade.mode === 'exact') output.upgrade = requirement.upgrade.value
  if (requirement.upgrade.mode === 'at_least') output.upgrade = { at_least: requirement.upgrade.value }
  if (requirement.effect) output.effect = requirement.effect
  if (requirement.uncursed) output.uncursed = true
  if (requirement.source) output.source = requirement.source
  if (requirement.identityGroup) output.identity_group = requirement.identityGroup
  if (requirement.maxDepth !== undefined) output.max_depth = requirement.maxDepth
  return output
}

export function toQueryDocument(state: QueryState): QueryDocument {
  const output: QueryDocument = { requirements: state.requirements.map(requirementToDocument) }
  if (state.maxDepth !== 24) output.max_depth = state.maxDepth
  if (state.requireBlacksmith) output.require_blacksmith = true
  if (state.excludeBlacksmithRewards) output.exclude_blacksmith_rewards = true
  if (state.fastMode) output.fast_mode = true
  if (state.challenges.length) output.challenges = [...state.challenges]
  return output
}

export function toQueryJson(state: QueryState): string {
  return JSON.stringify(toQueryDocument(state))
}

function requirementFromDocument(value: RequirementDocument): RequirementState {
  let tier = defaultTier()
  if (value.tier && 'exact' in value.tier) tier = { mode: 'exact', value: value.tier.exact }
  if (value.tier && 'at_least' in value.tier) tier = { mode: 'at_least', value: value.tier.at_least }
  if (value.tier && 'at_most' in value.tier) tier = { mode: 'at_most', value: value.tier.at_most }
  let upgrade = defaultUpgrade()
  if (typeof value.upgrade === 'number') upgrade = { mode: 'exact', value: value.upgrade }
  if (value.upgrade && typeof value.upgrade === 'object') upgrade = { mode: 'at_least', value: value.upgrade.at_least }
  return {
    kind: value.kind,
    item: value.item,
    tier,
    upgrade,
    effect: value.effect,
    uncursed: value.uncursed ?? false,
    source: value.source,
    identityGroup: value.identity_group,
    maxDepth: value.max_depth,
  }
}

export function fromQueryJson(json: string): QueryState {
  const document = JSON.parse(json) as QueryDocument
  return {
    requirements: document.requirements.map(requirementFromDocument),
    maxDepth: document.max_depth ?? 24,
    requireBlacksmith: document.require_blacksmith ?? false,
    excludeBlacksmithRewards: document.exclude_blacksmith_rewards ?? false,
    fastMode: document.fast_mode ?? false,
    challenges: document.challenges ? [...document.challenges] : [],
  }
}

export interface ValidationResult { valid: boolean; errors: string[] }

export function validateRequirement(requirement: RequirementState): string[] {
  const errors: string[] = []
  const item = requirement.item ? getItem(requirement.item) : undefined
  const kind = requirement.kind ?? item?.type
  if (!kind) errors.push('Choose an item category.')
  if (item && requirement.kind && item.type !== requirement.kind) errors.push('The item does not belong to this category.')
  if (requirement.tier.mode !== 'any') {
    if (requirement.item || (kind !== 'weapon' && kind !== 'armor')) errors.push('Tier filters require a wildcard weapon or armor.')
    const { mode, value } = requirement.tier
    if (mode === 'exact' && (value < 2 || value > 5)) errors.push('Exact tier must be 2 through 5.')
    if ((mode === 'at_least' || mode === 'at_most') && (value < 3 || value > 4)) errors.push('Tier bounds must be 3 or 4.')
  }
  if (requirement.upgrade.mode !== 'any') {
    const maximum = kind === 'ring' ? 4 : 3
    const minimum = requirement.upgrade.mode === 'exact' ? 1 : 0
    if (requirement.upgrade.value < minimum || requirement.upgrade.value > maximum) errors.push(`Upgrade must be ${minimum} through +${maximum}.`)
  }
  if (requirement.maxDepth !== undefined && (requirement.maxDepth < 1 || requirement.maxDepth > 24)) errors.push('Requirement floor must be 1 through 24.')
  if (requirement.effect && kind !== 'weapon' && kind !== 'armor') errors.push('Effects require a weapon or armor category.')
  if (requirement.effect && kind && !isCurseForCategory(kind, requirement.effect) && !getEffectNames(kind).includes(requirement.effect)) errors.push('The effect does not belong to this category.')
  if (requirement.uncursed && requirement.effect && kind && isCurseForCategory(kind, requirement.effect)) errors.push('An uncursed item cannot have a curse effect.')
  return errors
}

function getEffectNames(kind: string): string[] {
  const { effectNamesForCategory } = catalogHelpers
  return effectNamesForCategory(kind)
}

// Kept indirect so validation remains straightforward to mock in component tests.
import { effectNamesForCategory } from './catalog'
const catalogHelpers = { effectNamesForCategory }

export function validateQuery(state: QueryState): ValidationResult {
  const errors: string[] = []
  if (!state.requirements.length) errors.push('Add at least one requirement.')
  if (state.maxDepth < 1 || state.maxDepth > 24) errors.push('Maximum floor must be 1 through 24.')
  state.requirements.forEach((requirement, index) => {
    for (const error of validateRequirement(requirement)) errors.push(`Requirement ${index + 1}: ${error}`)
  })
  const groups = new Map<number, { kind?: string; item?: string }>()
  state.requirements.forEach((requirement) => {
    if (!requirement.identityGroup) return
    const current = { kind: requirement.kind ?? getItem(requirement.item ?? '')?.type, item: requirement.item }
    const previous = groups.get(requirement.identityGroup)
    if (previous && (previous.kind !== current.kind || (previous.item && current.item && previous.item !== current.item))) {
      errors.push(`Identity group ${requirement.identityGroup} has incompatible category or item requirements.`)
    } else if (!previous || (!previous.item && current.item)) {
      groups.set(requirement.identityGroup, current)
    }
  })
  return { valid: errors.length === 0, errors }
}
