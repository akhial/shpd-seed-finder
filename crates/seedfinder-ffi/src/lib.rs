//! Panic-contained C ABI for Apple frontends.

#![allow(unsafe_code)]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use serde_json::Value;
use shpd_seedfinder_core::seed::{self, DungeonSeed};
use shpd_seedfinder_core::{deep_link, json_query, results_export};
use shpd_seedfinder_session::{
    FilterPacketError, MAX_ACCEPTED_RESULTS, NativeSession, ScoutCallError, ScoutMatchError,
    ScoutPacketError, SearchError, StartSessionError, close_session, decide_start_packets,
    production_filter_packet, production_scout_matches, production_scout_packet, queries_continue,
    registry,
};

const OK: i32 = 0;
const INVALID: i32 = -1;
const INTERNAL: i32 = -2;
const UNKNOWN_HANDLE: i32 = -3;

fn request_slice<'a>(request: *const u8, len: usize) -> Option<&'a [u8]> {
    if request.is_null() {
        return None;
    }
    // SAFETY: the C contract requires `request` to reference `len` readable bytes.
    Some(unsafe { std::slice::from_raw_parts(request, len) })
}

fn return_packet(packet: Vec<u8>, out_packet: *mut *mut u8, out_len: *mut usize) -> i32 {
    if out_packet.is_null() || out_len.is_null() {
        return INVALID;
    }
    let boxed = packet.into_boxed_slice();
    let len = boxed.len();
    let raw = Box::into_raw(boxed).cast::<u8>();
    // SAFETY: both output pointers were checked and point to caller-owned slots.
    unsafe {
        out_packet.write(raw);
        out_len.write(len);
    }
    OK
}

fn clear_outputs(out_packet: *mut *mut u8, out_len: *mut usize) {
    // SAFETY: each non-null pointer is assumed writable by the ABI contract.
    unsafe {
        if !out_packet.is_null() {
            out_packet.write(ptr::null_mut());
        }
        if !out_len.is_null() {
            out_len.write(0);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_start_search(request: *const u8, request_len: usize) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(bytes) = request_slice(request, request_len) else {
            return 0;
        };
        match NativeSession::production_from_packet(bytes) {
            Ok(session) => registry().insert(session),
            Err(StartSessionError::Request(_) | StartSessionError::Spawn(_)) => 0,
        }
    }))
    .unwrap_or(0)
}

/// Starts a search which resumes a previous traversal: it scans only the
/// `scan_len` seeds beginning at `resume_from`, wrapping at the end of the
/// seed space. Callers obtain both values from `seedfinder_resume_hint` on the
/// stopped or completed session being refined.
#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_start_resumed_search(
    request: *const u8,
    request_len: usize,
    resume_from: u64,
    scan_len: u64,
) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(bytes) = request_slice(request, request_len) else {
            return 0;
        };
        match NativeSession::production_resumed_from_packet(bytes, resume_from, scan_len) {
            Ok(session) => registry().insert(session),
            Err(StartSessionError::Request(_) | StartSessionError::Spawn(_)) => 0,
        }
    }))
    .unwrap_or(0)
}

/// Writes `[resume_position, remaining]` for the session into `out_hint`,
/// which must reference two writable `i64` slots. The values are exact once
/// the session has stopped (any terminal status implies that) and meaningless
/// while it is running: a running session's hint can overshoot the work
/// actually done and must never be resumed from.
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn seedfinder_resume_hint(handle: i64, out_hint: *mut i64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if out_hint.is_null() {
            return INVALID;
        }
        let Some(session) = registry().get(handle) else {
            return UNKNOWN_HANDLE;
        };
        let hint = session.resume_hint();
        // SAFETY: `out_hint` points to space for two `i64` values by contract.
        unsafe { ptr::copy_nonoverlapping(hint.as_ptr(), out_hint, hint.len()) };
        OK
    }))
    .unwrap_or(INTERNAL)
}

/// Re-verifies `seeds_len` seed values against the `SSF8` query in `request`
/// and returns the surviving seeds as an `SSR1` packet in input order. This is
/// the "filter existing results" half of refining a search.
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn seedfinder_filter_seeds(
    request: *const u8,
    request_len: usize,
    seeds: *const u64,
    seeds_len: usize,
    out_packet: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        if out_packet.is_null() || out_len.is_null() || (seeds.is_null() && seeds_len != 0) {
            return INVALID;
        }
        let Some(bytes) = request_slice(request, request_len) else {
            return INVALID;
        };
        let seed_values = if seeds_len == 0 {
            &[]
        } else {
            // SAFETY: the C contract requires `seeds` to reference `seeds_len`
            // readable `u64` values.
            unsafe { std::slice::from_raw_parts(seeds, seeds_len) }
        };
        match production_filter_packet(bytes, seed_values) {
            Ok(packet) => return_packet(packet, out_packet, out_len),
            // A worker panic is an engine failure, not a caller error.
            Err(
                FilterPacketError::Filter(SearchError::WorkerPanicked)
                | FilterPacketError::Response(_)
                | FilterPacketError::Panicked,
            ) => INTERNAL,
            Err(FilterPacketError::Request(_) | FilterPacketError::Filter(_)) => INVALID,
        }
    }))
    .unwrap_or(INTERNAL)
}

