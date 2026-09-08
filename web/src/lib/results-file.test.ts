import { readFile } from "node:fs/promises";
import { beforeAll, describe, expect, it } from "vite-plus/test";
// The canonical frozen fixture, imported verbatim from the Rust core's test
// data so this codec can never silently drift from it.
import VERSION_1_FIXTURE from "../../../crates/seedfinder-core/tests/fixtures/results-export-v1.json?raw";
import WANDMAKER_QUEST_FIXTURE from "../../../crates/seedfinder-core/tests/fixtures/results-export-wandmaker-quest.json?raw";
import WEAPON_CATEGORIES_FIXTURE from "../../../crates/seedfinder-core/tests/fixtures/results-export-v1-weapon-categories.json?raw";
import { defaultQueryState, toQueryDocument } from "./query";
import { decodeResultsFile, encodeResultsFile, parsedSeedFromCode } from "./results-file";
import init from "./wasm/pkg/seedfinder.js";
import type { QueryState } from "./wasm/types";

/**
 * The results-file codec lives in the engine, so these are conformance tests
 * against the real wasm module rather than against a TypeScript restatement
 * of it. Node has no `fetch` for `file:` URLs, so the module is instantiated
 * from bytes instead of the browser's URL form.
 */
beforeAll(async () => {
  await init({
    module_or_path: await readFile(new URL("./wasm/pkg/seedfinder_bg.wasm", import.meta.url)),
  });
});

const loadedQuery: QueryState = {
  requirements: [
    {
      kind: "ring",
      item: "ring_tenacity",
      tier: { mode: "any", value: 3 },
      upgrade: { mode: "exact", value: 4 },
      uncursed: false,
      source: "imp_reward",
    },
    {
      kind: "wand",
      tier: { mode: "any", value: 3 },
      upgrade: { mode: "at_least", value: 2 },
      uncursed: true,
      identityGroup: 1,
      maxDepth: 9,
    },
  ],
  maxDepth: 12,
  requireBlacksmith: true,
  excludeBlacksmithRewards: false,
  challenges: ["barren_land"],
};

const file = (query: unknown, results: unknown[] = []) =>
  JSON.stringify({ format: "seed-seeker-results", query, results });

