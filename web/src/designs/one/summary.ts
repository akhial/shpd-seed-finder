import {
  displayItemName,
  getItem,
  kindFamily,
  sourceLabel,
  wildcardSpriteForKind,
  wildcardSprites,
} from "../../lib/catalog";
import { effectNamesOf, isAnyEnchantment } from "../../lib/query";
import type { ItemArt } from "../../lib/sprites";
import { itemArt } from "../../lib/sprites";
import type { ItemCategory, RequirementKind, RequirementState } from "../../lib/wasm/types";

export const categoryLabel: Record<ItemCategory, string> = {
  weapon: "Weapon",
  armor: "Armor",
  wand: "Wand",
  ring: "Ring",
  trinket: "Trinket",
};

export const categoryPlural: Record<ItemCategory, string> = {
  weapon: "Weapons",
  armor: "Armor",
  wand: "Wands",
  ring: "Rings",
  trinket: "Trinkets",
};

export const categoryTint: Record<ItemCategory, string> = {
  weapon: "#e2a24d",
  armor: "#8fb7e8",
  wand: "#c9a6e8",
  ring: "#e8d05f",
  trinket: "#bba0ee",
};

export const kindLabel: Record<RequirementKind, string> = {
  ...categoryLabel,
  melee_weapon: "Melee weapon",
  thrown_weapon: "Thrown weapon",
};

/** The broad family a requirement belongs to, used for grouping and sprites. */
export function requirementKind(requirement: RequirementState): ItemCategory | undefined {
  if (requirement.kind) return kindFamily(requirement.kind);
  return requirement.item ? getItem(requirement.item)?.type : undefined;
}

/**
 * The art for a requirement's chip. A requirement describes what to search for
 * rather than what some run holds, so it is resolved without a gem table and a
 * ring keeps the catalog's per-class cell — the same picture whatever seed is
 * on screen.
 */
export function requirementArt(requirement: RequirementState): ItemArt {
  if (requirement.item) {
    const item = getItem(requirement.item);
    if (item) return itemArt(item.sprite);
  }
  if (requirement.kind) return itemArt(wildcardSpriteForKind(requirement.kind));
  return itemArt(wildcardSprites[requirementKind(requirement) ?? "weapon"]);
}

export function requirementTitle(requirement: RequirementState): string {
  if (requirement.item) return displayItemName(requirement.item);
  if (requirementKind(requirement) === "trinket") return "Trinket";
  const kind = requirement.kind ? kindLabel[requirement.kind].toLowerCase() : "item";
  const tier = requirement.tier;
  if (tier.mode === "exact") return `Any tier-${tier.value} ${kind}`;
  if (tier.mode === "at_least") return `Any ${kind} · tier ${tier.value}+`;
  if (tier.mode === "at_most") return `Any ${kind} · tier ≤${tier.value}`;
  return `Any ${kind}`;
}

export function requirementDetails(requirement: RequirementState): string[] {
  const parts: string[] = [];
  if (requirement.selectTrinket) parts.push("choose at +3");
  if (requirement.upgrade.mode === "exact") parts.push(`exactly +${requirement.upgrade.value}`);
  if (requirement.upgrade.mode === "at_least")
    parts.push(`+${requirement.upgrade.value} or higher`);
  if (requirement.levelSum) parts.push(`levels ≥ ${requirement.levelSum.atLeast} together`);
  const effect = effectLabel(requirement);
  if (effect) parts.push(effect);
  if (requirement.uncursed) parts.push("uncursed");
  if (requirement.source) parts.push(sourceLabel(requirement.source));
  if (requirement.identityGroup) parts.push("same-kind stack");
  if (requirement.maxDepth !== undefined) parts.push(`floors 1–${requirement.maxDepth}`);
  return parts;
}

/** The effect filter as row text: a name, "effect: A/B/C" for a set, or "any enchantment". */
export function effectLabel(requirement: RequirementState): string | undefined {
  if (requirement.effect === undefined) return undefined;
  if (isAnyEnchantment(requirement.effect))
    return requirementKind(requirement) === "armor" ? "any glyph" : "any enchantment";
  const names = effectNamesOf(requirement.effect, requirement.kind);
  if (names.length === 0) return undefined;
  return names.length === 1 ? names[0] : `effect: ${names.join("/")}`;
}

/** The card title for an "any of these" slot. */
export const alternativesTitle = (count: number): string => `Any of ${count}`;