/// Reports whether the `SSF8` query in `candidate` continues the one in
/// `base`: a scope the candidate never widens and every base requirement
/// covered by a distinct candidate requirement at least as strict (equal or
/// strengthened).
/// Only a continuing query may reuse a stopped session's results and resume
/// hint (the filter-and-resume refine flow). Returns 1 when it continues,
/// 0 when it does not, and a negative code for an undecodable packet.
#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_query_continues(
    candidate: *const u8,
    candidate_len: usize,
    base: *const u8,
    base_len: usize,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let (Some(candidate), Some(base)) = (
            request_slice(candidate, candidate_len),
            request_slice(base, base_len),
        ) else {
            return INVALID;
        };
        match queries_continue(candidate, base) {
            Ok(continues) => i32::from(continues),
            Err(_) => INVALID,
        }
    }))
    .unwrap_or(INTERNAL)
}

/// Reports what pressing Start Search must do with the `SSF8` query in
/// `candidate`, per `docs/search-semantics.md`. `target` is the Target Query
/// (null when there is no Target, which always anchors), `target_set_empty`
/// and `target_has_uncovered_seeds` describe the Target Set and its coverage,
/// and `detached_base` is the last concluded run's query when — and only when
/// — that run was itself detached (null otherwise). The returned UTF-8 text is
/// one of `anchor`, `target-refine`, `target-filter`, `continue-detached` or
/// `detached`.
///
/// The continuation predicate is part of this decision: callers must not call
/// `seedfinder_query_continues` separately for it.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)] // The C ABI spells every input out flat.
pub extern "C" fn seedfinder_decide_start(
    candidate: *const u8,
    candidate_len: usize,
    target: *const u8,
    target_len: usize,
    target_set_empty: i32,
    target_has_uncovered_seeds: i32,
    detached_base: *const u8,
    detached_base_len: usize,
    out_packet: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        if out_packet.is_null() || out_len.is_null() {
            return INVALID;
        }
        let Some(candidate) = request_slice(candidate, candidate_len) else {
            return INVALID;
        };
        match decide_start_packets(
            candidate,
            request_slice(target, target_len),
            target_set_empty != 0,
            target_has_uncovered_seeds != 0,
            request_slice(detached_base, detached_base_len),
        ) {
            Ok(decision) => {
                return_packet(decision.as_str().as_bytes().to_vec(), out_packet, out_len)
            }
            Err(_) => INVALID,
        }
    }))
    .unwrap_or(INTERNAL)
}

#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_poll(
    handle: i64,
    max_results: u32,
    out_packet: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        if out_packet.is_null() || out_len.is_null() || !(1..=1024).contains(&max_results) {
            return INVALID;
        }
        let Some(session) = registry().get(handle) else {
            return UNKNOWN_HANDLE;
        };
        match session.poll(max_results as usize) {
            Ok(packet) => return_packet(packet, out_packet, out_len),
            Err(_) => INTERNAL,
        }
    }))
    .unwrap_or(INTERNAL)
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn seedfinder_status(handle: i64, out_status: *mut i64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if out_status.is_null() {
            return INVALID;
        }
        let Some(session) = registry().get(handle) else {
            return UNKNOWN_HANDLE;
        };
        let status = session.status();
        // SAFETY: `out_status` points to space for five `i64` values by contract.
        unsafe { ptr::copy_nonoverlapping(status.as_ptr(), out_status, status.len()) };
        OK
    }))
    .unwrap_or(INTERNAL)
}

#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_cancel(handle: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if let Some(session) = registry().get(handle) {
            session.cancel();
        }
    }));
}

#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_close(handle: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| close_session(registry(), handle)));
}

#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_scout(
    request: *const u8,
    request_len: usize,
    out_packet: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        if out_packet.is_null() || out_len.is_null() {
            return INVALID;
        }
        let Some(bytes) = request_slice(request, request_len) else {
            return INVALID;
        };
        match production_scout_packet(bytes) {
            Ok(packet) => return_packet(packet, out_packet, out_len),
            Err(ScoutCallError::Packet(ScoutPacketError::Request(_))) => INVALID,
            Err(
                ScoutCallError::Packet(ScoutPacketError::Response(_)) | ScoutCallError::Panicked,
            ) => INTERNAL,
        }
    }))
    .unwrap_or(INTERNAL)
}

/// Marks which items of a scouted world satisfy the `SSF8` query in `query`.
/// The scout request identifies the world exactly like `seedfinder_scout`, and
/// the returned UTF-8 JSON `{"matched": [<item indices>],
/// "matched_requirements": <n>, "total_requirements": <n>}` indexes the item
/// list of the `SSC2` packet `seedfinder_scout` returns for that same request:
/// scouting is deterministic, so both calls describe the same world.
#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_scout_matches(
    request: *const u8,
    request_len: usize,
    query: *const u8,
    query_len: usize,
    out_packet: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        if out_packet.is_null() || out_len.is_null() {
            return INVALID;
        }
        let (Some(request), Some(query)) = (
            request_slice(request, request_len),
            request_slice(query, query_len),
        ) else {
            return INVALID;
        };
        match scout_matches_document(request, query) {
            Ok(document) => return_packet(document.into_bytes(), out_packet, out_len),
            Err(ScoutMatchError::Request(_) | ScoutMatchError::Query(_)) => INVALID,
            Err(ScoutMatchError::Panicked) => INTERNAL,
        }
    }))
    .unwrap_or(INTERNAL)
}

