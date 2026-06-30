use jni::objects::{JByteBuffer, JObject, JString};
use jni::sys::{jboolean, jbyteArray, jfloat, jint, jlong};
use jni::JNIEnv;
use std::sync::Mutex;

use crate::core::TextEngine;

type EngineBox = Mutex<TextEngine>;

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
        Ok(guard) => f(&guard),
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
    Box::into_raw(Box::new(Mutex::new(TextEngine::new(
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
    with_engine_mut(handle, (), |engine| {
        *engine = TextEngine::new(atlas_width as u32, atlas_height as u32);
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
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    current_time_ms: jint,
    buffer: JByteBuffer,
) -> jint {
    let buf: &mut [u8] = match env.get_direct_buffer_address(buffer) {
        Ok(slice) => slice,
        Err(_) => return -1,
    };

    with_engine_mut(handle, -1, |engine| {
        engine.render_lyrics_frame_into(current_time_ms, buf)
    })
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
    bool_to_jboolean(with_engine_mut(handle, false, |engine| {
        engine.set_android_render_surface(
            env.get_native_interface(),
            surface.into_inner(),
            surface_width.max(0) as u32,
            surface_height.max(0) as u32,
            frame_width.max(0) as u32,
            frame_height.max(0) as u32,
        )
    }))
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_mocharealm_accompanist_lyrics_text_NativeTextEngine_nativeClearRenderSurface(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    with_engine_mut(handle, (), |engine| {
        engine.clear_android_render_surface();
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
    with_engine_mut(handle, -1, |engine| {
        engine.render_lyrics_frame_to_android_surface(current_time_ms)
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
    with_engine(handle, -1, |engine| {
        engine.hit_test_lyrics_line(x, y, current_time_ms)
    })
}
