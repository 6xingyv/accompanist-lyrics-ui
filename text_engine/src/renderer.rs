#[cfg(not(target_os = "android"))]
use cosmic_text::SwashCache;
use cosmic_text::{
    fontdb, Align, Attrs, Buffer, Family, FontSystem, Metrics, PhysicalGlyph, Shaping, Style, Weight,
    Wrap,
};
use serde::{Deserialize, Serialize};
use skia_safe::{font_style, Data, FontMgr, FontStyle, Typeface};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

mod draw;
mod font_fallback;
mod fonts;
mod layout;
mod scroll;
mod text_utils;

use draw::{
    accompaniment_visibility, apply_vertical_fade_skia, draw_breathing_dots_skia,
    draw_prepared_text_skia, interlude_visibility, make_interlude_slot, rgba_from_argb,
};
use scroll::{ManualScrollState, SpringLineState};
#[cfg(not(target_os = "android"))]
use draw::{apply_vertical_fade, draw_breathing_dots, draw_prepared_text};
use font_fallback::{cjk_family_priority, new_font_system};
use fonts::*;
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
// A line that leaves the focus dims to `FOCUS_ALPHA_MIN` over `..FALLOFF_MS` of
// distance (time before it starts / after it ends). Kept short so the dim is
// snappy — the old 6000ms falloff took ~3.6s to reach 0.4, which read as sluggish.
const FOCUS_ALPHA_MIN: f32 = 0.4;
const FOCUS_ALPHA_FALLOFF_MS: f32 = 800.0;
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
// A frame whose playback time moves backward at all, or jumps forward by more
// than this, is a *seek* (the user tapped a lyric) rather than natural
// playback advancing frame-by-frame. The spring chain models the lag of
// line-by-line progression, so it is suspended while a seek glides to its new
// position — otherwise a focus-index jump seeds the cascade with the stale
// rigid-block scroll and the list whips around.
const LINE_LAYOUT_SEEK_BACKWARD_MS: i32 = 1;
const LINE_LAYOUT_SEEK_FORWARD_MS: i32 = 600;
const MANUAL_SCROLL_MAX_FLING_VELOCITY: f32 = 14000.0;
// iOS UIScrollView fling: velocity decays as `rate^(elapsed_ms)`, NORMAL = 0.998
// (~2.0/s continuous friction). The old `exp(-4.8·dt)` killed flings ~2.4× too
// fast, which is why they felt stiff. Ported from ktiays/fluid-scroll.
const MANUAL_SCROLL_DECELERATION_RATE: f32 = 0.998;
const MANUAL_SCROLL_VELOCITY_EPSILON: f32 = 14.0;
// Overscroll bounce-back is critically damped (no underdamped wobble, which read
// as "weird"). It uses iOS SpringBack's response 0.575s (lambda = 2pi/0.575
// ~= 10.93, stiffness ~= 119.4, damping ~= 21.85).
const MANUAL_SCROLL_OVERSCROLL_STIFFNESS: f32 = 119.4;
const MANUAL_SCROLL_OVERSCROLL_DAMPING: f32 = 21.85;
// iOS rubber-band: `(1 - 1/(d/limit·c + 1))·limit`, c = 0.55. The old formula
// dropped `c`, so the edge resisted the pull too hard.
const MANUAL_SCROLL_RUBBER_BAND_LIMIT: f32 = 180.0;
const MANUAL_SCROLL_RUBBER_BAND_COEFFICIENT: f32 = 0.55;
// Manual scrolling releases the depth-of-field blur so the user can read while
// browsing. The blur stays released until this long after the *last touch input*
// (grab/drag/release), then eases back in — independent of the fling/return
// physics, so the automatic glide-back to the active line never re-toggles it.
// The fade-out is quicker than the fade-in so grabbing the list feels responsive
// while the blur eases back gently once you stop.
const MANUAL_SCROLL_BLUR_RESTORE_MS: u64 = 2500;
const MANUAL_SCROLL_BLUR_FADE_OUT_RATE: f32 = 12.0;
const MANUAL_SCROLL_BLUR_FADE_IN_RATE: f32 = 6.0;
const LYRIC_CLICK_SEEK_PENDING_MS: u64 = 1500;

