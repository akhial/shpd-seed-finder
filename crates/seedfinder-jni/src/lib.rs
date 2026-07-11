//! Thin Android JNI adapter over `shpd-seedfinder-session`.

#![allow(unsafe_code)]

use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JLongArray};
use jni::sys::{jint, jlong};
use shpd_seedfinder_session::{
    NativeSession, ScoutCallError, ScoutPacketError, StartSessionError, close_session,
    production_scout_packet, registry,
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
