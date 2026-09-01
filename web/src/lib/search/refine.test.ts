import { readFile } from "node:fs/promises";
import { beforeAll, describe, expect, it } from "vite-plus/test";
import init from "../wasm/pkg/seedfinder.js";
import type { ParsedSeed, QueryDocument } from "../wasm/types";
import { defaultQueryState, toQueryDocument } from "../query";
import {
  decideStart,
  distributeSegments,
  isContinuationOf,
  remainingSegments,
  segmentsLength,
} from "./refine";
import {
  initialCoordinatorState,
  type CoordinatorState,
  type SearchStatus,
  type TargetState,
} from "./coordinator-state";
import type { SeedRange } from "./traversal";

/**
 * The continuation rule lives in the engine, so these are conformance tests
 * against the real wasm module rather than against a TypeScript restatement
 * of it. Node has no `fetch` for `file:` URLs, so the module is instantiated
 * from bytes instead of the browser's URL form.
 */
beforeAll(async () => {
  await init({
    module_or_path: await readFile(new URL("../wasm/pkg/seedfinder_bg.wasm", import.meta.url)),
  });
});

const base: QueryDocument = {
  requirements: [{ kind: "ring", upgrade: { at_least: 2 } }],
};
const added: QueryDocument = {
  requirements: [
    { kind: "ring", upgrade: { at_least: 2 } },
    { kind: "weapon", upgrade: 3 },
  ],
};

describe("isContinuationOf", () => {
  it("accepts adding a requirement with identical scope", () => {
    expect(isContinuationOf(added, base)).toBe(true);
  });
  it("accepts reordered requirements and reordered keys", () => {
    const reordered: QueryDocument = {
      requirements: [
        { upgrade: 3, kind: "weapon" },
        { upgrade: { at_least: 2 }, kind: "ring" },
      ],
    };
    expect(isContinuationOf(reordered, base)).toBe(true);
  });
  it("accepts an unchanged query, which continues the run rather than restarting it", () => {
    expect(isContinuationOf(base, base)).toBe(true);
    // Equality is judged on content, not on key or requirement order.
    expect(
      isContinuationOf({ requirements: [{ upgrade: { at_least: 2 }, kind: "ring" }] }, base),
    ).toBe(true);
    expect(isContinuationOf(added, added)).toBe(true);
  });
  it("accepts strengthened requirements: the narrower query only matches seeds the base already found", () => {
    // Naming the ring, or raising its bound, keeps every candidate match
    // inside the base run's matches — the exact shape of the reported
    // "specific ring after any-ring runs" stall, which must refine and
    // resume rather than filter and stop.
    expect(
      isContinuationOf(
        { requirements: [{ kind: "ring", item: "ring_arcana", upgrade: { at_least: 2 } }] },
        base,
      ),
    ).toBe(true);
    expect(
      isContinuationOf(
        { requirements: [{ kind: "ring", upgrade: { at_least: 3 } }, { kind: "weapon" }] },
        base,
      ),
    ).toBe(true);
  });
  it("rejects removed or loosened requirements", () => {
    expect(isContinuationOf({ requirements: [] }, base)).toBe(false);
    expect(
      isContinuationOf(
        { requirements: [{ kind: "ring", upgrade: { at_least: 1 } }, { kind: "weapon" }] },
        base,
      ),
    ).toBe(false);
    expect(isContinuationOf({ requirements: [{ kind: "ring" }] }, base)).toBe(false);
    expect(isContinuationOf(base, added)).toBe(false);
  });
  it("respects requirement multiplicity", () => {
    const twoRings: QueryDocument = { requirements: [base.requirements[0], base.requirements[0]] };
    expect(isContinuationOf(twoRings, base)).toBe(true);
    expect(isContinuationOf(twoRings, twoRings)).toBe(true);
    expect(isContinuationOf(base, twoRings)).toBe(false);
    expect(
      isContinuationOf({ requirements: [base.requirements[0], { kind: "wand" }] }, twoRings),
    ).toBe(false);
  });
  it("rejects a widened scope", () => {
    expect(isContinuationOf({ ...added, max_depth: 9 }, base)).toBe(false);
    expect(isContinuationOf({ ...added, challenges: ["on_diet"] }, base)).toBe(false);
    expect(
      isContinuationOf({ ...added, challenges: ["on_diet"] }, { ...base, challenges: ["on_diet"] }),
    ).toBe(true);
    // Even an otherwise unchanged query restarts when the scope moves.
    expect(isContinuationOf({ ...base, max_depth: 9 }, base)).toBe(false);
  });
  it("ignores the retired fast-mode flag a stored query may still carry", () => {
    // The engine accepts `fast_mode` from documents written before fast mode
    // was removed and ignores it, so the flag can neither block a continuation
    // nor create one: only the requirements and the scope decide.
    expect(isContinuationOf({ ...added, fast_mode: true } as QueryDocument, base)).toBe(true);
    expect(isContinuationOf(added, { ...base, fast_mode: true } as QueryDocument)).toBe(true);
    expect(isContinuationOf({ ...base, fast_mode: true } as QueryDocument, added)).toBe(false);
  });
  it("accepts a narrowed world condition and rejects a relaxed one", () => {
    // The blacksmith flags and the quest filter only remove seeds, so
    // switching one on strengthens the base; switching it off, or swapping the
    // quest for another variant, has to rescan.
    expect(isContinuationOf({ ...added, require_blacksmith: true }, base)).toBe(true);
    expect(isContinuationOf(added, { ...base, require_blacksmith: true })).toBe(false);
    expect(isContinuationOf({ ...added, exclude_blacksmith_rewards: true }, base)).toBe(true);
    expect(isContinuationOf(added, { ...base, exclude_blacksmith_rewards: true })).toBe(false);
    expect(isContinuationOf({ ...added, wandmaker_quest: "rotberry" }, base)).toBe(true);
    expect(
      isContinuationOf(
        { ...added, wandmaker_quest: "rotberry" },
        { ...base, wandmaker_quest: "rotberry" },
      ),
    ).toBe(true);
    expect(isContinuationOf(added, { ...base, wandmaker_quest: "rotberry" })).toBe(false);
    expect(
      isContinuationOf(
        { ...added, wandmaker_quest: "corpse_dust" },
        { ...base, wandmaker_quest: "rotberry" },
      ),
    ).toBe(false);
  });
  it("compares requirements as the engine decodes them, not as they are written", () => {
    // Two spellings of the same requirement are the same requirement, which
    // a structural comparison over the JSON could not see.
    const shorthand: QueryDocument = { requirements: [{ kind: "weapon", upgrade: 3 }] };
    const spelledOut: QueryDocument = {
      requirements: [{ kind: "weapon", upgrade: { exact: 3 }, tier: "any" }],
    };
    expect(isContinuationOf(shorthand, spelledOut)).toBe(true);
    expect(isContinuationOf(spelledOut, shorthand)).toBe(true);
  });
  it("continues nothing when a query does not decode", () => {
    // An unreadable query describes no world set, so there is no coverage
    // argument to make and the only sound answer is a fresh scan.
    expect(isContinuationOf({ requirements: [{ item: "no_such_item" }] }, base)).toBe(false);
    expect(isContinuationOf(base, { requirements: [{ item: "no_such_item" }] })).toBe(false);
  });
});