#[derive(Debug, Deserialize)]
pub struct LyricsScene {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub locale: Option<String>,
    pub normal_font_size: Option<f32>,
    pub normal_line_height: Option<f32>,
    pub normal_font_weight: Option<u16>,
    pub normal_font_italic: Option<bool>,
    pub accompaniment_font_size: Option<f32>,
    pub accompaniment_line_height: Option<f32>,
    pub accompaniment_font_weight: Option<u16>,
    pub accompaniment_font_italic: Option<bool>,
    pub translation_font_size: Option<f32>,
    pub translation_line_height: Option<f32>,
    pub translation_font_weight: Option<u16>,
    pub translation_font_italic: Option<bool>,
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
    pub phonetic_font_weight: Option<u16>,
    pub phonetic_font_italic: Option<bool>,
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
    /// Glyph → family name the NDK `AFontMatcher` chose for characters the user's
    /// font chain can't cover (e.g. MiSans on Xiaomi, Noto elsewhere). The matcher
    /// already confirmed that face covers the glyph, so `select_family_for_cluster`
    /// trusts this directly rather than re-probing codepoint support (which is
    /// unreliable for CJK OTC faces) — that probe failure is why CJK used to drop
    /// to cosmic-text's hard-coded Roboto/Droid preset. Android-only.
    matched_char_family: HashMap<char, String>,
    skia_typefaces: HashMap<fontdb::ID, Typeface>,
    /// Set when a new scene is installed; cleared once its glyphs' Skia typefaces
    /// have been resolved into `skia_typefaces`. The font-id set is fixed for a
    /// given scene, so the resolve scan only needs to run when this is set rather
    /// than walking every glyph of every line on every frame.
    skia_typefaces_dirty: bool,
    font_selection_cache: HashMap<String, Option<String>>,
    // Lazy system-font fallback (Android): instead of loading the whole platform
    // font collection up front, ask the NDK `AFontMatcher` for the font that
    // covers each new glyph and load just that file into cosmic-text's db.
    #[cfg(target_os = "android")]
    font_matcher: Option<crate::system_fonts::FontMatcher>,
    #[cfg(target_os = "android")]
    matched_glyphs: std::collections::HashSet<(char, u16, bool)>,
    #[cfg(target_os = "android")]
    loaded_system_paths: std::collections::HashSet<String>,
    // Font attributes (weight/italic) applied to the text currently being shaped.
    // Set per text role in `prepare_scene`; `prepare_text_with_metadata` reads it
    // so size/weight/italic are configured independently per role.
    text_attrs: TextAttrs,
    phonetic_attrs: TextAttrs,
    last_render_debug_time_ms: Option<i32>,
    spring_layouts: Vec<SpringLineState>,
    /// Reused per-frame scratch so the spring cascade and the projected on-screen
    /// layout don't heap-allocate a fresh `Vec` on every frame.
    spring_chained_targets: Vec<f32>,
    frame_layouts: Vec<DynamicLineLayout>,
    last_spring_frame_at: Option<Instant>,
    last_spring_playback_ms: Option<i32>,
    last_seek_detection_playback_ms: Option<i32>,
    last_target_scroll_y: Option<f32>,
    pending_lyric_click_seek: Option<PendingLyricClickSeek>,
    layout_animation_active: bool,
    /// True while an unclassified playback-time jump glides the list as one
    /// rigid block. Explicit on-screen lyric clicks are seeded separately from
    /// their visible scroll and keep the ordinary click cascade.
    seek_glide_active: bool,
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

/// Per-role font attributes that are configured independently of font size.
#[derive(Debug, Clone, Copy)]
struct TextAttrs {
    weight: u16,
    italic: bool,
}

impl Default for TextAttrs {
    fn default() -> Self {
        Self {
            weight: 400,
            italic: false,
        }
    }
}

impl TextAttrs {
    fn cosmic_weight(self) -> Weight {
        Weight(self.weight)
    }

