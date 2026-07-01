#[cfg(target_os = "android")]
use crate::android_gpu::AndroidGpuRenderer;
use crate::atlas::{AtlasManager, Rect};
use crate::font::FontWrapper;
use crate::renderer::LyricsRenderer;
use rustybuzz::{Face, UnicodeBuffer};

use serde::{Deserialize, Serialize};
use std::ops::Deref;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Serialize, Deserialize)]
pub struct LayoutResult {
    pub glyph_count: usize,
    // Flat arrays for JNI transfer
    pub glyph_ids: Vec<u16>,
    pub positions: Vec<f32>,     // x, y interleaved (relative to baseline)
    pub atlas_rects: Vec<f32>,   // u, v, w, h in atlas
    pub glyph_offsets: Vec<f32>, // x_offset, y_offset interleaved (bearing from glyph origin to bitmap top-left)
    pub font_indices: Vec<u8>,   // Which font each glyph comes from (0 = primary, 1+ = fallback)
    pub total_width: f32,
    pub total_height: f32,
    pub ascent: f32,
    pub descent: f32,
}

#[derive(Clone)]
pub struct PendingUpload {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA data
}

/// A run of consecutive grapheme clusters that share the same font.
struct TextRun {
    text: String,
    font_index: usize, // 0 = primary, 1+ = fallback
}

pub struct TextEngine {
    atlas: AtlasManager,
    // Primary font
    font: Option<FontWrapper>,
    // Fallback fonts (system fonts, etc.)
    fallback_fonts: Vec<FontWrapper>,
    pending_uploads: Vec<PendingUpload>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    renderer: LyricsRenderer,
    #[cfg(target_os = "android")]
    gpu_renderer: Option<AndroidGpuRenderer>,
}

impl TextEngine {
    pub fn new(atlas_width: u32, atlas_height: u32) -> Self {
        Self {
            atlas: AtlasManager::new(atlas_width, atlas_height),
            font: None,
            fallback_fonts: Vec::new(),
            pending_uploads: Vec::new(),
            atlas_width,
            atlas_height,
            renderer: LyricsRenderer::new(),
            #[cfg(target_os = "android")]
            gpu_renderer: None,
        }
    }

    pub fn load_font_with_index(&mut self, font_bytes: Vec<u8>, face_index: u32) {
        self.reset_glyph_cache();
        self.renderer
            .load_font_bytes(font_bytes.clone(), face_index);

        // Init FontWrapper for primary font
        info!("Loading PRIMARY font: {} bytes", font_bytes.len());
        if let Some(wrapper) = FontWrapper::from_bytes_with_index(&font_bytes, 0, face_index) {
            self.font = Some(wrapper);
            info!("PRIMARY font loaded successfully");
        } else {
            warn!("ERROR: Failed to load primary font!");
            self.font = None;
        }
    }

    pub fn load_font_from_path(&mut self, path: &str, face_index: u32) -> bool {
        use memmap2::MmapOptions;

        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(e) => {
                warn!("Failed to open primary font path {}: {:?}", path, e);
                return false;
            }
        };

        let mmap = match unsafe { MmapOptions::new().map(&file) } {
            Ok(mmap) => mmap,
            Err(e) => {
                warn!("Failed to mmap primary font path {}: {:?}", path, e);
                return false;
            }
        };

