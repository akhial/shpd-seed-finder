//! Thin Android JNI adapter over `shpd-seedfinder-session`.

#![allow(unsafe_code)]

use std::num::NonZeroUsize;

use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JLongArray};
use jni::sys::{JNI_FALSE, jboolean, jint, jlong};
use shpd_seedfinder_core::{deep_link, engine_info, json_query, results_export, seed};
use shpd_seedfinder_session::{
    FilterPacketError, MAX_RESULTS, NativeSession, ScoutCallError, ScoutMatchError,
    ScoutPacketError, SearchError, StartSessionError, available_workers, close_session, json,
    production_filter_packet, production_scout_packet, queries_continue, registry,
};

fn throw_illegal_argument(env: &mut JNIEnv<'_>, message: impl AsRef<str>) {
    let _ = env.throw_new("java/lang/IllegalArgumentException", message.as_ref());
}

/// Maps the JNI boundary's "0 or negative = every available core" onto the
/// session API's `None`.
fn requested_workers(workers: jint) -> Option<NonZeroUsize> {
    usize::try_from(workers).ok().and_then(NonZeroUsize::new)
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
/// mask, UTF-8 seed code) or a legacy raw UTF-8 seed code, returning `SSC5`.
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

/// Marks which items of a scouted world satisfy the query in `query`.
/// The scout request identifies the world exactly like `scoutSeed`, and the
/// returned UTF-8 JSON `{"matched": [<item indices>], "matchedRequirements":
/// <n>, "totalRequirements": <n>}` indexes the item list of the `SSC5` packet
/// `scoutSeed` returns for that same request: scouting is deterministic, so
/// both calls describe the same world. Requirements claim distinct items and
/// the marks are a largest satisfiable selection, so a partially matching
/// query marks only the items it could explain.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_scoutMatches<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    request: JByteArray<'local>,
    query: JByteArray<'local>,
) -> JByteArray<'local> {
    let (request, query) = match (
        env.convert_byte_array(&request),
        env.convert_byte_array(&query),
    ) {
        (Ok(request), Ok(query)) => (request, query),
        (Err(error), _) | (_, Err(error)) => {
            throw_illegal_argument(&mut env, format!("invalid request array: {error}"));
            return JByteArray::default();
        }
    };
    match json::scout_matches_document(&request, &query) {
        Ok(document) => utf8_response(&mut env, &document, "scout match document"),
        Err(ScoutMatchError::Request(error) | ScoutMatchError::Query(error)) => {
            throw_illegal_argument(&mut env, error.to_string());
            JByteArray::default()
        }
        Err(ScoutMatchError::Panicked) => {
            android_error("canonical depth-24 scouting generation panicked");
            throw_illegal_state(&mut env, "native scouting generation failed");
            JByteArray::default()
        }
    }
}

/// `workers` is the number of search threads to spawn, clamped to the host's
/// parallelism; 0 or a negative value uses every available core.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_startSearch<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    request: JByteArray<'local>,
    workers: jint,
) -> jlong {
    let bytes = match env.convert_byte_array(&request) {
        Ok(bytes) => bytes,
        Err(error) => {
            throw_illegal_argument(&mut env, format!("invalid request array: {error}"));
            return 0;
        }
    };
    let session = match NativeSession::production_from_packet(&bytes, requested_workers(workers)) {
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
/// completed session being refined. `workers` behaves exactly as in
/// `startSearch`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_startResumedSearch<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    request: JByteArray<'local>,
    resume_from: jlong,
    scan_len: jlong,
    workers: jint,
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
    let workers = requested_workers(workers);
    let session =
        match NativeSession::production_resumed_from_packet(&bytes, resume_from, scan_len, workers)
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

/// Logical processors available to search workers, never less than one: the
/// ceiling for the app's worker selector.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_availableWorkers<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jint {
    jint::try_from(available_workers()).unwrap_or(jint::MAX)
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

/// Re-verifies specific seed values against the query in `request` and
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

/// Reports whether the query in `candidate` continues the one in
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

/// Reports what pressing Start Search must do with the query in
/// `candidate`, per `docs/search-semantics.md`. `target` is the Target Query
/// (`null` when there is no Target, which always anchors), `targetSetEmpty`
/// and `targetHasUncoveredSeeds` describe the Target Set and its coverage, and
/// `detachedBase` is the last concluded run's query when — and only when —
/// that run was itself detached (`null` otherwise). The returned UTF-8 text is
/// one of `anchor`, `target-refine`, `target-filter`, `continue-detached` or
/// `detached`.
///
/// The continuation predicate is part of this decision: callers must not call
/// `queryContinues` separately for it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_decideStart<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    candidate: JByteArray<'local>,
    target: JByteArray<'local>,
    target_set_empty: jboolean,
    target_has_uncovered_seeds: jboolean,
    detached_base: JByteArray<'local>,
) -> JByteArray<'local> {
    type Packets = (Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>);
    let packets: Result<Packets, jni::errors::Error> = (|| {
        Ok((
            env.convert_byte_array(&candidate)?,
            optional_packet(&env, &target)?,
            optional_packet(&env, &detached_base)?,
        ))
    })();
    let (candidate, target, detached_base) = match packets {
        Ok(packets) => packets,
        Err(error) => {
            throw_illegal_argument(&mut env, format!("invalid request array: {error}"));
            return JByteArray::default();
        }
    };
    match json::decide_start_name(
        &candidate,
        target.as_deref(),
        target_set_empty != JNI_FALSE,
        target_has_uncovered_seeds != JNI_FALSE,
        detached_base.as_deref(),
    ) {
        Ok(decision) => utf8_response(&mut env, decision, "start decision"),
        Err(error) => {
            throw_illegal_argument(&mut env, error.to_string());
            JByteArray::default()
        }
    }
}