    fn cosmic_style(self) -> Style {
        if self.italic {
            Style::Italic
        } else {
            Style::Normal
        }
    }
}

#[derive(Debug)]
struct PreparedScene {
    config: SceneConfig,
    lines: Vec<PreparedLine>,
    content_height: f32,
}

#[derive(Debug, Clone, Copy)]
struct PendingLyricClickSeek {
    source_index: usize,
    visible_scroll_y: f32,
    recorded_at: Instant,
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

#[derive(Debug, Clone, Copy)]
struct SceneConfig {
    width: u32,
    height: u32,
    normal_font_size: f32,
    normal_line_height: f32,
    normal_attrs: TextAttrs,
    accompaniment_font_size: f32,
    accompaniment_line_height: f32,
    accompaniment_attrs: TextAttrs,
    translation_font_size: f32,
    translation_line_height: f32,
    translation_attrs: TextAttrs,
    phonetic_font_size: f32,
    phonetic_line_height: f32,
    phonetic_attrs: TextAttrs,
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
            matched_char_family: HashMap::new(),
            skia_typefaces: HashMap::new(),
            skia_typefaces_dirty: true,
            font_selection_cache: HashMap::new(),
            #[cfg(target_os = "android")]
            font_matcher: crate::system_fonts::FontMatcher::new(),
            #[cfg(target_os = "android")]
            matched_glyphs: std::collections::HashSet::new(),
            #[cfg(target_os = "android")]
            loaded_system_paths: std::collections::HashSet::new(),
            text_attrs: TextAttrs::default(),
            phonetic_attrs: TextAttrs::default(),
            last_render_debug_time_ms: None,
            spring_layouts: Vec::new(),
            spring_chained_targets: Vec::new(),
            frame_layouts: Vec::new(),
            last_spring_frame_at: None,
            last_spring_playback_ms: None,
            last_seek_detection_playback_ms: None,
            last_target_scroll_y: None,
            pending_lyric_click_seek: None,
            layout_animation_active: false,
            seek_glide_active: false,
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


