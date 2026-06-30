#[cfg(not(target_os = "android"))]
use cosmic_text::SwashCache;
use cosmic_text::{
    fontdb, Align, Attrs, Buffer, Family, FontSystem, Metrics, PhysicalGlyph, Shaping, Weight, Wrap,
};
use serde::{Deserialize, Serialize};
use skia_safe::{font_style, Data, FontMgr, FontStyle, Typeface};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

mod draw;
mod font_fallback;
mod text_utils;

use draw::{
    accompaniment_visibility, draw_breathing_dots_skia, draw_prepared_text_skia,
    interlude_visibility, make_interlude_slot, rgba_from_argb,
};
#[cfg(not(target_os = "android"))]
use draw::{apply_vertical_fade, draw_breathing_dots, draw_prepared_text};
use font_fallback::{cjk_family_priority, new_font_system};
use text_utils::{
    contains_han, contains_rtl, has_trailing_whitespace, is_blank_text, is_punctuation_or_space,
    should_use_simple_animation, trailing_whitespace_count, trim_end_whitespace,
};

#[cfg(test)]
use draw::{awesome_glyph_effect_for_char, bounce, dip_and_rise, swell};

const DEFAULT_WIDTH: u32 = 1;
const DEFAULT_HEIGHT: u32 = 1;
const DEFAULT_PADDING_X: f32 = 16.0;
const DEFAULT_PADDING_Y: f32 = 8.0;
const DEFAULT_KEEP_ALIVE: f32 = 120.0;
const DEFAULT_NORMAL_FONT_SIZE: f32 = 34.0;
const DEFAULT_NORMAL_LINE_HEIGHT: f32 = 42.0;
const DEFAULT_ACCOMPANIMENT_FONT_SIZE: f32 = 20.0;
const DEFAULT_ACCOMPANIMENT_LINE_HEIGHT: f32 = 26.0;
const DEFAULT_TRANSLATION_FONT_SIZE: f32 = 16.0;
const DEFAULT_TRANSLATION_LINE_HEIGHT: f32 = 21.0;
const FADE_WIDTH: f32 = 100.0;
const ROW_GAP: f32 = 32.0;
const INTERLUDE_THRESHOLD_MS: i32 = 5000;
const DEFAULT_DOTS_NUMBER: u32 = 3;
const DEFAULT_DOTS_SIZE: f32 = 16.0;
const DEFAULT_DOTS_MARGIN: f32 = 12.0;
const DEFAULT_DOTS_ENTER_MS: f32 = 3000.0;
const DEFAULT_DOTS_STILL_MS: f32 = 200.0;
const DEFAULT_DOTS_DIP_MS: f32 = 3000.0;
const DEFAULT_DOTS_EXIT_MS: f32 = 200.0;
const DOTS_VERTICAL_PADDING: f32 = 12.0;
#[cfg(not(target_os = "android"))]
const TOP_FADE_PX: f32 = 20.0;
#[cfg(not(target_os = "android"))]
const BOTTOM_FADE_PX: f32 = 100.0;
const KARAOKE_INACTIVE_ALPHA: f32 = 0.2;
// Rows whose screen position is within this many line-heights of the focus
// anchor stay fully sharp, so the current cluster (main line plus the nested
// accompaniment a line or two below it — further still when the main carries a
// translation) is never blurred. Blur ramps up beyond this band.
const BLUR_SHARP_RADIUS_LINES: f32 = 2.5;
#[cfg(not(target_os = "android"))]
const MAX_GLYPH_BLUR_RADIUS: f32 = 36.0;
const SIMPLE_LIFT_PX: f32 = 4.0;
const SIMPLE_ANIMATION_DURATION_MS: f32 = 700.0;
const AWESOME_LIFT_PX: f32 = 4.0;
const AWESOME_FAST_CHAR_THRESHOLD_MS: f32 = 200.0;
const AWESOME_MIN_WORD_DURATION_MS: i32 = 1000;
const AWESOME_DURATION_RATIO: f32 = 0.8;
const AWESOME_MAX_SHADOW_BLUR_PX: f32 = 10.0;
// Base spring for the leading (focused) row. Low stiffness = a very soft, slow
// spring; damping gives a ratio of ~0.74 so it does ONE gentle stretch-and-
// settle and the energy dies quickly (no repeated wobbling). Far rows scale
// stiffness by `response` and damping by sqrt(response) so they stay at the same
// damping ratio while lagging behind — the springy "one after another" cascade.
const LINE_LAYOUT_SPRING_STIFFNESS: f32 = 100.0;
const LINE_LAYOUT_SPRING_DAMPING: f32 = 12.0;
// Each line is modeled as a mass connected by a spring to its neighbours so the
// list behaves like a chain during auto-scroll: the focused line leads and the
// rest follow one after another. A larger coupling makes the wave travel
// further along the list.
const LINE_LAYOUT_CHAIN_COUPLING: f32 = 0.65;
// How quickly the spring softens as lines get further from the focused row.
// Softer far-away rows lag behind the leading row, which is what produces the
// visible "spring up and settle" cascade. `MIN_RESPONSE` clamps how soft they
// can get so distant rows still eventually catch up.
const LINE_LAYOUT_DISTANCE_FALLOFF: f32 = 0.25;
const LINE_LAYOUT_MIN_RESPONSE: f32 = 0.35;
const LINE_LAYOUT_MAX_DT: f32 = 1.0 / 30.0;
// A tap-to-seek lands on a visible line, so it only ever needs to scroll about
// one viewport — let those animate with the spring. Only snap instantly when a
// seek would jump further than this many viewports (e.g. scrubbing the timeline
// to a far-away section, or loading a new song).
const LINE_LAYOUT_SEEK_RESET_DISTANCE_FACTOR: f32 = 1.6;
const LINE_LAYOUT_EPSILON: f32 = 0.08;
const MANUAL_SCROLL_HOLD_MS: u64 = 1800;
const MANUAL_SCROLL_MAX_FLING_VELOCITY: f32 = 14000.0;
const MANUAL_SCROLL_FLING_FRICTION: f32 = 4.8;
const MANUAL_SCROLL_VELOCITY_EPSILON: f32 = 14.0;
const MANUAL_SCROLL_RETURN_STIFFNESS: f32 = 360.0;
const MANUAL_SCROLL_RETURN_DAMPING: f32 = 32.0;
const MANUAL_SCROLL_OVERSCROLL_STIFFNESS: f32 = 520.0;
const MANUAL_SCROLL_OVERSCROLL_DAMPING: f32 = 38.0;
const MANUAL_SCROLL_RUBBER_BAND_LIMIT: f32 = 180.0;
// Manual scrolling releases the depth-of-field blur so the user can read while
// browsing. The blur stays released until this long after the *last touch input*
// (grab/drag/release), then eases back in — independent of the fling/return
// physics, so the automatic glide-back to the active line never re-toggles it.
// The fade-out is quicker than the fade-in so grabbing the list feels responsive
// while the blur eases back gently once you stop.
const MANUAL_SCROLL_BLUR_RESTORE_MS: u64 = 2500;
const MANUAL_SCROLL_BLUR_FADE_OUT_RATE: f32 = 16.0;
const MANUAL_SCROLL_BLUR_FADE_IN_RATE: f32 = 6.0;

