# Search semantics: the Target Set

Every frontend implements the same search lifecycle. The model exists so that
a user's found seeds are never thrown away by starting another search: the
only action that discards results is the explicit **Clear** button.

## Definitions

- **Target Query** — the query of the first search after boot (or after
  Clear, or the query embedded in an imported results file). It stays fixed
  until Clear or a new import.
- **Target Set** — every unique match the Target Query's traversal has
  delivered, uncapped (the display cap only limits what is listed). It grows
  when an extending search scans previously uncovered range; it never
  shrinks. The *displayed* list is capped at 1,024 rows on every platform —
  a run's full result set (survivors plus new finds) can exceed that, and
  the excess must still reach the Target Set and any later refine, just not
  the screen.
- **Target coverage** — the portion of the seed space the target traversal
  has scanned, kept as resume state (uncovered remainder). Imported results
  carry no coverage.
- **World conditions** — the query-level conditions that judge an otherwise
  unchanged world: require an accessible blacksmith, exclude the Smith
  rewards, and demand a Wandmaker quest. Switching one on, or naming a quest
  where none was demanded, only ever *removes* seeds, so one query's
  conditions can be **at least as strict** as another's: every flag the base
  sets is also set, and a base quest is demanded unchanged (an unfiltered base
  accepts any). The floor limit, challenges and fast mode are not world
  conditions — they change which world is generated, or how it is searched.
- **Continuation**: query B continues query A when the floor limit, challenge
  set and fast mode are identical, B's world conditions are at least as
  strict as A's, and every
  requirement of A is covered by a *distinct* requirement of B at least as
  strict — equal, or strengthened: an item named where A wanted any of its
  kind, a tightened upgrade/tier bound, a demanded source, effect, curse
  state, or per-item floor limit. (A plain requirement multiset superset is
  the special case where the covering requirements are equal.) Only then is
  every B-match inside A's covered region already in A's matches, which is
  what makes filter-and-resume sound; loosening any requirement breaks the
  containment and B must rescan. The engine owns this
  predicate — `SearchQuery::continues` in `seedfinder-core`, exposed as
  `seedfinder_query_continues` (C), `JniBindings.queryContinues` (Android)
  and `query_continues` (wasm) — and frontends should call it rather than
  re-derive it.
- **Shares an item**: some requirement of B and some requirement of A have
  the same kind, and either at least one of the two names no specific item or
  both name the same item. Scope and challenge differences are irrelevant
  here — a filter re-verifies seeds from scratch under B.

## Start decision

When Start Search runs query `Q` and a Target exists with a non-empty Target
Set:

1. **`Q` continues the Target Query** → *target refine*: re-verify the whole
   Target Set against `Q` (filter phase), display the survivors, then always
   resume scanning the target's uncovered remainder — even when the
   survivors already fill the display cap. Each resumed scan stops after it
   accepts about `RESULT_CAP` (1,024) *new* finds, the engine's per-session
   accept cap (the cap gates claiming work, so a scan may deliver slightly
   more than the cap but is guaranteed to advance coverage — a resumed pass
   never treads water). New finds match the Target Query by construction, so
   they join the Target Set and the coverage advances; repeating an identical
   query therefore keeps growing the Target Set by roughly a cap's worth of
   seeds per run. An *unsatisfiable* refine completes instantly with its
   coverage untouched: proving no seed can match consumes none of the
   remainder, so removing the impossible requirement later resumes where the
   target actually stopped.
2. **`Q` shares an item with the Target Query** → *target filter*: re-verify
   the whole Target Set against `Q` and display the survivors. No scanning;
   the Target Set and its coverage are untouched. Because the base is always
   the full Target Set — not the last run's survivors — loosening a
   requirement brings seeds back.
3. **Otherwise (unrelated)** → *detached scan*: a fresh full-range scan whose
   results replace the display, while the Target Query/Set/coverage are kept
   untouched for later related searches. If the previous run was itself a
   detached scan that `Q` continues, continue it (filter its results, resume
   its remainder) instead of rescanning — the classic pre-Target behaviour,
   scoped to the detached thread.

With no Target (boot, after Clear, or after a failed first run), the search
is an *anchor scan*: a fresh full-range scan that, on completion or cancel,
establishes the Target Query, Target Set, and coverage. A run that fails
establishes nothing. If the Target Set is empty (an anchor that found 0),
a continuing `Q` still resumes its coverage (case 1), but any other `Q`
re-anchors instead — an empty set holds nothing worth preserving.

Displayed results are always genuine matches of the query that produced
them. After mixed refines the display is not necessarily *exhaustive* for
the covered range (a narrower query may have skipped seeds an earlier one
would have kept); the tool trades exhaustiveness for never losing results.

## Import, Clear, failure

- **Import** replaces everything: the imported query becomes the Target
  Query, the imported seeds the Target Set, with empty coverage (refines of
  an import are filter-only).
- **Clear** drops the display, the Target, and all coverage. It is the only
  way to do so.
- **Failure** leaves the Target as it was; a failed run is never a
  continuation base.

## Surfacing

The status surfaces introduced for refine progress (desktop footer/status
bar, mobile snackbar, GNOME toasts) also carry the target notes:

- Target refine / target filter reuse the existing "Verifying…", "Kept X of
  Y…", "Refined: kept X of Y" notes, with *Y = the Target Set size*.
- A detached scan announces that the query is unrelated and the earlier
  results are kept (returning on a related search), since the display and
  the Target Set diverge at that moment.

The result cap, stats box, chips, and impossible-query warning are
unchanged.
