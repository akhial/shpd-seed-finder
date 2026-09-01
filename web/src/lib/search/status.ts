import type { CoordinatorState } from "./coordinator-state";

/** One relocated status note: refine progress, the detached-scan notice, or
 * the result-cap notice. `kind` lets the footer tint the cap warning without
 * re-parsing the text. */
export interface StatusNote {
  kind: "refine" | "detached" | "cap";
  text: string;
}

const plural = (count: number) => (count === 1 ? "" : "s");

/**
 * The transient status notes that live outside the results pane: desktop
 * layouts render them joined in the footer's status region, small layouts
 * surface changes as a snackbar. The "Impossible query" warning and the live
 * search stats are not status notes and stay in their panes.
 */
export function searchStatusNotes(state: CoordinatorState): StatusNote[] {
  const notes: StatusNote[] = [];
  const running = state.state === "running" || state.state === "stopping";
  if (running && state.filtering && state.refined) {
    notes.push({
      kind: "refine",
      text: `Verifying ${state.refined.of.toLocaleString()} previously found seed${plural(state.refined.of)}…`,
    });
  } else if (running && state.refined) {
    notes.push({
      kind: "refine",
      text: `Kept ${state.refined.kept.toLocaleString()} of ${state.refined.of.toLocaleString()} previous seed${plural(state.refined.of)} — searching for more…`,
    });
  } else if (state.refined && (state.state === "completed" || state.state === "cancelled")) {
    notes.push({
      kind: "refine",
      text: `Refined: kept ${state.refined.kept.toLocaleString()} of ${state.refined.of.toLocaleString()} previous seed${plural(state.refined.of)}.`,
    });
  }
  // A fresh detached scan is the one moment the display and the kept results
  // diverge, so say what happened to them. A continued detached scan tells
  // its own story through the refined note above.
  if (state.runKind === "detached" && state.target && !state.refined && state.state !== "idle") {
    notes.push({
      kind: "detached",
      text: "Unrelated query — detached search from previous results.",
    });
  }
  // The cap notice only speaks once a run has concluded: while an
  // accumulating scan runs, a full display is the expected state ("searching
  // for more" says what is happening), and during the filter phase `capped`
  // still describes the previous run.
  if (state.capped && !running)
    notes.push({ kind: "cap", text: "Result limit reached (1,024 seeds)." });
  return notes;
}
