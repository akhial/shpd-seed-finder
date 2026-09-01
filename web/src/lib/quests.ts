import type { QuestName, QuestVariant } from "./wasm/types";

const QUEST_LABELS: Record<QuestName, string> = {
  ghost: "Sad Ghost",
  wandmaker: "Wandmaker",
  blacksmith: "Blacksmith",
  imp: "Imp",
};

const VARIANT_LABELS: Record<QuestVariant, string> = {
  fetid_rat: "Fetid Rat",
  gnoll_trickster: "Gnoll Trickster",
  great_crab: "Great Crab",
  corpse_dust: "Corpse Dust",
  elemental_embers: "Elemental Embers",
  rotberry: "Rotberry",
  crystal: "Crystal Spire",
  gnoll: "Gnoll Geomancer",
  vault: "Vault",
};

/** Display name of a quest giver, e.g. `ghost` → "Sad Ghost". */
export const questLabel = (quest: QuestName): string => QUEST_LABELS[quest];

/** Display name of a quest variant, e.g. `elemental_embers` → "Elemental Embers". */
export const questVariantLabel = (variant: QuestVariant): string => VARIANT_LABELS[variant];