describe("remainingSegments", () => {
  it("drops each segment's own scanned prefix", () => {
    const segments: SeedRange[][] = [
      [
        { startSeed: 90, endSeedExclusive: 100 },
        { startSeed: 0, endSeedExclusive: 15 },
      ],
      [{ startSeed: 15, endSeedExclusive: 40 }],
    ];
    expect(remainingSegments(segments, { 0: [10, 2], 1: [25] })).toEqual([
      { startSeed: 2, endSeedExclusive: 15 },
    ]);
    expect(remainingSegments(segments, { 0: [3] })).toEqual([
      { startSeed: 93, endSeedExclusive: 100 },
      { startSeed: 0, endSeedExclusive: 15 },
      { startSeed: 15, endSeedExclusive: 40 },
    ]);
    expect(remainingSegments(segments, { 0: [10, 15], 1: [25] })).toEqual([]);
  });

  it("keeps the tail of a segment abandoned at the session result cap", () => {
    // The first segment stopped early (its cooperative session hit the
    // per-session accept cap) while the second segment still completed.
    // A cumulative count would wrongly skip the first segment's tail.
    const segments: SeedRange[][] = [
      [
        { startSeed: 900, endSeedExclusive: 1_000 },
        { startSeed: 0, endSeedExclusive: 60 },
      ],
    ];
    expect(remainingSegments(segments, { 0: [40, 60] })).toEqual([
      { startSeed: 940, endSeedExclusive: 1_000 },
    ]);
  });
});

describe("distributeSegments", () => {
  it("splits ranges into near-equal contiguous shares covering every seed once", () => {
    const ranges: SeedRange[] = [
      { startSeed: 10, endSeedExclusive: 25 },
      { startSeed: 40, endSeedExclusive: 47 },
    ];
    const shares = distributeSegments(ranges, 3);
    expect(shares).toHaveLength(3);
    const flattened = shares
      .flat()
      .flatMap((range) =>
        Array.from(
          { length: range.endSeedExclusive - range.startSeed },
          (_, offset) => range.startSeed + offset,
        ),
      );
    const expected = [
      ...Array.from({ length: 15 }, (_, offset) => 10 + offset),
      ...Array.from({ length: 7 }, (_, offset) => 40 + offset),
    ];
    expect(flattened).toEqual(expected);
    for (const share of shares) {
      expect(Math.abs(segmentsLength(share) - 22 / 3)).toBeLessThanOrEqual(1);
    }
  });
  it("handles more workers than seeds and empty input", () => {
    const shares = distributeSegments([{ startSeed: 5, endSeedExclusive: 7 }], 4);
    expect(shares).toHaveLength(4);
    expect(segmentsLength(shares.flat())).toBe(2);
    expect(distributeSegments([], 3).every((share) => share.length === 0)).toBe(true);
  });
});