fn scout_matches_document(request: &[u8], query: &[u8]) -> Result<String, ScoutMatchError> {
    let marks = production_scout_matches(request, query)?;
    Ok(serde_json::json!({
        "matched": marks.matched_indices(),
        "matched_requirements": marks.matched_requirements,
        "total_requirements": marks.total_requirements,
    })
    .to_string())
}

/// Masks partial, as-you-type UTF-8 seed input into uppercase groups of three
/// and returns it as UTF-8 text: non-letters are dropped, the first nine ASCII
/// letters are kept, and only those are uppercased. The masker is
/// `seed::format_input`, shared with every other frontend.
#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_seed_format(
    input: *const u8,
    input_len: usize,
    out_packet: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        if out_packet.is_null() || out_len.is_null() {
            return INVALID;
        }
        let Some(bytes) = request_slice(input, input_len) else {
            return INVALID;
        };
        let Ok(input) = std::str::from_utf8(bytes) else {
            return INVALID;
        };
        return_packet(seed::format_input(input).into_bytes(), out_packet, out_len)
    }))
    .unwrap_or(INTERNAL)
}

/// Parses UTF-8 seed-code text with the game's own rules and returns the UTF-8
/// JSON `{"code": "XXX-XXX-XXX", "value": <number>}`: the canonical code for
/// display and the numeric value `seedfinder_filter_seeds` takes. Input that
/// is not a seed code is rejected like every other invalid input.
#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_seed_parse(
    input: *const u8,
    input_len: usize,
    out_packet: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        if out_packet.is_null() || out_len.is_null() {
            return INVALID;
        }
        let Some(bytes) = request_slice(input, input_len) else {
            return INVALID;
        };
        let Ok(input) = std::str::from_utf8(bytes) else {
            return INVALID;
        };
        match seed_parse_document(input) {
            Ok(document) => return_packet(document.into_bytes(), out_packet, out_len),
            Err(_) => INVALID,
        }
    }))
    .unwrap_or(INTERNAL)
}

fn seed_parse_document(input: &str) -> Result<String, shpd_seedfinder_core::seed::SeedError> {
    let seed = DungeonSeed::from_code(input)?;
    Ok(serde_json::json!({ "code": seed.to_code(), "value": seed.value() }).to_string())
}

/// Encodes a results file from `{"query": <canonical query document>,
/// "seeds": ["AAA-AAA-AAA", ...], "app_version": "..."}` (UTF-8 JSON) into the
/// results-file text. Validation is the codec's:
/// `crates/seedfinder-core/src/results_export.rs`.
#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_results_encode(
    request: *const u8,
    request_len: usize,
    out_packet: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        if out_packet.is_null() || out_len.is_null() {
            return INVALID;
        }
        let Some(bytes) = request_slice(request, request_len) else {
            return INVALID;
        };
        let Ok(request) = std::str::from_utf8(bytes) else {
            return INVALID;
        };
        match results_encode_document(request) {
            Ok(contents) => return_packet(contents.into_bytes(), out_packet, out_len),
            Err(_) => INVALID,
        }
    }))
    .unwrap_or(INTERNAL)
}

/// Decodes results-file text into `{"query": <canonical query document>,
/// "seeds": [...], "dropped": <number>, "app_version": ..., "shpd_version":
/// ...}` (UTF-8 JSON). The seeds are already deduplicated and capped at the
/// shared result limit, `dropped` counts the exported entries that step
/// removed, and input above the codec's 2 MiB cap is rejected.
#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_results_decode(
    contents: *const u8,
    contents_len: usize,
    out_packet: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        if out_packet.is_null() || out_len.is_null() {
            return INVALID;
        }
        let Some(bytes) = request_slice(contents, contents_len) else {
            return INVALID;
        };
        let Ok(contents) = std::str::from_utf8(bytes) else {
            return INVALID;
        };
        match results_decode_document(contents) {
            Ok(document) => return_packet(document.into_bytes(), out_packet, out_len),
            Err(_) => INVALID,
        }
    }))
    .unwrap_or(INTERNAL)
}

