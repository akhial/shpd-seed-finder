import { describe, expect, it } from "vite-plus/test";
import { fromQueryJson, toQueryDocument, toQueryJson, validateQuery } from "../../lib/query";
import type { QueryState, RequirementState } from "../../lib/wasm/types";
import {
  applyEdit,
  boardItems,
  canStack,
  copyDepthOf,
  detach,
  joinAlternatives,
  removeItem,
  removeMember,
  setStackCount,
  setStackTotal,
  stackCount,
} from "./relations";

const req = (patch: Partial<RequirementState> = {}): RequirementState => ({
  kind: "weapon",
  tier: { mode: "any", value: 3 },
  upgrade: { mode: "any", value: 1 },
  uncursed: false,
  ...patch,
});

const asState = (requirements: RequirementState[]): QueryState => ({
  ...fromQueryJson('{"requirements":[]}'),
  requirements,
});
const names = (requirements: RequirementState[]) => requirements.map((r) => r.item ?? r.kind);
const item = (requirements: RequirementState[], index: number) => {
  const found = boardItems(requirements).find((entry) => entry.members.includes(index));
  if (!found) throw new Error(`no board item holds ${index}`);
  return found;
};

describe("either/or clusters", () => {
  it("dropping a chip on another makes one any_of slot, placed after the target", () => {
    const base = [req({ item: "spear" }), req({ kind: "armor" }), req({ item: "shuriken" })];
    const next = joinAlternatives(base, 2, 0);
    expect(names(next)).toEqual(["spear", "shuriken", "armor"]);
    expect(next[0].alternativeGroup).toBe(next[1].alternativeGroup);
    expect(boardItems(next).map((entry) => entry.members)).toEqual([[0, 1], [2]]);
    expect(toQueryDocument({ ...asState(next) }).requirements[0]).toHaveProperty("any_of");
  });

  it("joining a cluster drops a combined level, and leaving a pair dissolves it", () => {
    const base = [
      req({ item: "ring_might", kind: "ring", levelSum: { group: 1, atLeast: 3 } }),
      req({ item: "ring_might", kind: "ring", levelSum: { group: 1, atLeast: 3 } }),
      req({ item: "shuriken" }),
    ];
    const next = joinAlternatives(base, 0, 2);
    expect(next.every((r) => r.levelSum === undefined)).toBe(true);
    const out = detach(
      next,
      next.findIndex((r) => r.item === "shuriken"),
    );
    expect(out.every((r) => r.alternativeGroup === undefined)).toBe(true);
  });
});

