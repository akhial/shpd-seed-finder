//! Panic-contained C ABI for Apple frontends.

#![allow(unsafe_code)]

// Level generation is allocation-bound: a deep seed churns thousands of small
// buffers, and the platform allocators serialize badly across search workers.
// Every artifact built from this crate — the Windows DLL, the macOS
// staticlib — therefore carries mimalloc, exactly as the CLI does.
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use shpd_seedfinder_core::{deep_link, engine_info, json_query, results_export, seed};
use shpd_seedfinder_session::{
    FilterPacketError, MAX_RESULTS, NativeSession, ScoutCallError, ScoutMatchError,
    ScoutPacketError, SearchError, StartSessionError, available_workers, close_session,
    decide_start_packets, json, production_filter_packet, production_scout_packet,
    queries_continue, registry,
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

/// `workers` is the number of search threads to spawn, clamped to the host's
/// parallelism; 0 uses every available core.
#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_start_search(
    request: *const u8,
    request_len: usize,
    workers: u32,
) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(bytes) = request_slice(request, request_len) else {
            return 0;
        };
        match NativeSession::production_from_packet(bytes, requested_workers(workers)) {
            Ok(session) => registry().insert(session),
            Err(StartSessionError::Request(_) | StartSessionError::Spawn(_)) => 0,
        }
    }))
    .unwrap_or(0)
}

/// Starts a search which resumes a previous traversal: it scans only the
/// `scan_len` seeds beginning at `resume_from`, wrapping at the end of the
/// seed space. Callers obtain both values from `seedfinder_resume_hint` on the
/// stopped or completed session being refined. `workers` behaves exactly as in
/// `seedfinder_start_search`.
#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_start_resumed_search(
    request: *const u8,
    request_len: usize,
    resume_from: u64,
    scan_len: u64,
    workers: u32,
) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(bytes) = request_slice(request, request_len) else {
            return 0;
        };
        let workers = requested_workers(workers);
        match NativeSession::production_resumed_from_packet(bytes, resume_from, scan_len, workers) {
            Ok(session) => registry().insert(session),
            Err(StartSessionError::Request(_) | StartSessionError::Spawn(_)) => 0,
        }
    }))
    .unwrap_or(0)
}

/// Logical processors available to search workers, never less than one: the
/// ceiling for a frontend's worker selector.
#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_available_workers() -> u32 {
    u32::try_from(available_workers()).unwrap_or(u32::MAX)
}

/// Maps the ABI's `0` = every available core onto the session API's `None`.
fn requested_workers(workers: u32) -> Option<NonZeroUsize> {
    NonZeroUsize::new(workers as usize)
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

/// Re-verifies `seeds_len` seed values against the query in `request`
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

/// Reports whether the query in `candidate` continues the one in
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

/// Reports what pressing Start Search must do with the query in
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
        if out_packet.is_null()
            || out_len.is_null()
            || !usize::try_from(max_results).is_ok_and(|limit| (1..=MAX_RESULTS).contains(&limit))
        {
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

/// Marks which items of a scouted world satisfy the query in `query`.
/// The scout request identifies the world exactly like `seedfinder_scout`, and
/// the returned UTF-8 JSON `{"matched": [<item indices>],
/// "matchedRequirements": <n>, "totalRequirements": <n>}` indexes the item
/// list of the `SSC4` packet `seedfinder_scout` returns for that same request:
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
        match json::scout_matches_document(request, query) {
            Ok(document) => return_packet(document.into_bytes(), out_packet, out_len),
            Err(ScoutMatchError::Request(_) | ScoutMatchError::Query(_)) => INVALID,
            Err(ScoutMatchError::Panicked) => INTERNAL,
        }
    }))
    .unwrap_or(INTERNAL)
}