describe("decideStart", () => {
  const target = (
    query: QueryDocument,
    matches: ParsedSeed[],
    remainder: SeedRange[],
  ): TargetState => ({
    queryJson: JSON.stringify(query),
    query,
    matches,
    remainder,
  });
  const seeds = [{ value: 1, code: "AAA-AAA-AAB" }];
  const withTarget = (overrides: Partial<CoordinatorState>): CoordinatorState => ({
    ...initialCoordinatorState(1_000),
    state: "completed",
    target: target(base, seeds, [{ startSeed: 400, endSeedExclusive: 1_000 }]),
    ...overrides,
  });

  it("anchors when no Target exists", () => {
    expect(decideStart(initialCoordinatorState(1_000), base)).toBe("anchor");
  });
  it("refines a continuation of the Target Query", () => {
    expect(decideStart(withTarget({}), added)).toBe("target-refine");
    expect(decideStart(withTarget({}), base)).toBe("target-refine");
  });
  it("refines a strengthened Target Query: filter the set, then resume its coverage", () => {
    // Naming a specific ring after any-ring runs must keep scanning the
    // remainder for it, not stop at whatever the filter kept.
    expect(
      decideStart(withTarget({}), {
        requirements: [{ kind: "ring", item: "ring_arcana", upgrade: { at_least: 2 } }],
      }),
    ).toBe("target-refine");
    expect(
      decideStart(withTarget({}), { requirements: [{ kind: "ring", upgrade: { at_least: 3 } }] }),
    ).toBe("target-refine");
  });
  it("filters when the query shares an item without continuing", () => {
    // Loosening the ring's upgrade is not a continuation, but it is still about
    // rings, and filtering from the full Target Set brings seeds back.
    expect(
      decideStart(withTarget({}), { requirements: [{ kind: "ring", upgrade: { at_least: 1 } }] }),
    ).toBe("target-filter");
    // A named ring shares with the kind-level target requirement; scope and
    // challenge differences never affect sharing.
    expect(
      decideStart(withTarget({}), {
        requirements: [{ kind: "ring", item: "ring_wealth" }],
        max_depth: 5,
        challenges: ["on_diet"],
      }),
    ).toBe("target-filter");
  });
  it("does not share on a requirement whose category was left implicit", () => {
    // The browser used to treat a missing kind as a wildcard that shared with
    // every base requirement, so an item-only requirement always filtered the
    // Target Set. The engine compares kinds for equality, and the encoder now
    // always writes one, so a sword query against a ring target detaches.
    expect(decideStart(withTarget({}), { requirements: [{ item: "sword" }] })).toBe("detached");
    expect(
      decideStart(
        withTarget({}),
        toQueryDocument({
          ...defaultQueryState(),
          requirements: [
            {
              item: "sword",
              tier: { mode: "any", value: 3 },
              upgrade: { mode: "any", value: 1 },
              uncursed: false,
            },
          ],
        }),
      ),
    ).toBe("detached");
  });
  it("detaches an unrelated query, continuing a detached run when sound", () => {
    const wands: QueryDocument = { requirements: [{ kind: "wand" }] };
    expect(decideStart(withTarget({}), wands)).toBe("detached");
    const afterDetached = withTarget({
      runKind: "detached",
      queryJson: JSON.stringify(wands),
      matches: seeds,
    });
    expect(decideStart(afterDetached, wands)).toBe("continue-detached");
    expect(
      decideStart(afterDetached, { requirements: [...wands.requirements, { kind: "armor" }] }),
    ).toBe("continue-detached");
    // Only a concluded detached run has a known scanned region to continue.
    for (const state of ["idle", "running", "stopping", "failed", "imported"] as SearchStatus[]) {
      expect(
        decideStart(
          withTarget({
            runKind: "detached",
            queryJson: JSON.stringify(wands),
            matches: seeds,
            state,
          }),
          wands,
        ),
        state,
      ).toBe("detached");
    }
    // A loosened detached query rescans instead of continuing.
    expect(
      decideStart(
        withTarget({
          runKind: "detached",
          queryJson: JSON.stringify({ requirements: [...wands.requirements, { kind: "armor" }] }),
          matches: seeds,
        }),
        wands,
      ),
    ).toBe("detached");
  });
  it("re-anchors on an empty Target Set unless the query resumes its coverage", () => {
    const empty = withTarget({
      target: target(base, [], [{ startSeed: 400, endSeedExclusive: 1_000 }]),
    });
    expect(decideStart(empty, added)).toBe("target-refine");
    expect(decideStart(empty, { requirements: [{ kind: "wand" }] })).toBe("anchor");
    // Fully covered and empty: nothing to resume, nothing to keep.
    const covered = withTarget({ target: target(base, [], []) });
    expect(decideStart(covered, added)).toBe("anchor");
  });
});