describe("stacks", () => {
  it("a concrete stack encodes as plain repeats, no identity group", () => {
    const base = [
      req({ item: "ring_might", kind: "ring", upgrade: { mode: "exact", value: 2 } }),
      req({ kind: "wand" }),
    ];
    const next = setStackCount(base, item(base, 0), 3);
    expect(next).toHaveLength(4);
    expect(next.filter((r) => r.item === "ring_might")).toHaveLength(3);
    expect(next.every((r) => r.identityGroup === undefined)).toBe(true);
    // The board folds the repeats back into one ×3 chip.
    const board = boardItems(next);
    expect(board).toHaveLength(2);
    expect(stackCount(board[0])).toBe(3);
    expect(board[0].total).toBeUndefined();
    expect(validateQuery(asState(next)).valid).toBe(true);
    // The round trip through the document keeps the stack.
    const reloaded = fromQueryJson(toQueryJson(asState(next)));
    expect(stackCount(boardItems(reloaded.requirements)[0])).toBe(3);
  });

  it("a wildcard stack encodes as bare copies sharing an identity group", () => {
    const base = [req({ kind: "wand", upgrade: { mode: "at_least", value: 1 } })];
    const next = setStackCount(base, item(base, 0), 3);
    expect(next).toHaveLength(3);
    expect(new Set(next.map((r) => r.identityGroup)).size).toBe(1);
    expect(next[0].identityGroup).toBe(1);
    expect(
      next
        .slice(1)
        .every((r) => r.kind === "wand" && r.item === undefined && r.upgrade.mode === "any"),
    ).toBe(true);
    expect(validateQuery(asState(next)).valid).toBe(true);
    expect(stackCount(boardItems(next)[0])).toBe(3);
    // Shrinking to one dissolves the group entirely.
    const shrunk = setStackCount(next, item(next, 0), 1);
    expect(shrunk).toHaveLength(1);
    expect(shrunk[0].identityGroup).toBeUndefined();
  });

  it("an either/or cluster anchors a stack: every member carries the label", () => {
    const base = joinAlternatives(
      [req({ item: "runic_blade" }), req({ item: "war_hammer" })],
      1,
      0,
    );
    const next = setStackCount(base, item(base, 0), 3);
    expect(next).toHaveLength(4);
    expect(next.filter((r) => r.identityGroup === 1)).toHaveLength(4);
    expect(next.filter((r) => r.alternativeGroup !== undefined)).toHaveLength(2);
    expect(validateQuery(asState(next)).valid).toBe(true);
    const board = boardItems(next);
    expect(board).toHaveLength(1);
    expect(board[0].cluster).toBeDefined();
    expect(stackCount(board[0])).toBe(3);
    // Removing one cluster member keeps the stack on the survivor.
    const dissolved = removeMember(next, 1);
    expect(boardItems(dissolved)).toHaveLength(1);
    expect(stackCount(boardItems(dissolved)[0])).toBe(3);
    expect(validateQuery(asState(dissolved)).valid).toBe(true);
  });

  it("a plain-repeat stack trades its copies for labels when it joins a cluster", () => {
    const base = setStackCount(
      [req({ item: "spear" }), req({ item: "mace" })],
      item([req({ item: "spear" }), req({ item: "mace" })], 0),
      2,
    );
    const next = joinAlternatives(
      base,
      base.findIndex((r) => r.item === "mace"),
      0,
    );
    // The copy is now a bare weapon tied to the whole cluster.
    const bare = next.filter((r) => r.item === undefined);
    expect(bare).toHaveLength(1);
    expect(bare[0].identityGroup).toBeDefined();
    expect(
      next
        .filter((r) => r.alternativeGroup !== undefined)
        .every((r) => r.identityGroup === bare[0].identityGroup),
    ).toBe(true);
    expect(validateQuery(asState(next)).valid).toBe(true);
  });

  it("deleting the anchor deletes its copies and leaves no stale groups", () => {
    const wildcard = setStackCount(
      [req({ kind: "wand" }), req({ kind: "armor" })],
      item([req({ kind: "wand" }), req({ kind: "armor" })], 0),
      3,
    );
    const afterWildcard = removeItem(wildcard, item(wildcard, 0));
    expect(afterWildcard).toHaveLength(1);
    expect(afterWildcard[0].kind).toBe("armor");
    expect(afterWildcard.every((r) => r.identityGroup === undefined)).toBe(true);

    const total = setStackTotal(
      setStackCount(
        [req({ item: "ring_might", kind: "ring" })],
        item([req({ item: "ring_might", kind: "ring" })], 0),
        2,
      ),
      item(
        setStackCount(
          [req({ item: "ring_might", kind: "ring" })],
          item([req({ item: "ring_might", kind: "ring" })], 0),
          2,
        ),
        0,
      ),
      3,
    );
    const afterTotal = removeItem(total, item(total, 0));
    expect(afterTotal).toHaveLength(0);
  });

  it("ejecting a member from a stacked cluster strips its label", () => {
    let base = joinAlternatives([req({ item: "spear" }), req({ item: "mace" })], 1, 0);
    base = setStackCount(base, item(base, 0), 2);
    const ejected = detach(base, 0);
    const spear = ejected.find((r) => r.item === "spear");
    expect(spear?.alternativeGroup).toBeUndefined();
    expect(spear?.identityGroup).toBeUndefined();
    expect(validateQuery(asState(ejected)).valid).toBe(true);
  });
});

