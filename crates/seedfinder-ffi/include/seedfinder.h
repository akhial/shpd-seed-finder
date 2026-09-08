#ifndef SEEDFINDER_H
#define SEEDFINDER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// All functions are thread-safe. Packets use the same wire formats as JNI.
// Every query-taking call takes the canonical UTF-8 JSON query document that
// seedfinder_share_* and seedfinder_results_* already speak (the schema in
// README.md "Search queries"; a leading byte-order mark and whitespace are
// tolerated), so a frontend needs only one query encoder; a request that is
// not such a document, or carries more than 64 requirements, is rejected.
// Results use SSR1.
// Scout requests are SSQ2 magic[4], challenges:u16 little-endian, then the
// UTF-8 seed code in all remaining bytes. Legacy raw UTF-8 seed codes use mask 0.
// Selected scout requests use SSQ3, a LE u16 challenge mask, LE u16-length
// UTF-8 seed, LE u16-length override (empty=auto, none=deselected, or stable ID),
// followed by optional canonical query JSON. See docs/trinket-offers.md.
// Responses use SSC4 for legacy requests or SSC5 for SSQ3. SSC5 appends the
// selected stable ID as a BE u16-length string (empty=none) after the SSC4 deck.
// Each item's flags byte uses bit 0 for cursed
// and bit 1 for placement inside a secret room.
// workers is the number of search threads to spawn, clamped to the host's
// parallelism; 0 uses every available core.
int64_t seedfinder_start_search(const uint8_t *request, size_t request_len, uint32_t workers); // >0 handle, 0 on invalid request or spawn failure
// Starts a search that scans only the scan_len seeds beginning at resume_from,
// wrapping at the end of the seed space. Pass the values reported by
// seedfinder_resume_hint on the stopped session being refined. workers behaves
// exactly as in seedfinder_start_search.
int64_t seedfinder_start_resumed_search(const uint8_t *request, size_t request_len, uint64_t resume_from, uint64_t scan_len, uint32_t workers); // >0 handle, 0 on invalid request/hint or spawn failure
// Logical processors available to search workers, never less than one: the
// ceiling for a frontend's worker selector.
uint32_t seedfinder_available_workers(void);
int32_t seedfinder_poll(int64_t handle, uint32_t max_results, uint8_t **out_packet, size_t *out_len);
// [state, scanned, total, errorCode, probabilityBits]; state: 0 running,
// 1 completed, 2 cancelled, 3 failed. A stopped search keeps reporting
// state 0 until every queued result has been drained via seedfinder_poll,
// so poll before (or while) waiting on the state.
int32_t seedfinder_status(int64_t handle, int64_t out_status[5]);
// Writes [resume_position, remaining]: where and how much a follow-up search
// must scan to finish this session's coverage. Exact once the session
// stopped (any terminal state implies that); meaningless while it is
// running — never resume from a running session's hint.
int32_t seedfinder_resume_hint(int64_t handle, int64_t out_hint[2]);
// Reports whether the query in candidate continues the one in base:
// an identical depth and challenge set, world conditions (the
// blacksmith flags and the Wandmaker filter) at least as strict as base's,
// and every
// base requirement covered by a distinct candidate requirement at least as
// strict (equal or strengthened). Only a continuing query may reuse
// a stopped session's results and resume hint (filter-and-resume refining).
// Returns 1 when it continues, 0 when it does not, negative on invalid packets.
int32_t seedfinder_query_continues(const uint8_t *candidate, size_t candidate_len, const uint8_t *base, size_t base_len);
// Reports what pressing Start Search must do with the query in candidate,
// per docs/search-semantics.md. target is the Target Query (NULL when there is
// no Target, which always anchors), target_set_empty and
// target_has_uncovered_seeds (non-zero for true) describe the Target Set and
// its coverage, and detached_base is the last concluded run's query when — and
// only when — that run was itself detached (NULL otherwise). The returned
// UTF-8 text is one of "anchor", "target-refine", "target-filter",
// "continue-detached" or "detached"; the continuation predicate is part of the
// decision, so callers must not call seedfinder_query_continues separately for
// it. The return packet is freed with seedfinder_buffer_free.
int32_t seedfinder_decide_start(const uint8_t *candidate, size_t candidate_len, const uint8_t *target, size_t target_len, int32_t target_set_empty, int32_t target_has_uncovered_seeds, const uint8_t *detached_base, size_t detached_base_len, uint8_t **out_packet, size_t *out_len);
void    seedfinder_cancel(int64_t handle);
void    seedfinder_close(int64_t handle);
int32_t seedfinder_scout(const uint8_t *request, size_t request_len, uint8_t **out_packet, size_t *out_len);
// Marks which items of a scouted world satisfy the query in query. The
// scout request identifies the world exactly like seedfinder_scout, and the
// returned UTF-8 JSON {"matched": [<item indices>], "matchedRequirements":
// <n>, "totalRequirements": <n>} indexes the item list of the SSC2 packet
// seedfinder_scout returns for that same request: scouting is deterministic,
// so both calls describe the same world. Requirements claim distinct items and
// the marks are a largest satisfiable selection, so "matched" has exactly
// "matchedRequirements" entries and a partial match marks only the items it
// could explain. The return packet is freed with seedfinder_buffer_free.
int32_t seedfinder_scout_matches(const uint8_t *request, size_t request_len, const uint8_t *query, size_t query_len, uint8_t **out_packet, size_t *out_len);
// Re-verifies seeds_len numeric seed values against the query in request
// and returns the surviving seeds as an SSR1 packet in input order.
int32_t seedfinder_filter_seeds(const uint8_t *request, size_t request_len, const uint64_t *seeds, size_t seeds_len, uint8_t **out_packet, size_t *out_len);
// Share links carry a query as a compact code. Encode takes the canonical
// UTF-8 JSON query document and returns the full UTF-8 web link; decode takes
// any link form (web link, seedseeker:// link, or bare code) and returns the
// canonical UTF-8 JSON query document. Both return packets are freed with
// seedfinder_buffer_free.
int32_t seedfinder_share_encode(const uint8_t *query_json, size_t query_json_len, uint8_t **out_packet, size_t *out_len);
int32_t seedfinder_share_decode(const uint8_t *text, size_t text_len, uint8_t **out_packet, size_t *out_len);
// Returns the engine's own constants as UTF-8 JSON: {"shpdVersion", "shpdCommit",
// "totalSeeds", "maxResults", "limits": {"maxDepth", "exactTierMin",
// "exactTierMax", "boundedTierMin", "boundedTierMax", "identityGroupMax",
// "levelSumGroupMax", "maxUpgradeDefault", "maxUpgradeRing", "resultsFileMaxBytes"},
// "emptyBossFloors": [5,10,15], "questWindows": {"ghost", "wandmaker",
// "blacksmith", "imp"} each [first, last], "challenges": [{"name", "mask",
// "changesLevelGeneration"}, ...] in mask order, "searchStartStride"}. Every
// key is camelCase. Frontends read their limits from here instead of
// hardcoding mirrors. The return packet is freed with seedfinder_buffer_free.
int32_t seedfinder_engine_info(uint8_t **out_packet, size_t *out_len);
// Seed codes are the game's own base-26 text. Format masks partial,
// as-you-type UTF-8 input into uppercase groups of three — non-letters
// dropped, the first nine ASCII letters kept, and only those uppercased — and
// returns the UTF-8 text. Parse takes UTF-8 seed-code text and returns the
// UTF-8 JSON {"code": "XXX-XXX-XXX", "value": <number>}: the canonical code
// for display and the numeric value seedfinder_filter_seeds takes. Text that
// is not a seed code is rejected. Both return packets are freed with
// seedfinder_buffer_free.
int32_t seedfinder_seed_format(const uint8_t *input, size_t input_len, uint8_t **out_packet, size_t *out_len);
int32_t seedfinder_seed_parse(const uint8_t *input, size_t input_len, uint8_t **out_packet, size_t *out_len);
// Results files carry a query plus the seeds it found (docs/results-export-format.md).
// Encode takes UTF-8 JSON {"query": <canonical query document>, "seeds":
// ["AAA-AAA-AAA", ...], "app_version": "..."} and returns the UTF-8 results-file
// text; a non-canonical seed code or an invalid query is rejected. Decode takes
// the UTF-8 file text and returns UTF-8 JSON {"query": <canonical query
// document>, "seeds": [...], "dropped": <number>, "app_version": ...,
// "shpd_version": ...} with the seeds already deduplicated and capped at the
// shared result limit and "dropped" counting the exported entries that step
// removed; input above the engine's 2 MiB import cap is rejected. Both return
// packets are freed with seedfinder_buffer_free.
int32_t seedfinder_results_encode(const uint8_t *request, size_t request_len, uint8_t **out_packet, size_t *out_len);
int32_t seedfinder_results_decode(const uint8_t *contents, size_t contents_len, uint8_t **out_packet, size_t *out_len);
void    seedfinder_buffer_free(uint8_t *ptr, size_t len);

#ifdef __cplusplus
}
#endif

#endif
