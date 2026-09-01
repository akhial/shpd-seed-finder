import { describe, expect, it } from "vite-plus/test";
import {
  applyProgress,
  calculateRate,
  importedResultsState,
  initialCoordinatorState,
  markWorkerDone,
  mergeMatches,
  runSaturated,
  settleRun,
  type CoordinatorState,
} from "./coordinator-state";

const match = (value: number) => ({ value, code: value.toString().padStart(9, "A") });

describe("coordinator aggregation", () => {
  it("merges and sorts batches while dropping duplicate seeds", () => {
    expect(
      mergeMatches([match(4), match(2)], [match(3), match(2)]).matches.map((item) => item.value),
    ).toEqual([2, 3, 4]);
  });
  it("reports the cap without evicting any delivered match", () => {
    // Every delivered match belongs to a region recorded as scanned; evicting
    // one at the cap would silently lose it for a later refine.
    const merged = mergeMatches(
      [],
      Array.from({ length: 1_030 }, (_, value) => match(value)),
    );
    expect(merged.matches).toHaveLength(1_030);
    expect(merged.capped).toBe(true);
  });
  it("does not report the cap when duplicates collapse below it", () => {
    const seeds = Array.from({ length: 1_000 }, (_, value) => match(value));
    const merged = mergeMatches(seeds, seeds);
    expect(merged.matches).toHaveLength(1_000);
    expect(merged.capped).toBe(false);
  });
  it("calculates rate over synthetic progress samples", () => {
    expect(
      calculateRate([
        { at: 1_000, tested: 2_000 },
        { at: 3_000, tested: 8_000 },
      ]),
    ).toBe(3_000);
  });
  it("tracks per-segment scan positions and sums them into tested", () => {
    const state = {
      ...initialCoordinatorState(100),
      state: "running" as const,
      sessionId: 3,
      workerCount: 2,
      startedAt: 1_000,
    };
    const updated = applyProgress(state, {
      sessionId: 3,
      workerId: 0,
      scanned: [40, 5],
      matches: [match(1)],
      now: 2_000,
    });
    expect(updated.workerScanned[0]).toEqual([40, 5]);
    expect(updated.tested).toBe(45);
  });
  it("ignores stale session progress", () => {
    const state = {
      ...initialCoordinatorState(100),
      state: "running" as const,
      sessionId: 3,
      workerCount: 1,
      startedAt: 1_000,
    };
    const updated = applyProgress(state, {
      sessionId: 2,
      workerId: 0,
      scanned: [10],
      matches: [match(1)],
      now: 2_000,
    });
    expect(updated).toBe(state);
  });
  it("replaces state with imported results in file order and keeps the query snapshot", () => {
    const previous = {
      ...initialCoordinatorState(100),
      state: "completed" as const,
      sessionId: 5,
      tested: 42,
      matches: [match(9)],
    };
    const imported = importedResultsState(previous, [match(3), match(1)], {
      requirements: [{ kind: "wand" }],
    });
    expect(imported.state).toBe("imported");
    expect(imported.sessionId).toBe(5);
    expect(imported.tested).toBe(0);
    expect(imported.matches.map((item) => item.value)).toEqual([3, 1]);
    expect(imported.capped).toBe(false);
    expect(imported.importedDropped).toBe(0);
    expect(imported.query).toEqual({ requirements: [{ kind: "wand" }] });
  });
  it("reports the engine's dropped count and the cap without re-deriving either", () => {
    // The decoder already deduplicated and capped; this only reports what it
    // removed, so the drop count is never recomputed from the kept list.
    const deduped = importedResultsState(
      initialCoordinatorState(100),
      [match(3), match(1)],
      { requirements: [] },
      1,
    );
    expect(deduped.matches.map((item) => item.value)).toEqual([3, 1]);
    expect(deduped.importedDropped).toBe(1);
    expect(deduped.capped).toBe(false);

    const imported = importedResultsState(
      initialCoordinatorState(100),
      Array.from({ length: 1_024 }, (_, value) => match(value)),
      { requirements: [] },
      6,
    );
    expect(imported.matches).toHaveLength(1_024);
    expect(imported.capped).toBe(true);
    expect(imported.importedDropped).toBe(6);
  });
});

describe("stop bookkeeping", () => {
  const running = () => ({
    ...initialCoordinatorState(100),
    state: "running" as const,
    sessionId: 3,
    workerCount: 2,
    startedAt: 1_000,
  });

  it("accepts the final progress flush while stopping", () => {
    const stopping = { ...running(), state: "stopping" as const };
    const updated = applyProgress(stopping, {
      sessionId: 3,
      workerId: 0,
      scanned: [40],
      matches: [match(7)],
      now: 2_000,
    });
    expect(updated.workerScanned[0]).toEqual([40]);
    expect(updated.matches.map((item) => item.value)).toEqual([7]);
    expect(updated.state).toBe("stopping");
  });

  it("records exact final worker positions and cancels once all workers stop", () => {
    let state: CoordinatorState = { ...running(), state: "stopping" };
    state = markWorkerDone(state, {
      sessionId: 3,
      workerId: 0,
      scanned: [55],
      kind: "stopped",
      now: 2_000,
    });
    expect(state.state).toBe("stopping");
    state = markWorkerDone(state, {
      sessionId: 3,
      workerId: 1,
      scanned: [30, 15],
      kind: "done",
      now: 2_100,
    });
    expect(state.state).toBe("cancelled");
    expect(state.workerScanned).toEqual({ 0: [55], 1: [30, 15] });
    expect(state.tested).toBe(100);
  });

  it("completes a running search when every worker reports done", () => {
    let state: CoordinatorState = running();
    state = markWorkerDone(state, {
      sessionId: 3,
      workerId: 0,
      scanned: [50],
      kind: "done",
      now: 2_000,
    });
    state = markWorkerDone(state, {
      sessionId: 3,
      workerId: 1,
      scanned: [50],
      kind: "done",
      now: 2_100,
    });
    expect(state.state).toBe("completed");
  });

  it("ignores terminal reports from stale sessions", () => {
    const state = running();
    expect(
      markWorkerDone(state, { sessionId: 2, workerId: 0, scanned: [10], kind: "done", now: 2_000 }),
    ).toBe(state);
  });
});