describe("combined levels", () => {
  it("a total turns the stack into identical optional members", () => {
    let base = [req({ item: "ring_might", kind: "ring", upgrade: { mode: "exact", value: 2 } })];
    base = setStackCount(base, item(base, 0), 2);
    const next = setStackTotal(base, item(base, 0), 3);
    expect(next).toHaveLength(2);
    expect(next.every((r) => r.levelSum?.group === 1 && r.levelSum.atLeast === 3)).toBe(true);
    // The total speaks for the stack: per-member upgrades reset to any.
    expect(next.every((r) => r.upgrade.mode === "any")).toBe(true);
    const board = boardItems(next);
    expect(board).toHaveLength(1);
    expect(board[0].total).toBe(3);
    expect(stackCount(board[0])).toBe(2);
    expect(JSON.parse(toQueryJson(asState(next))).requirements[0].level_sum).toEqual({
      group: 1,
      at_least: 3,
    });
    expect(validateQuery(asState(next)).valid).toBe(true);
    // Clearing the total returns to plain repeats.
    const cleared = setStackTotal(next, boardItems(next)[0], undefined);
    expect(cleared.every((r) => r.levelSum === undefined)).toBe(true);
    expect(stackCount(boardItems(cleared)[0])).toBe(2);
  });

  it("a loaded level-sum document collapses back into one chip", () => {
    const state = fromQueryJson(
      '{"requirements":[' +
        '{"kind":"ring","item":"ring_might","level_sum":{"group":2,"at_least":4}},' +
        '{"kind":"ring","item":"ring_might","level_sum":{"group":2,"at_least":4}},' +
        '{"kind":"wand"}]}',
    );
    const board = boardItems(state.requirements);
    expect(board).toHaveLength(2);
    expect(board[0].total).toBe(4);
    expect(stackCount(board[0])).toBe(2);
  });
});

describe("the editor round trip", () => {
  it("applies count and total from the editor and rebuilds the stack", () => {
    let requirements = applyEdit([], null, req({ item: "ring_might", kind: "ring" }), 2, 3);
    expect(requirements).toHaveLength(2);
    expect(requirements.every((r) => r.levelSum?.atLeast === 3)).toBe(true);
    // Raising the count keeps the total; clearing it returns plain repeats.
    requirements = applyEdit(requirements, 0, requirements[0], 3, 5);
    expect(requirements).toHaveLength(3);
    expect(requirements.every((r) => r.levelSum?.atLeast === 5)).toBe(true);
    requirements = applyEdit(requirements, 0, requirements[0], 2, undefined);
    expect(requirements).toHaveLength(2);
    expect(requirements.every((r) => r.levelSum === undefined)).toBe(true);
    expect(requirements.filter((r) => r.item === "ring_might")).toHaveLength(2);
    expect(validateQuery(asState(requirements)).valid).toBe(true);
  });

  it("rebuilds the copies when the edit changes the anchor's category", () => {
    let requirements = applyEdit([], null, req({ kind: "wand" }), 3, undefined);
    expect(requirements.every((r) => r.kind === "wand")).toBe(true);
    // The old copies named wands; the edited chip asks for rings, so the
    // stack comes down and is rebuilt rather than keeping stale wands.
    requirements = applyEdit(requirements, 0, req({ kind: "ring" }), 3, undefined);
    expect(requirements).toHaveLength(3);
    expect(requirements.every((r) => r.kind === "ring")).toBe(true);
    expect(validateQuery(asState(requirements)).valid).toBe(true);
  });

  it("shrinking a level-sum stack from the editor drops its orphaned members", () => {
    let requirements = applyEdit([], null, req({ item: "ring_might", kind: "ring" }), 3, 4);
    expect(requirements).toHaveLength(3);
    requirements = applyEdit(
      requirements,
      0,
      req({ item: "ring_might", kind: "ring" }),
      1,
      undefined,
    );
    expect(requirements).toHaveLength(1);
    expect(requirements[0].levelSum).toBeUndefined();
  });
});

describe("categories", () => {
  it("a stack does not follow its chip into a cluster of another category", () => {
    // A copy has to name the kind it copies, and "ring or wand" names none, so
    // the second ring stays the standalone chip it already encodes as.
    let requirements = applyEdit([], null, req({ item: "ring_might", kind: "ring" }), 2, undefined);
    requirements = [...requirements, req({ kind: "wand" })];
    const joined = joinAlternatives(requirements, 0, 2);
    expect(joined.some((r) => r.identityGroup !== undefined)).toBe(false);
    expect(validateQuery(asState(joined)).valid).toBe(true);
    expect(boardItems(joined)).toHaveLength(2);
  });

  it("a wildcard stack lets its copies go when its chip joins another category", () => {
    // The copies were bare wands tied to the anchor by a label; a "wand or
    // spear" cluster is nothing they can be copies of, so they are dropped
    // rather than left behind as a stack the engine would refuse.
    let requirements = applyEdit([], null, req({ kind: "wand" }), 3, undefined);
    requirements = [...requirements, req({ item: "spear" })];
    const joined = joinAlternatives(requirements, 0, 3);
    expect(names(joined)).toEqual(["spear", "wand"]);
    expect(joined.some((r) => r.identityGroup !== undefined)).toBe(false);
    expect(joined[0].alternativeGroup).toBe(joined[1].alternativeGroup);
    expect(validateQuery(asState(joined)).valid).toBe(true);
    expect(stackCount(item(joined, 0))).toBe(1);
  });

  it("a cluster spanning two categories cannot grow a stack", () => {
    const requirements = joinAlternatives([req({ kind: "wand" }), req({ item: "spear" })], 0, 1);
    const cluster = item(requirements, 0);
    expect(cluster.members).toHaveLength(2);
    expect(canStack(requirements, cluster)).toBe(false);
    expect(setStackCount(requirements, cluster, 2)).toBe(requirements);
    // A cluster of one category is still free to stack.
    const weapons = joinAlternatives([req({ item: "mace" }), req({ item: "spear" })], 0, 1);
    expect(canStack(weapons, item(weapons, 0))).toBe(true);
  });
});