        self.reset_glyph_cache();
        self.renderer.load_font_path(path, face_index);
        if let Some(wrapper) = FontWrapper::from_mmap_with_index(mmap, 0, face_index) {
            self.font = Some(wrapper);
            true
        } else {
            self.font = None;
            warn!("Failed to parse primary font path {}", path);
            false
        }
    }

    /// Load the primary font from a file descriptor using memory mapping.
    /// The fd is duplicated internally so the caller can close it after this call.
    #[cfg(unix)]
    pub fn load_font_from_fd(
        &mut self,
        fd: i32,
        offset: u64,
        length: Option<usize>,
        face_index: u32,
    ) -> bool {
        use memmap2::MmapOptions;
        use std::os::unix::io::FromRawFd;

        let dup_fd = unsafe { libc::dup(fd) };
        if dup_fd < 0 {
            return false;
        }

        let file = unsafe { std::fs::File::from_raw_fd(dup_fd) };
        let mmap = match unsafe { MmapOptions::new().map(&file) } {
            Ok(mmap) => mmap,
            Err(e) => {
                warn!("Failed to mmap primary font fd: {:?}", e);
                return false;
            }
        };

        self.reset_glyph_cache();
        let font_len = length.unwrap_or_else(|| mmap.len().saturating_sub(offset as usize));
        let font_start = offset as usize;
        let font_end = font_start.saturating_add(font_len).min(mmap.len());
        if font_start < font_end {
            self.renderer
                .load_font_bytes(mmap[font_start..font_end].to_vec(), face_index);
        }
        if let Some(wrapper) =
            FontWrapper::from_mmap_with_range(mmap, 0, face_index, offset as usize, length)
        {
            self.font = Some(wrapper);
            true
        } else {
            self.font = None;
            warn!("Failed to parse primary font from fd");
            false
        }
    }

    pub fn load_fallback_font_with_index(&mut self, font_bytes: Vec<u8>, face_index: u32) {
        let font_id = self.fallback_fonts.len() + 1; // 0 is primary
        info!(
            "Loading FALLBACK font #{}: {} bytes",
            font_id,
            font_bytes.len()
        );
        if let Some(wrapper) = FontWrapper::from_bytes_with_index(&font_bytes, font_id, face_index)
        {
            self.reset_glyph_cache();
            self.renderer.load_font_bytes(font_bytes, face_index);
            self.fallback_fonts.push(wrapper);
            info!(
                "FALLBACK font #{} loaded, total fallbacks: {}",
                font_id,
                self.fallback_fonts.len()
            );
        } else {
            warn!("ERROR: Failed to load fallback font #{}!", font_id);
        }
    }

    pub fn load_fallback_font_from_path(&mut self, path: &str, face_index: u32) -> bool {
        use memmap2::MmapOptions;

        let font_id = self.fallback_fonts.len() + 1;
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(e) => {
                warn!("Failed to open fallback font path {}: {:?}", path, e);
                return false;
            }
        };

        let mmap = match unsafe { MmapOptions::new().map(&file) } {
            Ok(mmap) => mmap,
            Err(e) => {
                warn!("Failed to mmap fallback font path {}: {:?}", path, e);
                return false;
            }
        };

        if let Some(wrapper) = FontWrapper::from_mmap_with_index(mmap, font_id, face_index) {
            self.reset_glyph_cache();
            self.renderer.load_font_path(path, face_index);
            self.fallback_fonts.push(wrapper);
            true
        } else {
            warn!("Failed to parse fallback font path {}", path);
            false
        }
    }

    /// Load a fallback font from a file descriptor using memory mapping.
    /// This is more memory-efficient as it doesn't copy the entire font into RAM.
    /// The fd is duplicated internally so the caller can close it after this call.
    #[cfg(unix)]
    pub fn load_fallback_font_from_fd(
        &mut self,
        fd: i32,
        offset: u64,
        length: Option<usize>,
        face_index: u32,
    ) -> bool {
        use memmap2::MmapOptions;
        use std::os::unix::io::FromRawFd;

        let font_id = self.fallback_fonts.len() + 1;
        #[cfg(debug_assertions)]
        eprintln!(
            "[TextEngine] load_fallback_font_from_fd #{}: fd={}",
            font_id, fd
        );

        // Duplicate the FD so we own it
        let dup_fd = unsafe { libc::dup(fd) };
        if dup_fd < 0 {
            #[cfg(debug_assertions)]
            eprintln!("[TextEngine] Failed to dup fd!");
            return false;
        }

        // Create a File from the duplicated FD
        let file = unsafe { std::fs::File::from_raw_fd(dup_fd) };

        // Memory-map the file and slice by offset/length inside FontWrapper.
        let mmap = match unsafe { MmapOptions::new().map(&file) } {
            Ok(m) => m,
            Err(_e) => {
                #[cfg(debug_assertions)]
                eprintln!("[TextEngine] Failed to mmap: {:?}", _e);
                return false;
            }
        };

        // Create FontWrapper from mmap
        let font_len = length.unwrap_or_else(|| mmap.len().saturating_sub(offset as usize));
        let font_start = offset as usize;
        let font_end = font_start.saturating_add(font_len).min(mmap.len());
        if font_start < font_end {
            self.renderer
                .load_font_bytes(mmap[font_start..font_end].to_vec(), face_index);
        }
        if let Some(wrapper) =
            FontWrapper::from_mmap_with_range(mmap, font_id, face_index, offset as usize, length)
        {
            self.reset_glyph_cache();
            self.fallback_fonts.push(wrapper);
            #[cfg(debug_assertions)]
            eprintln!(
                "[TextEngine] Fallback font #{} loaded via mmap, total: {}",
                font_id,
                self.fallback_fonts.len()
            );
            true
        } else {
            #[cfg(debug_assertions)]
            eprintln!("[TextEngine] Failed to parse font from mmap!");
            false
        }
    }

    pub fn get_pending_uploads(&mut self) -> Vec<PendingUpload> {
        std::mem::take(&mut self.pending_uploads)
    }

    pub fn pending_uploads(&self) -> &[PendingUpload] {
        &self.pending_uploads
    }

    pub fn clear_pending_uploads(&mut self) {
        self.pending_uploads.clear();
    }

    pub fn has_pending_uploads(&self) -> bool {
        !self.pending_uploads.is_empty()
    }

    pub fn get_atlas_size(&self) -> (u32, u32) {
        (self.atlas_width, self.atlas_height)
    }

    /// Load the platform's system fonts into the lyrics renderer's fallback pool
    /// (Android only; see `LyricsRenderer::load_system_fonts`). Returns how many
    /// font files were loaded.
    #[cfg(target_os = "android")]
    pub fn load_system_fonts(&mut self) -> usize {
        self.renderer.load_system_fonts()
    }

    pub fn set_lyrics_scene_json(&mut self, json: &str) -> String {
        self.renderer
            .set_scene_json(json)
            .and_then(|metrics| serde_json::to_string(&metrics).map_err(|e| e.to_string()))
            .unwrap_or_else(|error| format!(r#"{{"error":{}}}"#, serde_json::json!(error)))
    }

    pub fn get_lyrics_renderer_metrics_json(&self) -> String {
        self.renderer.metrics_json()
    }

    #[cfg(not(target_os = "android"))]
    pub fn render_lyrics_frame_into(&mut self, current_time_ms: i32, pixels: &mut [u8]) -> i32 {
        self.renderer.render_frame_into(current_time_ms, pixels)
    }

    #[cfg(target_os = "android")]
    pub unsafe fn set_android_render_surface(
        &mut self,
        env: *mut jni::sys::JNIEnv,
        surface: jni::sys::jobject,
        surface_width: u32,
        surface_height: u32,
        frame_width: u32,
        frame_height: u32,
    ) -> bool {
        self.gpu_renderer = None;
        match AndroidGpuRenderer::from_java_surface(
            env,
            surface,
            surface_width,
            surface_height,
            frame_width,
            frame_height,
        ) {
            Ok(renderer) => {
                self.gpu_renderer = Some(renderer);
                true
            }
            Err(error) => {
                warn!("Failed to create Android GPU lyrics surface: {}", error);
                false
            }
        }
    }

    /// Install a render surface from a pre-acquired `ANativeWindow` pointer.
    /// Unlike [`set_android_render_surface`], this carries no `JNIEnv` and so can
    /// run on the dedicated render thread. Ownership of `window_ptr` transfers to
    /// the renderer (released on failure by `from_window_ptr`).
    #[cfg(target_os = "android")]
    pub unsafe fn set_android_render_surface_from_window(
        &mut self,
        window_ptr: *mut std::ffi::c_void,
        frame_width: u32,
        frame_height: u32,
    ) -> bool {
        self.gpu_renderer = None;
        match AndroidGpuRenderer::from_window_ptr(window_ptr, frame_width, frame_height) {
            Ok(renderer) => {
                self.gpu_renderer = Some(renderer);
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
    }

    #[cfg(target_os = "android")]
    pub fn clear_android_render_surface(&mut self) {
        if let Some(renderer) = self.gpu_renderer.as_mut() {
            if let Err(error) = renderer.clear() {
                warn!("Failed to clear Android GPU lyrics surface: {}", error);
            }
        }
        self.gpu_renderer = None;
    }

    #[cfg(target_os = "android")]
    pub fn render_lyrics_frame_to_android_surface(&mut self, current_time_ms: i32) -> i32 {
        let Some(mut gpu_renderer) = self.gpu_renderer.take() else {
            return -20;
        };

        let mut render_result = 0;
        let present_result = gpu_renderer.draw_frame(|canvas| {
            render_result = self
                .renderer
                .render_frame_to_canvas(current_time_ms, canvas);
        });
        self.gpu_renderer = Some(gpu_renderer);
        if let Err(error) = present_result {
            warn!("Failed to render Android GPU lyrics frame: {}", error);
            return -21;
        }
        render_result
    }

    pub fn hit_test_lyrics_line(&mut self, x: f32, y: f32, current_time_ms: i32) -> i32 {
        self.renderer.hit_test_line(x, y, current_time_ms)
    }

    pub fn begin_lyrics_scroll(&mut self) {
        self.renderer.begin_manual_scroll();
    }

    pub fn scroll_lyrics_by(&mut self, delta_y: f32) {
        self.renderer.scroll_manual_by(delta_y);
    }

    pub fn end_lyrics_scroll(&mut self, velocity_y: f32) {
        self.renderer.end_manual_scroll(velocity_y);
    }

    pub fn cancel_lyrics_scroll(&mut self) {
        self.renderer.cancel_manual_scroll();
    }

    pub fn reset_lyrics_scroll(&mut self) {
        self.renderer.reset_manual_scroll();
    }

    fn reset_glyph_cache(&mut self) {
        self.atlas = AtlasManager::new(self.atlas_width, self.atlas_height);
        self.pending_uploads.clear();
    }

    pub fn process_text(&mut self, text: &str, size_px: f32, weight: f32) -> LayoutResult {
        let empty_result = LayoutResult {
            glyph_count: 0,
            glyph_ids: vec![],
            positions: vec![],
            atlas_rects: vec![],
            glyph_offsets: vec![],
            font_indices: vec![],
            total_width: 0.0,
            total_height: 0.0,
            ascent: 0.0,
            descent: 0.0,
        };

        if self.font.is_none() {
            return empty_result;
        }

        if text.is_empty() {
            return empty_result;
        }

        // Quantize weight to reduce cache fragmentation (round to nearest 100)
        let weight_key = ((weight / 100.0).round() * 100.0) as u32;

        info!("========= PROCESSING TEXT =========");
        info!("Input: \"{}\" ({} chars)", text, text.chars().count());
        info!(
            "Font tower: 1 primary + {} fallbacks",
            self.fallback_fonts.len()
        );

        let runs = self.group_into_runs(text);

        info!("Grouped into {} runs", runs.len());

        let mut all_glyph_ids: Vec<u16> = Vec::new();
        let mut all_positions: Vec<f32> = Vec::new();
        let mut all_atlas_rects: Vec<f32> = Vec::new();
        let mut all_glyph_offsets: Vec<f32> = Vec::new();
        let mut all_font_indices: Vec<u8> = Vec::new();

        let mut x_cursor: f32 = 0.0;
        let mut max_ascent: f32 = 0.0;
        let mut max_descent: f32 = 0.0;
        let mut max_height: f32 = 0.0;

        for run in runs {
            let run_text = run.text;
            let font_idx = run.font_index;

            let font_name = if font_idx == 0 {
                "PRIMARY".to_string()
            } else {
                format!("FALLBACK#{}", font_idx)
            };
            info!("Run: font={} text=\"{}\"", font_name, run_text);

            let Some((font_data_ref, face_index)) = self.font_data_for_index(font_idx) else {
                continue;
            };

            // Create Face for shaping
            let mut face = match Face::from_slice(font_data_ref, face_index) {
                Some(f) => f,
                None => continue,
            };

            // Set font weight variation
            face.set_variations(&[rustybuzz::Variation {
                tag: rustybuzz::ttf_parser::Tag::from_bytes(b"wght"),
                value: weight,
            }]);

            // Shape the run with its own font
            let mut buffer = UnicodeBuffer::new();
            buffer.push_str(&run_text);
            let glyph_buffer = rustybuzz::shape(&face, &[], buffer);
            let glyph_infos = glyph_buffer.glyph_infos();
            let glyph_positions = glyph_buffer.glyph_positions();

            let units_per_em = face.units_per_em() as f32;
            let scale = size_px / units_per_em;

            // Update max metrics
            let run_ascent = face.ascender() as f32 * scale;
            let run_descent = face.descender() as f32 * scale;
            let run_height = face.height() as f32 * scale;
            if run_ascent > max_ascent {
                max_ascent = run_ascent;
            }
            if run_descent.abs() > max_descent.abs() {
                max_descent = run_descent;
            }
            if run_height > max_height {
                max_height = run_height;
            }

            for (info, gp) in glyph_infos.iter().zip(glyph_positions.iter()) {
                let glyph_id = info.glyph_id as u16;

                let glyph_info = if let Some(cached) = self.atlas.get_glyph_info_with_weight(
                    font_idx,
                    glyph_id,
                    size_px as u32,
                    weight_key,
                ) {
                    cached
                } else {
                    let sdf_result = if font_idx == 0 {
                        self.font
                            .as_mut()
                            .map(|f| f.generate_sdf(glyph_id, size_px, weight))
                    } else {
                        self.fallback_fonts
                            .get_mut(font_idx - 1)
                            .map(|f| f.generate_sdf(glyph_id, size_px, weight))
                    };

                    if let Some((bitmap, w, h, xmin, ymin)) = sdf_result {
                        if w > 0 && h > 0 {
                            if let Some(alloc_rect) = self.atlas.allocate(w, h) {
                                self.pending_uploads.push(PendingUpload {
                                    x: alloc_rect.x,
                                    y: alloc_rect.y,
                                    width: w,
                                    height: h,
                                    data: bitmap,
                                });
                                let drawable_rect = Rect {
                                    x: alloc_rect.x,
                                    y: alloc_rect.y,
                                    width: w,
                                    height: h,
                                };
                                let info = crate::atlas::GlyphInfo {
                                    rect: drawable_rect,
                                    allocated_rect: alloc_rect,
                                    x_bearing: xmin,
                                    y_bearing: ymin,
                                    last_used: 0, // Will be set by cache_glyph_with_weight
                                };
                                self.atlas.cache_glyph_with_weight(
                                    font_idx,
                                    glyph_id,
                                    size_px as u32,
                                    weight_key,
                                    info,
                                );
                                info
                            } else {
                                Self::empty_glyph_info()
                            }
                        } else {
                            crate::atlas::GlyphInfo {
                                rect: Rect {
                                    x: 0,
                                    y: 0,
                                    width: 0,
                                    height: 0,
                                },
                                allocated_rect: Rect {
                                    x: 0,
                                    y: 0,
                                    width: 0,
                                    height: 0,
                                },
                                x_bearing: xmin,
                                y_bearing: ymin,
                                last_used: 0,
                            }
                        }
                    } else {
                        Self::empty_glyph_info()
                    }
                };

                all_glyph_ids.push(glyph_id);
                all_font_indices.push(font_idx as u8);

                let x_pos = x_cursor + (gp.x_offset as f32 * scale);
                let y_pos = gp.y_offset as f32 * scale;
                all_positions.push(x_pos);
                all_positions.push(y_pos);

                all_glyph_offsets.push(glyph_info.x_bearing);
                all_glyph_offsets.push(glyph_info.y_bearing);

                x_cursor += gp.x_advance as f32 * scale;

                all_atlas_rects.push(glyph_info.rect.x as f32);
                all_atlas_rects.push(glyph_info.rect.y as f32);
                all_atlas_rects.push(glyph_info.rect.width as f32);
                all_atlas_rects.push(glyph_info.rect.height as f32);
            }
        }

        LayoutResult {
            glyph_count: all_glyph_ids.len(),
            glyph_ids: all_glyph_ids,
            positions: all_positions,
            atlas_rects: all_atlas_rects,
            glyph_offsets: all_glyph_offsets,
            font_indices: all_font_indices,
            total_width: x_cursor,
            total_height: max_height,
            ascent: max_ascent,
            descent: max_descent,
        }
    }

    fn font_data_for_index(&self, font_idx: usize) -> Option<(&[u8], u32)> {
        if font_idx == 0 {
            self.font
                .as_ref()
                .map(|font| (font.font_data.deref(), font.face_index))
        } else {
            self.fallback_fonts
                .get(font_idx - 1)
                .map(|font| (font.font_data.deref(), font.face_index))
        }
    }

    fn empty_glyph_info() -> crate::atlas::GlyphInfo {
        crate::atlas::GlyphInfo {
            rect: Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            allocated_rect: Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            x_bearing: 0.0,
            y_bearing: 0.0,
            last_used: 0,
        }
    }

    fn group_into_runs(&self, text: &str) -> Vec<TextRun> {
        let mut runs = Vec::new();
        let mut current_font: Option<usize> = None;
        let mut current_text = String::new();

        for cluster in UnicodeSegmentation::graphemes(text, true) {
            let font_index = self.select_font_for_cluster(cluster);
            match current_font {
                Some(idx) if idx == font_index => current_text.push_str(cluster),
                Some(idx) => {
                    runs.push(TextRun {
                        text: std::mem::take(&mut current_text),
                        font_index: idx,
                    });
                    current_text.push_str(cluster);
                    current_font = Some(font_index);
                }
                None => {
                    current_text.push_str(cluster);
                    current_font = Some(font_index);
                }
            }
        }

        if let Some(idx) = current_font {
            runs.push(TextRun {
                text: current_text,
                font_index: idx,
            });
        }

        runs
    }

    fn select_font_for_cluster(&self, cluster: &str) -> usize {
        for font_idx in 0..=self.fallback_fonts.len() {
            if let Some((data, face_index)) = self.font_data_for_index(font_idx) {
                if Self::font_shapes_cluster(data, face_index, cluster) {
                    return font_idx;
                }
            }
        }

        warn!("WARNING: cluster has NO GLYPH in any font: {:?}", cluster);
        0
    }

    fn font_shapes_cluster(data: &[u8], face_index: u32, cluster: &str) -> bool {
        let Some(face) = Face::from_slice(data, face_index) else {
            return false;
        };

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(cluster);
        let glyph_buffer = rustybuzz::shape(&face, &[], buffer);
        let glyph_infos = glyph_buffer.glyph_infos();
        !glyph_infos.is_empty() && glyph_infos.iter().all(|info| info.glyph_id != 0)
    }
}