describe("settleRun", () => {
  const query = { requirements: [{ kind: "ring" as const }] };
  const concluded = (overrides: Partial<CoordinatorState>): CoordinatorState => ({
    ...initialCoordinatorState(1_000),
    state: "completed",
    query,
    queryJson: JSON.stringify(query),
    matches: [match(11), match(22)],
    segments: [[{ startSeed: 0, endSeedExclusive: 1_000 }]],
    workerScanned: { 0: [400] },
    ...overrides,
  });

  it("establishes the Target from a concluded anchor run", () => {
    const settled = settleRun(concluded({ runKind: "anchor" }));
    expect(settled.target).toEqual({
      queryJson: JSON.stringify(query),
      query,
      matches: [match(11), match(22)],
      remainder: [{ startSeed: 400, endSeedExclusive: 1_000 }],
    });
  });

  it("grows the Target Set and advances coverage after a target refine", () => {
    const target = {
      queryJson: JSON.stringify(query),
      query,
      matches: [match(11), match(22), match(33)],
      remainder: [{ startSeed: 400, endSeedExclusive: 1_000 }],
    };
    // The refined run kept 11 and found 44 while scanning 400..700 of the remainder.
    const settled = settleRun(
      concluded({
        runKind: "target-refine",
        target,
        matches: [match(11), match(44)],
        segments: [[{ startSeed: 400, endSeedExclusive: 1_000 }]],
        workerScanned: { 0: [300] },
      }),
    );
    expect(settled.target?.matches.map((item) => item.value)).toEqual([11, 22, 33, 44]);
    expect(settled.target?.remainder).toEqual([{ startSeed: 700, endSeedExclusive: 1_000 }]);
    expect(settled.target?.query).toEqual(query);
  });

  it("leaves the Target alone after a target filter or a detached run", () => {
    const target = { queryJson: JSON.stringify(query), query, matches: [match(11)], remainder: [] };
    for (const runKind of ["target-filter", "detached"] as const) {
      const settled = settleRun(concluded({ runKind, target, matches: [match(99)] }));
      expect(settled.target).toBe(target);
    }
  });

  it("does nothing for a run that has not concluded, or that failed", () => {
    for (const state of ["running", "stopping", "failed", "idle"] as const) {
      const input = concluded({ state, runKind: "anchor" });
      expect(settleRun(input).target).toBeUndefined();
    }
  });
});

describe("imported results as Target", () => {
  it("makes the imported query and seeds the Target with no coverage", () => {
    const state = importedResultsState(initialCoordinatorState(1_000), [match(5), match(6)], {
      requirements: [{ kind: "wand" }],
    });
    expect(state.target?.matches.map((item) => item.value)).toEqual([5, 6]);
    expect(state.target?.query).toEqual({ requirements: [{ kind: "wand" }] });
    expect(state.target?.remainder).toEqual([]);
  });
});

describe("per-run accept quota", () => {
  it("keeps a refine running past the display cap until it adds a full quota of new finds", () => {
    // 1,024 survivors already fill the display; the resumed scan must still
    // run until it has found RESULT_CAP additional seeds.
    const survivors = Array.from({ length: 1_024 }, (_, value) => match(value));
    const base: CoordinatorState = {
      ...initialCoordinatorState(1_000_000),
      state: "running",
      sessionId: 1,
      workerCount: 1,
      startedAt: 0,
      matches: survivors,
      sessionBaseline: survivors.length,
      capped: true,
      runKind: "target-refine",
    };
    const partial = applyProgress(base, {
      sessionId: 1,
      workerId: 0,
      scanned: [500],
      matches: Array.from({ length: 500 }, (_, value) => match(2_000 + value)),
      now: 1_000,
    });
    expect(partial.state).toBe("running");
    expect(runSaturated(partial)).toBe(false);

    const saturated = applyProgress(partial, {
      sessionId: 1,
      workerId: 0,
      scanned: [1_100],
      matches: Array.from({ length: 524 }, (_, value) => match(3_000 + value)),
      now: 2_000,
    });
    expect(runSaturated(saturated)).toBe(true);
    expect(saturated.state).toBe("completed");
    expect(saturated.matches).toHaveLength(2_048);
  });

  it("still ends a fresh scan at the cap, whose baseline is zero", () => {
    const base: CoordinatorState = {
      ...initialCoordinatorState(1_000_000),
      state: "running",
      sessionId: 1,
      workerCount: 1,
      startedAt: 0,
    };
    const updated = applyProgress(base, {
      sessionId: 1,
      workerId: 0,
      scanned: [5_000],
      matches: Array.from({ length: 1_024 }, (_, value) => match(value)),
      now: 1_000,
    });
    expect(updated.state).toBe("completed");
  });
});
