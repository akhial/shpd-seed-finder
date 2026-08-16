//! Thin Android JNI adapter over `shpd-seedfinder-session`.

#![allow(unsafe_code)]

use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JLongArray};
use jni::sys::{JNI_FALSE, jboolean, jint, jlong};
use shpd_seedfinder_core::{deep_link, json_query};
use shpd_seedfinder_session::{
    FilterPacketError, NativeSession, ScoutCallError, ScoutPacketError, SearchError,
    StartSessionError, close_session, production_filter_packet, production_scout_packet,
    queries_continue, registry,
};

fn throw_illegal_argument(env: &mut JNIEnv<'_>, message: impl AsRef<str>) {
    let _ = env.throw_new("java/lang/IllegalArgumentException", message.as_ref());
}

fn throw_illegal_state(env: &mut JNIEnv<'_>, message: impl AsRef<str>) {
    let _ = env.throw_new("java/lang/IllegalStateException", message.as_ref());
}

#[cfg(target_os = "android")]
fn android_error(message: &str) {
    use std::ffi::{CString, c_char, c_int};

    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }
    const ANDROID_LOG_ERROR: c_int = 6;
    let (Ok(tag), Ok(text)) = (CString::new("SeedFinderNative"), CString::new(message)) else {
        return;
    };
    // SAFETY: both pointers are valid NUL-terminated strings during the call.
    unsafe {
        __android_log_write(ANDROID_LOG_ERROR, tag.as_ptr(), text.as_ptr());
    }
}

#[cfg(not(target_os = "android"))]
fn android_error(_message: &str) {}

#[unsafe(no_mangle)]
/// Scouts a seed from `SSQ2` bytes (`magic`, little-endian `u16` challenge
/// mask, UTF-8 seed code) or a legacy raw UTF-8 seed code, returning `SSC2`.
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_scoutSeed<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    request: JByteArray<'local>,
) -> JByteArray<'local> {
    let bytes = match env.convert_byte_array(&request) {
        Ok(bytes) => bytes,
        Err(error) => {
            throw_illegal_argument(&mut env, format!("invalid seed request array: {error}"));
            return JByteArray::default();
        }
    };
    let packet = match production_scout_packet(&bytes) {
        Ok(packet) => packet,
        Err(ScoutCallError::Packet(ScoutPacketError::Request(error))) => {
            throw_illegal_argument(&mut env, error.to_string());
            return JByteArray::default();
        }
        Err(ScoutCallError::Packet(ScoutPacketError::Response(error))) => {
            throw_illegal_state(&mut env, format!("cannot encode scout response: {error}"));
            return JByteArray::default();
        }
        Err(ScoutCallError::Panicked) => {
            android_error("canonical depth-24 scouting generation panicked");
            throw_illegal_state(&mut env, "native scouting generation failed");
            return JByteArray::default();
        }
    };
    match env.byte_array_from_slice(&packet) {
        Ok(array) => array,
        Err(error) => {
            throw_illegal_state(&mut env, format!("cannot allocate scout response: {error}"));
            JByteArray::default()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_startSearch<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    request: JByteArray<'local>,
) -> jlong {
    let bytes = match env.convert_byte_array(&request) {
        Ok(bytes) => bytes,
        Err(error) => {
            throw_illegal_argument(&mut env, format!("invalid request array: {error}"));
            return 0;
        }
    };
    let session = match NativeSession::production_from_packet(&bytes) {
        Ok(session) => session,
        Err(StartSessionError::Request(error)) => {
            throw_illegal_argument(&mut env, error.to_string());
            return 0;
        }
        Err(StartSessionError::Spawn(error)) => {
            throw_illegal_state(&mut env, format!("cannot start native search: {error:?}"));
            return 0;
        }
    };
    registry().insert(session)
}

/// Starts a search which resumes a previous traversal: it scans only the
/// `scanLen` seeds beginning at `resumeFrom`, wrapping at the end of the seed
/// space. Callers obtain both values from `resumeHint` on the stopped or
/// completed session being refined.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_startResumedSearch<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    request: JByteArray<'local>,
    resume_from: jlong,
    scan_len: jlong,
) -> jlong {
    let bytes = match env.convert_byte_array(&request) {
        Ok(bytes) => bytes,
        Err(error) => {
            throw_illegal_argument(&mut env, format!("invalid request array: {error}"));
            return 0;
        }
    };
    let (Ok(resume_from), Ok(scan_len)) = (u64::try_from(resume_from), u64::try_from(scan_len))
    else {
        throw_illegal_argument(&mut env, "resumeFrom and scanLen must be non-negative");
        return 0;
    };
    let session = match NativeSession::production_resumed_from_packet(&bytes, resume_from, scan_len)
    {
        Ok(session) => session,
        Err(StartSessionError::Request(error)) => {
            throw_illegal_argument(&mut env, error.to_string());
            return 0;
        }
        Err(StartSessionError::Spawn(error)) => {
            throw_illegal_argument(&mut env, format!("cannot start resumed search: {error:?}"));
            return 0;
        }
    };
    registry().insert(session)
}

