use jni::objects::{JByteBuffer, JObject, JString, ReleaseMode};
use jni::sys::{jboolean, jbyteArray, jfloat, jint, jintArray, jlong, jobject, jstring};
use jni::JNIEnv;
use std::sync::Mutex;

#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};
#[cfg(target_os = "android")]
use std::sync::OnceLock;
#[cfg(target_os = "android")]
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::io::Read;
#[cfg(unix)]
use std::os::fd::FromRawFd;

#[cfg(target_os = "android")]
use crate::android_gpu::AndroidGpuRenderer;
use lyrics_renderer::TextEngine;

struct EngineState {
    engine: TextEngine,
    /// Playback time used for the most recently submitted frame.  Android's
    /// native music-foundation path owns the authoritative clock, so hit-tests
    /// must use this rather than the Kotlin fallback clock.
    last_rendered_time_ms: i32,
    #[cfg(target_os = "android")]
    background_reactive: bool,
    #[cfg(target_os = "android")]
    music_foundation_clock: MusicFoundationClock,
    #[cfg(target_os = "android")]
    gpu_renderer: Option<AndroidGpuRenderer>,
}

impl EngineState {
    fn new(atlas_width: u32, atlas_height: u32) -> Self {
        Self {
            engine: TextEngine::new(atlas_width, atlas_height),
            last_rendered_time_ms: 0,
            #[cfg(target_os = "android")]
            background_reactive: false,
            #[cfg(target_os = "android")]
            music_foundation_clock: MusicFoundationClock::default(),
            #[cfg(target_os = "android")]
            gpu_renderer: None,
        }
    }
}

type EngineBox = Mutex<EngineState>;

fn bool_to_jboolean(value: bool) -> jboolean {
    if value {
        1
    } else {
        0
    }
}

unsafe fn with_engine_mut<R>(
    handle: jlong,
    fallback: R,
    f: impl FnOnce(&mut TextEngine) -> R,
) -> R {
    with_state_mut(handle, fallback, |state| f(&mut state.engine))
}

unsafe fn with_state_mut<R>(
    handle: jlong,
    fallback: R,
    f: impl FnOnce(&mut EngineState) -> R,
) -> R {
    if handle == 0 {
        return fallback;
    }

    let engine = &*(handle as *mut EngineBox);
    match engine.lock() {
        Ok(mut guard) => f(&mut guard),
        Err(_) => fallback,
    }
}

unsafe fn with_engine<R>(handle: jlong, fallback: R, f: impl FnOnce(&TextEngine) -> R) -> R {
    if handle == 0 {
        return fallback;
    }

    let engine = &*(handle as *mut EngineBox);
    match engine.lock() {
        Ok(guard) => f(&guard.engine),
        Err(_) => fallback,
    }
}

// --- music-foundation native clock ------------------------------------------
// music-foundation and text_engine are separate Android cdylibs. Resolve this
// optional process-local C ABI once, then read the audio engine's atomics directly
// from the lyrics render thread. This avoids AudioEngine.getPlaybackStatus() and
// NativeTextEngine.setCurrentPosition() JNI round-trips for the lyrics clock.
#[cfg(target_os = "android")]
#[repr(C)]
struct MfPlaybackClock {
    position_ms: u64,
    is_paused: u8,
    /// Appended field — must match music-foundation `MfPlaybackClock`.
    duration_ms: u64,
}

/// The player publishes its frame counter from a lifecycle watcher (currently
/// roughly 20 Hz). The render surface however runs at display cadence, so using
/// those snapshots directly turns syllable progress into visible 50 ms steps.
/// Keep a native monotonic anchor and only re-anchor for a real discontinuity.
#[cfg(target_os = "android")]
#[derive(Default)]
struct MusicFoundationClock {
    anchor_position_ms: f64,
    anchor_at: Option<Instant>,
    paused: bool,
}

#[cfg(target_os = "android")]
impl MusicFoundationClock {
    fn sample(&mut self, snapshot: &MfPlaybackClock) -> i32 {
        let now = Instant::now();
        let reported_position = snapshot.position_ms as f64;
        let reported_paused = snapshot.is_paused != 0;

        let Some(anchor_at) = self.anchor_at else {
            self.anchor_position_ms = reported_position;
            self.anchor_at = Some(now);
            self.paused = reported_paused;
            return reported_position.min(i32::MAX as f64) as i32;
        };

        let predicted_position = if self.paused {
            self.anchor_position_ms
        } else {
            self.anchor_position_ms + now.duration_since(anchor_at).as_secs_f64() * 1000.0
        };
        let position_error = reported_position - predicted_position;
        // music-foundation publishes a ~20 Hz audio-frame snapshot. Its normal
        // publication lag stays below this threshold, whereas an adjacent-line
        // tap can be a small *backward* seek. Treat both directions equally so
        // an optimistic forward-only clock never ignores a nearby previous-line
        // seek and leaves the scroll animation targeting the old line.
        let is_seek_or_track_change = position_error.abs() > 120.0;

        if reported_paused || self.paused || is_seek_or_track_change || position_error > 120.0 {
            self.anchor_position_ms = reported_position;
            self.anchor_at = Some(now);
        }
        self.paused = reported_paused;

        let displayed_position = if reported_paused {
            self.anchor_position_ms
        } else {
            // saturating: never panic if Instant ever goes backwards on a device
            let elapsed_ms = self
                .anchor_at
                .map(|anchor| now.saturating_duration_since(anchor).as_secs_f64() * 1000.0)
                .unwrap_or(0.0);
            self.anchor_position_ms + elapsed_ms
        };
        displayed_position.clamp(0.0, i32::MAX as f64) as i32
    }
}