    fn should_log_render_debug(&mut self, current_time_ms: i32) -> bool {
        let should_log = self
            .last_render_debug_time_ms
            .is_none_or(|last| (current_time_ms - last).abs() >= 1000);
        if should_log {
            self.last_render_debug_time_ms = Some(current_time_ms);
        }
        should_log
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
        self.skia_typefaces_dirty = true;
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

        // Blur anchor = the currently sung line's on-screen position (see the
        // Android path for the rationale); focused lines are forced fully sharp.
        let blur_anchor_y = {
            let idx = scene.focus_anchor_index(current_time_ms);
            dynamic_layouts
                .get(idx)
                .map(|layout| layout.top - scroll_y)
                .unwrap_or(scene.config.keep_alive)
        };

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
            let blur_radius = if scene.is_line_focused(line, current_time_ms) {
                0.0
            } else {
                scene.blur_radius_for_screen_y(y, blur_anchor_y)
            };

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
            let (_focus_start, focus_end) = scene.focus_group_range(current_time_ms);
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

        // A seek (the user tapped a lyric, or scrubbed) is a discontinuous jump
        // in playback time. If it lands while a manual/plain-list view is still
        // on screen, seed the layout springs from that visible list scroll before
        // clearing the manual offset. This makes a manually revealed far lyric
        // behave like an ordinary on-screen lyric click instead of being judged
        // against the old auto-scroll target and snapping as an "ultra far" jump.
        self.prepare_seek_transition(current_time_ms, target_layouts.len());

        // The combined scroll = auto position + the manual-scroll offset (rubber
        // banded). We split it: the spring cascade animates only the AUTO part,
        // and the manual displacement is applied afterward as a flat shift. That
        // way the inter-line ripple always runs on the auto component (present the
        // moment auto-scroll resumes after a manual scroll) while the manual
        // gesture stays exactly 1:1 (responsive fling, no spring trailing it).
        let combined_scroll_y = self.update_manual_scroll_target(
            current_time_ms,
            auto_scroll_y,
            max_scroll_y,
            target_layouts.len(),
        );
        let manual_displacement = combined_scroll_y - auto_scroll_y;
        if self.manual_scroll_plain_list_active() {
            self.project_uniform_frame_layout(&target_layouts, combined_scroll_y);
        } else {
            self.animate_frame_layout(
                current_time_ms,
                &target_layouts,
                auto_scroll_y,
                height as f32,
                focus_end,
            );
            if manual_displacement.abs() > 0.001 {
                for layout in self.frame_layouts.iter_mut() {
                    layout.top -= manual_displacement;
                }
            }
        }
        // While the user manually scrolls the depth-of-field blur is eased away
        // so the lyrics stay sharp for reading.
        let blur_scale = (1.0 - self.manual_scroll_blur_release).clamp(0.0, 1.0);
        // The spring pass filled the reused `frame_layouts` buffer; borrow it back
        // (shared) alongside the scene for the draw pass.
        let dynamic_layouts = &self.frame_layouts;
        let Some(scene) = &self.scene else {
            return -3;
        };

        let mut frame_stats = FrameGlyphStats::default();
        let mut visible_font_ids = Vec::new();
        // `dynamic_layouts` are already in screen space (scroll folded in), so the
        // visible window is simply the surface plus the keep-alive margin.
        let visible_top = -keep_alive;
        let visible_bottom = height as f32 + keep_alive;

        // Anchor the depth-of-field blur on the *currently sung* line's actual
        // on-screen position — which springs/lags as the list scrolls — not the
        // fixed keep-alive slot. With the fixed anchor, while the scroll spring is
        // still catching up the active line sits off the anchor and gets blurred
        // even though it's the focus. Falls back to keep_alive before the first
        // line is focused.
        let blur_anchor_y = {
            let idx = scene.focus_anchor_index(current_time_ms);
            dynamic_layouts
                .get(idx)
                .map(|layout| layout.top)
                .unwrap_or(keep_alive)
        };

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
            // The whole active cluster (the sung main line and its nested
            // accompaniment, all with start<=t<=end) is always crisp; everything
            // else blurs by its distance from the active line.
            let blur_radius = if scene.is_line_focused(line, current_time_ms) {
                0.0
            } else {
                scene.blur_radius_for_screen_y(y, blur_anchor_y) * blur_scale
            };

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

        // Fade the top and bottom edges so lines dissolve as they scroll off. The
        // top fade stays inside the keep-alive gap so the active line (anchored at
        // keep_alive from the top) is never touched.
        apply_vertical_fade_skia(
            canvas,
            scene.config.width as f32,
            height as f32,
            keep_alive * 0.7,
            height as f32 * 0.12,
        );

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

    pub fn hit_test_line(&mut self, x: f32, y: f32, current_time_ms: i32) -> i32 {
        let Some(scene) = &self.scene else {
            self.pending_lyric_click_seek = None;
            return -1;
        };

        if x < 0.0 || y < 0.0 || x > scene.config.width as f32 || y > scene.config.height as f32 {
            self.pending_lyric_click_seek = None;
            return -1;
        }

        let dynamic_layouts = scene.dynamic_line_layouts(current_time_ms);
        let auto_scroll_y = scene.scroll_y_for_time_with_layouts(current_time_ms, &dynamic_layouts);
        let scroll_y = self.manual_scroll_projected_scroll(
            auto_scroll_y,
            scene.max_scroll_for_layouts(&dynamic_layouts),
        );
        let content_y = y + scroll_y;
        let hit = scene
            .lines
            .iter()
            .enumerate()
            .find(|(index, _)| {
                dynamic_layouts.get(*index).is_some_and(|layout| {
                    layout.text_visibility > 0.001
                        && content_y >= layout.top
                        && content_y <= layout.top + layout.height
                })
            });
        if let Some((_, line)) = hit {
            let source_index = line.source_index;
            self.pending_lyric_click_seek = Some(PendingLyricClickSeek {
                source_index,
                visible_scroll_y: scroll_y,
                recorded_at: Instant::now(),
            });
            source_index as i32
        } else {
            self.pending_lyric_click_seek = None;
            -1
        }
    }


}

#[derive(Debug)]
struct FontTextSpan {
    text: String,
    metadata: usize,
    family_name: Option<String>,
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

        // Anchor on the FIRST line of the overlap group so the scroll holds in
        // place while any line in a batch of overlapping timelines is still being
        // sung — instead of hopping forward the instant the first line ends.
        let focus_index = self.focus_group_range(current_time_ms).0;
        let focus_top = self.cluster_top_for_line(focus_index, layouts);
        let target = focus_top - self.config.keep_alive;
        target.clamp(0.0, self.max_scroll_for_layouts(layouts))
    }