/// Reads a nullable `byte[]` argument: Java `null` means the packet is absent,
/// which the start decision reads as "no Target" / "no detached base".
fn optional_packet(
    env: &JNIEnv<'_>,
    array: &JByteArray<'_>,
) -> Result<Option<Vec<u8>>, jni::errors::Error> {
    if array.is_null() {
        return Ok(None);
    }
    env.convert_byte_array(array).map(Some)
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

/// Returns the engine's own constants as UTF-8 JSON: the pinned upstream
/// version, the seed-space size, the query bounds, the empty boss floors, the
/// quest depth windows, the challenge list with each bit's effect on
/// generation, and the search start stride. Frontends read their limits from
/// here instead of hardcoding mirrors. The document is
/// `engine_info::document`, shared with the C and browser bridges.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_engineInfo<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JByteArray<'local> {
    utf8_response(
        &mut env,
        &engine_info::document().to_string(),
        "engine info document",
    )
}

/// Masks partial, as-you-type UTF-8 seed input into uppercase groups of three
/// (both UTF-8 bytes): non-letters are dropped, the first nine ASCII letters
/// are kept, and only those are uppercased — never a locale-dependent
/// uppercase of the whole string. The masker is `seed::format_input`, shared
/// with every other frontend.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_formatSeedCode<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JByteArray<'local>,
) -> JByteArray<'local> {
    let Some(input) = utf8_argument(&mut env, &input, "seed input") else {
        return JByteArray::default();
    };
    utf8_response(&mut env, &seed::format_input(&input), "seed input")
}

/// Parses UTF-8 seed-code text with the game's own rules, returning the UTF-8
/// JSON `{"code": "XXX-XXX-XXX", "value": <number>}`: the canonical code for
/// display and the numeric value `filterSeeds` takes. Text that is not a seed
/// code throws with the codec's own message.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_parseSeedCode<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JByteArray<'local>,
) -> JByteArray<'local> {
    let Some(input) = utf8_argument(&mut env, &input, "seed code") else {
        return JByteArray::default();
    };
    match seed::parse_document(&input) {
        Ok(document) => utf8_response(&mut env, &document, "seed document"),
        Err(error) => {
            throw_illegal_argument(&mut env, error.to_string());
            JByteArray::default()
        }
    }
}

/// Encodes a results file from the UTF-8 JSON request `{"query": <canonical
/// query document>, "seeds": ["AAA-AAA-AAA", ...], "app_version": "..."}`,
/// returning the UTF-8 results-file text. The codec is
/// `crates/seedfinder-core/src/results_export.rs`, specified in
/// `docs/results-export-format.md`; failures throw with the codec's own
/// message.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_resultsEncode<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    request: JByteArray<'local>,
) -> JByteArray<'local> {
    let Some(request) = utf8_argument(&mut env, &request, "results request") else {
        return JByteArray::default();
    };
    match results_export::encode_document(&request) {
        Ok(contents) => utf8_response(&mut env, &contents, "results file"),
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

/// Decodes UTF-8 results-file text into the UTF-8 JSON document `{"query":
/// <canonical query document>, "seeds": [...], "dropped": <number>,
/// "app_version": ..., "shpd_version": ...}`. The seeds are already
/// deduplicated and capped at the shared result limit, `dropped` counts the
/// exported entries that step removed, and input above the engine's 2 MiB
/// import cap is rejected. Failures throw with the codec's own message.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_seedseeker_app_engine_JniBindings_resultsDecode<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    contents: JByteArray<'local>,
) -> JByteArray<'local> {
    let Some(contents) = utf8_argument(&mut env, &contents, "results file") else {
        return JByteArray::default();
    };
    match results_export::decode_document(&contents) {
        Ok(document) => utf8_response(&mut env, &document, "results document"),
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
    if !usize::try_from(max_results).is_ok_and(|limit| (1..=MAX_RESULTS).contains(&limit)) {
        throw_illegal_argument(&mut env, format!("maxResults must be 1..={MAX_RESULTS}"));
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
