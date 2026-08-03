//! Panic-contained C ABI for Apple frontends.

#![allow(unsafe_code)]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use shpd_seedfinder_session::{
    FilterPacketError, NativeSession, ScoutCallError, ScoutPacketError, SearchError,
    StartSessionError, close_session, production_filter_packet, production_scout_packet,
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

/// Re-verifies `seeds_len` seed values against the `SSF7` query in `request`
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

/// Reports whether the `SSF7` query in `candidate` continues the one in
/// `base`: identical scope and every base requirement covered by a distinct
/// candidate requirement at least as strict (equal or strengthened).
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
        let mut packet = b"SSF7".to_vec();
        packet.extend_from_slice(&[24, 0, 0, 0, 0, 1, 2, 0, 10]);
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