#[cfg(target_os = "android")]
type MfGetPlaybackClock = unsafe extern "C" fn(*mut MfPlaybackClock) -> bool;
#[cfg(target_os = "android")]
type MfGetAudioRms = unsafe extern "C" fn() -> f32;

#[cfg(target_os = "android")]
struct RetryingSymbol<T: Copy> {
    value: OnceLock<T>,
    retry_after: Mutex<Option<Instant>>,
}

#[cfg(target_os = "android")]
impl<T: Copy> RetryingSymbol<T> {
    const fn new() -> Self {
        Self {
            value: OnceLock::new(),
            retry_after: Mutex::new(None),
        }
    }

    fn get_or_try_init(&self, resolve: impl FnOnce() -> Option<T>) -> Option<T> {
        if let Some(value) = self.value.get() {
            return Some(*value);
        }

        let now = Instant::now();
        {
            let mut retry_after = self.retry_after.lock().ok()?;
            if retry_after.is_some_and(|deadline| now < deadline) {
                return None;
            }
            // Loading order differs between Android runtimes. A miss must not be
            // cached forever, but retrying dlopen at render cadence is wasteful.
            *retry_after = Some(now + Duration::from_secs(1));
        }

        let resolved = resolve()?;
        let _ = self.value.set(resolved);
        self.value.get().copied().or(Some(resolved))
    }
}

#[cfg(target_os = "android")]
static MF_GET_PLAYBACK_CLOCK: RetryingSymbol<MfGetPlaybackClock> = RetryingSymbol::new();
#[cfg(target_os = "android")]
static MF_GET_AUDIO_RMS: RetryingSymbol<MfGetAudioRms> = RetryingSymbol::new();
#[cfg(target_os = "android")]
static MF_RESOLVE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "android")]
static MF_RESOLVE_SUCCESSES: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "android")]
static MF_CLOCK_READ_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "android")]
static MF_CLOCK_READ_SUCCESSES: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "android")]
static MF_CLOCK_LAST_POSITION_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "android")]
static MF_CLOCK_LAST_PAUSED: AtomicU32 = AtomicU32::new(1);
#[cfg(target_os = "android")]
static MF_RENDER_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "android")]
static MF_RENDER_LAST_RESULT: AtomicI32 = AtomicI32::new(0);

#[cfg(target_os = "android")]
fn resolve_mf_playback_clock() -> Option<MfGetPlaybackClock> {
    MF_RESOLVE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    let handle = unsafe { libc::dlopen(c"libjni_bridge.so".as_ptr(), libc::RTLD_NOW) };
    if handle.is_null() {
        return None;
    }
    let symbol = unsafe { libc::dlsym(handle, c"mf_get_playback_clock".as_ptr()) };
    if symbol.is_null() {
        unsafe { libc::dlclose(handle) };
        return None;
    }
    // SAFETY: the exported music-foundation symbol has the exact C ABI declared
    // above and `dlopen` keeps its owning library resident for this process.
    MF_RESOLVE_SUCCESSES.fetch_add(1, Ordering::Relaxed);
    Some(unsafe { std::mem::transmute::<*mut libc::c_void, MfGetPlaybackClock>(symbol) })
}

#[cfg(target_os = "android")]
fn music_foundation_clock() -> Option<MfPlaybackClock> {
    MF_CLOCK_READ_CALLS.fetch_add(1, Ordering::Relaxed);
    let get_clock = MF_GET_PLAYBACK_CLOCK.get_or_try_init(resolve_mf_playback_clock)?;
    let mut clock = MfPlaybackClock {
        position_ms: 0,
        is_paused: 1,
        duration_ms: 0,
    };
    if unsafe { get_clock(&mut clock) } {
        MF_CLOCK_READ_SUCCESSES.fetch_add(1, Ordering::Relaxed);
        MF_CLOCK_LAST_POSITION_MS.store(clock.position_ms, Ordering::Relaxed);
        MF_CLOCK_LAST_PAUSED.store(u32::from(clock.is_paused), Ordering::Relaxed);
        Some(clock)
    } else {
        None
    }
}