    fn focus_alpha(&self, line: &PreparedLine, current_time_ms: i32) -> f32 {
        if self.is_line_focused(line, current_time_ms) {
            return 1.0;
        }

        let progress = if current_time_ms < line.start {
            1.0 - (line.start - current_time_ms) as f32 / FOCUS_ALPHA_FALLOFF_MS
        } else {
            1.0 - (current_time_ms - line.effective_end) as f32 / FOCUS_ALPHA_FALLOFF_MS
        };
        focus_alpha_from_progress(progress)
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

    /// Extends the focused range to cover every line whose singing overlaps it —
    /// so a run of lines with overlapping timelines (duet trades, a main line and
    /// its nested accompaniment, back-to-back lines with no gap) is treated as ONE
    /// scroll batch. Accompaniment lines are ordinary entries in `self.lines`, so
    /// they participate in the overlap chain and are never ignored. The scroll
    /// anchor holds on the group's first line, and the cascade's rigid block ends
    /// at the group's last line, until the whole batch has finished.
    fn focus_group_range(&self, current_time_ms: i32) -> (usize, usize) {
        let (mut first, mut last) = self.focus_index_range(current_time_ms);
        // Walk backward while the previous line hadn't finished when this one began
        // (their timelines overlap or touch).
        while first > 0 && self.lines[first - 1].effective_end >= self.lines[first].start {
            first -= 1;
        }
        // Walk forward while the next line begins before this one finishes.
        while last + 1 < self.lines.len()
            && self.lines[last].effective_end >= self.lines[last + 1].start
        {
            last += 1;
        }
        (first, last)
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

fn focus_alpha_from_progress(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    FOCUS_ALPHA_MIN + (1.0 - FOCUS_ALPHA_MIN) * eased
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
            normal_attrs: TextAttrs::default(),
            accompaniment_font_size: 20.0,
            accompaniment_line_height: 26.0,
            accompaniment_attrs: TextAttrs::default(),
            translation_font_size: 16.0,
            translation_line_height: 21.0,
            translation_attrs: TextAttrs::default(),
            phonetic_font_size: 13.0,
            phonetic_line_height: 16.0,
            phonetic_attrs: TextAttrs::default(),
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
    fn focus_alpha_is_symmetric_for_past_and_future_lines() {
        let scene = PreparedScene {
            config: test_config(),
            lines: vec![test_line(ClusterRole::Standalone, 1_000, 2_000, 50.0)],
            content_height: 0.0,
        };
        let line = &scene.lines[0];

        for distance in [0, 100, 400, 800, 1_200] {
            let future = scene.focus_alpha(line, line.start - distance);
            let past = scene.focus_alpha(line, line.effective_end + distance);
            assert!(
                (future - past).abs() < 0.0001,
                "distance {distance} should have matching future/past alpha, got {future} vs {past}"
            );
        }

        assert_eq!(scene.focus_alpha(line, line.start - 800), FOCUS_ALPHA_MIN);
        assert_eq!(scene.focus_alpha(line, line.effective_end + 800), FOCUS_ALPHA_MIN);
    }

    #[test]
    fn hit_test_records_pending_lyric_click_seek() {
        let mut renderer = LyricsRenderer::new();
        renderer.scene = Some(PreparedScene {
            config: test_config(),
            lines: vec![test_line(ClusterRole::Standalone, 1_000, 2_000, 50.0)],
            content_height: 0.0,
        });

        let hit = renderer.hit_test_line(20.0, 40.0, 1_000);

        assert_eq!(hit, 0);
        let pending = renderer.pending_lyric_click_seek.expect("pending click");
        assert_eq!(pending.source_index, 0);
        assert_eq!(pending.visible_scroll_y, 0.0);

        let miss = renderer.hit_test_line(20.0, 180.0, 1_000);
        assert_eq!(miss, -1);
        assert!(renderer.pending_lyric_click_seek.is_none());
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

        // Exit window is end(2500) + linger(200) .. + fade(400) = 2700..3100.
        let exiting = scene.dynamic_line_layouts(2_900)[1];
        assert!(exiting.height > 0.0 && exiting.height < 30.0);

        assert_eq!(scene.dynamic_line_layouts(3_200)[1].height, 0.0);
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
