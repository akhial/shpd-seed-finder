//! Thin Android JNI adapter over `shpd-seedfinder-session`.

#![allow(unsafe_code)]

use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JLongArray};
use jni::sys::{JNI_FALSE, jboolean, jint, jlong};
use serde_json::Value;
use shpd_seedfinder_core::seed::DungeonSeed;
use shpd_seedfinder_core::wire::WireError;
use shpd_seedfinder_core::{deep_link, json_query, results_export};
use shpd_seedfinder_session::{
    FilterPacketError, MAX_ACCEPTED_RESULTS, NativeSession, ScoutCallError, ScoutMatchError,
    ScoutPacketError, SearchError, StartDecision, StartSessionError, close_session,
    decide_start_packets, production_filter_packet, production_scout_matches,
    production_scout_packet, queries_continue, registry,
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

/// Marks which items of a scouted world satisfy the `SSF8` query in `query`.
/// The scout request identifies the world exactly like `scoutSeed`, and the
/// returned UTF-8 JSON `{"matched": [<item indices>], "matched_requirements":
/// <n>, "total_requirements": <n>}` indexes the item list of the `SSC2` packet
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
    match scout_matches_document(&request, &query) {
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

fn scout_matches_document(request: &[u8], query: &[u8]) -> Result<String, ScoutMatchError> {
    let marks = production_scout_matches(request, query)?;
    Ok(serde_json::json!({
        "matched": marks.matched_indices(),
        "matched_requirements": marks.matched_requirements,
        "total_requirements": marks.total_requirements,
    })
    .to_string())
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

/// Reports what pressing Start Search must do with the `SSF8` query in
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
    match decide_start_name(
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

fn decide_start_name(
    candidate: &[u8],
    target: Option<&[u8]>,
    target_set_empty: bool,
    target_has_uncovered_seeds: bool,
    detached_base: Option<&[u8]>,
) -> Result<&'static str, WireError> {
    decide_start_packets(
        candidate,
        target,
        target_set_empty,
        target_has_uncovered_seeds,
        detached_base,
    )
    .map(StartDecision::as_str)
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
    match results_encode_document(&request) {
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
    match results_decode_document(&contents) {
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

#[cfg(test)]
mod tests {
    // The bridge functions themselves need a live JVM; these cover the codec
    // delegation behind them, which is where every platform-visible rule lives.
    use serde_json::{Value, json};
    use shpd_seedfinder_core::catalog::item;
    use shpd_seedfinder_core::challenges::Challenges;
    use shpd_seedfinder_core::json_query;
    use shpd_seedfinder_core::seed::DungeonSeed;
    use shpd_seedfinder_core::wire::{decode_scout_world, encode_query};
    use shpd_seedfinder_session::{
        MAX_ACCEPTED_RESULTS, production_scout_packet, production_scout_world,
    };

    use super::{
        decide_start_name, results_decode_document, results_encode_document, scout_matches_document,
    };

    #[test]
    fn start_decision_bridge_reports_the_documented_names() {
        use shpd_seedfinder_core::catalog::ItemKind;
        use shpd_seedfinder_core::query::{
            Requirement, SearchQuery, TierRequirement, UpgradeRequirement,
        };

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
            decide_start_name(&target, Some(&target), false, true, None).unwrap(),
            "target-refine"
        );
        assert_eq!(
            decide_start_name(&deeper, Some(&target), false, true, None).unwrap(),
            "target-filter"
        );
        assert_eq!(
            decide_start_name(&armor, Some(&target), false, true, None).unwrap(),
            "detached"
        );
        assert_eq!(
            decide_start_name(&narrowed, Some(&target), false, true, Some(&armor)).unwrap(),
            "continue-detached"
        );
        // A missing Target anchors, and so does an empty Target Set the query
        // does not continue.
        assert_eq!(
            decide_start_name(&target, None, false, true, None).unwrap(),
            "anchor"
        );
        assert_eq!(
            decide_start_name(&deeper, Some(&target), true, true, None).unwrap(),
            "anchor"
        );

        assert!(decide_start_name(b"bad", Some(&target), false, true, None).is_err());
        assert!(decide_start_name(&target, Some(b"bad"), false, true, None).is_err());
        assert!(decide_start_name(&target, None, false, true, Some(b"bad")).is_err());
    }

    #[test]
    fn scout_match_envelope_indexes_the_scout_packet() {
        // Scouting is deterministic, so the marks index exactly the item list
        // the SSC2 packet of the same request carries.
        let seed = DungeonSeed::MIN;
        let world = production_scout_world(seed, Challenges::NONE).unwrap();
        let known = &world.items[0];
        let document = json!({
            "requirements": [{
                "item": item(known.item).stable_id,
                "max_depth": known.depth,
            }],
        });
        let query = encode_query(&json_query::decode(&document.to_string()).unwrap()).unwrap();

        let envelope: Value =
            serde_json::from_str(&scout_matches_document(b"AAA-AAA-AAA", &query).unwrap()).unwrap();
        assert_eq!(envelope["total_requirements"], 1);
        assert_eq!(envelope["matched_requirements"], 1);
        let matched = envelope["matched"].as_array().unwrap();
        assert_eq!(matched.len(), 1);
        let index = usize::try_from(matched[0].as_u64().unwrap()).unwrap();
        let packet = production_scout_packet(b"AAA-AAA-AAA").unwrap();
        let scouted = decode_scout_world(&packet).unwrap();
        assert!(index < scouted.items.len());
        assert_eq!(scouted.items[index].item, known.item);

        // An unsatisfiable requirement still reports the requirement count.
        let impossible = encode_query(
            &json_query::decode(
                r#"{"requirements":[{"item":"sword","max_depth":1}],"max_depth":1}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let envelope: Value =
            serde_json::from_str(&scout_matches_document(b"AAA-AAA-AAA", &impossible).unwrap())
                .unwrap();
        assert_eq!(envelope["total_requirements"], 1);
        assert!(envelope["matched"].as_array().unwrap().len() <= 1);

        assert!(scout_matches_document(b"AAA-AAA-AA0", &query).is_err());
        assert!(scout_matches_document(b"AAA-AAA-AAA", b"bad").is_err());
    }

    /// The frozen cross-platform fixtures: the Android bridge decodes exactly
    /// the documents every other platform decodes.
    const RESULTS_FIXTURES: [&str; 3] = [
        include_str!("../../seedfinder-core/tests/fixtures/results-export-v1.json"),
        include_str!(
            "../../seedfinder-core/tests/fixtures/results-export-v1-weapon-categories.json"
        ),
        include_str!("../../seedfinder-core/tests/fixtures/results-export-wandmaker-quest.json"),
    ];

    #[test]
    fn results_files_round_trip_through_the_frozen_fixtures() {
        for fixture in RESULTS_FIXTURES {
            let decoded: Value =
                serde_json::from_str(&results_decode_document(fixture).unwrap()).unwrap();
            assert_eq!(decoded["shpd_version"], "3.3.8");
            assert!(!decoded["seeds"].as_array().unwrap().is_empty());
            assert_eq!(decoded["dropped"], 0);

            let request = json!({
                "query": decoded["query"],
                "seeds": decoded["seeds"],
                "app_version": "test",
            });
            let encoded = results_encode_document(&request.to_string()).unwrap();
            let round_tripped: Value =
                serde_json::from_str(&results_decode_document(&encoded).unwrap()).unwrap();
            assert_eq!(round_tripped["query"], decoded["query"]);
            assert_eq!(round_tripped["seeds"], decoded["seeds"]);
            assert_eq!(round_tripped["dropped"], 0);
            assert_eq!(round_tripped["app_version"], "test");
        }
    }

    #[test]
    fn results_decoding_dedupes_caps_and_refuses_oversized_files() {
        let file = json!({
            "format": "seed-seeker-results",
            "query": {"requirements": [{"item": "sword"}]},
            "results": (0..MAX_ACCEPTED_RESULTS + 10)
                .map(|index| json!({
                    "seed": DungeonSeed::new(
                        u64::try_from(index % MAX_ACCEPTED_RESULTS).unwrap(),
                    )
                    .unwrap()
                    .to_code()
                }))
                .collect::<Vec<_>>(),
        });
        let decoded: Value =
            serde_json::from_str(&results_decode_document(&file.to_string()).unwrap()).unwrap();
        assert_eq!(
            decoded["seeds"].as_array().unwrap().len(),
            MAX_ACCEPTED_RESULTS
        );
        // Ten duplicates: importers report exactly what dedupe-and-cap removed.
        assert_eq!(decoded["dropped"], 10);
        assert!(decoded["app_version"].is_null());

        let oversized = " ".repeat(super::results_export::MAX_FILE_BYTES + 1);
        let error = results_decode_document(&oversized).unwrap_err();
        assert!(error.contains("too large"), "{error}");
    }

    #[test]
    fn results_encoding_fails_on_invalid_queries_and_seed_codes() {
        let invalid_query = json!({"query": {"requirements": []}, "seeds": []});
        assert!(results_encode_document(&invalid_query.to_string()).is_err());

        let invalid_seed = json!({
            "query": {"requirements": [{"item": "sword"}]},
            "seeds": ["aaa-aaa-aab"],
        });
        let error = results_encode_document(&invalid_seed.to_string()).unwrap_err();
        assert!(error.contains("canonical"), "{error}");

        assert!(results_encode_document("not json").is_err());
        assert!(results_encode_document(r#"{"seeds":[]}"#).is_err());
    }
}
