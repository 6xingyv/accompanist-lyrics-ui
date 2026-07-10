use jni::objects::{JByteBuffer, JObject, JString};
use jni::sys::{jboolean, jbyteArray, jfloat, jint, jintArray, jlong};
use jni::JNIEnv;
use std::sync::Mutex;

#[cfg(target_os = "android")]
use crate::android_gpu::AndroidGpuRenderer;
use lyrics_renderer::TextEngine;

struct EngineState {
    engine: TextEngine,
    #[cfg(target_os = "android")]
    gpu_renderer: Option<AndroidGpuRenderer>,
}

impl EngineState {
    fn new(atlas_width: u32, atlas_height: u32) -> Self {
        Self {
            engine: TextEngine::new(atlas_width, atlas_height),
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
    with_engine_mut(handle, -1, |engine| {
        engine.hit_test_lyrics_line(x, y, current_time_ms)
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
    let mut buf = vec![0i32; expected];
    if env.get_int_array_region(pixels, 0, &mut buf).is_err() {
        return;
    }
    // ARGB_8888 ints → u32 (0xAARRGGBB).
    let argb: Vec<u32> = buf.iter().map(|&v| v as u32).collect();
    with_engine_mut(handle, (), |engine| {
        engine.set_background_art(&argb, width as usize, height as usize, seed as u32);
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
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeSetPlaybackState(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    playing: jboolean,
    reactive: jboolean,
) {
    with_engine_mut(handle, (), |engine| {
        engine.set_playback_state(playing != 0, reactive != 0);
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
