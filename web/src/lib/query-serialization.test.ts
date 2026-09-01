import { describe, expect, it } from "vite-plus/test";
import {
  FLOOR_LIMIT_OPTIONS,
  canonicalEffect,
  defaultQueryState,
  fromQueryJson,
  nearestOptionIndex,
  normalizeFloorLimit,
  slotCount,
  toQueryDocument,
  toQueryJson,
} from "./query";
import { weaponEnchantments } from "./catalog";
import type { QueryState, RequirementState } from "./wasm/types";

describe("query serialization", () => {
  it("omits query and requirement defaults", () => {
    expect(
      toQueryJson({
        ...defaultQueryState(),
        requirements: [
          {
            kind: "wand",
            tier: { mode: "any", value: 3 },
            upgrade: { mode: "any", value: 1 },
            uncursed: false,
          },
        ],
      }),
    ).toBe('{"requirements":[{"kind":"wand"}]}');
  });

  it("emits tier and upgrade wire forms exactly", () => {
    const state = {
      ...defaultQueryState(),
      requirements: [
        {
          kind: "armor" as const,
          tier: { mode: "at_least" as const, value: 4 },
          upgrade: { mode: "at_least" as const, value: 2 },
          uncursed: false,
        },
        {
          kind: "ring" as const,
          item: "ring_haste",
          tier: { mode: "any" as const, value: 3 },
          upgrade: { mode: "exact" as const, value: 4 },
          uncursed: false,
        },
      ],
      challenges: ["on_diet" as const, "into_darkness" as const],
    };
    expect(JSON.parse(toQueryJson(state))).toEqual({
      requirements: [
        { kind: "armor", tier: { at_least: 4 }, upgrade: { at_least: 2 } },
        { kind: "ring", item: "ring_haste", upgrade: 4 },
      ],
      challenges: ["on_diet", "into_darkness"],
    });
  });

  it("serializes and round-trips melee and thrown weapon kinds", () => {
    const state: QueryState = {
      ...defaultQueryState(),
      requirements: [
        {
          kind: "melee_weapon",
          tier: { mode: "exact", value: 5 },
          upgrade: { mode: "any", value: 1 },
          uncursed: false,
        },
        {
          kind: "thrown_weapon",
          tier: { mode: "any", value: 3 },
          upgrade: { mode: "any", value: 1 },
          uncursed: false,
        },
        {
          kind: "thrown_weapon",
          item: "shuriken",
          tier: { mode: "any", value: 3 },
          upgrade: { mode: "any", value: 1 },
          uncursed: false,
        },
      ],
    };
    expect(JSON.parse(toQueryJson(state))).toEqual({
      requirements: [
        { kind: "melee_weapon", tier: { exact: 5 } },
        { kind: "thrown_weapon" },
        { kind: "thrown_weapon", item: "shuriken" },
      ],
    });
    expect(fromQueryJson(toQueryJson(state))).toEqual(state);
    // Pre-existing documents with a plain weapon kind keep decoding unchanged.
    expect(fromQueryJson('{"requirements":[{"kind":"weapon"}]}').requirements[0].kind).toBe(
      "weapon",
    );
  });

  it("always writes a requirement category, deriving it from the item", () => {
    // The engine's start decision compares categories for equality, so a
    // requirement that omitted its kind used to share with everything. Both
    // directions of the mapping fill it in from the named item.
    const state: QueryState = {
      ...defaultQueryState(),
      requirements: [
        {
          item: "sword",
          tier: { mode: "any", value: 3 },
          upgrade: { mode: "any", value: 1 },
          uncursed: false,
        },
      ],
    };
    expect(toQueryJson(state)).toBe('{"requirements":[{"kind":"weapon","item":"sword"}]}');
    expect(
      fromQueryJson(
        '{"requirements":[{"item":"sword"},{"item":"ring_haste"},{"kind":"wand"}]}',
      ).requirements.map((requirement) => requirement.kind),
    ).toEqual(["weapon", "ring", "wand"]);
    // An unknown item names no category, and the document stays kind-less
    // rather than gaining a made-up one.
    expect(
      fromQueryJson('{"requirements":[{"item":"no_such_item"}]}').requirements[0].kind,
    ).toBeUndefined();
  });

  it("carries the Wandmaker quest and defaults it to any", () => {
    const base = {
      ...defaultQueryState(),
      requirements: [
        {
          kind: "wand" as const,
          tier: { mode: "any" as const, value: 3 },
          upgrade: { mode: "any" as const, value: 1 },
          uncursed: false,
        },
      ],
    };
    expect(toQueryJson(base)).toBe('{"requirements":[{"kind":"wand"}]}');
    expect(fromQueryJson(toQueryJson(base)).wandmakerQuest).toBeUndefined();

    for (const variant of ["corpse_dust", "elemental_embers", "rotberry"] as const) {
      const state: QueryState = { ...base, wandmakerQuest: variant };
      expect(JSON.parse(toQueryJson(state))).toEqual({
        requirements: [{ kind: "wand" }],
        wandmaker_quest: variant,
      });
      expect(fromQueryJson(toQueryJson(state))).toEqual(state);
    }

    // An unknown quest fails the import rather than silently widening it.
    expect(() =>
      fromQueryJson('{"requirements":[{"kind":"wand"}],"wandmaker_quest":"dust"}'),
    ).toThrowError(/Wandmaker quest/);
  });

  it("round-trips a fully loaded state", () => {
    const state: QueryState = {
      requirements: [
        {
          kind: "weapon",
          item: undefined,
          tier: { mode: "at_most", value: 4 },
          upgrade: { mode: "exact", value: 3 },
          effect: "Blazing",
          uncursed: false,
          source: "locked_chest",
          identityGroup: 2,
          maxDepth: 8,
        },
      ],
      maxDepth: 19,
      requireBlacksmith: true,
      excludeBlacksmithRewards: true,
      challenges: ["faith_is_my_armor", "hostile_champions"],
    };
    expect(fromQueryJson(toQueryJson(state))).toEqual(state);
  });

  it("loads a query saved with the retired fast-mode flag and drops it", () => {
    // Saved queries, presets and share links written before fast mode was
    // removed still open; the engine's codec ignores the key the same way.
    const legacy = '{"requirements":[{"kind":"wand"}],"max_depth":12,"fast_mode":true}';
    const current = '{"requirements":[{"kind":"wand"}],"max_depth":12}';
    expect(fromQueryJson(legacy)).toEqual(fromQueryJson(current));
    // And nothing writes the key back out.
    expect(toQueryJson(fromQueryJson(legacy))).toBe(current);
  });

  it("snaps stored empty boss-floor limits to the equivalent floor below", () => {
    const state = fromQueryJson(
      '{"requirements":[{"kind":"wand","max_depth":5},{"kind":"ring","max_depth":10}],"max_depth":15}',
    );
    expect(state.maxDepth).toBe(14);
    expect(state.requirements.map((requirement) => requirement.maxDepth)).toEqual([4, 9]);
  });

  it("offers every floor except the empty boss floors as a limit", () => {
    expect(FLOOR_LIMIT_OPTIONS).toHaveLength(21);
    expect(FLOOR_LIMIT_OPTIONS).not.toContain(5);
    expect(FLOOR_LIMIT_OPTIONS).not.toContain(10);
    expect(FLOOR_LIMIT_OPTIONS).not.toContain(15);
    expect(FLOOR_LIMIT_OPTIONS).toContain(20);
    expect(FLOOR_LIMIT_OPTIONS).toContain(24);
    expect([4, 5, 9, 10, 14, 15, 20, 24].map(normalizeFloorLimit)).toEqual([
      4, 4, 9, 9, 14, 14, 20, 24,
    ]);
  });

  it("maps slider values to indices, snapping off-list values to the nearest option below", () => {
    // Every selectable floor maps to its own slot.
    FLOOR_LIMIT_OPTIONS.forEach((floor, index) => {
      expect(nearestOptionIndex(FLOOR_LIMIT_OPTIONS, floor)).toBe(index);
    });
    // Empty boss floors land on the slot of the equivalent floor below.
    expect(nearestOptionIndex(FLOOR_LIMIT_OPTIONS, 5)).toBe(FLOOR_LIMIT_OPTIONS.indexOf(4));
    expect(nearestOptionIndex(FLOOR_LIMIT_OPTIONS, 10)).toBe(FLOOR_LIMIT_OPTIONS.indexOf(9));
    expect(nearestOptionIndex(FLOOR_LIMIT_OPTIONS, 15)).toBe(FLOOR_LIMIT_OPTIONS.indexOf(14));
    // Out-of-range values snap to the nearest option below, never slot 0.
    expect(nearestOptionIndex(FLOOR_LIMIT_OPTIONS, 30)).toBe(FLOOR_LIMIT_OPTIONS.length - 1);
    expect(nearestOptionIndex(FLOOR_LIMIT_OPTIONS, 0)).toBe(0);
  });

  const plain = (patch: Partial<RequirementState>): RequirementState => ({
    kind: "weapon",
    tier: { mode: "any", value: 3 },
    upgrade: { mode: "any", value: 1 },
    uncursed: false,
    ...patch,
  });

  it("writes effect sets as a bare name, a catalog-ordered list, or the any_enchantment shorthand", () => {
    const doc = (effect: RequirementState["effect"]) =>
      toQueryDocument({ ...defaultQueryState(), requirements: [plain({ effect })] })
        .requirements[0];
    expect(doc("Blazing")).toEqual({ kind: "weapon", effect: "Blazing" });
    expect(doc(["Blazing"])).toEqual({ kind: "weapon", effect: "Blazing" });
    // Out-of-order input is written in catalog order.
    expect(doc(["Vampiric", "Blocking", "Projecting"])).toEqual({
      kind: "weapon",
      effect: ["Blocking", "Projecting", "Vampiric"],
    });
    expect(doc([...weaponEnchantments].reverse())).toEqual({
      kind: "weapon",
      effect: "any_enchantment",
    });
    expect(doc("any_enchantment")).toEqual({ kind: "weapon", effect: "any_enchantment" });
    expect(doc([])).toEqual({ kind: "weapon" });
    expect(canonicalEffect(["Blocking", "Annoying"], "melee_weapon")).toEqual([
      "Blocking",
      "Annoying",
    ]);
  });

  it("loads an old document with a bare effect name unchanged", () => {
    const state = fromQueryJson('{"requirements":[{"kind":"weapon","effect":"Blazing"}]}');
    expect(state.requirements[0].effect).toBe("Blazing");
    expect(toQueryJson(state)).toBe('{"requirements":[{"kind":"weapon","effect":"Blazing"}]}');
  });

  it("round-trips effect lists, any_enchantment and combined levels", () => {
    const state: QueryState = {
      ...defaultQueryState(),
      requirements: [
        plain({
          item: "greatshield",
          upgrade: { mode: "exact", value: 2 },
          effect: ["Blocking", "Projecting", "Vampiric"],
        }),
        plain({ kind: "armor", effect: "any_enchantment", uncursed: true }),
        plain({ kind: "ring", item: "ring_might", levelSum: { group: 1, atLeast: 4 } }),
        plain({ kind: "ring", item: "ring_might", levelSum: { group: 1, atLeast: 4 } }),
      ],
    };
    expect(JSON.parse(toQueryJson(state))).toEqual({
      requirements: [
        {
          kind: "weapon",
          item: "greatshield",
          upgrade: 2,
          effect: ["Blocking", "Projecting", "Vampiric"],
        },
        { kind: "armor", effect: "any_enchantment", uncursed: true },
        { kind: "ring", item: "ring_might", level_sum: { group: 1, at_least: 4 } },
        { kind: "ring", item: "ring_might", level_sum: { group: 1, at_least: 4 } },
      ],
    });
    // The unreleased upgrade_sum key is refused, not reinterpreted.
    expect(() =>
      fromQueryJson('{"requirements":[{"kind":"ring","upgrade_sum":{"group":1,"at_least":2}}]}'),
    ).toThrowError(/upgrade_sum/);
    expect(fromQueryJson(toQueryJson(state))).toEqual(state);
    // A single-name list decodes to the bare-name form so a round trip is the identity.
    expect(
      fromQueryJson('{"requirements":[{"kind":"weapon","effect":["Blazing"]}]}').requirements[0]
        .effect,
    ).toBe("Blazing");
    expect(() => fromQueryJson('{"requirements":[{"kind":"weapon","effect":7}]}')).toThrowError(
      /effect/,
    );
  });

  it("writes alternative groups as one any_of entry and reads them back with fresh group ids", () => {
    const state: QueryState = {
      ...defaultQueryState(),
      requirements: [
        plain({ item: "spear", upgrade: { mode: "exact", value: 3 }, alternativeGroup: 7 }),
        plain({ kind: "wand" }),
        plain({
          kind: "thrown_weapon",
          item: "shuriken",
          upgrade: { mode: "exact", value: 2 },
          alternativeGroup: 7,
        }),
        plain({ item: "sword", alternativeGroup: 2 }),
        plain({ item: "mace", alternativeGroup: 2 }),
        // A group of one is a plain requirement.
        plain({ item: "dagger", alternativeGroup: 9 }),
      ],
    };
    expect(JSON.parse(toQueryJson(state))).toEqual({
      requirements: [
        {
          any_of: [
            { kind: "weapon", item: "spear", upgrade: 3 },
            { kind: "thrown_weapon", item: "shuriken", upgrade: 2 },
          ],
        },
        { kind: "wand" },
        {
          any_of: [
            { kind: "weapon", item: "sword" },
            { kind: "weapon", item: "mace" },
          ],
        },
        { kind: "weapon", item: "dagger" },
      ],
    });
    expect(slotCount(state.requirements)).toBe(4);
    const reread = fromQueryJson(toQueryJson(state));
    // Members regroup contiguously at the first member's position, numbered 1, 2, … in document order.
    expect(
      reread.requirements.map((requirement) => [requirement.item, requirement.alternativeGroup]),
    ).toEqual([
      ["spear", 1],
      ["shuriken", 1],
      [undefined, undefined],
      ["sword", 2],
      ["mace", 2],
      ["dagger", undefined],
    ]);
    expect(toQueryJson(reread)).toBe(toQueryJson(state));
    expect(() => fromQueryJson('{"requirements":[{"any_of":[]}]}')).toThrowError(/any_of/);
    expect(() =>
      fromQueryJson('{"requirements":[{"any_of":[{"any_of":[{"item":"sword"}]}]}]}'),
    ).toThrowError(/nest/);
  });
});