/// Returns `[resumePosition, remaining]` for a session: where and how much a
/// follow-up traversal must scan to finish this session's coverage of the
/// seed space. Exact once the session has stopped (any terminal status
/// implies that); meaningless while it is running — never resume from a
/// running session's hint.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_resumeHint<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> JLongArray<'local> {
    let Some(session) = registry().get(handle) else {
        throw_illegal_state(&mut env, "unknown or closed native search handle");
        return JLongArray::default();
    };
    let hint = session.resume_hint();
    let array = match env.new_long_array(2) {
        Ok(array) => array,
        Err(error) => {
            throw_illegal_state(&mut env, format!("cannot allocate hint array: {error}"));
            return JLongArray::default();
        }
    };
    if let Err(error) = env.set_long_array_region(&array, 0, &hint) {
        throw_illegal_state(&mut env, format!("cannot populate hint array: {error}"));
        return JLongArray::default();
    }
    array
}

/// Re-verifies specific seed values against the `SSF8` query in `request` and
/// returns the surviving seeds as an `SSR1` packet in input order. This is the
/// "filter existing results" half of refining a search.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_filterSeeds<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    request: JByteArray<'local>,
    seeds: JLongArray<'local>,
) -> JByteArray<'local> {
    let bytes = match env.convert_byte_array(&request) {
        Ok(bytes) => bytes,
        Err(error) => {
            throw_illegal_argument(&mut env, format!("invalid request array: {error}"));
            return JByteArray::default();
        }
    };
    let seed_count = match env.get_array_length(&seeds) {
        Ok(length) => usize::try_from(length).unwrap_or_default(),
        Err(error) => {
            throw_illegal_argument(&mut env, format!("invalid seeds array: {error}"));
            return JByteArray::default();
        }
    };
    let mut seed_slots = vec![0_i64; seed_count];
    if let Err(error) = env.get_long_array_region(&seeds, 0, &mut seed_slots) {
        throw_illegal_argument(&mut env, format!("invalid seeds array: {error}"));
        return JByteArray::default();
    }
    let Ok(seed_values) = seed_slots
        .into_iter()
        .map(u64::try_from)
        .collect::<Result<Vec<_>, _>>()
    else {
        throw_illegal_argument(&mut env, "seed values must be non-negative");
        return JByteArray::default();
    };
    let packet = match production_filter_packet(&bytes, &seed_values) {
        Ok(packet) => packet,
        Err(FilterPacketError::Request(error)) => {
            throw_illegal_argument(&mut env, error.to_string());
            return JByteArray::default();
        }
        // A worker panic is an engine failure, not a caller error: log the
        // diagnostic like every other panic path and throw the state error.
        Err(FilterPacketError::Filter(SearchError::WorkerPanicked)) => {
            android_error("native seed filtering worker panicked");
            throw_illegal_state(&mut env, "native seed filtering failed");
            return JByteArray::default();
        }
        Err(FilterPacketError::Filter(error)) => {
            throw_illegal_argument(&mut env, format!("cannot filter seeds: {error:?}"));
            return JByteArray::default();
        }
        Err(FilterPacketError::Response(error)) => {
            throw_illegal_state(&mut env, format!("cannot encode result packet: {error}"));
            return JByteArray::default();
        }
        Err(FilterPacketError::Panicked) => {
            android_error("native seed filtering panicked");
            throw_illegal_state(&mut env, "native seed filtering failed");
            return JByteArray::default();
        }
    };
    match env.byte_array_from_slice(&packet) {
        Ok(array) => array,
        Err(error) => {
            throw_illegal_state(&mut env, format!("cannot allocate result packet: {error}"));
            JByteArray::default()
        }
    }
}

/// Reports whether the `SSF8` query in `candidate` continues the one in
/// `base` — the soundness precondition for the filter-and-resume refine flow.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_queryContinues<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    candidate: JByteArray<'local>,
    base: JByteArray<'local>,
) -> jboolean {
    let (candidate, base) = match (
        env.convert_byte_array(&candidate),
        env.convert_byte_array(&base),
    ) {
        (Ok(candidate), Ok(base)) => (candidate, base),
        (Err(error), _) | (_, Err(error)) => {
            throw_illegal_argument(&mut env, format!("invalid request array: {error}"));
            return JNI_FALSE;
        }
    };
    match queries_continue(&candidate, &base) {
        Ok(continues) => u8::from(continues),
        Err(error) => {
            throw_illegal_argument(&mut env, error.to_string());
            JNI_FALSE
        }
    }
}