describe("results file", () => {
  it("values a seed code through the engine seed parser", () => {
    expect(parsedSeedFromCode("AAA-AAA-AAA")).toEqual({ code: "AAA-AAA-AAA", value: 0 });
    expect(parsedSeedFromCode("AAA-AAA-AAB")).toEqual({ code: "AAA-AAA-AAB", value: 1 });
  });

  it("round-trips the query and seeds through encode and decode", () => {
    const text = encodeResultsFile(toQueryDocument(loadedQuery), ["AAA-AAA-BUH", "ABC-DEF-GHI"]);
    const decoded = decodeResultsFile(text);
    expect(decoded.appVersion).toBeDefined();
    // A file written today carries the game version this engine targets.
    expect(decoded.shpdVersion).toBe("4.0.0-RC-1");
    expect(decoded.query).toEqual(loadedQuery);
    expect(decoded.seeds).toEqual(["AAA-AAA-BUH", "ABC-DEF-GHI"]);
    expect(decoded.dropped).toBe(0);
  });

  it("always decodes the canonical frozen version-1 fixture", () => {
    const decoded = decodeResultsFile(VERSION_1_FIXTURE);
    expect(decoded.appVersion).toBe("0.6.1");
    // The fixture keeps the version it was written under, not this engine's.
    expect(decoded.shpdVersion).toBe("3.3.8");
    expect(decoded.query).toEqual(loadedQuery);
    expect(decoded.seeds).toEqual(["AAA-AAA-BUH", "ABC-DEF-GHI"]);
  });

  it("accepts the narrowed weapon kinds and keeps them through a round-trip", () => {
    // Widening "melee_weapon"/"thrown_weapon" back to "weapon" on either side
    // would silently change the query.
    const decoded = decodeResultsFile(WEAPON_CATEGORIES_FIXTURE);
    expect(decoded.query.requirements.map((requirement) => requirement.kind)).toEqual([
      "thrown_weapon",
      "melee_weapon",
      "weapon",
    ]);
    expect(decoded.query.requirements[1].item).toBe("sword");
    expect(decoded.seeds).toEqual(["AAA-AAA-ACO"]);
    const reEncoded = decodeResultsFile(encodeResultsFile(decoded.queryDocument, decoded.seeds));
    expect(reEncoded.query).toEqual(decoded.query);
  });

  it("decodes the canonical frozen Wandmaker-quest fixture", () => {
    const decoded = decodeResultsFile(WANDMAKER_QUEST_FIXTURE);
    expect(decoded.query.wandmakerQuest).toBe("rotberry");
    expect(decoded.query.maxDepth).toBe(9);
    expect(decoded.seeds).toEqual(["AAA-AAA-BUH", "ABC-DEF-GHI"]);
  });

  it("round-trips a query that carries a Wandmaker quest", () => {
    const quested: QueryState = { ...loadedQuery, wandmakerQuest: "corpse_dust" };
    const text = encodeResultsFile(toQueryDocument(quested), ["AAA-AAA-BUH"]);
    expect(decodeResultsFile(text).query).toEqual(quested);
  });

  it("ignores unknown envelope and per-result fields from future releases", () => {
    const decoded = decodeResultsFile(
      JSON.stringify({
        format: "seed-seeker-results",
        format_version: 1,
        exported_at: "2031-01-01T00:00:00Z",
        future_minor_field: { nested: true },
        query: { requirements: [{ item: "sword" }] },
        results: [{ seed: "AAA-AAA-AAB", future_note: "still fine" }],
      }),
    );
    expect(decoded.seeds).toEqual(["AAA-AAA-AAB"]);
    expect(decoded.query.maxDepth).toBe(24);
  });

  it("reports the engine dedupe-and-cap drop count", () => {
    const decoded = decodeResultsFile(
      file({ requirements: [{ item: "sword" }] }, [
        { seed: "AAA-AAA-AAC" },
        { seed: "AAA-AAA-AAB" },
        { seed: "AAA-AAA-AAC" },
      ]),
    );
    expect(decoded.seeds).toEqual(["AAA-AAA-AAC", "AAA-AAA-AAB"]);
    expect(decoded.dropped).toBe(1);
  });

  it("surfaces the engine message for a malformed file", () => {
    for (const text of ["not json", "[]", "{}", '{"format":"other"}']) {
      expect(() => decodeResultsFile(text), text).toThrowError(/not a Seed Seeker results file/i);
    }
    // Unusable query content and non-canonical seed codes are the engine's
    // verdict too, reported with its own wording.
    expect(() =>
      decodeResultsFile(file({ requirements: [{ item: "item_from_the_future" }] })),
    ).toThrowError(/item_from_the_future/);
    expect(() =>
      decodeResultsFile(file({ requirements: [{ item: "sword" }], wished_luck: 7 })),
    ).toThrowError(/wished_luck/);
    expect(() =>
      decodeResultsFile(file({ requirements: [{ item: "sword" }] }, [{ seed: "aaa-aaa-aab" }])),
    ).toThrowError(/result 1/i);
  });

  it("round-trips a minimal query with no results", () => {
    const query: QueryState = {
      ...defaultQueryState(),
      requirements: [
        {
          kind: "wand",
          tier: { mode: "any", value: 3 },
          upgrade: { mode: "any", value: 1 },
          uncursed: false,
        },
      ],
    };
    const decoded = decodeResultsFile(encodeResultsFile(toQueryDocument(query), []));
    expect(decoded.query).toEqual(query);
    expect(decoded.seeds).toEqual([]);
  });

  it("round-trips alternative groups, effect sets and combined levels through the engine codec", () => {
    const base = {
      tier: { mode: "any" as const, value: 3 },
      upgrade: { mode: "any" as const, value: 1 },
      uncursed: false,
    };
    const query: QueryState = {
      ...defaultQueryState(),
      requirements: [
        {
          ...base,
          kind: "weapon",
          item: "spear",
          upgrade: { mode: "exact", value: 3 },
          alternativeGroup: 1,
        },
        {
          ...base,
          kind: "thrown_weapon",
          item: "shuriken",
          upgrade: { mode: "exact", value: 2 },
          alternativeGroup: 1,
        },
        {
          ...base,
          kind: "weapon",
          item: "greatshield",
          upgrade: { mode: "exact", value: 2 },
          effect: ["Blocking", "Projecting", "Vampiric"],
        },
        { ...base, kind: "armor", effect: "any_enchantment", uncursed: true },
        { ...base, kind: "ring", item: "ring_might", levelSum: { group: 2, atLeast: 4 } },
        { ...base, kind: "ring", item: "ring_might", levelSum: { group: 2, atLeast: 4 } },
      ],
    };
    const decoded = decodeResultsFile(encodeResultsFile(toQueryDocument(query), ["AAA-AAA-BUH"]));
    expect(decoded.query).toEqual(query);
    // The engine writes the document back in the same canonical form the app does.
    expect(decoded.queryDocument).toEqual(toQueryDocument(query));
    expect(decoded.seeds).toEqual(["AAA-AAA-BUH"]);
  });

  it("surfaces the engine verdict on an unattainable combined level and a sum inside any_of", () => {
    expect(() =>
      decodeResultsFile(
        file({
          requirements: [
            { item: "ring_might", level_sum: { group: 1, at_least: 11 } },
            { item: "ring_might", level_sum: { group: 1, at_least: 11 } },
          ],
        }),
      ),
    ).toThrowError(/11|level|total/i);
    expect(() =>
      decodeResultsFile(
        file({
          requirements: [
            {
              any_of: [
                { item: "ring_might", level_sum: { group: 1, at_least: 2 } },
                { item: "ring_haste" },
              ],
            },
          ],
        }),
      ),
    ).toThrowError(/any_of|alternative|sum/i);
  });
});