fn results_encode_document(request_json: &str) -> Result<String, String> {
    let request: Value = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid results request JSON: {error}"))?;
    let query_value = request
        .get("query")
        .filter(|value| value.is_object())
        .ok_or("the results request is missing its \"query\" object")?;
    let query = json_query::decode(&query_value.to_string())?;
    let seeds = request
        .get("seeds")
        .and_then(Value::as_array)
        .ok_or("the results request is missing its \"seeds\" list")?
        .iter()
        .enumerate()
        .map(|(index, entry)| results_request_seed(index, entry))
        .collect::<Result<Vec<_>, _>>()?;
    let app_version = request
        .get("app_version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(results_export::encode(&query, &seeds, app_version))
}

fn results_request_seed(index: usize, entry: &Value) -> Result<DungeonSeed, String> {
    let code = entry
        .as_str()
        .ok_or_else(|| format!("seed {}: expected a seed code string", index + 1))?;
    if !results_export::is_canonical_code(code) {
        return Err(format!(
            "seed {}: seed code must use the canonical XXX-XXX-XXX form",
            index + 1
        ));
    }
    DungeonSeed::from_code(code).map_err(|error| format!("seed {}: {error}", index + 1))
}

fn results_decode_document(contents: &str) -> Result<String, String> {
    let file = results_export::decode(contents)?;
    let (seeds, dropped) = results_export::dedupe_and_cap(&file.seeds, MAX_ACCEPTED_RESULTS);
    Ok(serde_json::json!({
        "query": json_query::encode(&file.query),
        "seeds": seeds.iter().copied().map(DungeonSeed::to_code).collect::<Vec<_>>(),
        "dropped": dropped,
        "app_version": file.app_version,
        "shpd_version": file.shpd_version,
    })
    .to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_share_encode(
    query_json: *const u8,
    query_json_len: usize,
    out_packet: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        if out_packet.is_null() || out_len.is_null() {
            return INVALID;
        }
        let Some(bytes) = request_slice(query_json, query_json_len) else {
            return INVALID;
        };
        let Ok(document) = std::str::from_utf8(bytes) else {
            return INVALID;
        };
        let Ok(query) = json_query::decode(document) else {
            return INVALID;
        };
        match deep_link::encode_link(&query) {
            Ok(link) => return_packet(link.into_bytes(), out_packet, out_len),
            Err(_) => INVALID,
        }
    }))
    .unwrap_or(INTERNAL)
}

#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_share_decode(
    text: *const u8,
    text_len: usize,
    out_packet: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        if out_packet.is_null() || out_len.is_null() {
            return INVALID;
        }
        let Some(bytes) = request_slice(text, text_len) else {
            return INVALID;
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            return INVALID;
        };
        match deep_link::decode_text(text) {
            Ok(query) => return_packet(
                json_query::encode(&query).to_string().into_bytes(),
                out_packet,
                out_len,
            ),
            Err(_) => INVALID,
        }
    }))
    .unwrap_or(INTERNAL)
}