/// Reads a UTF-8 string argument, throwing `IllegalArgumentException` and
/// returning `None` when the array cannot be read or is not UTF-8.
fn utf8_argument(env: &mut JNIEnv<'_>, array: &JByteArray<'_>, what: &str) -> Option<String> {
    let bytes = match env.convert_byte_array(array) {
        Ok(bytes) => bytes,
        Err(error) => {
            throw_illegal_argument(env, format!("invalid {what} array: {error}"));
            return None;
        }
    };
    let Ok(text) = String::from_utf8(bytes) else {
        throw_illegal_argument(env, format!("the {what} is not valid UTF-8"));
        return None;
    };
    Some(text)
}

fn utf8_response<'local>(env: &mut JNIEnv<'local>, text: &str, what: &str) -> JByteArray<'local> {
    match env.byte_array_from_slice(text.as_bytes()) {
        Ok(array) => array,
        Err(error) => {
            throw_illegal_state(env, format!("cannot allocate {what}: {error}"));
            JByteArray::default()
        }
    }
}

/// Encodes the canonical JSON query document in `queryDocument` as a full
/// shareable web link (both UTF-8 bytes). The codec is
/// `crates/seedfinder-core/src/deep_link.rs`, specified in
/// `docs/share-link-format.md`; failures throw with the codec's own message.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_shareEncode<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    query_document: JByteArray<'local>,
) -> JByteArray<'local> {
    let Some(document) = utf8_argument(&mut env, &query_document, "query document") else {
        return JByteArray::default();
    };
    let query = match json_query::decode(&document) {
        Ok(query) => query,
        Err(error) => {
            throw_illegal_argument(&mut env, error);
            return JByteArray::default();
        }
    };
    match deep_link::encode_link(&query) {
        Ok(link) => utf8_response(&mut env, &link, "share link"),
        Err(error) => {
            throw_illegal_argument(&mut env, error);
            JByteArray::default()
        }
    }
}

/// Decodes any accepted share-link form (full web link, custom-scheme link,
/// or bare code) back into the canonical JSON query document, both UTF-8
/// bytes. Failures throw with the codec's own message.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_shareDecode<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    text: JByteArray<'local>,
) -> JByteArray<'local> {
    let Some(text) = utf8_argument(&mut env, &text, "share link text") else {
        return JByteArray::default();
    };
    match deep_link::decode_text(&text) {
        Ok(query) => {
            let document = json_query::encode(&query).to_string();
            utf8_response(&mut env, &document, "query document")
        }
        Err(error) => {
            throw_illegal_argument(&mut env, error);
            JByteArray::default()
        }
    }
}

/// Pulls the share code out of user-facing link text, or returns null when
/// the text carries no plausible code — the non-throwing probe frontends use
/// to ignore links (e.g. the bare site URL) that are not share links.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_shareExtract<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    text: JByteArray<'local>,
) -> JByteArray<'local> {
    let Some(text) = utf8_argument(&mut env, &text, "share link text") else {
        return JByteArray::default();
    };
    match deep_link::extract_code(&text) {
        Some(code) => utf8_response(&mut env, code, "share code"),
        None => JByteArray::default(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_poll<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    max_results: jint,
) -> JByteArray<'local> {
    if !(1..=1024).contains(&max_results) {
        throw_illegal_argument(&mut env, "maxResults must be 1..=1024");
        return JByteArray::default();
    }
    let Some(session) = registry().get(handle) else {
        throw_illegal_state(&mut env, "unknown or closed native search handle");
        return JByteArray::default();
    };
    let packet = match session.poll(usize::try_from(max_results).unwrap_or_default()) {
        Ok(packet) => packet,
        Err(error) => {
            throw_illegal_state(&mut env, format!("cannot encode result packet: {error}"));
            return JByteArray::default();
        }
    };
    match env.byte_array_from_slice(&packet) {
        Ok(array) => array,
        Err(error) => {
            throw_illegal_state(&mut env, format!("cannot allocate result packet: {error}"));
            JByteArray::default()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_status<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> JLongArray<'local> {
    let Some(session) = registry().get(handle) else {
        throw_illegal_state(&mut env, "unknown or closed native search handle");
        return JLongArray::default();
    };
    let status = session.status();
    if let Some(diagnostic) = session.take_failure_diagnostic() {
        android_error(&diagnostic);
    }
    let array = match env.new_long_array(5) {
        Ok(array) => array,
        Err(error) => {
            throw_illegal_state(&mut env, format!("cannot allocate status array: {error}"));
            return JLongArray::default();
        }
    };
    if let Err(error) = env.set_long_array_region(&array, 0, &status) {
        throw_illegal_state(&mut env, format!("cannot populate status array: {error}"));
        return JLongArray::default();
    }
    array
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_cancel<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) {
    let Some(session) = registry().get(handle) else {
        throw_illegal_state(&mut env, "unknown or closed native search handle");
        return;
    };
    session.cancel();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_close<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) {
    close_session(registry(), handle);
}