describe("copy floor limits", () => {
  it("the anchor and its copies carry independent floor limits", () => {
    const requirements = applyEdit(
      [],
      null,
      req({
        item: "plate_armor",
        kind: "armor",
        upgrade: { mode: "exact", value: 3 },
        maxDepth: 4,
      }),
      2,
      undefined,
      9,
    );
    expect(requirements).toHaveLength(2);
    expect(requirements[0].maxDepth).toBe(4);
    expect(requirements[1].maxDepth).toBe(9);
    // Still one chip: a repeat with only a floor limit folds into its stack.
    const board = boardItems(requirements);
    expect(board).toHaveLength(1);
    expect(stackCount(board[0])).toBe(2);
    expect(copyDepthOf(requirements, board[0])).toBe(9);
    expect(validateQuery(asState(requirements)).valid).toBe(true);
    // The round trip through the document keeps both limits.
    const reloaded = fromQueryJson(toQueryJson(asState(requirements))).requirements;
    expect(reloaded.map((r) => r.maxDepth)).toEqual([4, 9]);
    expect(boardItems(reloaded)).toHaveLength(1);
  });

  it("unlimited copies stay unlimited while the anchor is floor-bound", () => {
    const requirements = applyEdit(
      [],
      null,
      req({ kind: "armor", upgrade: { mode: "exact", value: 3 }, maxDepth: 4 }),
      2,
      undefined,
      undefined,
    );
    expect(requirements[0].maxDepth).toBe(4);
    expect(requirements[1].maxDepth).toBeUndefined();
    expect(requirements[1].identityGroup).toBe(requirements[0].identityGroup);
    expect(validateQuery(asState(requirements)).valid).toBe(true);
  });

  it("a wildcard stack limits its bare copies without constraining them otherwise", () => {
    let requirements = applyEdit(
      [],
      null,
      req({ kind: "wand", upgrade: { mode: "at_least", value: 2 } }),
      2,
      undefined,
      9,
    );
    expect(requirements.slice(1).every((r) => r.maxDepth === 9 && r.upgrade.mode === "any")).toBe(
      true,
    );
    expect(validateQuery(asState(requirements)).valid).toBe(true);
    // Growing the stack from the chip badge keeps the copies' floor.
    requirements = setStackCount(requirements, item(requirements, 0), 3);
    expect(requirements).toHaveLength(3);
    expect(requirements.slice(1).every((r) => r.maxDepth === 9)).toBe(true);
  });

  it("editing away the limit clears it from every copy", () => {
    let requirements = applyEdit([], null, req({ item: "longsword" }), 3, undefined, 6);
    expect(requirements.slice(1).every((r) => r.maxDepth === 6)).toBe(true);
    requirements = applyEdit(requirements, 0, req({ item: "longsword" }), 3, undefined, undefined);
    expect(requirements.every((r) => r.maxDepth === undefined)).toBe(true);
  });

  it("the copies keep their floor when the stack follows its chip into a cluster", () => {
    let requirements = applyEdit(
      [],
      null,
      req({ item: "ring_might", kind: "ring" }),
      2,
      undefined,
      7,
    );
    requirements = [...requirements, req({ item: "ring_haste", kind: "ring" })];
    const joined = joinAlternatives(requirements, 0, 2);
    const copy = joined.find((r) => r.item === undefined);
    expect(copy?.maxDepth).toBe(7);
    expect(validateQuery(asState(joined)).valid).toBe(true);
  });
});