#[cfg(target_os = "android")]
fn renderer_diagnostics_snapshot() -> String {
    format!(
        "mf_symbol_resolve_attempts={}\nmf_symbol_resolve_successes={}\nmf_clock_read_calls={}\nmf_clock_read_successes={}\nmf_clock_last_position_ms={}\nmf_clock_last_paused={}\nmf_render_calls={}\nmf_render_last_result={}",
        MF_RESOLVE_ATTEMPTS.load(Ordering::Relaxed),
        MF_RESOLVE_SUCCESSES.load(Ordering::Relaxed),
        MF_CLOCK_READ_CALLS.load(Ordering::Relaxed),
        MF_CLOCK_READ_SUCCESSES.load(Ordering::Relaxed),
        MF_CLOCK_LAST_POSITION_MS.load(Ordering::Relaxed),
        MF_CLOCK_LAST_PAUSED.load(Ordering::Relaxed),
        MF_RENDER_CALLS.load(Ordering::Relaxed),
        MF_RENDER_LAST_RESULT.load(Ordering::Relaxed),
    )
}

#[cfg(target_os = "android")]
fn music_foundation_audio_rms() -> Option<f32> {
    let get_rms = MF_GET_AUDIO_RMS.get_or_try_init(|| {
        let handle = unsafe { libc::dlopen(c"libjni_bridge.so".as_ptr(), libc::RTLD_NOW) };
        if handle.is_null() {
            return None;
        }
        let symbol = unsafe { libc::dlsym(handle, c"mf_get_audio_rms".as_ptr()) };
        if symbol.is_null() {
            unsafe { libc::dlclose(handle) };
            return None;
        }
        Some(unsafe { std::mem::transmute::<*mut libc::c_void, MfGetAudioRms>(symbol) })
    })?;
    Some(unsafe { get_rms() })
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeCreate(
    _env: JNIEnv,
    _this: JObject,
    atlas_width: jint,
    atlas_height: jint,
) -> jlong {
    crate::init_logging();
    Box::into_raw(Box::new(Mutex::new(EngineState::new(
        atlas_width as u32,
        atlas_height as u32,
    )))) as jlong
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeDestroy(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    if handle != 0 {
        let _ = Box::from_raw(handle as *mut EngineBox);
    }
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeInit(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    atlas_width: jint,
    atlas_height: jint,
) {
    with_state_mut(handle, (), |engine| {
        *engine = EngineState::new(atlas_width as u32, atlas_height as u32);
    });
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeLoadFont(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    bytes: jbyteArray,
    face_index: jint,
) -> jboolean {
    let byte_vec = env.convert_byte_array(bytes).unwrap_or_default();
    bool_to_jboolean(with_engine_mut(handle, false, |engine| {
        engine.load_font_with_index(byte_vec, face_index.max(0) as u32);
        true
    }))
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeLoadFontPath(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    path: JString,
    face_index: jint,
) -> jboolean {
    let path_str: String = env.get_string(path).map(|s| s.into()).unwrap_or_default();
    bool_to_jboolean(with_engine_mut(handle, false, |engine| {
        engine.load_font_from_path(&path_str, face_index.max(0) as u32)
    }))
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeLoadFontFd(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    fd: jint,
    offset: jlong,
    length: jlong,
    face_index: jint,
) -> jboolean {
    #[cfg(unix)]
    {
        let length = if length < 0 {
            None
        } else {
            Some(length as usize)
        };
        bool_to_jboolean(with_engine_mut(handle, false, |engine| {
            engine.load_font_from_fd(fd, offset.max(0) as u64, length, face_index.max(0) as u32)
        }))
    }
    #[cfg(not(unix))]
    {
        let _ = (handle, fd, offset, length, face_index);
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeLoadFallbackFont(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    bytes: jbyteArray,
    face_index: jint,
) -> jboolean {
    let byte_vec = env.convert_byte_array(bytes).unwrap_or_default();
    bool_to_jboolean(with_engine_mut(handle, false, |engine| {
        engine.load_fallback_font_with_index(byte_vec, face_index.max(0) as u32);
        true
    }))
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeLoadFallbackFontPath(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    path: JString,
    face_index: jint,
) -> jboolean {
    let path_str: String = env.get_string(path).map(|s| s.into()).unwrap_or_default();
    bool_to_jboolean(with_engine_mut(handle, false, |engine| {
        engine.load_fallback_font_from_path(&path_str, face_index.max(0) as u32)
    }))
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeLoadFallbackFontFd(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    fd: jint,
    offset: jlong,
    length: jlong,
    face_index: jint,
) -> jboolean {
    #[cfg(unix)]
    {
        let length = if length < 0 {
            None
        } else {
            Some(length as usize)
        };
        bool_to_jboolean(with_engine_mut(handle, false, |engine| {
            engine.load_fallback_font_from_fd(
                fd,
                offset.max(0) as u64,
                length,
                face_index.max(0) as u32,
            )
        }))
    }
    #[cfg(not(unix))]
    {
        let _ = (handle, fd, offset, length, face_index);
        0
    }
}

/// Enumerate the platform's system fonts (NDK) into the renderer's fallback pool.
/// Returns the number of font files loaded (0 off Android). The user's own fonts
/// must be configured first so they keep priority.
#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeLoadSystemFonts(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    #[cfg(target_os = "android")]
    {
        with_engine_mut(handle, 0, |engine| engine.load_system_fonts() as jint)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = handle;
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeProcessText<
    'local,
>(
    env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
    text: JString<'local>,
    size_px: jfloat,
    weight: jfloat,
) -> JString<'local> {
    let text_str: String = env.get_string(text).map(|s| s.into()).unwrap_or_default();
    let result = with_engine_mut(handle, None, |engine| {
        Some(engine.process_text(&text_str, size_px, weight))
    });

    let json = result
        .and_then(|value| serde_json::to_string(&value).ok())
        .unwrap_or_else(|| "{}".to_string());

    env.new_string(&json)
        .unwrap_or_else(|_| env.new_string("{}").unwrap())
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeHasPendingUploads(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jboolean {
    bool_to_jboolean(with_engine(handle, false, |engine| {
        engine.has_pending_uploads()
    }))
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeGetPendingUploads<
    'local,
>(
    env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JString<'local> {
    let uploads = with_engine_mut(handle, Vec::new(), |engine| engine.get_pending_uploads());

    let json_uploads: Vec<serde_json::Value> = uploads
        .iter()
        .map(|u| {
            serde_json::json!({
                "x": u.x,
                "y": u.y,
                "width": u.width,
                "height": u.height,
                "data": base64_encode(&u.data)
            })
        })
        .collect();

    let json = serde_json::to_string(&json_uploads).unwrap_or_else(|_| "[]".to_string());
    env.new_string(&json)
        .unwrap_or_else(|_| env.new_string("[]").unwrap())
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeGetAtlasSize<
    'local,
>(
    env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JString<'local> {
    let (width, height) = with_engine(handle, (0, 0), |engine| engine.get_atlas_size());
    let json = format!(r#"{{"width":{},"height":{}}}"#, width, height);
    env.new_string(&json)
        .unwrap_or_else(|_| env.new_string("{}").unwrap())
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeSetLyricsScene<
    'local,
>(
    env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
    scene_json: JString<'local>,
) -> JString<'local> {
    let scene_json: String = env
        .get_string(scene_json)
        .map(|s| s.into())
        .unwrap_or_default();
    let json = with_engine_mut(handle, "{}".to_string(), |engine| {
        engine.set_lyrics_scene_json(&scene_json)
    });
    env.new_string(&json)
        .unwrap_or_else(|_| env.new_string("{}").unwrap())
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeSetLyricsSceneDirect(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    scene_utf8: JByteBuffer,
    length: jint,
) -> jboolean {
    if length <= 0 {
        return 0;
    }
    let bytes = match env.get_direct_buffer_address(scene_utf8) {
        Ok(bytes) if length as usize <= bytes.len() => &bytes[..length as usize],
        _ => return 0,
    };
    let Ok(scene_json) = std::str::from_utf8(bytes) else {
        return 0;
    };
    bool_to_jboolean(with_engine_mut(handle, false, |engine| {
        // Scene rebuilds on track change re-shape player chrome text. A panic
        // here would otherwise abort the whole process on Android.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine.set_lyrics_scene_json(scene_json)
        })) {
            Ok(json) if json.contains("\"error\"") => {
                warn!("set_lyrics_scene_json failed: {json}");
                false
            }
            Ok(_) => true,
            Err(_) => {
                warn!("set_lyrics_scene_json paniced");
                false
            }
        }
    }))
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeGetLyricsRendererMetrics<
    'local,
>(
    env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
) -> JString<'local> {
    let json = with_engine(handle, "{}".to_string(), |engine| {
        engine.get_lyrics_renderer_metrics_json()
    });
    env.new_string(&json)
        .unwrap_or_else(|_| env.new_string("{}").unwrap())
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(CHARS[b0 >> 2] as char);
        result.push(CHARS[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(CHARS[((b1 & 0x0F) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(CHARS[b2 & 0x3F] as char);
        } else {
            result.push('=');
        }
    }

    result
}

fn write_i32(buf: &mut [u8], offset: usize, val: i32) {
    let bytes = val.to_ne_bytes();
    buf[offset..offset + 4].copy_from_slice(&bytes);
}

fn write_f32(buf: &mut [u8], offset: usize, val: f32) {
    let bytes = val.to_ne_bytes();
    buf[offset..offset + 4].copy_from_slice(&bytes);
}

fn write_u16(buf: &mut [u8], offset: usize, val: u16) {
    let bytes = val.to_ne_bytes();
    buf[offset..offset + 2].copy_from_slice(&bytes);
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeProcessTextDirect(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    text: JString,
    size_px: jfloat,
    weight: jfloat,
    buffer: JByteBuffer,
) -> jint {
    let text_str: String = match env.get_string(text) {
        Ok(s) => s.into(),
        Err(_) => return -1,
    };

    let buf: &mut [u8] = match env.get_direct_buffer_address(buffer) {
        Ok(slice) => slice,
        Err(_) => return -1,
    };

    let result_and_size = with_engine_mut(handle, None, |engine| {
        let result = engine.process_text(&text_str, size_px, weight);
        let atlas_size = engine.get_atlas_size();
        Some((result, atlas_size))
    });

    let Some((result, (atlas_width, atlas_height))) = result_and_size else {
        return -1;
    };

    let header_size = 16;
    let glyph_size = 28;
    let required_size = header_size + result.glyph_count * glyph_size;

    if buf.len() < required_size {
        return -2;
    }

    let atlas_w_f = atlas_width as f32;
    let atlas_h_f = atlas_height as f32;

    write_i32(buf, 0, result.glyph_count as i32);
    write_f32(buf, 4, result.total_width);
    write_f32(buf, 8, result.ascent);
    write_f32(buf, 12, result.descent);

    for i in 0..result.glyph_count {
        let offset = header_size + i * glyph_size;
        let pos_idx = i * 2;
        let rect_idx = i * 4;

        write_u16(buf, offset, result.glyph_ids[i]);
        write_u16(buf, offset + 2, 0);
        write_f32(buf, offset + 4, result.positions[pos_idx]);
        write_f32(buf, offset + 8, result.positions[pos_idx + 1]);
        write_f32(buf, offset + 12, result.atlas_rects[rect_idx] / atlas_w_f);
        write_f32(
            buf,
            offset + 16,
            result.atlas_rects[rect_idx + 1] / atlas_h_f,
        );
        write_f32(
            buf,
            offset + 20,
            result.atlas_rects[rect_idx + 2] / atlas_w_f,
        );
        write_f32(
            buf,
            offset + 24,
            result.atlas_rects[rect_idx + 3] / atlas_h_f,
        );
    }

    result.glyph_count as jint
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeGetPendingUploadsDirect(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    buffer: JByteBuffer,
) -> jint {
    let buf: &mut [u8] = match env.get_direct_buffer_address(buffer) {
        Ok(slice) => slice,
        Err(_) => return -1,
    };

    let upload_count_and_size = with_engine(handle, None, |engine| {
        let uploads = engine.pending_uploads();
        let mut required_size = 4;
        for upload in uploads {
            required_size += 16;
            required_size += (upload.width * upload.height * 4) as usize;
        }
        Some((uploads.len(), required_size))
    });

    let Some((upload_count, required_size)) = upload_count_and_size else {
        return -1;
    };

    if upload_count == 0 {
        if buf.len() >= 4 {
            write_i32(buf, 0, 0);
        }
        return 0;
    }

    if buf.len() < required_size {
        return -2;
    }

    let uploads = with_engine_mut(handle, Vec::new(), |engine| {
        let uploads = engine.get_pending_uploads();
        engine.clear_pending_uploads();
        uploads
    });

    let mut offset = 0;
    write_i32(buf, offset, uploads.len() as i32);
    offset += 4;

    for upload in &uploads {
        write_i32(buf, offset, upload.x as i32);
        offset += 4;
        write_i32(buf, offset, upload.y as i32);
        offset += 4;
        write_i32(buf, offset, upload.width as i32);
        offset += 4;
        write_i32(buf, offset, upload.height as i32);
        offset += 4;

        let data_size = upload.data.len();
        buf[offset..offset + data_size].copy_from_slice(&upload.data);
        offset += data_size;
    }

    uploads.len() as jint
}

#[cfg(not(target_os = "android"))]
#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeRenderLyricsFrameDirect(
    _env: JNIEnv,
    _this: JObject,
    _handle: jlong,
    _current_time_ms: jint,
    _buffer: JByteBuffer,
) -> jint {
    -20
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeSetRenderSurface(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    surface: JObject,
    surface_width: jint,
    surface_height: jint,
    frame_width: jint,
    frame_height: jint,
) -> jboolean {
    bool_to_jboolean(with_state_mut(handle, false, |state| {
        state.gpu_renderer = None;
        match AndroidGpuRenderer::from_java_surface(
            env.get_native_interface(),
            surface.into_inner(),
            surface_width.max(0) as u32,
            surface_height.max(0) as u32,
            frame_width.max(0) as u32,
            frame_height.max(0) as u32,
        ) {
            Ok(renderer) => {
                state.gpu_renderer = Some(renderer);
                true
            }
            Err(error) => {
                warn!("Failed to create Android GPU lyrics surface: {}", error);
                false
            }
        }
    }))
}

/// Acquire an `ANativeWindow` from a Java `Surface` on the calling (main) thread
/// — the only step that needs a `JNIEnv`. Returns the raw pointer as a `jlong`
/// (0 on failure) to be handed to the render thread.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeAcquireNativeWindow(
    env: JNIEnv,
    _this: JObject,
    surface: JObject,
) -> jlong {
    crate::android_gpu::acquire_native_window(env.get_native_interface(), surface.into_inner())
        as jlong
}

/// Release a window pointer from `nativeAcquireNativeWindow` that was never
/// handed to `nativeSetRenderSurfaceFromWindow`. No `JNIEnv`/engine lock needed.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeReleaseNativeWindow(
    _env: JNIEnv,
    _this: JObject,
    window_ptr: jlong,
) {
    crate::android_gpu::release_native_window(window_ptr as *mut std::ffi::c_void);
}

/// Build the EGL renderer from a pre-acquired window pointer. Carries no
/// `JNIEnv`, so it can run on the dedicated render thread. Consumes `window_ptr`.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeSetRenderSurfaceFromWindow(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    window_ptr: jlong,
    frame_width: jint,
    frame_height: jint,
) -> jboolean {
    bool_to_jboolean(with_state_mut(handle, false, |state| {
        state.gpu_renderer = None;
        match AndroidGpuRenderer::from_window_ptr(
            window_ptr as *mut std::ffi::c_void,
            frame_width.max(0) as u32,
            frame_height.max(0) as u32,
        ) {
            Ok(renderer) => {
                state.gpu_renderer = Some(renderer);
                true
            }
            Err(error) => {
                warn!(
                    "Failed to create Android GPU lyrics surface from window: {}",
                    error
                );
                false
            }
        }
    }))
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeClearRenderSurface(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    with_state_mut(handle, (), |state| {
        if let Some(renderer) = state.gpu_renderer.as_mut() {
            if let Err(error) = renderer.clear() {
                warn!("Failed to clear Android GPU lyrics surface: {}", error);
            }
        }
        state.gpu_renderer = None;
    });
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeRenderLyricsFrameToSurface(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    current_time_ms: jint,
) -> jint {
    with_state_mut(handle, -1, |state| {
        let Some(mut gpu_renderer) = state.gpu_renderer.take() else {
            return -20;
        };

        state.last_rendered_time_ms = current_time_ms;
        let mut render_result = 0;
        let present_result = gpu_renderer.draw_frame(|canvas| {
            render_result = state
                .engine
                .render_lyrics_frame_to_canvas(current_time_ms, canvas);
        });
        state.gpu_renderer = Some(gpu_renderer);
        if let Err(error) = present_result {
            warn!("Failed to render Android GPU lyrics frame: {}", error);
            return -21;
        }
        render_result
    })
}

/// Render a frame against music-foundation's native playback atomics when that
/// engine is present. `fallback_time_ms` keeps lyrics-ui usable as a standalone
/// library (including the sample app) where no music-foundation cdylib is loaded.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeRenderLyricsFrameToSurfaceFromMusicFoundation(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    fallback_time_ms: jint,
) -> jint {
    MF_RENDER_CALLS.fetch_add(1, Ordering::Relaxed);
    with_state_mut(handle, -1, |state| {
        let Some(mut gpu_renderer) = state.gpu_renderer.take() else {
            return -20;
        };

        let clock = music_foundation_clock();
        let current_time_ms = if let Some(snapshot) = clock.as_ref() {
            state.music_foundation_clock.sample(snapshot)
        } else {
            fallback_time_ms
        };
        state.last_rendered_time_ms = current_time_ms;
        if let Some(snapshot) = clock {
            let playing = snapshot.is_paused == 0;
            let duration_ms = snapshot.duration_ms.min(i32::MAX as u64) as i32;
            // Mesh reactivity + portrait transport chrome both read this sample
            // so Kotlin never pushes isPlaying/duration for the native player.
            state
                .engine
                .set_playback_state(playing, state.background_reactive);
            state.engine.set_player_live_playback(playing, duration_ms);
        }
        if state.background_reactive {
            if let Some(rms) = music_foundation_audio_rms() {
                // music-foundation publishes a perceptual 0..1 visual envelope.
                // The renderer's standalone FFT path stores loudness in a 0..6
                // working range, so convert only once at this boundary.
                let level = if rms.is_finite() {
                    rms.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                lyrics_renderer::audio::set_external_loudness(level * 6.0);
            }
        }

        let mut render_result = 0;
        let present_result = gpu_renderer.draw_frame(|canvas| {
            // Never let a Rust panic tear down the process mid-frame. Track
            // transitions (clear art / empty lyrics / next title) have hit
            // panics deep in the draw path that surface as SIGABRT on Android.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state
                    .engine
                    .render_lyrics_frame_to_canvas(current_time_ms, canvas)
            }));
            render_result = match result {
                Ok(code) => code,
                Err(_) => {
                    warn!("lyrics render paniced; skipping frame");
                    -22
                }
            };
        });
        state.gpu_renderer = Some(gpu_renderer);
        if let Err(error) = present_result {
            warn!("Failed to render Android GPU lyrics surface: {}", error);
            return -21;
        }
        MF_RENDER_LAST_RESULT.store(render_result, Ordering::Relaxed);
        render_result
    })
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_ui_diagnostics_LyricsUiDiagnostics_nativeSnapshot(
    env: JNIEnv,
    _this: JObject,
) -> jstring {
    let snapshot = format!(
        "{}\n\n-- buffered lyrics renderer logs --\n{}",
        renderer_diagnostics_snapshot(),
        crate::diagnostics_snapshot(),
    );
    env.new_string(snapshot)
        .map(|value| value.into_inner())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeBeginLyricsScroll(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    with_engine_mut(handle, (), |engine| {
        engine.begin_lyrics_scroll();
    });
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeScrollLyricsBy(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    delta_y: jfloat,
) {
    with_engine_mut(handle, (), |engine| {
        engine.scroll_lyrics_by(delta_y);
    });
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeEndLyricsScroll(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    velocity_y: jfloat,
) {
    with_engine_mut(handle, (), |engine| {
        engine.end_lyrics_scroll(velocity_y);
    });
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeCancelLyricsScroll(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    with_engine_mut(handle, (), |engine| {
        engine.cancel_lyrics_scroll();
    });
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeResetLyricsScroll(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    with_engine_mut(handle, (), |engine| {
        engine.reset_lyrics_scroll();
    });
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeHitTestLyricsLine(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jfloat,
    y: jfloat,
    current_time_ms: jint,
) -> jint {
    with_state_mut(handle, -1, |state| {
        // A negative value asks for the time of the last submitted frame. This
        // keeps hit-testing and rendering on the same native music clock.
        let time_ms = if current_time_ms < 0 {
            state.last_rendered_time_ms
        } else {
            current_time_ms
        };
        state.engine.hit_test_lyrics_line(x, y, time_ms)
    })
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeHitTestTopBar(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jfloat,
    y: jfloat,
) -> jboolean {
    bool_to_jboolean(with_engine(handle, false, |engine| {
        engine.hit_test_top_bar_button(x, y)
    }))
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativePlayerPointerDown(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jfloat,
    y: jfloat,
) -> jint {
    with_engine_mut(handle, 0, |engine| engine.player_pointer_down(x, y))
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativePlayerPointerUp(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jfloat,
    y: jfloat,
) -> jint {
    with_engine_mut(handle, 0, |engine| engine.player_pointer_up(x, y))
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeCancelPlayerPointer(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    with_engine_mut(handle, (), |engine| engine.cancel_player_pointer());
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeSetBackgroundArt(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    pixels: jintArray,
    width: jint,
    height: jint,
    seed: jint,
) {
    if width <= 0 || height <= 0 {
        return;
    }
    let expected = (width as usize) * (height as usize);
    let len = env.get_array_length(pixels).unwrap_or(0) as usize;
    if len < expected {
        return;
    }
    // Borrow/pin the Java pixels only for the synchronous mesh construction.
    // MeshGradient consumes the slice before returning, so a permanent Rust copy
    // at the JNI boundary is unnecessary. The VM may still choose to pin or copy.
    let Ok(argb) = env.get_primitive_array_critical(pixels, ReleaseMode::NoCopyBack) else {
        return;
    };
    let pixels = std::slice::from_raw_parts(argb.as_ptr() as *const u32, expected);
    with_engine_mut(handle, (), |engine| {
        engine.set_background_art(pixels, width as usize, height as usize, seed as u32);
    });
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeClearBackground(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    with_engine_mut(handle, (), |engine| {
        engine.clear_background();
    });
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeBeginQueueReorder(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jfloat,
    y: jfloat,
) -> jint {
    with_engine_mut(handle, -1, |engine| engine.begin_queue_reorder(x, y))
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeUpdateQueueReorder(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    y: jfloat,
) {
    with_engine_mut(handle, (), |engine| engine.update_queue_reorder(y));
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeFinishQueueReorder(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jlong {
    with_engine_mut(handle, -1, |engine| engine.finish_queue_reorder())
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeCancelQueueReorder(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    with_engine_mut(handle, (), |engine| engine.cancel_queue_reorder());
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeSetQueueArtwork(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    key: JString,
    pixels: jintArray,
    width: jint,
    height: jint,
) {
    if width <= 0 || height <= 0 {
        return;
    }
    let Ok(key): Result<String, _> = env.get_string(key).map(Into::into) else {
        return;
    };
    let expected = (width as usize) * (height as usize);
    if (env.get_array_length(pixels).unwrap_or(0) as usize) < expected {
        return;
    }
    let Ok(argb) = env.get_primitive_array_critical(pixels, ReleaseMode::NoCopyBack) else {
        return;
    };
    let pixels = std::slice::from_raw_parts(argb.as_ptr() as *const u32, expected);
    with_engine_mut(handle, (), |engine| {
        engine.set_queue_artwork(key, pixels, width as usize, height as usize);
    });
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeClearQueueArtworks(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    with_engine_mut(handle, (), |engine| engine.clear_queue_artworks());
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeSetPlaybackState(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    playing: jboolean,
    reactive: jboolean,
) {
    with_state_mut(handle, (), |state| {
        #[cfg(target_os = "android")]
        {
            state.background_reactive = reactive != 0;
            state
                .engine
                .set_playback_state(playing != 0, state.background_reactive);
        }
        #[cfg(not(target_os = "android"))]
        state.engine.set_playback_state(playing != 0, reactive != 0);
    });
}

// --- Process-global audio analysis (no engine handle) -----------------------
// Fed from an ExoPlayer TeeAudioProcessor (same process) and read in-process by
// the mesh-gradient renderer each frame.

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeAudioAnalysis_nativePushAudioData(
    env: JNIEnv,
    _class: JObject,
    buffer: JByteBuffer,
    float_count: jint,
) {
    if float_count <= 0 {
        return;
    }
    let bytes: &[u8] = match env.get_direct_buffer_address(buffer) {
        Ok(slice) => slice,
        Err(_) => return,
    };
    let available = bytes.len() / 4;
    let count = (float_count as usize).min(available);
    if count == 0 {
        return;
    }
    // Direct ByteBuffers are allocated 8-byte aligned, so reading them as f32 is
    // sound; clamp to the byte length so a short buffer can't over-read.
    let samples = unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const f32, count) };
    lyrics_renderer::audio::push_pcm(samples);
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeAudioAnalysis_nativeSetSampleRate(
    _env: JNIEnv,
    _class: JObject,
    sample_rate: jfloat,
) {
    lyrics_renderer::audio::set_sample_rate(sample_rate);
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeAudioAnalysis_nativeReset(
    _env: JNIEnv,
    _class: JObject,
) {
    lyrics_renderer::audio::reset();
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeLyricsParser_nativeParseToWire(
    env: JNIEnv,
    _class: JObject,
    content: JString,
) -> jobject {
    let Ok(content) = env.get_string(content) else {
        return std::ptr::null_mut();
    };
    direct_buffer(&env, lyrics_parser::parse_wire(&content.to_string_lossy()))
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeLyricsParser_nativeParseToPlainText(
    env: JNIEnv,
    _class: JObject,
    content: JString,
) -> jobject {
    let Ok(content) = env.get_string(content) else {
        return std::ptr::null_mut();
    };
    direct_buffer(
        &env,
        lyrics_parser::parse_plain_text(&content.to_string_lossy()).into_bytes(),
    )
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeLyricsParser_nativeParseFdToWire(
    env: JNIEnv,
    _class: JObject,
    fd: jint,
) -> jobject {
    parse_fd(&env, fd, lyrics_parser::parse_wire)
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeLyricsParser_nativeParseFdToPlainText(
    env: JNIEnv,
    _class: JObject,
    fd: jint,
) -> jobject {
    parse_fd(&env, fd, |content| {
        lyrics_parser::parse_plain_text(content).into_bytes()
    })
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeLyricsParser_nativeReleaseBuffer(
    env: JNIEnv,
    _class: JObject,
    buffer: JByteBuffer,
) {
    let Ok(bytes) = env.get_direct_buffer_address(buffer) else {
        return;
    };
    if bytes.is_empty() {
        return;
    }
    let slice = std::ptr::slice_from_raw_parts_mut(bytes.as_mut_ptr(), bytes.len());
    drop(Box::from_raw(slice));
}

fn direct_buffer(env: &JNIEnv, bytes: Vec<u8>) -> jobject {
    if bytes.is_empty() {
        return std::ptr::null_mut();
    }
    let boxed = bytes.into_boxed_slice();
    let raw = Box::into_raw(boxed);
    match env.new_direct_byte_buffer(unsafe { &mut *raw }) {
        Ok(buffer) => buffer.into_inner(),
        Err(_) => {
            unsafe { drop(Box::from_raw(raw)) };
            std::ptr::null_mut()
        }
    }
}

#[cfg(unix)]
fn parse_fd(env: &JNIEnv, fd: jint, parse: impl FnOnce(&str) -> Vec<u8>) -> jobject {
    let duplicated_fd = unsafe { libc::dup(fd) };
    if duplicated_fd < 0 {
        return std::ptr::null_mut();
    }
    // Own only the duplicated descriptor. Dropping this File must never close the
    // descriptor supplied by ParcelFileDescriptor on the Java side.
    let mut file = unsafe { std::fs::File::from_raw_fd(duplicated_fd) };
    let mut content = String::new();
    if file.read_to_string(&mut content).is_err() {
        return std::ptr::null_mut();
    }
    direct_buffer(env, parse(&content))
}

#[cfg(not(unix))]
fn parse_fd(_env: &JNIEnv, _fd: jint, _parse: impl FnOnce(&str) -> Vec<u8>) -> jobject {
    std::ptr::null_mut()
}