#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_buffer_free(pointer: *mut u8, len: usize) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if pointer.is_null() {
            return;
        }
        let slice = ptr::slice_from_raw_parts_mut(pointer, len);
        // SAFETY: this exactly reverses `Box::into_raw` in `return_packet`.
        unsafe { drop(Box::from_raw(slice)) };
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query_packet() -> Vec<u8> {
        let mut packet = b"SSF8".to_vec();
        packet.extend_from_slice(&[24, 0, 0, 0, 0, 0, 1, 2, 0, 10]);
        packet.extend_from_slice(b"wand_frost");
        packet.extend_from_slice(&[0, 0, 1, 2, 0, 0, 0, 0, 0, 0]);
        packet
    }

    unsafe fn take_packet(pointer: *mut u8, len: usize) -> Vec<u8> {
        // SAFETY: test receives the allocation and length from this library.
        let packet = unsafe { std::slice::from_raw_parts(pointer, len) }.to_vec();
        seedfinder_buffer_free(pointer, len);
        packet
    }

    #[test]
    fn scout_round_trip_and_buffer_free() {
        let request = b"AAA-AAA-AAA";
        let mut pointer = ptr::null_mut();
        let mut len = 0;
        assert_eq!(
            seedfinder_scout(
                request.as_ptr(),
                request.len(),
                &raw mut pointer,
                &raw mut len
            ),
            OK
        );
        assert!(!pointer.is_null());
        let packet = unsafe { take_packet(pointer, len) };
        assert_eq!(&packet[..4], b"SSC2");
        seedfinder_buffer_free(ptr::null_mut(), 0);
    }

    fn call_scout_matches(request: &[u8], query: &[u8]) -> Result<Value, i32> {
        let mut pointer = ptr::null_mut();
        let mut len = 0;
        let code = seedfinder_scout_matches(
            request.as_ptr(),
            request.len(),
            query.as_ptr(),
            query.len(),
            &raw mut pointer,
            &raw mut len,
        );
        if code != OK {
            return Err(code);
        }
        let packet = unsafe { take_packet(pointer, len) };
        Ok(serde_json::from_str(&String::from_utf8(packet).unwrap()).unwrap())
    }

    /// Scouting is deterministic, so the marks index exactly the item list of
    /// the `SSC2` packet a scout of the same request returns.
    fn scouted_world(request: &[u8]) -> shpd_seedfinder_core::model::GeneratedWorld {
        let mut pointer = ptr::null_mut();
        let mut len = 0;
        assert_eq!(
            seedfinder_scout(
                request.as_ptr(),
                request.len(),
                &raw mut pointer,
                &raw mut len
            ),
            OK
        );
        let packet = unsafe { take_packet(pointer, len) };
        shpd_seedfinder_core::wire::decode_scout_world(&packet).unwrap()
    }

    #[test]
    fn scout_matches_envelope_indexes_the_scout_packet() {
        use shpd_seedfinder_core::catalog::item;

        let request = b"AAA-AAA-AAA";
        let world = scouted_world(request);
        let envelope = call_scout_matches(request, &query_packet()).unwrap();
        assert_eq!(envelope["total_requirements"], 1);
        let matched = envelope["matched"].as_array().unwrap();
        assert_eq!(
            u64::try_from(matched.len()).unwrap(),
            envelope["matched_requirements"].as_u64().unwrap()
        );
        for index in matched {
            let index = usize::try_from(index.as_u64().unwrap()).unwrap();
            assert!(index < world.items.len());
            // The pinned query asks for exactly one Wand of Frost.
            assert_eq!(item(world.items[index].item).stable_id, "wand_frost");
        }

        // A requirement taken from the world itself must mark that very item.
        let known = &world.items[0];
        let document = serde_json::json!({
            "requirements": [{
                "item": item(known.item).stable_id,
                "max_depth": known.depth,
            }],
        });
        let query = shpd_seedfinder_core::wire::encode_query(
            &json_query::decode(&document.to_string()).unwrap(),
        )
        .unwrap();
        let envelope = call_scout_matches(request, &query).unwrap();
        assert_eq!(envelope["matched_requirements"], 1);
        assert_eq!(envelope["total_requirements"], 1);
        let matched = envelope["matched"].as_array().unwrap();
        assert_eq!(matched.len(), 1);
        let index = usize::try_from(matched[0].as_u64().unwrap()).unwrap();
        assert_eq!(world.items[index].item, known.item);
    }

    #[test]
    fn scout_matches_rejects_invalid_input() {
        let query = query_packet();
        assert_eq!(call_scout_matches(b"AAA-AAA-AAA", b"bad"), Err(INVALID));
        assert_eq!(call_scout_matches(b"AAA-AAA-AA0", &query), Err(INVALID));

        let mut pointer = ptr::null_mut();
        let mut len = 0;
        assert_eq!(
            seedfinder_scout_matches(
                b"AAA-AAA-AAA".as_ptr(),
                11,
                ptr::null(),
                0,
                &raw mut pointer,
                &raw mut len
            ),
            INVALID
        );
        assert_eq!(
            seedfinder_scout_matches(
                b"AAA-AAA-AAA".as_ptr(),
                11,
                query.as_ptr(),
                query.len(),
                ptr::null_mut(),
                &raw mut len
            ),
            INVALID
        );
    }

    #[test]
    fn start_poll_status_cancel_close_lifecycle() {
        let request = query_packet();
        let handle = seedfinder_start_search(request.as_ptr(), request.len());
        assert!(handle > 0);
        let mut status = [0; 5];
        assert_eq!(seedfinder_status(handle, status.as_mut_ptr()), OK);
        let mut pointer = ptr::null_mut();
        let mut len = 0;
        assert_eq!(
            seedfinder_poll(handle, 1, &raw mut pointer, &raw mut len),
            OK
        );
        let packet = unsafe { take_packet(pointer, len) };
        assert_eq!(&packet[..4], b"SSR1");
        seedfinder_cancel(handle);
        seedfinder_close(handle);
        seedfinder_close(handle);
        assert_eq!(
            seedfinder_status(handle, status.as_mut_ptr()),
            UNKNOWN_HANDLE
        );
    }

    #[test]
    fn resumed_search_and_hint_lifecycle() {
        let request = query_packet();
        let handle = seedfinder_start_search(request.as_ptr(), request.len());
        assert!(handle > 0);
        seedfinder_cancel(handle);
        // A stopped search keeps reporting state 0 until every queued result
        // is drained, so the loop must poll while it waits.
        let mut status = [0; 5];
        loop {
            let mut packet = ptr::null_mut();
            let mut packet_len = 0;
            assert_eq!(
                seedfinder_poll(handle, 16, &raw mut packet, &raw mut packet_len),
                OK
            );
            if !packet.is_null() {
                seedfinder_buffer_free(packet, packet_len);
            }
            assert_eq!(seedfinder_status(handle, status.as_mut_ptr()), OK);
            if status[0] != 0 {
                break;
            }
            std::thread::yield_now();
        }
        let mut hint = [0_i64; 2];
        assert_eq!(seedfinder_resume_hint(handle, hint.as_mut_ptr()), OK);
        assert!(hint[0] >= 0);
        assert!(hint[1] >= 0);
        seedfinder_close(handle);

        let resumed = seedfinder_start_resumed_search(
            request.as_ptr(),
            request.len(),
            u64::try_from(hint[0]).unwrap(),
            4,
        );
        assert!(resumed > 0);
        seedfinder_cancel(resumed);
        seedfinder_close(resumed);

        // A scan length beyond the seed space is rejected before spawning.
        assert_eq!(
            seedfinder_start_resumed_search(request.as_ptr(), request.len(), 0, u64::MAX),
            0
        );
        assert_eq!(
            seedfinder_resume_hint(handle, hint.as_mut_ptr()),
            UNKNOWN_HANDLE
        );
        assert_eq!(seedfinder_resume_hint(handle, ptr::null_mut()), INVALID);
    }

    #[test]
    fn query_continuation_bridge_decodes_and_compares() {
        let request = query_packet();
        assert_eq!(
            seedfinder_query_continues(
                request.as_ptr(),
                request.len(),
                request.as_ptr(),
                request.len()
            ),
            1
        );
        assert_eq!(
            seedfinder_query_continues(b"bad".as_ptr(), 3, request.as_ptr(), request.len()),
            INVALID
        );
        assert_eq!(
            seedfinder_query_continues(ptr::null(), 0, request.as_ptr(), request.len()),
            INVALID
        );
    }

    fn call_decide_start(
        candidate: &[u8],
        target: Option<&[u8]>,
        target_set_empty: bool,
        target_has_uncovered_seeds: bool,
        detached_base: Option<&[u8]>,
    ) -> Result<String, i32> {
        let mut pointer = ptr::null_mut();
        let mut len = 0;
        let (target_pointer, target_len) =
            target.map_or((ptr::null(), 0), |packet| (packet.as_ptr(), packet.len()));
        let (base_pointer, base_len) =
            detached_base.map_or((ptr::null(), 0), |packet| (packet.as_ptr(), packet.len()));
        let code = seedfinder_decide_start(
            candidate.as_ptr(),
            candidate.len(),
            target_pointer,
            target_len,
            i32::from(target_set_empty),
            i32::from(target_has_uncovered_seeds),
            base_pointer,
            base_len,
            &raw mut pointer,
            &raw mut len,
        );
        if code != OK {
            return Err(code);
        }
        Ok(String::from_utf8(unsafe { take_packet(pointer, len) }).unwrap())
    }

    fn call_text_entry(
        entry: extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize) -> i32,
        input: &str,
    ) -> Result<String, i32> {
        let mut pointer = ptr::null_mut();
        let mut len = 0;
        let code = entry(input.as_ptr(), input.len(), &raw mut pointer, &raw mut len);
        if code != OK {
            return Err(code);
        }
        Ok(String::from_utf8(unsafe { take_packet(pointer, len) }).unwrap())
    }

    #[test]
    fn seed_bridge_masks_input_and_parses_codes() {
        let format = |input| call_text_entry(seedfinder_seed_format, input);
        let parse = |input| call_text_entry(seedfinder_seed_parse, input);

        // The masker filters ASCII letters before uppercasing, so non-ASCII
        // input contributes nothing.
        assert_eq!(format("abcD").unwrap(), "ABC-D");
        assert_eq!(format(" 1a!b@c#d$e%f^g&h*i extra").unwrap(), "ABC-DEF-GHI");
        assert_eq!(format("\u{131}ab").unwrap(), "AB");
        assert_eq!(format("").unwrap(), "");

        let parsed: Value = serde_json::from_str(&parse("AAA-AAA-AAB").unwrap()).unwrap();
        assert_eq!(parsed["code"], "AAA-AAA-AAB");
        assert_eq!(parsed["value"], 1);
        // Non-canonical but parseable input round-trips to the canonical code.
        let lowercase: Value = serde_json::from_str(&parse("aaa-aaa-aab").unwrap()).unwrap();
        assert_eq!(lowercase, parsed);
        let masked_code = format("aaaaaaaab").unwrap();
        let masked: Value = serde_json::from_str(&parse(&masked_code).unwrap()).unwrap();
        assert_eq!(masked, parsed);

        // Undashed lowercase is not a code by the game's own rules.
        assert_eq!(parse("aaaaaaaab"), Err(INVALID));
        assert_eq!(parse("AAA-AAA-AA0"), Err(INVALID));

        let mut pointer = ptr::null_mut();
        let mut len = 0;
        assert_eq!(
            seedfinder_seed_format(ptr::null(), 0, &raw mut pointer, &raw mut len),
            INVALID
        );
        assert_eq!(
            seedfinder_seed_parse(ptr::null(), 0, &raw mut pointer, &raw mut len),
            INVALID
        );
        assert_eq!(
            seedfinder_seed_parse(b"AAA-AAA-AAB".as_ptr(), 11, ptr::null_mut(), &raw mut len),
            INVALID
        );
    }

    #[test]
    fn start_decision_bridge_reports_the_documented_names() {
        use shpd_seedfinder_core::catalog::ItemKind;
        use shpd_seedfinder_core::challenges::Challenges;
        use shpd_seedfinder_core::query::{
            Requirement, SearchQuery, TierRequirement, UpgradeRequirement,
        };
        use shpd_seedfinder_core::wire::encode_query;

        let requirement = |kind| Requirement {
            kind,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
        };
        let query = |kind| SearchQuery {
            requirements: vec![requirement(kind)],
            max_depth: 24,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        let target = encode_query(&query(ItemKind::Ring)).unwrap();
        let deeper = encode_query(&SearchQuery {
            max_depth: 9,
            ..query(ItemKind::Ring)
        })
        .unwrap();
        let armor = encode_query(&query(ItemKind::Armor)).unwrap();
        let mut narrowed_query = query(ItemKind::Armor);
        narrowed_query.requirements.push(Requirement {
            upgrade: UpgradeRequirement::AtLeast(2),
            ..requirement(ItemKind::Armor)
        });
        let narrowed = encode_query(&narrowed_query).unwrap();

        assert_eq!(
            call_decide_start(&target, Some(&target), false, true, None).unwrap(),
            "target-refine"
        );
        assert_eq!(
            call_decide_start(&deeper, Some(&target), false, true, None).unwrap(),
            "target-filter"
        );
        assert_eq!(
            call_decide_start(&armor, Some(&target), false, true, None).unwrap(),
            "detached"
        );
        assert_eq!(
            call_decide_start(&narrowed, Some(&target), false, true, Some(&armor)).unwrap(),
            "continue-detached"
        );
        // A null Target anchors, and so does an empty Target Set the query
        // does not continue.
        assert_eq!(
            call_decide_start(&target, None, false, true, None).unwrap(),
            "anchor"
        );
        assert_eq!(
            call_decide_start(&deeper, Some(&target), true, true, None).unwrap(),
            "anchor"
        );

        // Every undecodable packet is rejected, as are missing output slots.
        assert_eq!(
            call_decide_start(b"bad", Some(&target), false, true, None),
            Err(INVALID)
        );
        assert_eq!(
            call_decide_start(&target, Some(b"bad"), false, true, None),
            Err(INVALID)
        );
        assert_eq!(
            call_decide_start(&target, Some(&target), false, true, Some(b"bad")),
            Err(INVALID)
        );
        assert_eq!(
            call_decide_start(&[], Some(&target), false, true, None),
            Err(INVALID)
        );
        let mut len = 0;
        assert_eq!(
            seedfinder_decide_start(
                target.as_ptr(),
                target.len(),
                ptr::null(),
                0,
                0,
                1,
                ptr::null(),
                0,
                ptr::null_mut(),
                &raw mut len
            ),
            INVALID
        );
    }

    #[test]
    fn filter_seeds_returns_ssr1_and_rejects_invalid_input() {
        let request = query_packet();
        let seeds = [0_u64, 5];
        let mut pointer = ptr::null_mut();
        let mut len = 0;
        assert_eq!(
            seedfinder_filter_seeds(
                request.as_ptr(),
                request.len(),
                seeds.as_ptr(),
                seeds.len(),
                &raw mut pointer,
                &raw mut len
            ),
            OK
        );
        let packet = unsafe { take_packet(pointer, len) };
        assert_eq!(&packet[..4], b"SSR1");

        let mut pointer = ptr::null_mut();
        assert_eq!(
            seedfinder_filter_seeds(
                request.as_ptr(),
                request.len(),
                ptr::null(),
                0,
                &raw mut pointer,
                &raw mut len
            ),
            OK
        );
        let packet = unsafe { take_packet(pointer, len) };
        assert_eq!(packet, b"SSR1\0\0");

        let mut pointer = ptr::null_mut();
        assert_eq!(
            seedfinder_filter_seeds(
                request.as_ptr(),
                request.len(),
                ptr::null(),
                2,
                &raw mut pointer,
                &raw mut len
            ),
            INVALID
        );
        assert_eq!(
            seedfinder_filter_seeds(
                b"bad".as_ptr(),
                3,
                seeds.as_ptr(),
                seeds.len(),
                &raw mut pointer,
                &raw mut len
            ),
            INVALID
        );
    }

    /// The frozen cross-platform fixtures: the Apple bridge decodes exactly
    /// the documents every other platform decodes.
    const RESULTS_FIXTURES: [&str; 3] = [
        include_str!("../../seedfinder-core/tests/fixtures/results-export-v1.json"),
        include_str!(
            "../../seedfinder-core/tests/fixtures/results-export-v1-weapon-categories.json"
        ),
        include_str!("../../seedfinder-core/tests/fixtures/results-export-wandmaker-quest.json"),
    ];

    fn call_results(
        function: extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize) -> i32,
        input: &str,
    ) -> Result<String, i32> {
        let mut pointer = ptr::null_mut();
        let mut len = 0;
        let code = function(input.as_ptr(), input.len(), &raw mut pointer, &raw mut len);
        if code != OK {
            return Err(code);
        }
        let packet = unsafe { take_packet(pointer, len) };
        Ok(String::from_utf8(packet).unwrap())
    }

    #[test]
    fn results_files_round_trip_through_the_frozen_fixtures() {
        for fixture in RESULTS_FIXTURES {
            let decoded: Value =
                serde_json::from_str(&call_results(seedfinder_results_decode, fixture).unwrap())
                    .unwrap();
            assert_eq!(decoded["shpd_version"], "3.3.8");
            assert!(!decoded["seeds"].as_array().unwrap().is_empty());
            assert_eq!(decoded["dropped"], 0);

            let request = serde_json::json!({
                "query": decoded["query"],
                "seeds": decoded["seeds"],
                "app_version": "test",
            })
            .to_string();
            let encoded = call_results(seedfinder_results_encode, &request).unwrap();
            let round_tripped: Value =
                serde_json::from_str(&call_results(seedfinder_results_decode, &encoded).unwrap())
                    .unwrap();
            assert_eq!(round_tripped["query"], decoded["query"]);
            assert_eq!(round_tripped["seeds"], decoded["seeds"]);
            assert_eq!(round_tripped["dropped"], 0);
            assert_eq!(round_tripped["app_version"], "test");
        }
    }

    #[test]
    fn results_decoding_dedupes_caps_and_refuses_oversized_files() {
        let results = (0..MAX_ACCEPTED_RESULTS + 10)
            .map(|index| {
                serde_json::json!({
                    "seed": DungeonSeed::new(
                        u64::try_from(index % MAX_ACCEPTED_RESULTS).unwrap(),
                    )
                    .unwrap()
                    .to_code()
                })
            })
            .collect::<Vec<_>>();
        let file = serde_json::json!({
            "format": "seed-seeker-results",
            "query": {"requirements": [{"item": "sword"}]},
            "results": results,
        })
        .to_string();
        let decoded: Value =
            serde_json::from_str(&call_results(seedfinder_results_decode, &file).unwrap()).unwrap();
        assert_eq!(
            decoded["seeds"].as_array().unwrap().len(),
            MAX_ACCEPTED_RESULTS
        );
        // Ten duplicates: importers report exactly what dedupe-and-cap removed.
        assert_eq!(decoded["dropped"], 10);
        assert!(decoded["app_version"].is_null());

        let oversized = " ".repeat(results_export::MAX_FILE_BYTES + 1);
        assert_eq!(
            call_results(seedfinder_results_decode, &oversized),
            Err(INVALID)
        );
    }

    #[test]
    fn results_encoding_rejects_invalid_requests() {
        let invalid_query = r#"{"query":{"requirements":[]},"seeds":[]}"#;
        assert_eq!(
            call_results(seedfinder_results_encode, invalid_query),
            Err(INVALID)
        );
        let invalid_seed =
            r#"{"query":{"requirements":[{"item":"sword"}]},"seeds":["aaa-aaa-aab"]}"#;
        assert_eq!(
            call_results(seedfinder_results_encode, invalid_seed),
            Err(INVALID)
        );
        assert_eq!(
            call_results(seedfinder_results_encode, "not json"),
            Err(INVALID)
        );

        let mut len = 0;
        assert_eq!(
            seedfinder_results_encode(ptr::null(), 0, ptr::null_mut(), &raw mut len),
            INVALID
        );
        assert_eq!(
            seedfinder_results_decode(ptr::null(), 0, ptr::null_mut(), &raw mut len),
            INVALID
        );
    }

    #[test]
    fn share_links_round_trip_and_reject_garbage() {
        let document = br#"{"requirements":[{"item":"wand_fireblast","upgrade":{"at_least":3}}]}"#;
        let mut pointer = ptr::null_mut();
        let mut len = 0;
        assert_eq!(
            seedfinder_share_encode(
                document.as_ptr(),
                document.len(),
                &raw mut pointer,
                &raw mut len
            ),
            OK
        );
        let link = unsafe { take_packet(pointer, len) };
        assert_eq!(
            std::str::from_utf8(&link).unwrap(),
            "https://shpd-seed-seeker.web.app/#q=EAGWhMA"
        );
        assert_eq!(
            seedfinder_share_decode(link.as_ptr(), link.len(), &raw mut pointer, &raw mut len),
            OK
        );
        let decoded = unsafe { take_packet(pointer, len) };
        // Decoding returns the canonical document, which spells out the kind.
        assert_eq!(
            std::str::from_utf8(&decoded).unwrap(),
            r#"{"requirements":[{"item":"wand_fireblast","kind":"wand","upgrade":{"at_least":3}}]}"#
        );

        assert_eq!(
            seedfinder_share_encode(b"not json".as_ptr(), 8, &raw mut pointer, &raw mut len),
            INVALID
        );
        assert_eq!(
            seedfinder_share_decode(b"!!!".as_ptr(), 3, &raw mut pointer, &raw mut len),
            INVALID
        );
        assert_eq!(
            seedfinder_share_decode(ptr::null(), 0, &raw mut pointer, &raw mut len),
            INVALID
        );
        assert_eq!(
            seedfinder_share_encode(
                document.as_ptr(),
                document.len(),
                ptr::null_mut(),
                &raw mut len
            ),
            INVALID
        );
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        assert_eq!(seedfinder_start_search(ptr::null(), 0), 0);
        assert_eq!(seedfinder_start_search(b"bad".as_ptr(), 3), 0);
        let mut pointer = ptr::null_mut();
        let mut len = 0;
        assert_eq!(
            seedfinder_scout(ptr::null(), 0, &raw mut pointer, &raw mut len),
            INVALID
        );
        assert_eq!(
            seedfinder_scout(b"bad".as_ptr(), 3, &raw mut pointer, &raw mut len),
            INVALID
        );
        assert_eq!(
            seedfinder_scout(b"AAA-AAA-AAA".as_ptr(), 11, ptr::null_mut(), &raw mut len),
            INVALID
        );
        assert_eq!(
            seedfinder_poll(i64::MAX, 1, &raw mut pointer, &raw mut len),
            UNKNOWN_HANDLE
        );
        assert_eq!(
            seedfinder_poll(i64::MAX, 0, &raw mut pointer, &raw mut len),
            INVALID
        );
        assert_eq!(seedfinder_status(i64::MAX, ptr::null_mut()), INVALID);
    }
}