/// Returns the engine's own constants as UTF-8 JSON: the pinned upstream
/// version, the seed-space size, the query bounds, the empty boss floors, the
/// quest depth windows, the challenge list with each bit's effect on
/// generation, and the search start stride. Frontends read their limits from
/// here instead of hardcoding mirrors. The document is
/// `engine_info::document`, shared with the Android and browser bridges.
#[unsafe(no_mangle)]
pub extern "C" fn seedfinder_engine_info(out_packet: *mut *mut u8, out_len: *mut usize) -> i32 {
    clear_outputs(out_packet, out_len);
    catch_unwind(AssertUnwindSafe(|| {
        return_packet(
            engine_info::document().to_string().into_bytes(),
            out_packet,
            out_len,
        )
    }))
    .unwrap_or(INTERNAL)
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
        match seed::parse_document(input) {
            Ok(document) => return_packet(document.into_bytes(), out_packet, out_len),
            Err(_) => INVALID,
        }
    }))
    .unwrap_or(INTERNAL)
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
        match results_export::encode_document(request) {
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
        match results_export::decode_document(contents) {
            Ok(document) => return_packet(document.into_bytes(), out_packet, out_len),
            Err(_) => INVALID,
        }
    }))
    .unwrap_or(INTERNAL)
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
    use serde_json::Value;

    use super::*;

    /// The request bytes a frontend sends for a query: its canonical JSON
    /// document — here a +2 Wand of Frost anywhere in the dungeon.
    fn query_packet() -> Vec<u8> {
        br#"{"requirements":[{"item":"wand_frost","upgrade":2}]}"#.to_vec()
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
        assert_eq!(&packet[..4], b"SSC4");
        seedfinder_buffer_free(ptr::null_mut(), 0);
    }

    #[test]
    fn scout_matches_bridge_returns_the_shared_envelope_and_maps_errors() {
        let request = b"AAA-AAA-AAA";
        let query = query_packet();
        let call = |request: &[u8], query: &[u8]| {
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
            Ok(String::from_utf8(unsafe { take_packet(pointer, len) }).unwrap())
        };

        assert_eq!(
            call(request, &query).unwrap(),
            json::scout_matches_document(request, &query).unwrap()
        );
        assert_eq!(call(request, b"bad"), Err(INVALID));
        assert_eq!(call(b"AAA-AAA-AA0", &query), Err(INVALID));

        let mut pointer = ptr::null_mut();
        let mut len = 0;
        assert_eq!(
            seedfinder_scout_matches(
                request.as_ptr(),
                request.len(),
                ptr::null(),
                0,
                &raw mut pointer,
                &raw mut len
            ),
            INVALID
        );
        assert_eq!(
            seedfinder_scout_matches(
                request.as_ptr(),
                request.len(),
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
        // An explicit worker count and one far beyond the host's parallelism
        // both start fine; the session clamps.
        let handle = seedfinder_start_search(request.as_ptr(), request.len(), u32::MAX);
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
        let handle = seedfinder_start_search(request.as_ptr(), request.len(), 0);
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
            1,
        );
        assert!(resumed > 0);
        seedfinder_cancel(resumed);
        seedfinder_close(resumed);

        // A scan length beyond the seed space is rejected before spawning.
        assert_eq!(
            seedfinder_start_resumed_search(request.as_ptr(), request.len(), 0, u64::MAX, 0),
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

    #[test]
    fn engine_info_returns_the_shared_document() {
        let mut pointer = ptr::null_mut();
        let mut len = 0;
        assert_eq!(seedfinder_engine_info(&raw mut pointer, &raw mut len), OK);
        let info: Value = serde_json::from_slice(&unsafe { take_packet(pointer, len) }).unwrap();
        assert_eq!(info, engine_info::document());

        assert_eq!(
            seedfinder_engine_info(ptr::null_mut(), &raw mut len),
            INVALID
        );
    }

    #[test]
    fn seed_bridge_returns_the_shared_text_and_rejects_bad_input() {
        assert_eq!(
            call_text_entry(seedfinder_seed_format, " 1a!b@c").unwrap(),
            seed::format_input(" 1a!b@c")
        );
        assert_eq!(
            call_text_entry(seedfinder_seed_parse, "aaa-aaa-aab").unwrap(),
            seed::parse_document("aaa-aaa-aab").unwrap()
        );
        assert_eq!(
            call_text_entry(seedfinder_seed_parse, "AAA-AAA-AA0"),
            Err(INVALID)
        );

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
    fn start_decision_bridge_maps_nulls_flags_and_error_codes() {
        let target = query_packet();
        let call = |candidate: &[u8], target: Option<&[u8]>, base: Option<&[u8]>| {
            let mut pointer = ptr::null_mut();
            let mut len = 0;
            let (target_pointer, target_len) =
                target.map_or((ptr::null(), 0), |packet| (packet.as_ptr(), packet.len()));
            let (base_pointer, base_len) =
                base.map_or((ptr::null(), 0), |packet| (packet.as_ptr(), packet.len()));
            let code = seedfinder_decide_start(
                candidate.as_ptr(),
                candidate.len(),
                target_pointer,
                target_len,
                0,
                1,
                base_pointer,
                base_len,
                &raw mut pointer,
                &raw mut len,
            );
            if code != OK {
                return Err(code);
            }
            Ok(String::from_utf8(unsafe { take_packet(pointer, len) }).unwrap())
        };

        // A null Target is "no Target"; a present one reaches the decision.
        assert_eq!(
            call(&target, None, None).unwrap(),
            json::decide_start_name(&target, None, false, true, None).unwrap()
        );
        assert_eq!(
            call(&target, Some(&target), None).unwrap(),
            json::decide_start_name(&target, Some(&target), false, true, None).unwrap()
        );
        assert_eq!(call(&target, Some(&target), None).unwrap(), "target-refine");

        // Every undecodable packet is rejected, as is a null candidate.
        assert_eq!(call(b"bad", Some(&target), None), Err(INVALID));
        assert_eq!(call(&target, Some(b"bad"), None), Err(INVALID));
        assert_eq!(call(&target, None, Some(b"bad")), Err(INVALID));
        assert_eq!(call(&[], Some(&target), None), Err(INVALID));

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
    /// Text-in, text-out entry points only marshal bytes around one shared
    /// function, so each is checked against that function plus its own
    /// null and error-code handling; behaviour is tested where it lives.
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
    fn results_bridge_returns_the_shared_documents_and_rejects_bad_input() {
        let fixture = include_str!("../../seedfinder-core/tests/fixtures/results-export-v1.json");
        let decoded = call_text_entry(seedfinder_results_decode, fixture).unwrap();
        assert_eq!(decoded, results_export::decode_document(fixture).unwrap());

        let decoded: Value = serde_json::from_str(&decoded).unwrap();
        let request = serde_json::json!({
            "query": decoded["query"],
            "seeds": decoded["seeds"],
            "app_version": "test",
        })
        .to_string();
        assert_eq!(
            call_text_entry(seedfinder_results_encode, &request).unwrap(),
            results_export::encode_document(&request).unwrap()
        );

        assert_eq!(
            call_text_entry(seedfinder_results_decode, "not json"),
            Err(INVALID)
        );
        assert_eq!(
            call_text_entry(seedfinder_results_encode, r#"{"seeds":[]}"#),
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
            "https://shpd-seed-seeker.web.app/#q=QAMtCYAA"
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
        assert_eq!(seedfinder_start_search(ptr::null(), 0, 0), 0);
        assert_eq!(seedfinder_start_search(b"bad".as_ptr(), 3, 0), 0);
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