#[derive(Debug, Deserialize)]
pub struct LyricsScene {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub locale: Option<String>,
    pub normal_font_size: Option<f32>,
    pub normal_line_height: Option<f32>,
    pub accompaniment_font_size: Option<f32>,
    pub accompaniment_line_height: Option<f32>,
    pub translation_font_size: Option<f32>,
    pub translation_line_height: Option<f32>,
    pub phonetic_gap: Option<f32>,
    pub padding_x: Option<f32>,
    pub padding_y: Option<f32>,
    pub keep_alive: Option<f32>,
    pub text_color: Option<u32>,
    pub show_translation: Option<bool>,
    pub show_phonetic: Option<bool>,
    pub use_blur_effect: Option<bool>,
    pub blur_delta: Option<f32>,
    pub phonetic_font_size: Option<f32>,
    pub phonetic_line_height: Option<f32>,
    pub breathing_dots_number: Option<u32>,
    pub breathing_dots_size: Option<f32>,
    pub breathing_dots_margin: Option<f32>,
    pub breathing_dots_enter_ms: Option<f32>,
    pub breathing_dots_still_ms: Option<f32>,
    pub breathing_dots_dip_ms: Option<f32>,
    pub breathing_dots_exit_ms: Option<f32>,
    pub breathing_dots_color: Option<u32>,
    pub lines: Vec<LyricsLineInput>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum LyricsLineInput {
    #[serde(rename = "karaoke")]
    Karaoke(KaraokeLineInput),
    #[serde(rename = "synced")]
    Synced(SyncedLineInput),
}

#[derive(Debug, Deserialize)]
pub struct KaraokeLineInput {
    pub source_index: Option<usize>,
    pub cluster_index: Option<usize>,
    pub cluster_role: Option<ClusterRoleInput>,
    pub start: i32,
    pub end: i32,
    pub is_accompaniment: bool,
    pub alignment: AlignmentInput,
    pub translation: Option<String>,
    pub phonetic: Option<String>,
    pub syllables: Vec<SyllableInput>,
}

#[derive(Debug, Deserialize)]
pub struct SyncedLineInput {
    pub source_index: Option<usize>,
    pub cluster_index: Option<usize>,
    pub cluster_role: Option<ClusterRoleInput>,
    pub start: i32,
    pub end: i32,
    pub content: String,
    pub translation: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SyllableInput {
    pub content: String,
    pub start: i32,
    pub end: i32,
    #[allow(dead_code)]
    pub phonetic: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlignmentInput {
    Start,
    End,
    Unspecified,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClusterRoleInput {
    Standalone,
    Main,
    BeforeAccompaniment,
    AfterAccompaniment,
}

#[derive(Debug, Serialize)]
pub struct RendererMetrics {
    pub width: u32,
    pub height: u32,
    pub line_count: usize,
    pub content_height: f32,
}

#[derive(Debug)]
pub struct LyricsRenderer {
    font_system: FontSystem,
    #[cfg(not(target_os = "android"))]
    swash_cache: SwashCache,
    font_stack: Vec<RendererFontFace>,
    skia_typefaces: HashMap<fontdb::ID, Typeface>,
    font_selection_cache: HashMap<String, Option<String>>,
    last_render_debug_time_ms: Option<i32>,
    spring_layouts: Vec<SpringLineState>,
    last_spring_frame_at: Option<Instant>,
    last_spring_playback_ms: Option<i32>,
    last_target_scroll_y: Option<f32>,
    layout_animation_active: bool,
    manual_scroll: ManualScrollState,
    last_manual_scroll_frame_at: Option<Instant>,
    manual_scroll_active: bool,
    /// 0.0 = depth-of-field blur fully applied, 1.0 = blur fully released.
    /// Ramps toward 1.0 while the user manually scrolls so everything is sharp,
    /// and back toward 0.0 once the list settles at the auto position again.
    manual_scroll_blur_release: f32,
    #[cfg(not(target_os = "android"))]
    blurred_glyph_cache: HashMap<BlurredGlyphCacheKey, BlurredGlyphMask>,
    locale: String,
    scene: Option<PreparedScene>,
}

#[derive(Debug, Clone)]
struct RendererFontFace {
    id: fontdb::ID,
    family_name: String,
}

#[derive(Debug)]
struct PreparedScene {
    config: SceneConfig,
    lines: Vec<PreparedLine>,
    content_height: f32,
}

#[derive(Debug, Default)]
struct TypefaceEnsureStats {
    scene_glyphs: usize,
    scene_font_ids: usize,
    typefaces_before: usize,
    typefaces_after: usize,
    missing_before: usize,
    missing_after: usize,
    loaded_from_system: usize,
    loaded_from_source: usize,
    failed_faces: Vec<String>,
}

#[derive(Debug, Default)]
struct FrameGlyphStats {
    visible_lines: usize,
    visible_glyphs: usize,
    visible_font_ids: usize,
    missing_typeface_glyphs: usize,
}

/// Per-line scroll spring. Only the *scroll* is sprung; each line's content-space
/// top and height stay deterministic, so an interlude/accompaniment growing or
/// shrinking moves the layout along a smooth eased curve instead of being sprung
/// (which overshot and made the whole list vibrate). The cascade lives here: rows
/// chase the same scroll target but soften/lag with distance and couple to
/// neighbours.
#[derive(Debug, Clone, Copy)]
struct SpringLineState {
    scroll: f32,
    velocity: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct ManualScrollState {
    offset: f32,
    velocity: f32,
    dragging: bool,
    hold_until: Option<Instant>,
    /// Blur stays released until this instant. Set purely from real touch input
    /// (grab / drag / release / cancel) and never touched by the fling/return
    /// physics, so the automatic glide-back can't re-trigger the blur.
    blur_engaged_until: Option<Instant>,
}

#[derive(Debug, Clone, Copy)]
struct SceneConfig {
    width: u32,
    height: u32,
    normal_font_size: f32,
    normal_line_height: f32,
    accompaniment_font_size: f32,
    accompaniment_line_height: f32,
    translation_font_size: f32,
    translation_line_height: f32,
    phonetic_font_size: f32,
    phonetic_line_height: f32,
    phonetic_gap: f32,
    padding_x: f32,
    padding_y: f32,
    keep_alive: f32,
    text_color: u32,
    show_translation: bool,
    show_phonetic: bool,
    use_blur_effect: bool,
    blur_delta: f32,
    breathing_dots: BreathingDotsConfig,
}

#[derive(Debug, Clone, Copy)]
struct BreathingDotsConfig {
    number: u32,
    size: f32,
    margin: f32,
    enter_ms: f32,
    still_ms: f32,
    dip_ms: f32,
    exit_ms: f32,
    color: u32,
}

#[derive(Debug)]
struct PreparedLine {
    source_index: usize,
    cluster_index: usize,
    cluster_role: ClusterRole,
    start: i32,
    end: i32,
    effective_end: i32,
    height: f32,
    right_aligned: bool,
    interlude: Option<PreparedInterlude>,
    kind: PreparedLineKind,
    translation: Option<PreparedText>,
    phonetic: Option<PreparedText>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClusterRole {
    Standalone,
    Main,
    BeforeAccompaniment,
    AfterAccompaniment,
}

impl ClusterRole {
    fn is_nested_accompaniment(self) -> bool {
        matches!(
            self,
            ClusterRole::BeforeAccompaniment | ClusterRole::AfterAccompaniment
        )
    }
}

impl From<ClusterRoleInput> for ClusterRole {
    fn from(value: ClusterRoleInput) -> Self {
        match value {
            ClusterRoleInput::Standalone => ClusterRole::Standalone,
            ClusterRoleInput::Main => ClusterRole::Main,
            ClusterRoleInput::BeforeAccompaniment => ClusterRole::BeforeAccompaniment,
            ClusterRoleInput::AfterAccompaniment => ClusterRole::AfterAccompaniment,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DynamicLineLayout {
    top: f32,
    height: f32,
    text_visibility: f32,
    interlude_visibility: f32,
}

#[derive(Debug)]
struct PreparedInterlude {
    start: i32,
    end: i32,
    right_aligned: bool,
    height: f32,
}

#[derive(Debug)]
enum PreparedLineKind {
    Karaoke {
        is_accompaniment: bool,
        is_rtl: bool,
        syllables: Vec<PreparedSyllable>,
        text: PreparedText,
    },
    Synced {
        text: PreparedText,
    },
}

#[derive(Debug, Clone)]
struct PreparedSyllable {
    content: String,
    start: i32,
    end: i32,
    word_id: usize,
    float_end: i32,
    use_awesome: bool,
    char_count: usize,
    char_offset_in_word: usize,
    word_start: i32,
    word_end: i32,
    word_duration: i32,
    word_char_count: usize,
    word_pivot_x: f32,
    word_pivot_y: f32,
    layout_x: f32,
    layout_width: f32,
}

#[derive(Debug, Clone)]
struct PreparedText {
    rows: Vec<PreparedRow>,
    height: f32,
    first_baseline: f32,
}

#[derive(Debug, Clone)]
struct PreparedRow {
    y: f32,
    width: f32,
    min_x: f32,
    max_x: f32,
    glyphs: Vec<PreparedGlyph>,
}

#[derive(Debug, Clone)]
struct PreparedGlyph {
    physical: PhysicalGlyph,
    x: f32,
    syllable_index: Option<usize>,
    glyph_index_in_syllable: usize,
    animation_char_index: f32,
    alpha_multiplier: f32,
    is_phonetic: bool,
}

#[derive(Debug, Clone, Copy)]
struct KaraokeBrush {
    active_edge: f32,
    row_min_x: f32,
    row_max_x: f32,
    is_rtl: bool,
}

#[derive(Debug, Clone, Copy)]
struct GlyphRenderEffect {
    offset_y: f32,
    scale: f32,
    shadow_blur_radius: f32,
    scale_pivot: Option<(f32, f32)>,
}

#[cfg(not(target_os = "android"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BlurredGlyphCacheKey {
    cache_key: cosmic_text::CacheKey,
    radius: u8,
}

#[cfg(not(target_os = "android"))]
#[derive(Debug)]
struct BlurredGlyphMask {
    origin_x: i32,
    origin_y: i32,
    width: usize,
    height: usize,
    alpha: Vec<u8>,
}

impl Default for GlyphRenderEffect {
    fn default() -> Self {
        Self {
            offset_y: 0.0,
            scale: 1.0,
            shadow_blur_radius: 0.0,
            scale_pivot: None,
        }
    }
}

fn match_skia_typeface_for_face(face: &fontdb::FaceInfo) -> Option<Typeface> {
    let style = FontStyle::new(
        font_style::Weight::from(face.weight.0 as i32),
        font_style::Width::NORMAL,
        match face.style {
            fontdb::Style::Normal => font_style::Slant::Upright,
            fontdb::Style::Italic => font_style::Slant::Italic,
            fontdb::Style::Oblique => font_style::Slant::Oblique,
        },
    );

    with_skia_font_mgr(|font_mgr| {
        face.families
            .iter()
            .map(|(name, _)| name.as_str())
            .chain(std::iter::once(face.post_script_name.as_str()))
            .filter(|name| !name.is_empty())
            .find_map(|name| font_mgr.match_family_style(name, style))
    })
}

fn with_skia_font_mgr<R>(f: impl FnOnce(&FontMgr) -> R) -> R {
    thread_local! {
        static FONT_MGR: std::cell::RefCell<Option<FontMgr>> = std::cell::RefCell::new(None);
    }

    FONT_MGR.with(|cell| {
        let needs_init = cell.borrow().is_none();
        if needs_init {
            *cell.borrow_mut() = Some(FontMgr::new());
        }
        let font_mgr = cell.borrow();
        f(font_mgr
            .as_ref()
            .expect("thread-local Skia FontMgr must be initialized"))
    })
}

fn skia_typeface_from_path(path: &std::path::Path, face_index: u32) -> Option<Typeface> {
    let data = Data::from_filename(path)?;
    skia_typeface_from_data(data, face_index)
}

fn skia_typeface_from_bytes(bytes: &[u8], face_index: u32) -> Option<Typeface> {
    let data = Data::new_copy(bytes);
    skia_typeface_from_data(data, face_index)
}

fn skia_typeface_from_data(data: Data, face_index: u32) -> Option<Typeface> {
    FontMgr::new().new_from_data(data.as_bytes(), face_index as usize)
}

fn skia_typeface_from_face_source(face: &fontdb::FaceInfo) -> Option<Typeface> {
    match &face.source {
        fontdb::Source::Binary(bytes) => {
            skia_typeface_from_bytes(bytes.as_ref().as_ref(), face.index)
        }
        fontdb::Source::File(path) => skia_typeface_from_path(path, face.index),
        fontdb::Source::SharedFile(path, _) => skia_typeface_from_path(path, face.index),
    }
}

fn collect_text_font_usage(text: &PreparedText, font_ids: &mut Vec<fontdb::ID>) -> usize {
    let mut glyph_count = 0;
    for row in &text.rows {
        for glyph in &row.glyphs {
            glyph_count += 1;
            let font_id = glyph.physical.cache_key.font_id;
            if !font_ids.contains(&font_id) {
                font_ids.push(font_id);
            }
        }
    }
    glyph_count
}

fn count_text_missing_typeface_glyphs(
    text: &PreparedText,
    typefaces: &HashMap<fontdb::ID, Typeface>,
) -> usize {
    text.rows
        .iter()
        .flat_map(|row| row.glyphs.iter())
        .filter(|glyph| !typefaces.contains_key(&glyph.physical.cache_key.font_id))
        .count()
}

fn describe_font_face(id: fontdb::ID, face: &fontdb::FaceInfo) -> String {
    let family = face
        .families
        .first()
        .map(|(name, _)| name.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(face.post_script_name.as_str());
    let source = match &face.source {
        fontdb::Source::Binary(_) => "binary".to_string(),
        fontdb::Source::File(path) => path.display().to_string(),
        fontdb::Source::SharedFile(path, _) => path.display().to_string(),
    };
    format!(
        "id={:?} family={} index={} weight={} style={:?} source={}",
        id, family, face.index, face.weight.0, face.style, source
    )
}

#[cfg(target_os = "android")]
fn should_read_font_path_for_skia(path: &str) -> bool {
    let path = path.replace('\\', "/");
    !(path.starts_with("/system/fonts/")
        || path.starts_with("/apex/")
        || path.starts_with("/product/fonts/")
        || path.starts_with("/vendor/fonts/"))
}

#[cfg(not(target_os = "android"))]
fn should_read_font_path_for_skia(_path: &str) -> bool {
    true
}

#[derive(Debug, Clone)]
struct MeasuredSyllable {
    index: usize,
    word_id: usize,
    content: String,
    use_awesome: bool,
    text: PreparedText,
    phonetic: Option<PreparedText>,
    width: f32,
    first_baseline: f32,
    height: f32,
}

#[derive(Debug, Clone)]
struct WrappedMeasuredLine {
    syllables: Vec<MeasuredSyllable>,
    total_width: f32,
}

impl LyricsRenderer {
    pub fn new() -> Self {
        Self {
            font_system: new_font_system("en-US".to_string(), fontdb::Database::new()),
            #[cfg(not(target_os = "android"))]
            swash_cache: SwashCache::new(),
            font_stack: Vec::new(),
            skia_typefaces: HashMap::new(),
            font_selection_cache: HashMap::new(),
            last_render_debug_time_ms: None,
            spring_layouts: Vec::new(),
            last_spring_frame_at: None,
            last_spring_playback_ms: None,
            last_target_scroll_y: None,
            layout_animation_active: false,
            manual_scroll: ManualScrollState::default(),
            last_manual_scroll_frame_at: None,
            manual_scroll_active: false,
            manual_scroll_blur_release: 0.0,
            #[cfg(not(target_os = "android"))]
            blurred_glyph_cache: HashMap::new(),
            locale: "en-US".to_string(),
            scene: None,
        }
    }

    #[cfg(not(target_os = "android"))]
    fn reset_cpu_render_cache(&mut self) {
        self.blurred_glyph_cache.clear();
        self.swash_cache = SwashCache::new();
    }

    #[cfg(target_os = "android")]
    fn reset_cpu_render_cache(&mut self) {}

    pub fn load_font_bytes(&mut self, bytes: Vec<u8>, face_index: u32) {
        let skia_typeface = skia_typeface_from_bytes(bytes.as_slice(), face_index);
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(fontdb::Source::Binary(Arc::new(bytes)));
        self.register_loaded_face(ids.as_slice(), face_index, skia_typeface);
        self.font_selection_cache.clear();
        self.reset_cpu_render_cache();
        self.reset_layout_animation_state();
        self.reset_manual_scroll();
        self.scene = None;
    }

    pub fn load_font_path(&mut self, path: &str, face_index: u32) -> bool {
        if !std::path::Path::new(path).exists() {
            return false;
        }
        let skia_typeface = if should_read_font_path_for_skia(path) {
            skia_typeface_from_path(std::path::Path::new(path), face_index)
        } else {
            None
        };
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(fontdb::Source::File(std::path::PathBuf::from(path)));
        self.register_loaded_face(ids.as_slice(), face_index, skia_typeface);
        self.font_selection_cache.clear();
        self.reset_cpu_render_cache();
        self.reset_layout_animation_state();
        self.reset_manual_scroll();
        self.scene = None;
        true
    }

    fn register_loaded_face(
        &mut self,
        ids: &[fontdb::ID],
        face_index: u32,
        skia_typeface: Option<Typeface>,
    ) {
        let selected_id = ids
            .iter()
            .copied()
            .find(|id| {
                self.font_system
                    .db()
                    .face(*id)
                    .is_some_and(|face| face.index == face_index)
            })
            .or_else(|| ids.first().copied());

        let Some(id) = selected_id else {
            return;
        };

        let Some((family_name, typeface)) = self.font_system.db().face(id).map(|face| {
            let family_name = face
                .families
                .first()
                .map(|(name, _)| name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| face.post_script_name.clone());
            let typeface = skia_typeface.or_else(|| match_skia_typeface_for_face(face));
            (family_name, typeface)
        }) else {
            return;
        };

        if family_name.is_empty() {
            return;
        }

        self.font_stack.push(RendererFontFace { id, family_name });
        if let Some(typeface) = typeface {
            self.skia_typefaces.insert(id, typeface);
        }
    }

    fn ensure_skia_typefaces_for_scene(&mut self) -> TypefaceEnsureStats {
        let Some(scene) = &self.scene else {
            return TypefaceEnsureStats::default();
        };

        let mut scene_font_ids = Vec::new();
        let mut scene_glyphs = 0;
        for line in &scene.lines {
            match &line.kind {
                PreparedLineKind::Karaoke { text, .. } => {
                    scene_glyphs += collect_text_font_usage(text, &mut scene_font_ids);
                }
                PreparedLineKind::Synced { text } => {
                    scene_glyphs += collect_text_font_usage(text, &mut scene_font_ids);
                }
            }
            if let Some(translation) = &line.translation {
                scene_glyphs += collect_text_font_usage(translation, &mut scene_font_ids);
            }
            if let Some(phonetic) = &line.phonetic {
                scene_glyphs += collect_text_font_usage(phonetic, &mut scene_font_ids);
            }
        }

        let missing_ids: Vec<fontdb::ID> = scene_font_ids
            .iter()
            .copied()
            .filter(|id| !self.skia_typefaces.contains_key(id))
            .collect();
        let mut stats = TypefaceEnsureStats {
            scene_glyphs,
            scene_font_ids: scene_font_ids.len(),
            typefaces_before: self.skia_typefaces.len(),
            missing_before: missing_ids.len(),
            ..TypefaceEnsureStats::default()
        };

        for id in missing_ids {
            if self.skia_typefaces.contains_key(&id) {
                continue;
            }

            let Some(face) = self.font_system.db().face(id) else {
                stats
                    .failed_faces
                    .push(format!("id={:?} face=<missing>", id));
                continue;
            };

            if let Some(typeface) = match_skia_typeface_for_face(face) {
                self.skia_typefaces.insert(id, typeface);
                stats.loaded_from_system += 1;
                continue;
            }

            if let Some(typeface) = skia_typeface_from_face_source(face) {
                self.skia_typefaces.insert(id, typeface);
                stats.loaded_from_source += 1;
            } else {
                stats.failed_faces.push(describe_font_face(id, face));
            }
        }

        stats.typefaces_after = self.skia_typefaces.len();
        stats.missing_after = scene_font_ids
            .iter()
            .filter(|id| !self.skia_typefaces.contains_key(id))
            .count();
        stats
    }

    fn should_log_render_debug(&mut self, current_time_ms: i32) -> bool {
        let should_log = self
            .last_render_debug_time_ms
            .is_none_or(|last| (current_time_ms - last).abs() >= 1000);
        if should_log {
            self.last_render_debug_time_ms = Some(current_time_ms);
        }
        should_log
    }

    fn reset_layout_animation_state(&mut self) {
        self.spring_layouts.clear();
        self.last_spring_frame_at = None;
        self.last_spring_playback_ms = None;
        self.last_target_scroll_y = None;
        self.layout_animation_active = false;
    }

    pub fn reset_manual_scroll(&mut self) {
        self.manual_scroll = ManualScrollState::default();
        self.last_manual_scroll_frame_at = None;
        self.manual_scroll_active = false;
        self.manual_scroll_blur_release = 0.0;
    }

    fn engage_manual_scroll_blur(&mut self, now: Instant) {
        self.manual_scroll.blur_engaged_until =
            Some(now + Duration::from_millis(MANUAL_SCROLL_BLUR_RESTORE_MS));
    }

    pub fn begin_manual_scroll(&mut self) {
        let now = Instant::now();
        self.manual_scroll.dragging = true;
        self.manual_scroll.velocity = 0.0;
        self.manual_scroll.hold_until = None;
        self.manual_scroll_active = true;
        self.last_manual_scroll_frame_at = Some(now);
        self.engage_manual_scroll_blur(now);
    }

    pub fn scroll_manual_by(&mut self, delta_y: f32) {
        if !delta_y.is_finite() {
            return;
        }
        self.manual_scroll.offset += delta_y;
        self.manual_scroll.velocity = 0.0;
        self.manual_scroll.hold_until = None;
        self.manual_scroll.dragging = true;
        self.manual_scroll_active = true;
        self.engage_manual_scroll_blur(Instant::now());
    }

    pub fn end_manual_scroll(&mut self, velocity_y: f32) {
        let now = Instant::now();
        self.manual_scroll.dragging = false;
        self.manual_scroll.velocity = velocity_y.clamp(
            -MANUAL_SCROLL_MAX_FLING_VELOCITY,
            MANUAL_SCROLL_MAX_FLING_VELOCITY,
        );
        if self.manual_scroll.velocity.abs() <= MANUAL_SCROLL_VELOCITY_EPSILON {
            self.manual_scroll.velocity = 0.0;
            self.manual_scroll.hold_until =
                Some(now + Duration::from_millis(MANUAL_SCROLL_HOLD_MS));
        } else {
            self.manual_scroll.hold_until = None;
        }
        self.manual_scroll_active = true;
        self.last_manual_scroll_frame_at = Some(now);
        self.engage_manual_scroll_blur(now);
    }

    pub fn cancel_manual_scroll(&mut self) {
        let now = Instant::now();
        self.manual_scroll.dragging = false;
        self.manual_scroll.velocity = 0.0;
        self.manual_scroll.hold_until =
            Some(now + Duration::from_millis(MANUAL_SCROLL_HOLD_MS / 2));
        self.manual_scroll_active = self.manual_scroll.offset.abs() > LINE_LAYOUT_EPSILON;
        self.engage_manual_scroll_blur(now);
    }

    fn update_manual_scroll_target(&mut self, auto_scroll_y: f32, max_scroll_y: f32) -> f32 {
        let now = Instant::now();
        let dt = self
            .last_manual_scroll_frame_at
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or(0.0)
            .clamp(0.001, LINE_LAYOUT_MAX_DT);
        self.last_manual_scroll_frame_at = Some(now);

        let mut active = self.manual_scroll.dragging;
        if !self.manual_scroll.dragging {
            if self.manual_scroll.velocity.abs() > MANUAL_SCROLL_VELOCITY_EPSILON {
                self.manual_scroll.offset += self.manual_scroll.velocity * dt;
                self.manual_scroll.velocity *= (-MANUAL_SCROLL_FLING_FRICTION * dt).exp();
                if self.manual_scroll.velocity.abs() <= MANUAL_SCROLL_VELOCITY_EPSILON {
                    self.manual_scroll.velocity = 0.0;
                    self.manual_scroll.hold_until =
                        Some(now + Duration::from_millis(MANUAL_SCROLL_HOLD_MS));
                }
                active = true;
            }

            let lower_offset = -auto_scroll_y;
            let upper_offset = max_scroll_y - auto_scroll_y;
            let bounded_offset = self.manual_scroll.offset.clamp(lower_offset, upper_offset);
            let overscrolled =
                (self.manual_scroll.offset - bounded_offset).abs() > LINE_LAYOUT_EPSILON;
            if overscrolled {
                self.manual_scroll.hold_until = None;
                active |= spring_step(
                    &mut self.manual_scroll.offset,
                    &mut self.manual_scroll.velocity,
                    bounded_offset,
                    MANUAL_SCROLL_OVERSCROLL_STIFFNESS,
                    MANUAL_SCROLL_OVERSCROLL_DAMPING,
                    dt,
                );
            } else if self.manual_scroll.velocity == 0.0
                && self.manual_scroll.offset.abs() > LINE_LAYOUT_EPSILON
            {
                let hold_until = self
                    .manual_scroll
                    .hold_until
                    .get_or_insert_with(|| now + Duration::from_millis(MANUAL_SCROLL_HOLD_MS));
                if now >= *hold_until {
                    active |= spring_step(
                        &mut self.manual_scroll.offset,
                        &mut self.manual_scroll.velocity,
                        0.0,
                        MANUAL_SCROLL_RETURN_STIFFNESS,
                        MANUAL_SCROLL_RETURN_DAMPING,
                        dt,
                    );
                } else {
                    active = true;
                }
            } else if self.manual_scroll.offset.abs() <= LINE_LAYOUT_EPSILON
                && self.manual_scroll.velocity.abs() <= MANUAL_SCROLL_VELOCITY_EPSILON
            {
                self.manual_scroll.offset = 0.0;
                self.manual_scroll.velocity = 0.0;
                self.manual_scroll.hold_until = None;
            }
        }

        let mut manual_scroll_active = active
            || self.manual_scroll.dragging
            || self.manual_scroll.offset.abs() > LINE_LAYOUT_EPSILON
            || self.manual_scroll.velocity.abs() > MANUAL_SCROLL_VELOCITY_EPSILON;

        // Depth-of-field blur is released while the finger is down and for a
        // fixed window after the last touch input, then eases back in. This is a
        // pure, monotonic timer driven only by touch events — the fling/return
        // physics never touch `blur_engaged_until`, so the automatic glide-back
        // to the active line (or normal playback auto-scroll) can never flip the
        // blur off/on again. Keep rendering while the blur is engaged or fading
        // so the ease-in still runs even when playback is paused.
        let blur_engaged = self.manual_scroll.dragging
            || self
                .manual_scroll
                .blur_engaged_until
                .is_some_and(|until| now < until);
        let blur_target = if blur_engaged { 1.0 } else { 0.0 };
        if blur_engaged {
            manual_scroll_active = true;
        }
        if (self.manual_scroll_blur_release - blur_target).abs() > 0.001 {
            let rate = if blur_target > self.manual_scroll_blur_release {
                MANUAL_SCROLL_BLUR_FADE_OUT_RATE
            } else {
                MANUAL_SCROLL_BLUR_FADE_IN_RATE
            };
            let factor = 1.0 - (-rate * dt).exp();
            self.manual_scroll_blur_release += (blur_target - self.manual_scroll_blur_release) * factor;
            if (self.manual_scroll_blur_release - blur_target).abs() <= 0.001 {
                self.manual_scroll_blur_release = blur_target;
            } else {
                manual_scroll_active = true;
            }
        }

        self.manual_scroll_active = manual_scroll_active;
        self.manual_scroll_projected_scroll(auto_scroll_y, max_scroll_y)
    }

    fn manual_scroll_projected_scroll(&self, auto_scroll_y: f32, max_scroll_y: f32) -> f32 {
        let raw_scroll_y = auto_scroll_y + self.manual_scroll.offset;
        rubber_band_scroll(raw_scroll_y, max_scroll_y)
    }

    /// Advances the per-line scroll springs one frame and returns each line's
    /// on-screen layout. The content-space top and height come straight from
    /// `target_layouts` (deterministic, already eased), and only the *scroll*
    /// offset is sprung per line — so a row's screen top is
    /// `content_top - scroll[i]`.
    ///
    /// Splitting scroll out from the layout is what stops interlude/accompaniment
    /// resizes from vibrating: a height change moves the deterministic content
    /// tops smoothly without perturbing any spring. Meanwhile the focused row's
    /// scroll spring is stiff and far rows soften/lag and couple to neighbours,
    /// so a focus change still ripples through the list like a spring chain.
    fn animate_frame_layout(
        &mut self,
        current_time_ms: i32,
        target_layouts: &[DynamicLineLayout],
        target_scroll_y: f32,
        viewport_height: f32,
        focus_end: usize,
    ) -> Vec<DynamicLineLayout> {
        let now = Instant::now();
        let project = |scroll_of: &dyn Fn(usize) -> f32| -> Vec<DynamicLineLayout> {
            target_layouts
                .iter()
                .enumerate()
                .map(|(index, layout)| DynamicLineLayout {
                    top: layout.top - scroll_of(index),
                    ..*layout
                })
                .collect()
        };

        // Snap (rather than animate) only when the geometry can't be carried
        // over: the scene changed, this is the first frame, or the scroll has to
        // jump further than a tap could ever require. A tap-to-seek lands on a
        // visible row, so its scroll delta stays under the threshold and springs
        // the list to the new position — giving the seek its scroll animation.
        let seek_reset_distance =
            (viewport_height * LINE_LAYOUT_SEEK_RESET_DISTANCE_FACTOR).max(1.0);
        let scroll_jump = self
            .last_target_scroll_y
            .map(|last| (target_scroll_y - last).abs());
        // Don't snap while a manual scroll/fling/return is in flight — that
        // motion is user-driven and always smooth, and a large spring-back could
        // otherwise trip the distance threshold and make the list jump.
        let should_reset = target_layouts.len() != self.spring_layouts.len()
            || self.last_spring_playback_ms.is_none()
            || (!self.manual_scroll_active
                && scroll_jump.is_none_or(|jump| jump > seek_reset_distance));

        if should_reset {
            self.spring_layouts = vec![
                SpringLineState {
                    scroll: target_scroll_y,
                    velocity: 0.0,
                };
                target_layouts.len()
            ];
            self.last_spring_frame_at = Some(now);
            self.last_spring_playback_ms = Some(current_time_ms);
            self.last_target_scroll_y = Some(target_scroll_y);
            self.layout_animation_active = false;
            return project(&|_| target_scroll_y);
        }

        let dt = self
            .last_spring_frame_at
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or(0.0)
            .clamp(0.001, LINE_LAYOUT_MAX_DT);
        self.last_spring_frame_at = Some(now);
        self.last_spring_playback_ms = Some(current_time_ms);
        self.last_target_scroll_y = Some(target_scroll_y);

        // While the finger is down the list must track 1:1, so snap every line's
        // scroll to the target and skip the cascade (which is meant for
        // auto-scroll and fling, not direct dragging).
        if self.manual_scroll.dragging {
            for state in self.spring_layouts.iter_mut() {
                state.scroll = target_scroll_y;
                state.velocity = 0.0;
            }
            self.layout_animation_active = false;
            return project(&|_| target_scroll_y);
        }

        // Everything from the focused row upward moves as one rigid block: those
        // rows share the full-stiffness spring and take no coupling, so they just
        // shove up together to clear room for the focused row. The spring chain —
        // softening/lagging with distance and coupled to the row above — only runs
        // *below* the focus, so the upcoming lines stretch and settle one after
        // another while the already-sung lines above leave cleanly as a slab.
        let count = self.spring_layouts.len();
        let anchor_hi = focus_end.min(count.saturating_sub(1));
        let mut chained_targets = vec![target_scroll_y; count];
        for index in (anchor_hi + 1)..count {
            let previous_delta = self.spring_layouts[index - 1].scroll - target_scroll_y;
            chained_targets[index] += previous_delta * LINE_LAYOUT_CHAIN_COUPLING;
        }

        let mut active = false;
        for (index, state) in self.spring_layouts.iter_mut().enumerate() {
            // Focus row and everything above it = rigid block (response 1.0); only
            // rows below the focus soften with distance to form the cascade.
            let response = if index > focus_end {
                (1.0 - (index - focus_end) as f32 * LINE_LAYOUT_DISTANCE_FALLOFF)
                    .clamp(LINE_LAYOUT_MIN_RESPONSE, 1.0)
            } else {
                1.0
            };
            // Far rows soften (lower stiffness) so they lag and create the
            // cascade, but their damping is scaled by sqrt(response) instead of
            // response. That keeps the damping *ratio* constant across every row
            // (ratio scales with damping / sqrt(stiffness)), so distant rows do a
            // single soft stretch-and-settle like the leading row instead of
            // dropping underdamped and wobbling back and forth.
            let damping_response = response.powf(0.3);
            active |= spring_step(
                &mut state.scroll,
                &mut state.velocity,
                chained_targets[index],
                LINE_LAYOUT_SPRING_STIFFNESS * response,
                LINE_LAYOUT_SPRING_DAMPING * damping_response,
                dt,
            );
        }

        self.layout_animation_active = active;
        let scrolls = self
            .spring_layouts
            .iter()
            .map(|state| state.scroll)
            .collect::<Vec<_>>();
        project(&|index| scrolls[index])
    }

    pub fn set_scene_json(&mut self, json: &str) -> Result<RendererMetrics, String> {
        let scene: LyricsScene = serde_json::from_str(json).map_err(|error| error.to_string())?;
        let prepared = self.prepare_scene(scene)?;
        let metrics = RendererMetrics {
            width: prepared.config.width,
            height: prepared.config.height,
            line_count: prepared.lines.len(),
            content_height: prepared.content_height,
        };
        self.scene = Some(prepared);
        self.reset_layout_animation_state();
        self.reset_manual_scroll();
        Ok(metrics)
    }

    fn set_locale(&mut self, locale: &str) {
        if self.font_system.locale() == locale {
            self.locale = locale.to_string();
            return;
        }

        let replacement = new_font_system(locale.to_string(), fontdb::Database::new());
        let old = std::mem::replace(&mut self.font_system, replacement);
        let (_, db) = old.into_locale_and_db();
        self.font_system = new_font_system(locale.to_string(), db);
        self.locale = locale.to_string();
        self.font_selection_cache.clear();
        self.reset_cpu_render_cache();
    }

    pub fn metrics_json(&self) -> String {
        let Some(scene) = &self.scene else {
            return "{}".to_string();
        };
        serde_json::to_string(&RendererMetrics {
            width: scene.config.width,
            height: scene.config.height,
            line_count: scene.lines.len(),
            content_height: scene.content_height,
        })
        .unwrap_or_else(|_| "{}".to_string())
    }

    #[cfg(not(target_os = "android"))]
    pub fn render_frame_into(&mut self, current_time_ms: i32, pixels: &mut [u8]) -> i32 {
        let Some(scene) = &self.scene else {
            return -3;
        };

        let width = scene.config.width.max(DEFAULT_WIDTH);
        let height = scene.config.height.max(DEFAULT_HEIGHT);
        let required = width as usize * height as usize * 4;
        if pixels.len() < required {
            return -2;
        }

        pixels[..required].fill(0);

        let dynamic_layouts = scene.dynamic_line_layouts(current_time_ms);
        let scroll_y = scene.scroll_y_for_time_with_layouts(current_time_ms, &dynamic_layouts);
        let visible_top = scroll_y - scene.config.keep_alive;
        let visible_bottom = scroll_y + height as f32 + scene.config.keep_alive;
        let base_color = rgba_from_argb(scene.config.text_color);

        for (line_index, line) in scene.lines.iter().enumerate() {
            let Some(dynamic_layout) = dynamic_layouts.get(line_index) else {
                continue;
            };
            if dynamic_layout.height <= 0.001 {
                continue;
            }

            let line_top = dynamic_layout.top;
            let line_bottom = dynamic_layout.top + dynamic_layout.height;
            if line_bottom < visible_top || line_top > visible_bottom {
                continue;
            }

            let y = line_top - scroll_y;
            let text_y_offset = line
                .interlude
                .as_ref()
                .map(|slot| slot.height * dynamic_layout.interlude_visibility)
                .unwrap_or(0.0);
            let distance_alpha =
                scene.focus_alpha(line, current_time_ms) * dynamic_layout.text_visibility;
            let blur_radius = scene.blur_radius_for_screen_y(y, scene.config.keep_alive);

            if let Some(interlude) = &line.interlude {
                if dynamic_layout.interlude_visibility > 0.001 {
                    draw_breathing_dots(
                        pixels,
                        width,
                        height,
                        y + DOTS_VERTICAL_PADDING,
                        interlude,
                        &scene.config,
                        current_time_ms,
                        scene.focus_alpha(line, current_time_ms)
                            * dynamic_layout.interlude_visibility,
                    );
                }
            }

            if dynamic_layout.text_visibility <= 0.001 {
                continue;
            }

            match &line.kind {
                PreparedLineKind::Karaoke {
                    is_accompaniment,
                    is_rtl,
                    syllables,
                    text,
                } => {
                    let line_alpha = if *is_accompaniment { 0.68 } else { 1.0 } * distance_alpha;
                    draw_prepared_text(
                        &mut self.font_system,
                        &mut self.swash_cache,
                        pixels,
                        width,
                        height,
                        &mut self.blurred_glyph_cache,
                        text,
                        scene.config.padding_x,
                        y + text_y_offset + scene.config.padding_y,
                        base_color,
                        line_alpha,
                        blur_radius,
                        Some((current_time_ms, *is_rtl, syllables)),
                    );
                }
                PreparedLineKind::Synced { text } => {
                    draw_prepared_text(
                        &mut self.font_system,
                        &mut self.swash_cache,
                        pixels,
                        width,
                        height,
                        &mut self.blurred_glyph_cache,
                        text,
                        scene.config.padding_x,
                        y + text_y_offset + scene.config.padding_y,
                        base_color,
                        distance_alpha,
                        blur_radius,
                        None,
                    );
                }
            }

            let mut detail_y =
                y + text_y_offset + scene.config.padding_y + line.main_text_height() + ROW_GAP;
            if let Some(translation) = &line.translation {
                draw_prepared_text(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    pixels,
                    width,
                    height,
                    &mut self.blurred_glyph_cache,
                    translation,
                    scene.config.padding_x,
                    detail_y,
                    base_color,
                    0.42 * distance_alpha,
                    blur_radius,
                    None,
                );
                detail_y += translation.height + ROW_GAP;
            }
            if let Some(phonetic) = &line.phonetic {
                draw_prepared_text(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    pixels,
                    width,
                    height,
                    &mut self.blurred_glyph_cache,
                    phonetic,
                    scene.config.padding_x,
                    detail_y,
                    base_color,
                    0.55 * distance_alpha,
                    blur_radius,
                    None,
                );
            }
        }

        apply_vertical_fade(pixels, width, height, TOP_FADE_PX, BOTTOM_FADE_PX);
        0
    }

    pub fn render_frame_to_canvas(
        &mut self,
        current_time_ms: i32,
        canvas: &skia_safe::Canvas,
    ) -> i32 {
        let typeface_stats = self.ensure_skia_typefaces_for_scene();
        let should_log_debug = self.should_log_render_debug(current_time_ms);

        let (
            target_layouts,
            auto_scroll_y,
            max_scroll_y,
            height,
            keep_alive,
            base_color,
            focus_end,
        ) = {
            let Some(scene) = &self.scene else {
                return -3;
            };

            let target_layouts = scene.dynamic_line_layouts(current_time_ms);
            let auto_scroll_y =
                scene.scroll_y_for_time_with_layouts(current_time_ms, &target_layouts);
            let max_scroll_y = scene.max_scroll_for_layouts(&target_layouts);
            let (_focus_start, focus_end) = scene.focus_index_range(current_time_ms);
            (
                target_layouts,
                auto_scroll_y,
                max_scroll_y,
                scene.config.height.max(DEFAULT_HEIGHT),
                scene.config.keep_alive,
                rgba_from_argb(scene.config.text_color),
                focus_end,
            )
        };

        let target_scroll_y = self.update_manual_scroll_target(auto_scroll_y, max_scroll_y);
        let dynamic_layouts = self.animate_frame_layout(
            current_time_ms,
            &target_layouts,
            target_scroll_y,
            height as f32,
            focus_end,
        );
        // While the user manually scrolls the depth-of-field blur is eased away
        // so the lyrics stay sharp for reading.
        let blur_scale = (1.0 - self.manual_scroll_blur_release).clamp(0.0, 1.0);
        let Some(scene) = &self.scene else {
            return -3;
        };

        let mut frame_stats = FrameGlyphStats::default();
        let mut visible_font_ids = Vec::new();
        // `dynamic_layouts` are already in screen space (scroll folded in), so the
        // visible window is simply the surface plus the keep-alive margin.
        let visible_top = -keep_alive;
        let visible_bottom = height as f32 + keep_alive;

        for (line_index, line) in scene.lines.iter().enumerate() {
            let Some(dynamic_layout) = dynamic_layouts.get(line_index) else {
                continue;
            };
            if dynamic_layout.height <= 0.001 {
                continue;
            }

            let line_top = dynamic_layout.top;
            let line_bottom = dynamic_layout.top + dynamic_layout.height;
            if line_bottom < visible_top || line_top > visible_bottom {
                continue;
            }

            let y = line_top;
            let text_y_offset = line
                .interlude
                .as_ref()
                .map(|slot| slot.height * dynamic_layout.interlude_visibility)
                .unwrap_or(0.0);
            let distance_alpha =
                scene.focus_alpha(line, current_time_ms) * dynamic_layout.text_visibility;
            let blur_radius = scene.blur_radius_for_screen_y(y, keep_alive) * blur_scale;

            if let Some(interlude) = &line.interlude {
                if dynamic_layout.interlude_visibility > 0.001 {
                    draw_breathing_dots_skia(
                        canvas,
                        y + DOTS_VERTICAL_PADDING,
                        interlude,
                        &scene.config,
                        current_time_ms,
                        scene.focus_alpha(line, current_time_ms)
                            * dynamic_layout.interlude_visibility,
                    );
                }
            }

            if dynamic_layout.text_visibility <= 0.001 {
                continue;
            }

            if should_log_debug {
                frame_stats.visible_lines += 1;
            }

            match &line.kind {
                PreparedLineKind::Karaoke {
                    is_accompaniment,
                    is_rtl,
                    syllables,
                    text,
                } => {
                    if should_log_debug {
                        frame_stats.visible_glyphs +=
                            collect_text_font_usage(text, &mut visible_font_ids);
                        frame_stats.missing_typeface_glyphs +=
                            count_text_missing_typeface_glyphs(text, &self.skia_typefaces);
                    }
                    let line_alpha = if *is_accompaniment { 0.68 } else { 1.0 } * distance_alpha;
                    draw_prepared_text_skia(
                        canvas,
                        &self.skia_typefaces,
                        text,
                        scene.config.padding_x,
                        y + text_y_offset + scene.config.padding_y,
                        base_color,
                        line_alpha,
                        blur_radius,
                        Some((current_time_ms, *is_rtl, syllables)),
                    );
                }
                PreparedLineKind::Synced { text } => {
                    if should_log_debug {
                        frame_stats.visible_glyphs +=
                            collect_text_font_usage(text, &mut visible_font_ids);
                        frame_stats.missing_typeface_glyphs +=
                            count_text_missing_typeface_glyphs(text, &self.skia_typefaces);
                    }
                    draw_prepared_text_skia(
                        canvas,
                        &self.skia_typefaces,
                        text,
                        scene.config.padding_x,
                        y + text_y_offset + scene.config.padding_y,
                        base_color,
                        distance_alpha,
                        blur_radius,
                        None,
                    );
                }
            }

            let mut detail_y =
                y + text_y_offset + scene.config.padding_y + line.main_text_height() + ROW_GAP;
            if let Some(translation) = &line.translation {
                if should_log_debug {
                    frame_stats.visible_glyphs +=
                        collect_text_font_usage(translation, &mut visible_font_ids);
                    frame_stats.missing_typeface_glyphs +=
                        count_text_missing_typeface_glyphs(translation, &self.skia_typefaces);
                }
                draw_prepared_text_skia(
                    canvas,
                    &self.skia_typefaces,
                    translation,
                    scene.config.padding_x,
                    detail_y,
                    base_color,
                    0.42 * distance_alpha,
                    blur_radius,
                    None,
                );
                detail_y += translation.height + ROW_GAP;
            }
            if let Some(phonetic) = &line.phonetic {
                if should_log_debug {
                    frame_stats.visible_glyphs +=
                        collect_text_font_usage(phonetic, &mut visible_font_ids);
                    frame_stats.missing_typeface_glyphs +=
                        count_text_missing_typeface_glyphs(phonetic, &self.skia_typefaces);
                }
                draw_prepared_text_skia(
                    canvas,
                    &self.skia_typefaces,
                    phonetic,
                    scene.config.padding_x,
                    detail_y,
                    base_color,
                    0.55 * distance_alpha,
                    blur_radius,
                    None,
                );
            }
        }

        if should_log_debug {
            frame_stats.visible_font_ids = visible_font_ids.len();
            info!(
                "[LyricsRenderer] frame time={} visible_lines={} visible_glyphs={} visible_font_ids={} missing_visible_glyphs={} scene_glyphs={} scene_font_ids={} typefaces_before={} typefaces_after={} missing_scene_before={} missing_scene_after={} loaded_system={} loaded_source={} failed_faces={}",
                current_time_ms,
                frame_stats.visible_lines,
                frame_stats.visible_glyphs,
                frame_stats.visible_font_ids,
                frame_stats.missing_typeface_glyphs,
                typeface_stats.scene_glyphs,
                typeface_stats.scene_font_ids,
                typeface_stats.typefaces_before,
                typeface_stats.typefaces_after,
                typeface_stats.missing_before,
                typeface_stats.missing_after,
                typeface_stats.loaded_from_system,
                typeface_stats.loaded_from_source,
                typeface_stats.failed_faces.len()
            );
            if !typeface_stats.failed_faces.is_empty() {
                warn!(
                    "[LyricsRenderer] failed typefaces: {}",
                    typeface_stats
                        .failed_faces
                        .iter()
                        .take(6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" | ")
                );
            }
        }

        if self.layout_animation_active || self.manual_scroll_active {
            1
        } else {
            0
        }
    }

    pub fn hit_test_line(&self, x: f32, y: f32, current_time_ms: i32) -> i32 {
        let Some(scene) = &self.scene else {
            return -1;
        };

        if x < 0.0 || y < 0.0 || x > scene.config.width as f32 || y > scene.config.height as f32 {
            return -1;
        }

        let dynamic_layouts = scene.dynamic_line_layouts(current_time_ms);
        let auto_scroll_y = scene.scroll_y_for_time_with_layouts(current_time_ms, &dynamic_layouts);
        let scroll_y = self.manual_scroll_projected_scroll(
            auto_scroll_y,
            scene.max_scroll_for_layouts(&dynamic_layouts),
        );
        let content_y = y + scroll_y;
        scene
            .lines
            .iter()
            .enumerate()
            .find(|(index, _)| {
                dynamic_layouts.get(*index).is_some_and(|layout| {
                    layout.text_visibility > 0.001
                        && content_y >= layout.top
                        && content_y <= layout.top + layout.height
                })
            })
            .map(|(_, line)| line.source_index as i32)
            .unwrap_or(-1)
    }

    fn prepare_scene(&mut self, scene: LyricsScene) -> Result<PreparedScene, String> {
        let locale = scene.locale.as_deref().unwrap_or("en-US");
        self.set_locale(locale);

        let config = SceneConfig {
            width: scene.width.unwrap_or(DEFAULT_WIDTH).max(DEFAULT_WIDTH),
            height: scene.height.unwrap_or(DEFAULT_HEIGHT).max(DEFAULT_HEIGHT),
            normal_font_size: scene.normal_font_size.unwrap_or(DEFAULT_NORMAL_FONT_SIZE),
            normal_line_height: scene
                .normal_line_height
                .unwrap_or(DEFAULT_NORMAL_LINE_HEIGHT),
            accompaniment_font_size: scene
                .accompaniment_font_size
                .unwrap_or(DEFAULT_ACCOMPANIMENT_FONT_SIZE),
            accompaniment_line_height: scene
                .accompaniment_line_height
                .unwrap_or(DEFAULT_ACCOMPANIMENT_LINE_HEIGHT),
            translation_font_size: scene
                .translation_font_size
                .unwrap_or(DEFAULT_TRANSLATION_FONT_SIZE),
            translation_line_height: scene
                .translation_line_height
                .unwrap_or(DEFAULT_TRANSLATION_LINE_HEIGHT),
            phonetic_font_size: scene.phonetic_font_size.unwrap_or_else(|| {
                scene
                    .translation_font_size
                    .unwrap_or(DEFAULT_TRANSLATION_FONT_SIZE)
            }),
            phonetic_line_height: scene.phonetic_line_height.unwrap_or_else(|| {
                scene
                    .translation_line_height
                    .unwrap_or(DEFAULT_TRANSLATION_LINE_HEIGHT)
            }),
            phonetic_gap: scene.phonetic_gap.unwrap_or(4.0).max(0.0),
            padding_x: scene.padding_x.unwrap_or(DEFAULT_PADDING_X),
            padding_y: scene.padding_y.unwrap_or(DEFAULT_PADDING_Y),
            keep_alive: scene.keep_alive.unwrap_or(DEFAULT_KEEP_ALIVE),
            text_color: scene.text_color.unwrap_or(0xffff_ffff),
            show_translation: scene.show_translation.unwrap_or(true),
            show_phonetic: scene.show_phonetic.unwrap_or(true),
            use_blur_effect: scene.use_blur_effect.unwrap_or(true),
            blur_delta: scene.blur_delta.unwrap_or(3.0).max(0.0),
            breathing_dots: BreathingDotsConfig {
                number: scene
                    .breathing_dots_number
                    .unwrap_or(DEFAULT_DOTS_NUMBER)
                    .clamp(1, 8),
                size: scene
                    .breathing_dots_size
                    .unwrap_or(DEFAULT_DOTS_SIZE)
                    .max(1.0),
                margin: scene
                    .breathing_dots_margin
                    .unwrap_or(DEFAULT_DOTS_MARGIN)
                    .max(0.0),
                enter_ms: scene
                    .breathing_dots_enter_ms
                    .unwrap_or(DEFAULT_DOTS_ENTER_MS)
                    .max(1.0),
                still_ms: scene
                    .breathing_dots_still_ms
                    .unwrap_or(DEFAULT_DOTS_STILL_MS)
                    .max(0.0),
                dip_ms: scene
                    .breathing_dots_dip_ms
                    .unwrap_or(DEFAULT_DOTS_DIP_MS)
                    .max(1.0),
                exit_ms: scene
                    .breathing_dots_exit_ms
                    .unwrap_or(DEFAULT_DOTS_EXIT_MS)
                    .max(1.0),
                color: scene
                    .breathing_dots_color
                    .unwrap_or_else(|| scene.text_color.unwrap_or(0xffff_ffff)),
            },
        };

        let content_width = (config.width as f32 - config.padding_x * 2.0).max(1.0);
        let mut lines = Vec::with_capacity(scene.lines.len());
        let mut cursor_y = config.keep_alive;
        let mut previous_end: Option<i32> = None;
        let mut previous_right_aligned = false;

        for (line_index, input) in scene.lines.into_iter().enumerate() {
            let mut prepared = match input {
                LyricsLineInput::Karaoke(line) => {
                    let source_index = line.source_index.unwrap_or(line_index);
                    let cluster_index = line.cluster_index.unwrap_or(source_index);
                    let cluster_role = line
                        .cluster_role
                        .map(ClusterRole::from)
                        .unwrap_or(ClusterRole::Standalone);
                    let font_size = if line.is_accompaniment {
                        config.accompaniment_font_size
                    } else {
                        config.normal_font_size
                    };
                    let line_height = if line.is_accompaniment {
                        config.accompaniment_line_height
                    } else {
                        config.normal_line_height
                    };
                    let is_rtl = line.syllables.iter().any(|s| contains_rtl(&s.content));
                    let right_aligned = match line.alignment {
                        AlignmentInput::Start | AlignmentInput::Unspecified => is_rtl,
                        AlignmentInput::End => !is_rtl,
                    };
                    let mut prepared_syllables =
                        prepare_karaoke_syllables(&line.syllables, line.is_accompaniment);
                    let prepared_text = self.prepare_karaoke_text_layout(
                        &line.syllables,
                        &mut prepared_syllables,
                        font_size,
                        line_height,
                        content_width,
                        right_aligned,
                        is_rtl,
                        config.show_phonetic,
                        config.phonetic_font_size,
                        config.phonetic_line_height,
                        config.phonetic_gap,
                    );
                    let translation = if config.show_translation {
                        line.translation.as_deref().and_then(|translation| {
                            self.prepare_detail_text(
                                translation,
                                config.translation_font_size,
                                config.translation_line_height,
                                content_width,
                                right_aligned,
                            )
                        })
                    } else {
                        None
                    };
                    let phonetic = if config.show_phonetic {
                        line.phonetic.as_deref().and_then(|phonetic| {
                            self.prepare_detail_text(
                                phonetic,
                                config.phonetic_font_size,
                                config.phonetic_line_height,
                                content_width,
                                right_aligned,
                            )
                        })
                    } else {
                        None
                    };
                    let mut height = prepared_text.height + config.padding_y * 2.0;
                    if let Some(translation) = &translation {
                        height += translation.height + ROW_GAP;
                    }
                    if let Some(phonetic) = &phonetic {
                        height += phonetic.height + ROW_GAP;
                    }
                    PreparedLine {
                        source_index,
                        cluster_index,
                        cluster_role,
                        start: line.start,
                        end: line.end,
                        effective_end: line.end,
                        height,
                        right_aligned,
                        interlude: None,
                        kind: PreparedLineKind::Karaoke {
                            is_accompaniment: line.is_accompaniment,
                            is_rtl,
                            syllables: prepared_syllables,
                            text: prepared_text,
                        },
                        translation,
                        phonetic,
                    }
                }
                LyricsLineInput::Synced(line) => {
                    let source_index = line.source_index.unwrap_or(line_index);
                    let cluster_index = line.cluster_index.unwrap_or(source_index);
                    let cluster_role = line
                        .cluster_role
                        .map(ClusterRole::from)
                        .unwrap_or(ClusterRole::Standalone);
                    let is_rtl = contains_rtl(&line.content);
                    let text = self.prepare_plain_text(
                        &line.content,
                        config.normal_font_size,
                        config.normal_line_height,
                        content_width,
                        is_rtl,
                    );
                    let translation = if config.show_translation {
                        line.translation.as_deref().and_then(|translation| {
                            self.prepare_detail_text(
                                translation,
                                config.translation_font_size,
                                config.translation_line_height,
                                content_width,
                                is_rtl,
                            )
                        })
                    } else {
                        None
                    };
                    let mut height = text.height + config.padding_y * 2.0;
                    if let Some(translation) = &translation {
                        height += translation.height + ROW_GAP;
                    }
                    PreparedLine {
                        source_index,
                        cluster_index,
                        cluster_role,
                        start: line.start,
                        end: line.end,
                        effective_end: line.end,
                        height,
                        right_aligned: is_rtl,
                        interlude: None,
                        kind: PreparedLineKind::Synced { text },
                        translation,
                        phonetic: None,
                    }
                }
            };

            if let Some(interlude) = make_interlude_slot(
                line_index,
                prepared.start,
                previous_end,
                if line_index == 0 {
                    prepared.right_aligned
                } else {
                    previous_right_aligned
                },
                &config,
            ) {
                prepared.height += interlude.height;
                prepared.interlude = Some(interlude);
            }

            cursor_y += prepared.height;
            previous_end = Some(prepared.end);
            previous_right_aligned = prepared.right_aligned;
            lines.push(prepared);
        }

        let mut cluster_end_times = HashMap::<usize, i32>::new();
        for line in &lines {
            cluster_end_times
                .entry(line.cluster_index)
                .and_modify(|end| *end = (*end).max(line.end))
                .or_insert(line.end);
        }
        for line in &mut lines {
            if line.cluster_role == ClusterRole::Main {
                if let Some(end) = cluster_end_times.get(&line.cluster_index) {
                    line.effective_end = *end;
                }
            }
        }

        Ok(PreparedScene {
            config,
            lines,
            content_height: cursor_y + config.keep_alive,
        })
    }

    fn prepare_detail_text(
        &mut self,
        text: &str,
        font_size: f32,
        line_height: f32,
        width: f32,
        right_aligned: bool,
    ) -> Option<PreparedText> {
        if text.trim().is_empty() {
            return None;
        }
        Some(self.prepare_plain_text(text, font_size, line_height, width, right_aligned))
    }

    fn prepare_plain_text(
        &mut self,
        text: &str,
        font_size: f32,
        line_height: f32,
        width: f32,
        right_aligned: bool,
    ) -> PreparedText {
        self.prepare_text_with_metadata(
            std::iter::once((text, 0usize)),
            text,
            font_size,
            line_height,
            width,
            right_aligned,
        )
    }

    fn prepare_karaoke_text_layout(
        &mut self,
        input: &[SyllableInput],
        syllables: &mut [PreparedSyllable],
        font_size: f32,
        line_height: f32,
        width: f32,
        right_aligned: bool,
        is_rtl: bool,
        show_phonetic: bool,
        phonetic_font_size: f32,
        phonetic_line_height: f32,
        phonetic_gap: f32,
    ) -> PreparedText {
        if input.is_empty() {
            return PreparedText {
                rows: Vec::new(),
                height: line_height,
                first_baseline: line_height,
            };
        }

        let space_width = self.measure_karaoke_space_width(font_size, line_height);
        let measured = input
            .iter()
            .enumerate()
            .map(|(index, syllable)| {
                let word_id = syllables
                    .get(index)
                    .map(|item| item.word_id)
                    .unwrap_or(index);
                let use_awesome = syllables
                    .get(index)
                    .map(|item| item.use_awesome)
                    .unwrap_or(false);
                self.measure_karaoke_syllable(
                    index,
                    word_id,
                    use_awesome,
                    &syllable.content,
                    syllable.phonetic.as_deref(),
                    font_size,
                    line_height,
                    show_phonetic,
                    phonetic_font_size,
                    phonetic_line_height,
                    space_width,
                )
            })
            .collect::<Vec<_>>();

        let wrapped = self.calculate_balanced_lines(&measured, width, font_size, line_height);
        self.position_karaoke_wrapped_lines(
            wrapped,
            syllables,
            width,
            line_height,
            phonetic_line_height,
            phonetic_gap,
            right_aligned,
            is_rtl,
        )
    }

    fn measure_karaoke_space_width(&mut self, font_size: f32, line_height: f32) -> f32 {
        let text = self.prepare_text_with_metadata(
            std::iter::once((" ", 0usize)),
            " ",
            font_size,
            line_height,
            font_size.max(1.0) * 4.0,
            false,
        );
        prepared_text_width(&text).max(font_size * 0.25)
    }

    fn measure_karaoke_syllable(
        &mut self,
        index: usize,
        word_id: usize,
        use_awesome: bool,
        content: &str,
        phonetic: Option<&str>,
        font_size: f32,
        line_height: f32,
        show_phonetic: bool,
        phonetic_font_size: f32,
        phonetic_line_height: f32,
        space_width: f32,
    ) -> MeasuredSyllable {
        let text = self.prepare_single_syllable_text(content, index, font_size, line_height);
        let mut width = prepared_text_width(&text);
        let phonetic_text = if show_phonetic {
            phonetic
                .filter(|value| !value.trim().is_empty())
                .map(|value| {
                    self.prepare_text_with_metadata(
                        std::iter::once((value, index + 1)),
                        value,
                        phonetic_font_size,
                        phonetic_line_height,
                        1_000_000.0,
                        false,
                    )
                })
        } else {
            None
        };

        let trailing_spaces = trailing_whitespace_count(content);
        if trailing_spaces > 0 {
            let trimmed = trim_end_whitespace(content);
            let trimmed_width = if trimmed.is_empty() {
                0.0
            } else {
                let trimmed_text =
                    self.prepare_single_syllable_text(trimmed, index, font_size, line_height);
                prepared_text_width(&trimmed_text)
            };
            if width <= trimmed_width + 0.5 {
                width = trimmed_width + space_width * trailing_spaces as f32;
            }
        }
        if let Some(phonetic_text) = &phonetic_text {
            width = width.max(prepared_text_width(phonetic_text));
        }

        let draw_text = if use_awesome {
            self.prepare_awesome_syllable_text(content, index, font_size, line_height)
        } else {
            text.clone()
        };

        MeasuredSyllable {
            index,
            word_id,
            content: content.to_string(),
            use_awesome,
            first_baseline: text.first_baseline,
            height: text.height,
            text: draw_text,
            phonetic: phonetic_text,
            width,
        }
    }

    fn prepare_awesome_syllable_text(
        &mut self,
        content: &str,
        index: usize,
        font_size: f32,
        line_height: f32,
    ) -> PreparedText {
        let mut glyphs = Vec::new();
        let mut x_offset = 0.0f32;
        let mut first_baseline = None;

        for (char_index, ch) in content.chars().enumerate() {
            let char_text = ch.to_string();
            let measured =
                self.prepare_single_syllable_text(&char_text, index, font_size, line_height);
            first_baseline.get_or_insert(measured.first_baseline);
            for source_row in &measured.rows {
                for source_glyph in &source_row.glyphs {
                    let mut glyph = source_glyph.clone();
                    glyph.physical.x += x_offset.round() as i32;
                    glyph.x += x_offset;
                    glyph.glyph_index_in_syllable = char_index;
                    glyph.animation_char_index = char_index as f32;
                    glyphs.push(glyph);
                }
            }
            x_offset += prepared_text_width(&measured);
        }

        PreparedText {
            rows: vec![PreparedRow {
                y: 0.0,
                width: x_offset,
                min_x: 0.0,
                max_x: x_offset,
                glyphs,
            }],
            height: line_height,
            first_baseline: first_baseline.unwrap_or(line_height),
        }
    }

    fn prepare_single_syllable_text(
        &mut self,
        content: &str,
        index: usize,
        font_size: f32,
        line_height: f32,
    ) -> PreparedText {
        self.prepare_text_with_metadata(
            std::iter::once((content, index + 1)),
            content,
            font_size,
            line_height,
            1_000_000.0,
            false,
        )
    }

    fn calculate_balanced_lines(
        &mut self,
        syllable_layouts: &[MeasuredSyllable],
        available_width: f32,
        font_size: f32,
        line_height: f32,
    ) -> Vec<WrappedMeasuredLine> {
        if syllable_layouts.is_empty() {
            return Vec::new();
        }

        let n = syllable_layouts.len();
        let mut costs = vec![f64::INFINITY; n + 1];
        let mut breaks = vec![0usize; n + 1];
        costs[0] = 0.0;

        for i in 1..=n {
            let mut current_line_width = 0.0f32;
            for j in (1..=i).rev() {
                if j > 1 && syllable_layouts[j - 2].word_id == syllable_layouts[j - 1].word_id {
                    current_line_width += syllable_layouts[j - 1].width;
                    if current_line_width > available_width {
                        break;
                    }
                    continue;
                }

                current_line_width += syllable_layouts[j - 1].width;
                if current_line_width > available_width {
                    break;
                }

                let badness = (available_width - current_line_width).powi(2) as f64;
                if costs[j - 1].is_finite() && costs[j - 1] + badness < costs[i] {
                    costs[i] = costs[j - 1] + badness;
                    breaks[i] = j - 1;
                }
            }
        }

        if !costs[n].is_finite() {
            return self.calculate_greedy_wrapped_lines(
                syllable_layouts,
                available_width,
                font_size,
                line_height,
            );
        }

        let mut lines = Vec::new();
        let mut current_index = n;
        while current_index > 0 {
            let start_index = breaks[current_index];
            if start_index >= current_index {
                return self.calculate_greedy_wrapped_lines(
                    syllable_layouts,
                    available_width,
                    font_size,
                    line_height,
                );
            }

            let trimmed = self.trim_display_line_trailing_spaces(
                &syllable_layouts[start_index..current_index],
                font_size,
                line_height,
            );
            if !trimmed.syllables.is_empty() {
                lines.insert(0, trimmed);
            }
            current_index = start_index;
        }

        lines
    }

    fn calculate_greedy_wrapped_lines(
        &mut self,
        syllable_layouts: &[MeasuredSyllable],
        available_width: f32,
        font_size: f32,
        line_height: f32,
    ) -> Vec<WrappedMeasuredLine> {
        let mut lines = Vec::new();
        let mut current_line = Vec::<MeasuredSyllable>::new();
        let mut current_line_width = 0.0f32;

        let mut word_groups = Vec::<Vec<MeasuredSyllable>>::new();
        if let Some(first) = syllable_layouts.first() {
            let mut current_word_id = first.word_id;
            let mut current_word_group = Vec::new();
            for layout in syllable_layouts {
                if layout.word_id != current_word_id {
                    word_groups.push(current_word_group);
                    current_word_group = Vec::new();
                    current_word_id = layout.word_id;
                }
                current_word_group.push(layout.clone());
            }
            word_groups.push(current_word_group);
        }

        for word_syllables in word_groups {
            let word_width = word_syllables
                .iter()
                .map(|layout| layout.width)
                .sum::<f32>();

            if current_line_width + word_width <= available_width {
                current_line_width += word_width;
                current_line.extend(word_syllables);
                continue;
            }

            if !current_line.is_empty() {
                let trimmed =
                    self.trim_display_line_trailing_spaces(&current_line, font_size, line_height);
                if !trimmed.syllables.is_empty() {
                    lines.push(trimmed);
                }
                current_line.clear();
                current_line_width = 0.0;
            }

            if word_width <= available_width {
                current_line_width += word_width;
                current_line.extend(word_syllables);
            } else {
                for syllable in word_syllables {
                    if current_line_width + syllable.width > available_width
                        && !current_line.is_empty()
                    {
                        let trimmed = self.trim_display_line_trailing_spaces(
                            &current_line,
                            font_size,
                            line_height,
                        );
                        if !trimmed.syllables.is_empty() {
                            lines.push(trimmed);
                        }
                        current_line.clear();
                        current_line_width = 0.0;
                    }
                    current_line_width += syllable.width;
                    current_line.push(syllable);
                }
            }
        }

        if !current_line.is_empty() {
            let trimmed =
                self.trim_display_line_trailing_spaces(&current_line, font_size, line_height);
            if !trimmed.syllables.is_empty() {
                lines.push(trimmed);
            }
        }

        lines
    }

    fn trim_display_line_trailing_spaces(
        &mut self,
        display_line_syllables: &[MeasuredSyllable],
        font_size: f32,
        line_height: f32,
    ) -> WrappedMeasuredLine {
        if display_line_syllables.is_empty() {
            return WrappedMeasuredLine {
                syllables: Vec::new(),
                total_width: 0.0,
            };
        }

        let mut processed = display_line_syllables.to_vec();
        while processed
            .last()
            .is_some_and(|layout| is_blank_text(&layout.content))
        {
            processed.pop();
        }

        if processed.is_empty() {
            return WrappedMeasuredLine {
                syllables: Vec::new(),
                total_width: 0.0,
            };
        }

        let last_index = processed.len() - 1;
        let original_content = processed[last_index].content.clone();
        let trimmed_content = trim_end_whitespace(&original_content);
        if trimmed_content.len() < original_content.len() {
            if trimmed_content.is_empty() {
                processed.pop();
            } else {
                let index = processed[last_index].index;
                let text = self.prepare_single_syllable_text(
                    trimmed_content,
                    index,
                    font_size,
                    line_height,
                );
                let draw_text = if processed[last_index].use_awesome {
                    self.prepare_awesome_syllable_text(
                        trimmed_content,
                        index,
                        font_size,
                        line_height,
                    )
                } else {
                    text.clone()
                };
                let phonetic_width = processed[last_index]
                    .phonetic
                    .as_ref()
                    .map(prepared_text_width)
                    .unwrap_or(0.0);
                let mut replacement = processed[last_index].clone();
                replacement.content = trimmed_content.to_string();
                replacement.first_baseline = text.first_baseline;
                replacement.height = text.height;
                replacement.text = draw_text;
                replacement.width = prepared_text_width(&text).max(phonetic_width);
                processed[last_index] = replacement;
            }
        }

        let total_width = processed.iter().map(|layout| layout.width).sum::<f32>();
        WrappedMeasuredLine {
            syllables: processed,
            total_width,
        }
    }

    fn position_karaoke_wrapped_lines(
        &self,
        wrapped_lines: Vec<WrappedMeasuredLine>,
        syllables: &mut [PreparedSyllable],
        canvas_width: f32,
        line_height: f32,
        phonetic_line_height: f32,
        phonetic_gap: f32,
        right_aligned: bool,
        is_rtl: bool,
    ) -> PreparedText {
        let mut rows = Vec::new();
        let mut first_baseline = None;
        let mut bounds_by_word = HashMap::<usize, (f32, f32, f32)>::new();
        let has_phonetic_in_block = wrapped_lines.iter().any(|line| {
            line.syllables
                .iter()
                .any(|layout| layout.phonetic.is_some())
        });
        let row_height = line_height
            + if has_phonetic_in_block {
                phonetic_line_height * 0.7
            } else {
                0.0
            };
        let first_row_offset = if has_phonetic_in_block {
            phonetic_line_height * 0.7
        } else {
            0.0
        };

        for (line_index, wrapped_line) in wrapped_lines.into_iter().enumerate() {
            if wrapped_line.syllables.is_empty() {
                continue;
            }

            let max_baseline = wrapped_line
                .syllables
                .iter()
                .map(|layout| layout.first_baseline)
                .fold(0.0, f32::max);
            first_baseline.get_or_insert(max_baseline);

            let row_top_y = line_index as f32 * row_height + first_row_offset;
            let start_x = if right_aligned {
                canvas_width - wrapped_line.total_width
            } else {
                0.0
            };
            let mut current_x = if is_rtl {
                start_x + wrapped_line.total_width
            } else {
                start_x
            };
            let mut row_glyphs = Vec::new();

            for layout in wrapped_line.syllables {
                let position_x = if is_rtl {
                    current_x - layout.width
                } else {
                    current_x
                };
                let vertical_offset = max_baseline - layout.first_baseline;
                let position_y = row_top_y + vertical_offset;
                let bottom_y = position_y + layout.height;
                if let Some(syllable) = syllables.get_mut(layout.index) {
                    syllable.layout_x = position_x;
                    syllable.layout_width = layout.width;
                }

                bounds_by_word
                    .entry(layout.word_id)
                    .and_modify(|bounds| {
                        bounds.0 = bounds.0.min(position_x);
                        bounds.1 = bounds.1.max(position_x + layout.width);
                        bounds.2 = bounds.2.max(bottom_y);
                    })
                    .or_insert((position_x, position_x + layout.width, bottom_y));

                let shift_x = position_x.round() as i32;
                let shift_y = position_y.round() as i32;
                for source_row in &layout.text.rows {
                    for source_glyph in &source_row.glyphs {
                        let mut glyph = source_glyph.clone();
                        glyph.physical.x += shift_x;
                        glyph.physical.y += shift_y;
                        glyph.x += position_x;
                        row_glyphs.push(glyph);
                    }
                }
                if let Some(phonetic_text) = &layout.phonetic {
                    let phonetic_y = position_y - phonetic_text.height + phonetic_gap;
                    let phonetic_shift_x = position_x.round() as i32;
                    let phonetic_shift_y = phonetic_y.round() as i32;
                    let phonetic_animation_index =
                        (layout.content.chars().count().saturating_sub(1) as f32) * 0.5;
                    for source_row in &phonetic_text.rows {
                        for source_glyph in &source_row.glyphs {
                            let mut glyph = source_glyph.clone();
                            glyph.physical.x += phonetic_shift_x;
                            glyph.physical.y += phonetic_shift_y;
                            glyph.x += position_x;
                            glyph.animation_char_index = phonetic_animation_index;
                            glyph.alpha_multiplier = 0.4;
                            glyph.is_phonetic = true;
                            row_glyphs.push(glyph);
                        }
                    }
                }

                if is_rtl {
                    current_x -= layout.width;
                } else {
                    current_x += layout.width;
                }
            }

            rows.push(PreparedRow {
                y: row_top_y,
                width: wrapped_line.total_width,
                min_x: start_x,
                max_x: start_x + wrapped_line.total_width,
                glyphs: row_glyphs,
            });
        }

        for syllable in syllables {
            if let Some((min_x, max_x, bottom_y)) = bounds_by_word.get(&syllable.word_id) {
                syllable.word_pivot_x = (min_x + max_x) * 0.5;
                syllable.word_pivot_y = *bottom_y;
            }
        }

        let row_count = rows.len().max(1);
        PreparedText {
            rows,
            height: row_count as f32
                * (line_height
                    + if has_phonetic_in_block {
                        phonetic_line_height
                    } else {
                        0.0
                    }),
            first_baseline: first_baseline.unwrap_or(line_height),
        }
    }

    fn prepare_text_with_metadata<'a>(
        &mut self,
        spans: impl Iterator<Item = (&'a str, usize)>,
        fallback_text: &str,
        font_size: f32,
        line_height: f32,
        width: f32,
        right_aligned: bool,
    ) -> PreparedText {
        let metrics = Metrics::new(font_size, line_height);
        let font_spans = self.build_font_spans(spans, fallback_text);
        let first_family_name = self.font_stack.first().map(|face| face.family_name.clone());
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        {
            let mut borrowed = buffer.borrow_with(&mut self.font_system);
            borrowed.set_wrap(Wrap::WordOrGlyph);
            borrowed.set_size(Some(width.max(1.0)), None);
            let default_attrs = Attrs::new().weight(Weight::NORMAL);
            let rich_spans = font_spans
                .iter()
                .map(|span| {
                    let attrs = match span.family_name.as_deref() {
                        Some(family_name) => default_attrs
                            .clone()
                            .family(Family::Name(family_name))
                            .metadata(span.metadata),
                        None => default_attrs.clone().metadata(span.metadata),
                    };
                    (span.text.as_str(), attrs)
                })
                .collect::<Vec<_>>();
            if rich_spans.is_empty() {
                let fallback_attrs = match first_family_name.as_deref() {
                    Some(family_name) => default_attrs
                        .clone()
                        .family(Family::Name(family_name))
                        .metadata(0),
                    None => default_attrs.clone().metadata(0),
                };
                borrowed.set_text(
                    fallback_text,
                    &fallback_attrs,
                    Shaping::Advanced,
                    Some(if right_aligned {
                        Align::Right
                    } else {
                        Align::Left
                    }),
                );
            } else {
                borrowed.set_rich_text(
                    rich_spans
                        .iter()
                        .map(|(text, attrs)| (*text, attrs.clone())),
                    &default_attrs,
                    Shaping::Advanced,
                    Some(if right_aligned {
                        Align::Right
                    } else {
                        Align::Left
                    }),
                );
            }
            borrowed.shape_until_scroll(false);
        }

        let mut borrowed = buffer.borrow_with(&mut self.font_system);
        let mut rows = Vec::new();
        let mut first_baseline = None;
        let mut glyph_counters_by_syllable = HashMap::<usize, usize>::new();
        for run in borrowed.layout_runs() {
            first_baseline.get_or_insert(run.line_y - run.line_top);
            let mut glyphs = Vec::with_capacity(run.glyphs.len());
            for glyph in run.glyphs.iter() {
                let syllable_index = (glyph.metadata > 0).then_some(glyph.metadata - 1);
                let glyph_index_in_syllable = syllable_index
                    .map(|index| {
                        let counter = glyph_counters_by_syllable.entry(index).or_insert(0);
                        let current = *counter;
                        *counter += 1;
                        current
                    })
                    .unwrap_or(0);
                glyphs.push(PreparedGlyph {
                    physical: glyph.physical((0.0, run.line_y), 1.0),
                    x: glyph.x,
                    syllable_index,
                    glyph_index_in_syllable,
                    animation_char_index: glyph_index_in_syllable as f32,
                    alpha_multiplier: 1.0,
                    is_phonetic: false,
                });
            }
            rows.push(PreparedRow {
                y: run.line_top,
                width: run.line_w,
                min_x: 0.0,
                max_x: run.line_w,
                glyphs,
            });
        }

        let height = rows
            .last()
            .map(|row| row.y + line_height)
            .unwrap_or(line_height);

        PreparedText {
            rows,
            height,
            first_baseline: first_baseline.unwrap_or(line_height),
        }
    }

    fn build_font_spans<'a>(
        &mut self,
        spans: impl Iterator<Item = (&'a str, usize)>,
        fallback_text: &str,
    ) -> Vec<FontTextSpan> {
        let mut result = Vec::new();
        for (text, metadata) in spans {
            self.push_font_spans_for_text(&mut result, text, metadata);
        }

        if result.is_empty() && !fallback_text.is_empty() {
            self.push_font_spans_for_text(&mut result, fallback_text, 0);
        }
        result
    }

    fn push_font_spans_for_text(
        &mut self,
        result: &mut Vec<FontTextSpan>,
        text: &str,
        metadata: usize,
    ) {
        for cluster in UnicodeSegmentation::graphemes(text, true) {
            let family_name = self.select_family_for_cluster(cluster);
            if let Some(last) = result.last_mut() {
                if last.metadata == metadata && last.family_name == family_name {
                    last.text.push_str(cluster);
                    continue;
                }
            }

            result.push(FontTextSpan {
                text: cluster.to_string(),
                metadata,
                family_name,
            });
        }
    }

    fn select_family_for_cluster(&mut self, cluster: &str) -> Option<String> {
        if let Some(cached) = self.font_selection_cache.get(cluster) {
            return cached.clone();
        }
        let selected = self.select_family_for_cluster_uncached(cluster);
        self.font_selection_cache
            .insert(cluster.to_string(), selected.clone());
        selected
    }

    fn select_family_for_cluster_uncached(&mut self, cluster: &str) -> Option<String> {
        if self.font_stack.is_empty() {
            return None;
        }

        if contains_han(cluster) {
            let mut selected: Option<(usize, usize)> = None;
            for index in 0..self.font_stack.len() {
                let id = self.font_stack[index].id;
                if !self.font_supports_cluster(id, cluster) {
                    continue;
                }
                let priority =
                    cjk_family_priority(&self.font_stack[index].family_name, &self.locale);
                match selected {
                    Some((best_priority, best_index))
                        if best_priority < priority
                            || (best_priority == priority && best_index <= index) => {}
                    _ => selected = Some((priority, index)),
                }
            }

            if let Some((_, index)) = selected {
                return Some(self.font_stack[index].family_name.clone());
            }
        }

        for index in 0..self.font_stack.len() {
            let id = self.font_stack[index].id;
            if self.font_supports_cluster(id, cluster) {
                return Some(self.font_stack[index].family_name.clone());
            }
        }

        self.font_stack.first().map(|face| face.family_name.clone())
    }

    fn font_supports_cluster(&mut self, id: fontdb::ID, cluster: &str) -> bool {
        let expected = cluster.chars().filter(|ch| !ch.is_control()).count();
        if expected == 0 {
            return true;
        }

        self.font_system
            .get_font_supported_codepoints_in_word(id, fontdb::Weight::NORMAL, cluster)
            .is_some_and(|count| count >= expected)
    }
}

#[derive(Debug)]
struct FontTextSpan {
    text: String,
    metadata: usize,
    family_name: Option<String>,
}

fn prepare_karaoke_syllables(
    input: &[SyllableInput],
    is_accompaniment: bool,
) -> Vec<PreparedSyllable> {
    let mut prepared = input
        .iter()
        .map(|syllable| PreparedSyllable {
            content: syllable.content.clone(),
            start: syllable.start,
            end: syllable.end,
            word_id: 0,
            float_end: syllable.end,
            use_awesome: false,
            char_count: syllable.content.chars().count(),
            char_offset_in_word: 0,
            word_start: syllable.start,
            word_end: syllable.end,
            word_duration: (syllable.end - syllable.start).max(1),
            word_char_count: syllable.content.chars().count().max(1),
            word_pivot_x: 0.0,
            word_pivot_y: 0.0,
            layout_x: 0.0,
            layout_width: 0.0,
        })
        .collect::<Vec<_>>();

    if prepared.is_empty() {
        return prepared;
    }

    let mut word_start_index = 0usize;
    let mut word_id_ranges = Vec::<std::ops::Range<usize>>::new();
    for index in 0..prepared.len() {
        if has_trailing_whitespace(&prepared[index].content) {
            word_id_ranges.push(word_start_index..index + 1);
            word_start_index = index + 1;
        }
    }
    if word_start_index < prepared.len() {
        word_id_ranges.push(word_start_index..prepared.len());
    }

    for (word_id, range) in word_id_ranges.into_iter().enumerate() {
        let word_start = prepared[range.start].start;
        let word_end = prepared[range.end - 1].end;
        let word_duration = (word_end - word_start).max(1);
        let word_content = prepared[range.clone()]
            .iter()
            .map(|syllable| syllable.content.as_str())
            .collect::<String>();
        let word_char_count = word_content.chars().count().max(1);
        let per_char_duration = word_duration as f32 / word_char_count as f32;
        let use_awesome = !is_accompaniment
            && word_duration >= AWESOME_MIN_WORD_DURATION_MS
            && per_char_duration > AWESOME_FAST_CHAR_THRESHOLD_MS
            && !should_use_simple_animation(&word_content);

        let mut char_offset = 0usize;
        for index in range {
            prepared[index].word_id = word_id;
            prepared[index].use_awesome = use_awesome;
            prepared[index].char_offset_in_word = char_offset;
            prepared[index].word_start = word_start;
            prepared[index].word_end = word_end;
            prepared[index].word_duration = word_duration;
            prepared[index].word_char_count = word_char_count;
            char_offset += prepared[index].char_count;
        }
    }

    let line_end = prepared.last().map(|syllable| syllable.end).unwrap_or(0);
    let mut float_ends = vec![line_end; prepared.len()];
    for index in 0..prepared.len() {
        let mut end = (prepared[index].start + 700).min(line_end);
        if index + 1 < prepared.len() && prepared[index + 1].use_awesome {
            end = end.min(prepared[index + 1].start);
        }
        float_ends[index] = end.max(prepared[index].start + 1);
    }

    for index in (0..prepared.len().saturating_sub(1)).rev() {
        if !prepared[index + 1].use_awesome && float_ends[index] > float_ends[index + 1] {
            float_ends[index] = float_ends[index + 1].max(prepared[index].start + 1);
        }
    }

    for (syllable, float_end) in prepared.iter_mut().zip(float_ends) {
        syllable.float_end = float_end;
    }

    prepared
}

fn spring_step(
    value: &mut f32,
    velocity: &mut f32,
    target: f32,
    stiffness: f32,
    damping: f32,
    dt: f32,
) -> bool {
    let displacement = *value - target;
    let acceleration = -stiffness * displacement - damping * *velocity;
    *velocity += acceleration * dt;
    *value += *velocity * dt;

    if (*value - target).abs() <= LINE_LAYOUT_EPSILON && (*velocity).abs() <= LINE_LAYOUT_EPSILON {
        *value = target;
        *velocity = 0.0;
        false
    } else {
        true
    }
}

fn rubber_band_scroll(raw_scroll_y: f32, max_scroll_y: f32) -> f32 {
    if raw_scroll_y < 0.0 {
        -rubber_band_distance(-raw_scroll_y)
    } else if raw_scroll_y > max_scroll_y {
        max_scroll_y + rubber_band_distance(raw_scroll_y - max_scroll_y)
    } else {
        raw_scroll_y
    }
}

fn rubber_band_distance(distance: f32) -> f32 {
    let distance = distance.max(0.0);
    MANUAL_SCROLL_RUBBER_BAND_LIMIT * distance / (distance + MANUAL_SCROLL_RUBBER_BAND_LIMIT)
}

impl PreparedScene {
    fn max_scroll_for_layouts(&self, layouts: &[DynamicLineLayout]) -> f32 {
        let Some(last) = layouts.last() else {
            return 0.0;
        };
        // Enough to reveal the bottom of the content (its trailing keep-alive
        // pad) for short screens / tall final rows.
        let content_bottom_scroll =
            last.top + last.height + self.config.keep_alive - self.config.height as f32;
        // ...but also at least enough for the final row to scroll all the way up
        // to the focus anchor (keep_alive from the top), leaving the rest of the
        // screen empty below it. Without this the last line stalls partway down
        // the screen and can never reach the top.
        let last_line_anchor_scroll = last.top - self.config.keep_alive;
        content_bottom_scroll.max(last_line_anchor_scroll).max(0.0)
    }

    fn scroll_y_for_time_with_layouts(
        &self,
        current_time_ms: i32,
        layouts: &[DynamicLineLayout],
    ) -> f32 {
        if self.lines.is_empty() {
            return 0.0;
        }

        let focus_index = self.focus_anchor_index(current_time_ms);
        let focus_top = self.cluster_top_for_line(focus_index, layouts);
        let target = focus_top - self.config.keep_alive;
        target.clamp(0.0, self.max_scroll_for_layouts(layouts))
    }

    fn focus_alpha(&self, line: &PreparedLine, current_time_ms: i32) -> f32 {
        if self.is_line_focused(line, current_time_ms) {
            return 1.0;
        }

        let distance = if current_time_ms < line.start {
            (line.start - current_time_ms) as f32
        } else {
            (current_time_ms - line.effective_end) as f32
        };
        (1.0 - (distance / 6000.0)).clamp(0.28, 0.78)
    }

    fn focus_index_range(&self, current_time_ms: i32) -> (usize, usize) {
        let mut first = None;
        let mut last = None;
        for (index, line) in self.lines.iter().enumerate() {
            if self.is_line_focused(line, current_time_ms) {
                first.get_or_insert(index);
                last = Some(index);
            }
        }

        match (first, last) {
            (Some(first), Some(last)) => (first, last),
            _ => {
                let pending = self
                    .lines
                    .iter()
                    .position(|line| line.start > current_time_ms)
                    .unwrap_or_else(|| self.lines.len().saturating_sub(1));
                (pending, pending)
            }
        }
    }

    fn focus_anchor_index(&self, current_time_ms: i32) -> usize {
        self.lines
            .iter()
            .position(|line| self.is_line_focused(line, current_time_ms))
            .unwrap_or_else(|| {
                self.lines
                    .iter()
                    .position(|line| line.start > current_time_ms)
                    .unwrap_or_else(|| self.lines.len().saturating_sub(1))
            })
    }

    fn is_line_focused(&self, line: &PreparedLine, current_time_ms: i32) -> bool {
        current_time_ms >= line.start && current_time_ms <= line.effective_end
    }

    fn cluster_top_for_line(&self, line_index: usize, layouts: &[DynamicLineLayout]) -> f32 {
        let Some(line) = self.lines.get(line_index) else {
            return 0.0;
        };
        self.lines
            .iter()
            .enumerate()
            .find(|(index, candidate)| {
                candidate.cluster_index == line.cluster_index
                    && layouts
                        .get(*index)
                        .is_some_and(|layout| layout.text_visibility > 0.001)
            })
            .and_then(|(index, _)| layouts.get(index))
            .map(|layout| layout.top)
            .or_else(|| layouts.get(line_index).map(|layout| layout.top))
            .unwrap_or(0.0)
    }

    fn dynamic_line_layouts(&self, current_time_ms: i32) -> Vec<DynamicLineLayout> {
        let mut cursor_y = self.config.keep_alive;
        let mut layouts = Vec::with_capacity(self.lines.len());

        for line in &self.lines {
            let text_visibility = self.text_visibility_for_line(line, current_time_ms);
            let interlude_visibility = line
                .interlude
                .as_ref()
                .map(|slot| interlude_visibility(slot.start, slot.end, current_time_ms))
                .unwrap_or(0.0);
            let interlude_height = line
                .interlude
                .as_ref()
                .map(|slot| slot.height)
                .unwrap_or(0.0);
            let base_height = line.base_height();
            let height = interlude_height * interlude_visibility + base_height * text_visibility;
            layouts.push(DynamicLineLayout {
                top: cursor_y,
                height,
                text_visibility,
                interlude_visibility,
            });
            cursor_y += height;
        }

        layouts
    }

    fn text_visibility_for_line(&self, line: &PreparedLine, current_time_ms: i32) -> f32 {
        if line.cluster_role.is_nested_accompaniment() {
            accompaniment_visibility(line.start, line.end, current_time_ms)
        } else {
            1.0
        }
    }

    /// Depth-of-field blur keyed off the row's on-screen distance from the focus
    /// anchor (in line-height units) rather than its focus *index*. Because the
    /// screen position moves continuously as the list scrolls, the blur eases in
    /// and out smoothly and does not snap when the focused row changes.
    ///
    /// A sharp band of `BLUR_SHARP_RADIUS_LINES` around the anchor stays fully
    /// crisp so the *whole* current cluster — the main line and the accompaniment
    /// sitting a line or two below it — is in focus; blur only ramps up past that
    /// band. (A nested accompaniment is the current content but never sits exactly
    /// at the anchor, so without this band it would read as blurred.)
    fn blur_radius_for_screen_y(&self, screen_top: f32, anchor_y: f32) -> f32 {
        if !self.config.use_blur_effect || self.config.blur_delta <= 0.0 {
            return 0.0;
        }

        let unit = self.config.normal_line_height.max(1.0);
        let lines_away = (screen_top - anchor_y).abs() / unit;
        let blur_lines = (lines_away - BLUR_SHARP_RADIUS_LINES).clamp(0.0, 10.0);
        blur_lines * self.config.blur_delta
    }
}

impl PreparedLine {
    fn base_height(&self) -> f32 {
        self.height
            - self
                .interlude
                .as_ref()
                .map(|slot| slot.height)
                .unwrap_or(0.0)
    }

    fn main_text_height(&self) -> f32 {
        match &self.kind {
            PreparedLineKind::Karaoke { text, .. } => text.height,
            PreparedLineKind::Synced { text } => text.height,
        }
    }
}

fn prepared_text_width(text: &PreparedText) -> f32 {
    text.rows.iter().map(|row| row.width).fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_text(height: f32) -> PreparedText {
        PreparedText {
            rows: Vec::new(),
            height,
            first_baseline: height,
        }
    }

    fn test_config() -> SceneConfig {
        SceneConfig {
            width: 300,
            height: 200,
            normal_font_size: 34.0,
            normal_line_height: 42.0,
            accompaniment_font_size: 20.0,
            accompaniment_line_height: 26.0,
            translation_font_size: 16.0,
            translation_line_height: 21.0,
            phonetic_font_size: 13.0,
            phonetic_line_height: 16.0,
            phonetic_gap: 4.0,
            padding_x: 16.0,
            padding_y: 8.0,
            keep_alive: 32.0,
            text_color: 0xffff_ffff,
            show_translation: true,
            show_phonetic: true,
            use_blur_effect: true,
            blur_delta: 3.0,
            breathing_dots: BreathingDotsConfig {
                number: 3,
                size: 16.0,
                margin: 12.0,
                enter_ms: 3000.0,
                still_ms: 200.0,
                dip_ms: 3000.0,
                exit_ms: 200.0,
                color: 0xffff_ffff,
            },
        }
    }

    fn test_line(cluster_role: ClusterRole, start: i32, end: i32, height: f32) -> PreparedLine {
        PreparedLine {
            source_index: 0,
            cluster_index: 0,
            cluster_role,
            start,
            end,
            effective_end: end,
            height,
            right_aligned: false,
            interlude: None,
            kind: PreparedLineKind::Synced {
                text: test_text(height),
            },
            translation: None,
            phonetic: None,
        }
    }

    #[test]
    fn karaoke_right_alignment_allows_negative_start_for_long_rows() {
        let renderer = LyricsRenderer::new();
        let mut syllables = vec![PreparedSyllable {
            content: "oversized".to_string(),
            start: 0,
            end: 1000,
            word_id: 0,
            float_end: 700,
            use_awesome: false,
            char_count: 9,
            char_offset_in_word: 0,
            word_start: 0,
            word_end: 1000,
            word_duration: 1000,
            word_char_count: 9,
            word_pivot_x: 0.0,
            word_pivot_y: 0.0,
            layout_x: 0.0,
            layout_width: 0.0,
        }];
        let wrapped = vec![WrappedMeasuredLine {
            syllables: vec![MeasuredSyllable {
                index: 0,
                word_id: 0,
                content: "oversized".to_string(),
                use_awesome: false,
                text: test_text(42.0),
                phonetic: None,
                width: 140.0,
                first_baseline: 34.0,
                height: 42.0,
            }],
            total_width: 140.0,
        }];

        let text = renderer.position_karaoke_wrapped_lines(
            wrapped,
            &mut syllables,
            100.0,
            42.0,
            16.0,
            4.0,
            true,
            false,
        );

        assert_eq!(text.rows.len(), 1);
        assert_eq!(text.rows[0].min_x, -40.0);
        assert_eq!(text.rows[0].max_x, 100.0);
        assert_eq!(syllables[0].layout_x, -40.0);
    }

    #[test]
    fn nested_accompaniment_collapses_outside_compose_visibility_window() {
        let scene = PreparedScene {
            config: test_config(),
            lines: vec![
                test_line(ClusterRole::Main, 1_000, 2_000, 50.0),
                test_line(ClusterRole::AfterAccompaniment, 2_100, 2_500, 30.0),
            ],
            content_height: 0.0,
        };

        assert_eq!(scene.dynamic_line_layouts(1_050)[1].height, 0.0);

        let entering = scene.dynamic_line_layouts(1_800)[1];
        assert!(entering.height > 0.0 && entering.height < 30.0);

        assert_eq!(scene.dynamic_line_layouts(2_100)[1].height, 30.0);

        let exiting = scene.dynamic_line_layouts(3_300)[1];
        assert!(exiting.height > 0.0 && exiting.height < 30.0);

        assert_eq!(scene.dynamic_line_layouts(3_701)[1].height, 0.0);
    }

    #[test]
    fn newton_easing_functions_match_compose_control_points() {
        assert!((dip_and_rise(0.0, 0.4, 1.0) - 0.0).abs() < 0.0001);
        assert!((dip_and_rise(0.5, 0.4, 1.0) + 0.4).abs() < 0.0001);
        assert!((dip_and_rise(1.0, 0.4, 1.0) - 1.0).abs() < 0.0001);

        assert!((swell(0.0, 0.1) - 0.0).abs() < 0.0001);
        assert!((swell(0.5, 0.1) - 0.1).abs() < 0.0001);
        assert!((swell(1.0, 0.1) - 0.0).abs() < 0.0001);

        assert!((bounce(0.0) - 0.0).abs() < 0.0001);
        assert!((bounce(0.7) - 1.0).abs() < 0.0001);
        assert!((bounce(1.0) - 0.0).abs() < 0.0001);
    }

    #[test]
    fn awesome_offset_uses_compose_one_minus_progress_formula() {
        let syllable = PreparedSyllable {
            content: "awesome".to_string(),
            start: 0,
            end: 2_000,
            word_id: 0,
            float_end: 700,
            use_awesome: true,
            char_count: 7,
            char_offset_in_word: 0,
            word_start: 0,
            word_end: 2_000,
            word_duration: 2_000,
            word_char_count: 7,
            word_pivot_x: 0.0,
            word_pivot_y: 0.0,
            layout_x: 0.0,
            layout_width: 0.0,
        };
        let start_effect = awesome_glyph_effect_for_char(0.0, 0, &syllable);
        let mid_effect = awesome_glyph_effect_for_char(0.0, 800, &syllable);
        let end_effect = awesome_glyph_effect_for_char(0.0, 1_600, &syllable);

        assert!(start_effect.offset_y > 0.0);
        assert!(mid_effect.offset_y < 0.0);
        assert!(end_effect.offset_y.abs() < 0.0001);
    }
}
