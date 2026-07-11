//! Panic-contained C ABI for Apple frontends.

#![allow(unsafe_code)]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use shpd_seedfinder_session::{
    NativeSession, ScoutCallError, ScoutPacketError, StartSessionError, close_session,
    production_scout_packet, registry,
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
        let mut packet = b"SSF2".to_vec();
        packet.extend_from_slice(&[24, 0, 0, 1, 2, 0, 10]);
        packet.extend_from_slice(b"wand_frost");
        packet.extend_from_slice(&[1, 2, 0, 0, 0, 0]);
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
        assert_eq!(&packet[..4], b"SSC1");
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
