use cosmic_text::{
    fontdb, Align, Attrs, Buffer, Family, FontSystem, Metrics, PhysicalGlyph, Shaping, Style,
    Weight, Wrap,
};
use serde::{Deserialize, Serialize};
use skia_safe::{font_style, Data, FontMgr, FontStyle, Path, Typeface};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

mod draw;
mod font_fallback;
mod fonts;
mod layout;
mod player;
mod scroll;
mod text_utils;

use draw::{
    accompaniment_visibility, apply_vertical_fade_skia_band, cubic_bezier_easing,
    draw_breathing_dots_skia, draw_prepared_text_skia, draw_top_bar_skia, interlude_visibility,
    make_interlude_slot, rgba_from_argb,
};
use font_fallback::{cjk_family_priority, new_font_system};
use fonts::*;
use player::{
    PlayerButton, PlayerInput, PlayerInteractionState, PlayerScreenInput, PlayerTransitionState,
    PreparedPlayer,
};
use scroll::{ManualScrollState, SpringLineState};
use text_utils::{
    contains_han, has_trailing_whitespace, is_blank_text, is_punctuation_or_space,
    should_use_simple_animation, trailing_whitespace_count, trim_end_whitespace,
};

#[cfg(test)]
use draw::{awesome_glyph_effect_for_char, bounce, swell, syllable_lift_easing};

const DEFAULT_WIDTH: u32 = 1;
const DEFAULT_HEIGHT: u32 = 1;
const DEFAULT_PADDING_X: f32 = 16.0;
const DEFAULT_PADDING_Y: f32 = 16.0;
const DEFAULT_KEEP_ALIVE: f32 = 120.0;
const DEFAULT_NORMAL_FONT_SIZE: f32 = 34.0;
const DEFAULT_NORMAL_LINE_HEIGHT: f32 = 42.0;
const DEFAULT_ACCOMPANIMENT_FONT_SIZE: f32 = 20.0;
const DEFAULT_ACCOMPANIMENT_LINE_HEIGHT: f32 = 26.0;
const DEFAULT_TRANSLATION_FONT_SIZE: f32 = 16.0;
const DEFAULT_TRANSLATION_LINE_HEIGHT: f32 = 21.0;
const DEFAULT_ACCOMPANIMENT_TRANSLATION_FONT_SIZE: f32 = 14.0;
const DEFAULT_ACCOMPANIMENT_TRANSLATION_LINE_HEIGHT: f32 = 18.0;
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
// A scroll batch — the run of time-overlapping lines that auto-scroll anchors and
// moves as one rigid block (see `focus_group_range`), plus the main+accompaniment
// cluster that shares a scroll anchor — is capped at this many wrapped rows. A
// taller batch would freeze the scroll on its first line (pushing the sung line
// far below the focus), so it is "split" into smaller batches instead.
const MAX_SCROLL_GROUP_ROWS: usize = 3;
// Fraction of the content width each line uses in a duet song (lines aligned to
// both sides), so the two singers' lines occupy opposite 80% bands that overlap
// in the middle. Solo songs use the full width.
const DUET_LINE_WIDTH_RATIO: f32 = 0.85;
/// The top-bar overflow glyph is drawn in a 28dp circle, but its interactive
/// target follows the conventional 48dp square touch/click area.
const TOP_BAR_BUTTON_HIT_SCALE: f32 = 24.0 / 14.0;
const MAIN_LINE_UNFOCUSED_SCALE: f32 = 0.98;
const MAIN_LINE_FOCUS_ENTER_MS: f32 = 600.0;
const MAIN_LINE_FOCUS_EXIT_MS: f32 = 300.0;

// Wire contract with the Kotlin data layer. The grouped shape and camelCase field
// names are kept identical to `SceneStyle`/`LyricsSceneWire` in
// `ui/renderer/NativeLyricsScene.kt`, so the two sides share one vocabulary. Every
// group carries `#[serde(default)]` + a `Default` holding the same constants used
// by the engine, so a partial scene JSON still deserializes to sensible values.
#[derive(Debug, Deserialize)]
pub struct LyricsScene {
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Render pixels per host layout unit. Android supplies density × renderScale so
    /// viewport-driven lyric scaling is evaluated in dp instead of physical pixels;
    /// desktop/legacy scenes omit it and retain a 1:1 basis.
    #[serde(default, rename = "layoutDensity")]
    pub layout_density: Option<f32>,
    pub locale: Option<String>,
    /// Vertical content insets (px) for the lyrics band on a full-bleed surface.
    #[serde(default, rename = "contentTop")]
    pub content_top: Option<f32>,
    #[serde(default, rename = "contentBottom")]
    pub content_bottom: Option<f32>,
    /// Horizontal content insets (px) — the safe area's left/right (display cutout /
    /// side navigation bar in landscape). Lyrics + top bar are kept clear of them.
    #[serde(default, rename = "contentLeft")]
    pub content_left: Option<f32>,
    #[serde(default, rename = "contentRight")]
    pub content_right: Option<f32>,
    /// Optional player top bar (album thumbnail + title/artist + ⋯ button) rendered
    /// inside the surface. All geometry is in render px; the thumbnail image is the
    /// background artwork already installed via `set_background_art`.
    #[serde(default, rename = "topBar")]
    pub top_bar: Option<TopBarInput>,
    /// Optional full portrait player chrome. When present, Rust owns the complete
    /// player surface and constrains the lyric list to its flexible middle row.
    #[serde(default)]
    pub player: Option<PlayerInput>,
    #[serde(default)]
    pub style: SceneStyleInput,
    #[serde(default)]
    pub lines: Vec<LyricsLineInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopBarInput {
    pub title: String,
    pub artist: String,
    pub thumb_left: f32,
    pub thumb_top: f32,
    pub thumb_size: f32,
    pub thumb_radius: f32,
    pub text_left: f32,
    pub text_max_width: f32,
    pub title_top: f32,
    pub title_font_size: f32,
    pub title_line_height: f32,
    pub title_weight: u16,
    pub artist_top: f32,
    pub artist_font_size: f32,
    pub artist_line_height: f32,
    pub artist_alpha: f32,
    pub button_cx: f32,
    pub button_cy: f32,
    pub button_radius: f32,
}

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SceneStyleInput {
    pub typography: TypographyInput,
    pub spacing: SpacingInput,
    pub blur: BlurInput,
    pub focus: FocusInput,
    pub auto_scroll_spring: SpringInput,
    pub manual_scroll: ManualScrollInput,
    pub breathing_dots: BreathingDotsInput,
    pub text_color: u32,
    pub show_translation: bool,
    pub show_phonetic: bool,
}

impl Default for SceneStyleInput {
    fn default() -> Self {
        Self {
            typography: TypographyInput::default(),
            spacing: SpacingInput::default(),
            blur: BlurInput::default(),
            focus: FocusInput::default(),
            auto_scroll_spring: SpringInput::default(),
            manual_scroll: ManualScrollInput::default(),
            breathing_dots: BreathingDotsInput::default(),
            text_color: 0xffff_ffff,
            show_translation: true,
            show_phonetic: true,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TypographyInput {
    pub normal_font_size: f32,
    pub normal_line_height: f32,
    pub normal_font_weight: u16,
    pub normal_font_italic: bool,
    pub accompaniment_font_size: f32,
    pub accompaniment_line_height: f32,
    pub accompaniment_font_weight: u16,
    pub accompaniment_font_italic: bool,
    pub translation_font_size: f32,
    pub translation_line_height: f32,
    pub translation_font_weight: u16,
    pub translation_font_italic: bool,
    pub accompaniment_translation_font_size: f32,
    pub accompaniment_translation_line_height: f32,
    pub accompaniment_translation_font_weight: u16,
    pub accompaniment_translation_font_italic: bool,
    pub phonetic_font_size: f32,
    pub phonetic_line_height: f32,
    pub phonetic_font_weight: u16,
    pub phonetic_font_italic: bool,
}

impl Default for TypographyInput {
    fn default() -> Self {
        Self {
            normal_font_size: DEFAULT_NORMAL_FONT_SIZE,
            normal_line_height: DEFAULT_NORMAL_LINE_HEIGHT,
            normal_font_weight: 400,
            normal_font_italic: false,
            accompaniment_font_size: DEFAULT_ACCOMPANIMENT_FONT_SIZE,
            accompaniment_line_height: DEFAULT_ACCOMPANIMENT_LINE_HEIGHT,
            accompaniment_font_weight: 400,
            accompaniment_font_italic: false,
            translation_font_size: DEFAULT_TRANSLATION_FONT_SIZE,
            translation_line_height: DEFAULT_TRANSLATION_LINE_HEIGHT,
            translation_font_weight: 400,
            translation_font_italic: false,
            accompaniment_translation_font_size: DEFAULT_ACCOMPANIMENT_TRANSLATION_FONT_SIZE,
            accompaniment_translation_line_height: DEFAULT_ACCOMPANIMENT_TRANSLATION_LINE_HEIGHT,
            accompaniment_translation_font_weight: 400,
            accompaniment_translation_font_italic: false,
            phonetic_font_size: DEFAULT_TRANSLATION_FONT_SIZE,
            phonetic_line_height: DEFAULT_TRANSLATION_LINE_HEIGHT,
            phonetic_font_weight: 400,
            phonetic_font_italic: false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SpacingInput {
    pub horizontal_padding: f32,
    pub line_padding: f32,
    pub accompaniment_gap: f32,
    pub phonetic_gap: f32,
    pub focus_top_offset: f32,
    pub translation_gap: f32,
    pub accompaniment_translation_gap: f32,
}

impl Default for SpacingInput {
    fn default() -> Self {
        Self {
            horizontal_padding: DEFAULT_PADDING_X,
            line_padding: DEFAULT_PADDING_Y,
            accompaniment_gap: 0.0,
            phonetic_gap: 4.0,
            focus_top_offset: DEFAULT_KEEP_ALIVE,
            translation_gap: ROW_GAP,
            accompaniment_translation_gap: ROW_GAP,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BlurInput {
    pub enabled: bool,
    pub delta: f32,
    pub sharp_radius_lines: f32,
}

impl Default for BlurInput {
    fn default() -> Self {
        Self {
            enabled: true,
            delta: 3.0,
            sharp_radius_lines: BLUR_SHARP_RADIUS_LINES,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FocusInput {
    pub inactive_karaoke_alpha: f32,
    pub dim_min_alpha: f32,
    pub dim_falloff_ms: f32,
}

impl Default for FocusInput {
    fn default() -> Self {
        Self {
            inactive_karaoke_alpha: KARAOKE_INACTIVE_ALPHA,
            dim_min_alpha: FOCUS_ALPHA_MIN,
            dim_falloff_ms: FOCUS_ALPHA_FALLOFF_MS,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SpringInput {
    pub stiffness: f32,
    pub damping: f32,
    pub chain_coupling: f32,
    pub distance_falloff: f32,
    pub min_response: f32,
}

impl Default for SpringInput {
    fn default() -> Self {
        Self {
            stiffness: LINE_LAYOUT_SPRING_STIFFNESS,
            damping: LINE_LAYOUT_SPRING_DAMPING,
            chain_coupling: LINE_LAYOUT_CHAIN_COUPLING,
            distance_falloff: LINE_LAYOUT_DISTANCE_FALLOFF,
            min_response: LINE_LAYOUT_MIN_RESPONSE,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ManualScrollInput {
    pub max_fling_velocity: f32,
    pub deceleration_rate: f32,
    pub overscroll_stiffness: f32,
    pub overscroll_damping: f32,
    pub rubber_band_limit: f32,
    pub rubber_band_coefficient: f32,
    pub blur_restore_ms: u64,
    pub blur_fade_in_rate: f32,
    pub blur_fade_out_rate: f32,
}

impl Default for ManualScrollInput {
    fn default() -> Self {
        Self {
            max_fling_velocity: MANUAL_SCROLL_MAX_FLING_VELOCITY,
            deceleration_rate: MANUAL_SCROLL_DECELERATION_RATE,
            overscroll_stiffness: MANUAL_SCROLL_OVERSCROLL_STIFFNESS,
            overscroll_damping: MANUAL_SCROLL_OVERSCROLL_DAMPING,
            rubber_band_limit: MANUAL_SCROLL_RUBBER_BAND_LIMIT,
            rubber_band_coefficient: MANUAL_SCROLL_RUBBER_BAND_COEFFICIENT,
            blur_restore_ms: MANUAL_SCROLL_BLUR_RESTORE_MS,
            blur_fade_in_rate: MANUAL_SCROLL_BLUR_FADE_IN_RATE,
            blur_fade_out_rate: MANUAL_SCROLL_BLUR_FADE_OUT_RATE,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BreathingDotsInput {
    pub number: u32,
    pub size: f32,
    pub margin: f32,
    pub enter_ms: f32,
    pub still_ms: f32,
    pub dip_ms: f32,
    pub exit_ms: f32,
    pub color: u32,
}

impl Default for BreathingDotsInput {
    fn default() -> Self {
        Self {
            number: DEFAULT_DOTS_NUMBER,
            size: DEFAULT_DOTS_SIZE,
            margin: DEFAULT_DOTS_MARGIN,
            enter_ms: DEFAULT_DOTS_ENTER_MS,
            still_ms: DEFAULT_DOTS_STILL_MS,
            dip_ms: DEFAULT_DOTS_DIP_MS,
            exit_ms: DEFAULT_DOTS_EXIT_MS,
            color: 0xffff_ffff,
        }
    }
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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

/// Wall-clock cost of one `render_frame_to_canvas` call, split by phase.
/// Times are milliseconds. Hosts (desktop) can log these alongside flush/swap.
#[derive(Clone, Copy, Debug, Default)]
pub struct EngineFrameTiming {
    /// Resolve Skia typefaces for the scene (usually near-zero after first frame).
    pub typefaces_ms: f64,
    /// Dynamic line layouts, scroll targets, spring/manual-scroll cascade.
    pub layout_ms: f64,
    /// Mesh-gradient background (CPU breathe + Skia record of vertices/blur).
    pub background_ms: f64,
    /// Lyric line draw pass (glyph batches, DoF blur layers, edge fade).
    pub lyrics_ms: f64,
    /// In-surface player top bar.
    pub top_bar_ms: f64,
    /// Sum of the phases above (excludes any work outside the engine).
    pub total_ms: f64,
}

#[derive(Debug)]
pub struct LyricsRenderer {
    font_system: FontSystem,
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
    /// Per-primary-row visual scale, independent of line measurement and scroll.
    focus_scale_states: Vec<FocusScaleState>,
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
    locale: String,
    scene: Option<PreparedScene>,
    /// GPU mesh-gradient background, built from the current album art. `None` until
    /// art is supplied via [`set_background_art`]; drawn behind the lyrics.
    mesh_gradient: Option<crate::mesh::MeshGradient>,
    /// When true the engine owns the whole (full-bleed) surface: it clears to black
    /// and paints the mesh background before the lyrics. When false it behaves as a
    /// transparent overlay (legacy).
    background_enabled: bool,
    /// Whether the background reacts to audio loudness (paces time + amplitude).
    background_reactive: bool,
    /// Gates the background's time flow — frozen while playback is paused.
    playback_active: bool,
    /// Animation clock for the background (advanced by real frame dt, paced by
    /// loudness while playing) and the smoothed loudness driving the reactivity.
    mesh_time: f32,
    mesh_smoothed_loudness: f32,
    last_mesh_frame_at: Option<Instant>,
    /// Global fade-in for the background as new art becomes ready.
    background_alpha: f32,
    /// Timing of the most recent `render_frame_to_canvas` call.
    last_frame_timing: EngineFrameTiming,
    /// Native player pointer state and button press/release animation.
    player_interaction: PlayerInteractionState,
    player_transition: PlayerTransitionState,
    /// Authoritative portrait-player page. Host wire `screen` is only used as the
    /// initial value when a player scene first appears; subsequent Lyrics /
    /// Artwork / Queue taps update this field entirely inside Rust.
    player_screen: PlayerScreenInput,
    /// True once a player scene has been prepared at least once.
    player_screen_initialized: bool,
    /// Live duration/playing from music-foundation (or another native clock).
    /// When set, they override the wire values baked into [PreparedPlayer] so
    /// Kotlin never has to push per-frame transport state.
    player_live_duration_ms: Option<i32>,
    player_live_playing: Option<bool>,
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
    /// Player top bar (thumbnail + title/artist + ⋯ button), if the host supplied one.
    top_bar: Option<PreparedTopBar>,
    /// Complete native portrait player, mutually exclusive with the legacy top bar.
    player: Option<PreparedPlayer>,
}

/// Resolved top-bar geometry (render px) + shaped title/artist text.
#[derive(Debug)]
struct PreparedTopBar {
    thumb_left: f32,
    thumb_top: f32,
    thumb_size: f32,
    /// Capsule G2 clip, resolved once with the scene instead of rebuilt per frame.
    thumb_clip: Path,
    /// LyricsBlossom Wide mode draws a subtle border around the large cover.
    /// Portrait thumbnails keep this at zero.
    thumb_border_width: f32,
    text_left: f32,
    text_max_width: f32,
    title_top: f32,
    artist_top: f32,
    artist_alpha: f32,
    button_cx: f32,
    button_cy: f32,
    button_radius: f32,
    title: PreparedText,
    artist: PreparedText,
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
    /// Full-bleed player scene using the decompiled LyricsBlossom Wide layout.
    landscape_player: bool,
    normal_font_size: f32,
    normal_line_height: f32,
    normal_attrs: TextAttrs,
    accompaniment_font_size: f32,
    accompaniment_line_height: f32,
    accompaniment_attrs: TextAttrs,
    translation_font_size: f32,
    translation_line_height: f32,
    translation_attrs: TextAttrs,
    accompaniment_translation_font_size: f32,
    accompaniment_translation_line_height: f32,
    accompaniment_translation_attrs: TextAttrs,
    phonetic_font_size: f32,
    phonetic_line_height: f32,
    phonetic_attrs: TextAttrs,
    phonetic_gap: f32,
    // Gap (px) between a line's body and its translation: `translation_gap` for a
    // main/synced line, `accompaniment_translation_gap` for an accompaniment line.
    translation_gap: f32,
    accompaniment_translation_gap: f32,
    padding_x: f32,
    padding_y: f32,
    // Vertical content insets (px) for the lyrics band when the engine owns the
    // whole (full-bleed) surface: `content_top` is the status-bar + top-bar height,
    // `content_bottom` the navigation-bar height. Lyrics are clipped and edge-faded
    // to `[content_top, height - content_bottom]`. Both default 0 (legacy behavior:
    // the lyrics fill the surface, as when Compose sized the view to the band).
    content_top: f32,
    content_bottom: f32,
    // Horizontal content insets (px): the safe-area left/right (display cutout / side
    // navigation bar in landscape). In Wide mode these resolve the centered text
    // band; the separate clip insets below preserve the wider lyrics viewport.
    content_left: f32,
    content_right: f32,
    // Four-sided lyrics viewport clipping can be wider than the centered text band
    // in LyricsBlossom Wide mode. Stored as left/right insets from the surface.
    lyrics_clip_left: f32,
    lyrics_clip_right: f32,
    keep_alive: f32,
    text_color: u32,
    show_translation: bool,
    show_phonetic: bool,
    use_blur_effect: bool,
    blur_delta: f32,
    // Extra gap (px) between a main line and its own nested accompaniment.
    accompaniment_gap: f32,
    // Depth-of-field blur fully-sharp band radius, in line-height units.
    blur_sharp_radius_lines: f32,
    // Focus dimming / inactive karaoke syllable alpha.
    inactive_karaoke_alpha: f32,
    focus_dim_min_alpha: f32,
    focus_dim_falloff_ms: f32,
    breathing_dots: BreathingDotsConfig,
    // Scroll spring + manual-scroll physics (see [`ScrollParams`]).
    scroll_params: ScrollParams,
}

/// Auto-scroll spring cascade + manual (touch) scroll physics. Grouped so the
/// scroll module can read them as one `Copy` bundle. `Default` reproduces the
/// original hard-coded constants, so a renderer with no scene (unit tests) and
/// a scene that omits these JSON fields both behave exactly as before.
#[derive(Debug, Clone, Copy)]
struct ScrollParams {
    spring_stiffness: f32,
    spring_damping: f32,
    chain_coupling: f32,
    distance_falloff: f32,
    min_response: f32,
    max_fling_velocity: f32,
    deceleration_rate: f32,
    overscroll_stiffness: f32,
    overscroll_damping: f32,
    rubber_band_limit: f32,
    rubber_band_coefficient: f32,
    blur_restore_ms: u64,
    blur_fade_in_rate: f32,
    blur_fade_out_rate: f32,
}

impl Default for ScrollParams {
    fn default() -> Self {
        Self {
            spring_stiffness: LINE_LAYOUT_SPRING_STIFFNESS,
            spring_damping: LINE_LAYOUT_SPRING_DAMPING,
            chain_coupling: LINE_LAYOUT_CHAIN_COUPLING,
            distance_falloff: LINE_LAYOUT_DISTANCE_FALLOFF,
            min_response: LINE_LAYOUT_MIN_RESPONSE,
            max_fling_velocity: MANUAL_SCROLL_MAX_FLING_VELOCITY,
            deceleration_rate: MANUAL_SCROLL_DECELERATION_RATE,
            overscroll_stiffness: MANUAL_SCROLL_OVERSCROLL_STIFFNESS,
            overscroll_damping: MANUAL_SCROLL_OVERSCROLL_DAMPING,
            rubber_band_limit: MANUAL_SCROLL_RUBBER_BAND_LIMIT,
            rubber_band_coefficient: MANUAL_SCROLL_RUBBER_BAND_COEFFICIENT,
            blur_restore_ms: MANUAL_SCROLL_BLUR_RESTORE_MS,
            blur_fade_in_rate: MANUAL_SCROLL_BLUR_FADE_IN_RATE,
            blur_fade_out_rate: MANUAL_SCROLL_BLUR_FADE_OUT_RATE,
        }
    }
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
    /// Time the line's appearance (bloom) animation starts from. For a nested
    /// accompaniment this is its MAIN line's start, so the whole cluster blooms in
    /// together when the main appears; for every other line it's just `start`.
    entrance_start: i32,
    height: f32,
    right_aligned: bool,
    /// Horizontal draw offset (px) added to `padding_x`. Non-zero only for the
    /// right-aligned lines of a duet song, which are laid out in an 80%-wide band
    /// and shifted right so they still hug the true right edge.
    x_offset: f32,
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
        /// Visual progress direction; text shaping/order is kept separate.
        gradient_is_rtl: bool,
        syllables: Vec<PreparedSyllable>,
        text: PreparedText,
    },
    Synced {
        text: PreparedText,
    },
}

#[derive(Debug, Clone, Copy)]
struct FocusScaleState {
    value: f32,
    start: f32,
    target: f32,
    started_at: Option<Instant>,
    duration_ms: f32,
    initialized: bool,
}

impl Default for FocusScaleState {
    fn default() -> Self {
        Self {
            value: 1.0,
            start: 1.0,
            target: 1.0,
            started_at: None,
            duration_ms: 0.0,
            initialized: false,
        }
    }
}

impl FocusScaleState {
    fn sample(&mut self, now: Instant) -> bool {
        let Some(started_at) = self.started_at else {
            self.value = self.target;
            return false;
        };
        let progress = (now.saturating_duration_since(started_at).as_secs_f32() * 1000.0
            / self.duration_ms.max(1.0))
        .clamp(0.0, 1.0);
        let eased = if self.target > self.start {
            cubic_bezier_easing(progress, 0.0, 0.0, 0.2, 1.0)
        } else {
            cubic_bezier_easing(progress, 0.42, 0.0, 0.58, 1.0)
        };
        self.value = self.start + (self.target - self.start) * eased;
        if progress >= 1.0 {
            self.value = self.target;
            self.started_at = None;
            false
        } else {
            true
        }
    }

    fn update(&mut self, focused: bool, now: Instant) -> bool {
        let target = if focused {
            1.0
        } else {
            MAIN_LINE_UNFOCUSED_SCALE
        };
        if !self.initialized {
            self.value = target;
            self.start = target;
            self.target = target;
            self.initialized = true;
            return false;
        }

        self.sample(now);
        if (target - self.target).abs() > f32::EPSILON {
            self.start = self.value;
            self.target = target;
            self.duration_ms = if focused {
                MAIN_LINE_FOCUS_ENTER_MS
            } else {
                MAIN_LINE_FOCUS_EXIT_MS
            };
            self.started_at = Some(now);
        }
        self.sample(now)
    }
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
    syllable_segments: Vec<PreparedSyllableSegment>,
}

#[derive(Debug, Clone, Copy)]
struct PreparedSyllableSegment {
    syllable_index: usize,
    min_x: f32,
    max_x: f32,
    progress_start: f32,
    progress_end: f32,
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
    /// Alpha of the not-yet-sung part of the karaoke gradient (config-driven).
    inactive_alpha: f32,
}

#[derive(Debug, Clone, Copy)]
struct GlyphRenderEffect {
    offset_y: f32,
    scale: f32,
    shadow_blur_radius: f32,
    scale_pivot: Option<(f32, f32)>,
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
    allow_break_before: bool,
    char_offset_in_syllable: usize,
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
            focus_scale_states: Vec::new(),
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
            locale: "en-US".to_string(),
            scene: None,
            mesh_gradient: None,
            background_enabled: false,
            background_reactive: false,
            playback_active: false,
            mesh_time: 0.0,
            mesh_smoothed_loudness: 0.0,
            last_mesh_frame_at: None,
            background_alpha: 0.0,
            last_frame_timing: EngineFrameTiming::default(),
            player_interaction: PlayerInteractionState::default(),
            player_transition: PlayerTransitionState::default(),
            // Default resting page is full artwork; lyrics/queue are toggles off it.
            player_screen: PlayerScreenInput::Artwork,
            player_screen_initialized: false,
            player_live_duration_ms: None,
            player_live_playing: None,
        }
    }

    /// Timing breakdown for the most recent frame render.
    pub fn last_frame_timing(&self) -> EngineFrameTiming {
        self.last_frame_timing
    }

    /// Install the album artwork for the GPU mesh-gradient background. `pixels` is
    /// ARGB_8888 (`0xAARRGGBB`), row-major `width`×`height`. Enables the full-bleed
    /// background mode. `seed` keeps a song's control-point layout stable.
    pub fn set_background_art(&mut self, pixels: &[u32], width: usize, height: usize, seed: u32) {
        let replacing = self.background_enabled && self.mesh_gradient.is_some();
        let Some(mesh) = crate::mesh::MeshGradient::new(pixels, width, height, seed) else {
            // Keep the previous mesh on failure so a bad decode never blacks out
            // an already-visible player surface.
            return;
        };
        self.mesh_gradient = Some(mesh);
        self.background_enabled = true;
        // First artwork fades up from black. Track-to-track swaps stay fully
        // opaque so we never flash an empty black frame between covers.
        self.background_alpha = if replacing { 1.0 } else { 0.0 };
    }

    /// Turn the background off (revert to transparent-overlay behavior).
    pub fn clear_background(&mut self) {
        self.mesh_gradient = None;
        self.background_enabled = false;
    }

    /// Update playback state driving the background: `playing` gates the time flow,
    /// `reactive` enables loudness-driven pacing/amplitude.
    pub fn set_playback_state(&mut self, playing: bool, reactive: bool) {
        self.playback_active = playing;
        self.background_reactive = reactive;
    }

    /// Override portrait-player transport chrome with a native clock sample
    /// (music-foundation). Call every frame when a process-local clock is available.
    pub fn set_player_live_playback(&mut self, playing: bool, duration_ms: i32) {
        self.player_live_playing = Some(playing);
        self.player_live_duration_ms = Some(duration_ms.max(0));
        // Keep mesh time flow in lockstep with the same sample so hosts do not
        // need a separate Kotlin isPlaying push for the player surface.
        self.playback_active = playing;
    }

    /// Drop native live playback overrides (standalone hosts / tests).
    pub fn clear_player_live_playback(&mut self) {
        self.player_live_playing = None;
        self.player_live_duration_ms = None;
    }

    /// Select one of the three portrait pages and start the transition. Hosts
    /// should not echo this back through the scene wire.
    pub fn select_player_screen(&mut self, screen: PlayerScreenInput) {
        let previous = if self.player_screen_initialized {
            Some(self.player_screen)
        } else {
            self.scene
                .as_ref()
                .and_then(|scene| scene.player.as_ref())
                .map(|player| player.screen)
        };
        self.player_screen = screen;
        self.player_screen_initialized = true;
        self.player_interaction.reveal();
        if let Some(player) = self
            .scene
            .as_mut()
            .and_then(|scene| scene.player.as_mut())
        {
            player.screen = screen;
        }
        self.player_transition.set_target(previous, Some(screen));
    }

    fn apply_player_live_state(&mut self) {
        let Some(player) = self
            .scene
            .as_mut()
            .and_then(|scene| scene.player.as_mut())
        else {
            return;
        };
        player.screen = self.player_screen;
        if let Some(duration_ms) = self.player_live_duration_ms {
            player.duration_ms = duration_ms;
        }
        if let Some(playing) = self.player_live_playing {
            player.is_playing = playing;
        }
    }

    /// Draw the opaque mesh-gradient background across the whole surface and advance
    /// its (loudness-paced) animation clock. Returns whether the background is still
    /// animating (so the render loop keeps ticking). No-op unless `background_enabled`.
    fn draw_background(&mut self, canvas: &skia_safe::Canvas, width: f32, height: f32) -> bool {
        if !self.background_enabled {
            return false;
        }
        canvas.clear(skia_safe::Color::BLACK);

        let now = Instant::now();
        let frame_dt = self
            .last_mesh_frame_at
            .map(|last| now.saturating_duration_since(last).as_secs_f32().clamp(0.0, 0.1))
            .unwrap_or(0.0);
        let dt = frame_dt * 0.2;
        self.last_mesh_frame_at = Some(now);

        // Smooth loudness with real-time attack/release constants. The old fixed
        // per-frame factor changed with frame rate, and its amplitude could become
        // negative; because amp also shifted shader phase, transients visibly shook
        // the whole background.
        let raw_loudness = (crate::audio::current_metrics().loudness / 6.0).clamp(0.0, 1.0);
        self.mesh_smoothed_loudness = smooth_mesh_loudness(
            self.mesh_smoothed_loudness,
            raw_loudness,
            frame_dt,
        );
        let (amp, speed) = mesh_reactive_parameters(
            self.mesh_smoothed_loudness,
            self.background_reactive,
        );
        if self.playback_active {
            self.mesh_time += dt * speed;
        }
        // Ease the artwork in.
        self.background_alpha = (self.background_alpha + dt * 2.0).min(1.0);

        // Re-tessellate the control-point grid if the surface size/aspect changed
        // (cheap; only rebuilds on a real resize), then draw. The linear fade-in
        // progress is eased so the artwork blooms in smoothly rather than ramping.
        let eased_alpha = ease_in_out(self.background_alpha);
        if let Some(mesh) = self.mesh_gradient.as_mut() {
            mesh.ensure_grid(width, height);
        }
        if let Some(mesh) = self.mesh_gradient.as_ref() {
            mesh.draw(canvas, width, height, self.mesh_time, amp, eased_alpha);
        }
        // Keep animating while playing (frozen — and parkable — when paused).
        self.playback_active
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

    pub fn set_scene_json(&mut self, json: &str) -> Result<RendererMetrics, String> {
        let scene: LyricsScene = serde_json::from_str(json).map_err(|error| error.to_string())?;
        let mut prepared = self.prepare_scene(scene)?;
        // Rust owns the settled page once the player is live. Host wire `screen`
        // only seeds the first player appearance so metadata updates never yank
        // the user off the artwork/queue page mid-interaction.
        if let Some(player) = prepared.player.as_mut() {
            if self.player_screen_initialized {
                player.screen = self.player_screen;
            } else {
                self.player_screen = player.screen;
                self.player_screen_initialized = true;
                self.player_interaction.reveal();
            }
            if let Some(duration_ms) = self.player_live_duration_ms {
                player.duration_ms = duration_ms;
            }
            if let Some(playing) = self.player_live_playing {
                player.is_playing = playing;
            }
        } else {
            self.player_screen_initialized = false;
            self.player_screen = PlayerScreenInput::Artwork;
        }
        let previous_screen = self
            .scene
            .as_ref()
            .and_then(|scene| scene.player.as_ref())
            .map(|player| player.screen);
        let target_screen = prepared.player.as_ref().map(|player| player.screen);
        // Only re-target the transition when the page actually changes. Metadata
        // scene rebuilds keep the same screen and must not restart the animation.
        if previous_screen != target_screen {
            self.player_transition
                .set_target(previous_screen, target_screen);
        }
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
        if self.locale == locale {
            return;
        }

        // cosmic-text's locale is baked into the FontSystem, so swap it (carrying
        // the loaded db across) only when it actually differs.
        if self.font_system.locale() != locale {
            let replacement = new_font_system(locale.to_string(), fontdb::Database::new());
            let old = std::mem::replace(&mut self.font_system, replacement);
            let (_, db) = old.into_locale_and_db();
            self.font_system = new_font_system(locale.to_string(), db);
        }
        self.locale = locale.to_string();
        self.font_selection_cache.clear();

        // The NDK `AFontMatcher` picks a different system family per locale for the
        // same CJK codepoint (SC vs TC vs JP vs KR share Unicode ranges). It was
        // never told the scene's locale, so it fell back to the device default —
        // e.g. rendering a zh-Hant or ja song with Simplified glyphs. Sync it here
        // and drop the per-char resolutions so they're recomputed for the new
        // locale (fonts already loaded stay in the db).
        #[cfg(target_os = "android")]
        {
            if let Some(matcher) = self.font_matcher.as_mut() {
                matcher.set_locales(locale);
            }
            self.matched_char_family.clear();
            self.matched_glyphs.clear();
        }
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

    pub fn render_frame_to_canvas(
        &mut self,
        current_time_ms: i32,
        canvas: &skia_safe::Canvas,
    ) -> i32 {
        let frame_start = Instant::now();
        let mut timing = EngineFrameTiming::default();

        // Apply Rust-owned screen + native live transport before sampling the
        // transition or drawing chrome, so every frame sees the same state.
        self.apply_player_live_state();

        let phase = Instant::now();
        let typeface_stats = self.ensure_skia_typefaces_for_scene();
        timing.typefaces_ms = phase_ms(phase);
        let should_log_debug = self.should_log_render_debug(current_time_ms);
        let frame_now = Instant::now();
        let player_transition = self.player_transition.sample(frame_now);
        let bottom_chrome = self
            .player_interaction
            .bottom_chrome_sample(self.player_screen, frame_now);

        let phase = Instant::now();
        let (
            target_layouts,
            auto_scroll_y,
            max_scroll_y,
            width,
            height,
            content_top,
            content_bottom,
            lyrics_clip_left,
            lyrics_clip_right,
            keep_alive,
            base_color,
            focus_end,
        ) = {
            let Some(scene) = &self.scene else {
                self.last_frame_timing = timing;
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
                scene.config.width.max(DEFAULT_WIDTH) as f32,
                scene.config.height.max(DEFAULT_HEIGHT),
                scene.config.content_top,
                scene
                    .player
                    .as_ref()
                    .map_or(scene.config.content_bottom, |player| {
                        bottom_chrome.content_bottom(player.layout)
                    }),
                scene.config.lyrics_clip_left,
                scene.config.lyrics_clip_right,
                scene.config.keep_alive,
                rgba_from_argb(scene.config.text_color),
                focus_end,
            )
        };
        // First half of layout (targets); spring half is timed after background.
        let mut layout_ms = phase_ms(phase);

        // Bottom layer: the opaque GPU mesh-gradient background (no-op unless the
        // engine owns the full surface). Draws before any lyrics and advances its
        // own loudness-paced animation clock.
        let phase = Instant::now();
        let background_animating = self.draw_background(canvas, width, height as f32);
        timing.background_ms = phase_ms(phase);

        // A seek (the user tapped a lyric, or scrubbed) is a discontinuous jump
        // in playback time. If it lands while a manual/plain-list view is still
        // on screen, seed the layout springs from that visible list scroll before
        // clearing the manual offset. This makes a manually revealed far lyric
        // behave like an ordinary on-screen lyric click instead of being judged
        // against the old auto-scroll target and snapping as an "ultra far" jump.
        let phase = Instant::now();
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
        layout_ms += phase_ms(phase);
        timing.layout_ms = layout_ms;
        // While the user manually scrolls the depth-of-field blur is eased away
        // so the lyrics stay sharp for reading.
        let blur_scale = (1.0 - self.manual_scroll_blur_release).clamp(0.0, 1.0);
        let focus_scale_animating = {
            let line_count = self.scene.as_ref().map_or(0, |scene| scene.lines.len());
            self.focus_scale_states
                .resize(line_count, FocusScaleState::default());
            let now = Instant::now();
            let mut animating = false;
            if let Some(scene) = &self.scene {
                for (state, line) in self.focus_scale_states.iter_mut().zip(&scene.lines) {
                    let focused = !line.cluster_role.is_nested_accompaniment()
                        && scene.is_line_focused(line, current_time_ms);
                    animating |= state.update(focused, now);
                }
            }
            animating
        };
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

        // Each cluster's main line index, so a nested accompaniment can borrow its
        // main line's blur and stay bound to it (crisp/blurred as one unit).
        let cluster_main_index: HashMap<usize, usize> = scene
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.cluster_role == ClusterRole::Main)
            .map(|(index, line)| (line.cluster_index, index))
            .collect();

        // Confine the lyrics to the content band on a full-bleed surface so they
        // never draw under the top bar or navigation bar. When the engine owns an
        // opaque background, the lyrics are ALSO isolated into a layer so the edge
        // fade (`DstIn`, which scales framebuffer alpha) dissolves only the lyrics
        // and never darkens the background beneath them. Both a no-op in the legacy
        // transparent-overlay mode (insets 0, background disabled).
        // Round the band edges to whole pixels: the isolated lyrics layer is
        // composited with `Plus` (additive), and a fractional layer/clip edge leaves
        // a faint bright 1px seam at the band top/bottom under that blend (the edge
        // pixel gets fractional coverage that the fade doesn't fully zero). Aligning
        // to the pixel grid removes the seam.
        let phase = Instant::now();
        let band_left = lyrics_clip_left.round().clamp(0.0, width);
        let band_right = (width - lyrics_clip_right)
            .round()
            .clamp(band_left, width);
        let band_top = content_top.round();
        let band_bottom = (height as f32 - content_bottom).round();
        let inset_lyrics =
            content_top > 0.0
                || content_bottom > 0.0
                || lyrics_clip_left > 0.0
                || lyrics_clip_right > 0.0;
        let player_scene = scene.player.is_some();
        let (player_lyrics_alpha, player_lyrics_scale) = if player_scene {
            player_transition.content_transform(player::PlayerScreenInput::Lyrics)
        } else {
            (1.0, 1.0)
        };
        let isolate_lyrics = self.background_enabled || player_scene;
        if isolate_lyrics {
            let bounds = skia_safe::Rect::new(band_left, band_top, band_right, band_bottom);
            // The player transition needs an isolated lyrics layer even without a
            // mesh so the whole page can fade/scale as one unit. With a mesh, keep
            // the existing additive composition used by the lyrics renderer.
            let mut layer_paint = skia_safe::Paint::default();
            if self.background_enabled {
                layer_paint.set_blend_mode(skia_safe::BlendMode::Plus);
            }
            layer_paint.set_alpha_f(player_lyrics_alpha);
            canvas.save_layer(
                &skia_safe::canvas::SaveLayerRec::default()
                    .bounds(&bounds)
                    .paint(&layer_paint),
            );
            canvas.clip_rect(bounds, skia_safe::ClipOp::Intersect, false);
            if player_scene {
                let center_x = (band_left + band_right) * 0.5;
                let center_y = (band_top + band_bottom) * 0.5;
                canvas.translate((center_x, center_y));
                canvas.scale((player_lyrics_scale, player_lyrics_scale));
                canvas.translate((-center_x, -center_y));
            }
        } else if inset_lyrics {
            canvas.save();
            canvas.clip_rect(
                skia_safe::Rect::new(band_left, band_top, band_right, band_bottom),
                skia_safe::ClipOp::Intersect,
                true,
            );
        }

        for (line_index, line) in scene.lines.iter().enumerate() {
            if player_lyrics_alpha <= 0.001 {
                break;
            }
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
            // Left edge of this line's text: the left content inset + `padding_x`,
            // plus the duet right-band shift for right-aligned lines of a duet song
            // (zero otherwise).
            let origin_x = scene.config.content_left + scene.config.padding_x + line.x_offset;
            let focus_alpha = scene.focus_alpha(line, current_time_ms);
            let distance_alpha = focus_alpha * dynamic_layout.text_visibility;
            // Unify the dim of already-sung and not-yet-sung lines: as a line dims
            // away from focus, fade the karaoke "unsung" alpha back up to 1.0, so a
            // fully-dimmed upcoming line reads at the same alpha as a fully-dimmed
            // already-sung one (both just `focus_alpha`) instead of being an extra
            // `inactive_karaoke_alpha` darker. At full focus it stays the configured
            // inactive alpha, so the active line's karaoke sweep is unchanged.
            let effective_inactive_karaoke_alpha = unified_inactive_karaoke_alpha(
                focus_alpha,
                scene.config.focus_dim_min_alpha,
                scene.config.inactive_karaoke_alpha,
            );
            // A nested accompaniment shares its main line's blur so the whole
            // main+accompaniment cluster sharpens/blurs as one unit — crisp
            // whenever the main is the active line, and at the main line's blur
            // radius otherwise — instead of blurring on its own screen position.
            let blur_source_index = if line.cluster_role.is_nested_accompaniment() {
                cluster_main_index
                    .get(&line.cluster_index)
                    .copied()
                    .unwrap_or(line_index)
            } else {
                line_index
            };
            let blur_source_line = &scene.lines[blur_source_index];
            let blur_source_y = dynamic_layouts
                .get(blur_source_index)
                .map(|layout| layout.top)
                .unwrap_or(y);
            let blur_radius = if scene.is_line_focused(blur_source_line, current_time_ms) {
                0.0
            } else {
                scene.blur_radius_for_screen_y(blur_source_y, blur_anchor_y) * blur_scale
            };

            if let Some(interlude) = &line.interlude {
                if dynamic_layout.interlude_visibility > 0.001 {
                    draw_breathing_dots_skia(
                        canvas,
                        y + DOTS_VERTICAL_PADDING,
                        interlude,
                        &scene.config,
                        current_time_ms,
                    );
                }
            }

            if dynamic_layout.text_visibility <= 0.001 {
                continue;
            }

            if should_log_debug {
                frame_stats.visible_lines += 1;
            }

            // Compose LyricsLineItem scales only the primary row, around the
            // bottom-left or bottom-right corner selected by visual alignment.
            // Layout stays unchanged; this is a canvas-only transform.
            let main_line_scale = self
                .focus_scale_states
                .get(line_index)
                .map(|state| state.value)
                .unwrap_or(1.0);
            let scaled_main_line = !line.cluster_role.is_nested_accompaniment()
                && (main_line_scale - 1.0).abs() > 0.0001;
            if scaled_main_line {
                let pivot_x = if line.right_aligned {
                    scene.config.width as f32 - scene.config.content_right - scene.config.padding_x
                } else {
                    scene.config.content_left + scene.config.padding_x
                };
                let pivot_y = y + text_y_offset + line.height;
                canvas.save();
                canvas.translate((pivot_x, pivot_y));
                canvas.scale((main_line_scale, main_line_scale));
                canvas.translate((-pivot_x, -pivot_y));
            }

            // Nested accompaniment lines bloom out of the main line's adjacent edge
            // and retract back the same way. The scale tracks `text_visibility`
            // (the same enter/hold/exit curve as the alpha and the make-room
            // height), so the appear and disappear animations match: scale+alpha
            // 0->1 on entrance, 1->0 on exit, pivoted at the corner touching the
            // main line — bottom for a line above the main, top for one below — on
            // the side set by the main line's alignment.
            let accompaniment_scale = if line.cluster_role.is_nested_accompaniment() {
                dynamic_layout.text_visibility
            } else {
                1.0
            };
            let scaled_accompaniment = accompaniment_scale < 0.999;
            if scaled_accompaniment {
                let pivot_x = if line.right_aligned {
                    scene.config.width as f32 - scene.config.content_right - scene.config.padding_x
                } else {
                    scene.config.content_left + scene.config.padding_x
                };
                let draw_top = y + text_y_offset + scene.config.padding_y;
                let pivot_y = if line.cluster_role == ClusterRole::BeforeAccompaniment {
                    draw_top + line.main_text_height()
                } else {
                    draw_top
                };
                canvas.save();
                canvas.translate((pivot_x, pivot_y));
                canvas.scale((accompaniment_scale, accompaniment_scale));
                canvas.translate((-pivot_x, -pivot_y));
            }

            match &line.kind {
                PreparedLineKind::Karaoke {
                    is_accompaniment,
                    gradient_is_rtl,
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
                        origin_x,
                        y + text_y_offset + scene.config.padding_y,
                        base_color,
                        line_alpha,
                        blur_radius,
                        Some((
                            current_time_ms,
                            *gradient_is_rtl,
                            effective_inactive_karaoke_alpha,
                            syllables,
                        )),
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
                        origin_x,
                        y + text_y_offset + scene.config.padding_y,
                        base_color,
                        distance_alpha,
                        blur_radius,
                        None,
                    );
                }
            }

            let detail_gap = line.detail_gap(&scene.config);
            let mut detail_y =
                y + text_y_offset + scene.config.padding_y + line.main_text_height() + detail_gap;
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
                    origin_x,
                    detail_y,
                    base_color,
                    0.42 * distance_alpha,
                    blur_radius,
                    None,
                );
                detail_y += translation.height + detail_gap;
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
                    origin_x,
                    detail_y,
                    base_color,
                    0.55 * distance_alpha,
                    blur_radius,
                    None,
                );
            }

            if scaled_accompaniment {
                canvas.restore();
            }
            if scaled_main_line {
                canvas.restore();
            }
        }

        // Fade the top and bottom edges of the content band so lines dissolve as
        // they scroll off. The top fade stays inside the keep-alive gap so the
        // active line (anchored at keep_alive from the top) is never touched. The
        // band collapses to the whole surface when no insets are set.
        let band_height = (band_bottom - band_top).max(1.0);
        let inner_keep_alive = (keep_alive - band_top).max(0.0);
        if player_lyrics_alpha > 0.001 {
            apply_vertical_fade_skia_band(
                canvas,
                width,
                band_top,
                band_bottom,
                inner_keep_alive * 0.7,
                band_height * 0.12,
            );
        }

        if isolate_lyrics || inset_lyrics {
            canvas.restore();
        }
        timing.lyrics_ms = phase_ms(phase);

        // Player top bar (thumbnail + title/artist + ⋯ button) over the background,
        // in the top-inset region above the lyrics band. Returns whether an
        // overflowing title/artist is marqueeing so the loop keeps ticking.
        let phase = Instant::now();
        let mut top_bar_animating = false;
        if let Some(top_bar) = &scene.top_bar {
            top_bar_animating = draw_top_bar_skia(
                canvas,
                &self.skia_typefaces,
                self.mesh_gradient.as_ref().map(|mesh| mesh.thumbnail()),
                top_bar,
                current_time_ms,
            );
        }
        let player_animating = if let Some(player) = &scene.player {
            player::draw_player(
                canvas,
                &self.skia_typefaces,
                self.mesh_gradient.as_ref().map(|mesh| mesh.thumbnail()),
                player,
                &mut self.player_interaction,
                player_transition,
                bottom_chrome,
                current_time_ms,
            )
        } else {
            false
        };
        timing.top_bar_ms = phase_ms(phase);
        timing.total_ms = phase_ms(frame_start);
        self.last_frame_timing = timing;

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

        // Keep the render loop ticking every frame while playing: the host no longer
        // pushes a position each Compose frame (it interpolates the clock on the
        // render thread instead), so the loop must sustain itself so the karaoke
        // sweep, marquee and background all keep advancing. It still parks when
        // paused (`playback_active` false) and nothing else is animating.
        let _ = top_bar_animating;
        if self.layout_animation_active
            || self.manual_scroll_active
            || self.playback_active
            || background_animating
            || focus_scale_animating
            || player_animating
        {
            1
        } else {
            0
        }
    }

    /// Begin a native-player button gesture and return its stable action code.
    /// Zero means the point does not hit player chrome.
    pub fn player_pointer_down(&mut self, x: f32, y: f32) -> i32 {
        let Some(layout) = self
            .scene
            .as_ref()
            .and_then(|scene| scene.player.as_ref())
            .map(|player| player.layout)
        else {
            return 0;
        };
        let ui = player::PlayerUiLayout::resolve(layout, self.player_screen);
        self.player_interaction.press(&ui, x, y)
    }

    /// Finish a native-player button gesture.
    ///
    /// Mode nav:
    /// - Lyrics / Queue are toggles against the resting Artwork page.
    /// - Output never changes the page (host opens a media sheet).
    ///
    /// The action code is still returned so the host can handle transport /
    /// favorite / more / output / queue filters.
    pub fn player_pointer_up(&mut self, x: f32, y: f32) -> i32 {
        let Some(layout) = self
            .scene
            .as_ref()
            .and_then(|scene| scene.player.as_ref())
            .map(|player| player.layout)
        else {
            self.player_interaction.cancel();
            return 0;
        };
        let ui = player::PlayerUiLayout::resolve(layout, self.player_screen);
        let action = self.player_interaction.release(&ui, x, y);
        match action {
            code if code == PlayerButton::Lyrics as i32 => {
                let next = if self.player_screen == PlayerScreenInput::Lyrics {
                    PlayerScreenInput::Artwork
                } else {
                    PlayerScreenInput::Lyrics
                };
                self.select_player_screen(next);
            }
            code if code == PlayerButton::Output as i32 => {
                // Sheet-only: no page change, no selected state.
            }
            code if code == PlayerButton::Queue as i32 => {
                let next = if self.player_screen == PlayerScreenInput::Queue {
                    PlayerScreenInput::Artwork
                } else {
                    PlayerScreenInput::Queue
                };
                self.select_player_screen(next);
            }
            _ => {}
        }
        action
    }

    pub fn cancel_player_pointer(&mut self) {
        self.player_interaction.cancel();
    }

    /// Whether `(x, y)` (render px) falls on the top bar's 48dp ⋯ target.
    pub fn hit_test_top_bar_button(&self, x: f32, y: f32) -> bool {
        let Some(scene) = &self.scene else {
            return false;
        };
        let Some(bar) = &scene.top_bar else {
            return false;
        };
        let half_extent = bar.button_radius * TOP_BAR_BUTTON_HIT_SCALE;
        (x - bar.button_cx).abs() <= half_extent && (y - bar.button_cy).abs() <= half_extent
    }

    /// Whether a point belongs to player chrome outside the lyrics viewport.
    pub fn hit_test_top_bar_region(&self, x: f32, y: f32) -> bool {
        let Some(scene) = &self.scene else {
            return false;
        };
        if scene.top_bar.is_none()
            || x < 0.0
            || x > scene.config.width as f32
            || y < 0.0
            || y > scene.config.height as f32
        {
            return false;
        }
        if scene.config.landscape_player {
            x < scene.config.lyrics_clip_left
                || x > scene.config.width as f32 - scene.config.lyrics_clip_right
        } else {
            y < scene.config.content_top
        }
    }

    pub fn hit_test_line(&mut self, x: f32, y: f32, current_time_ms: i32) -> i32 {
        let Some(scene) = &self.scene else {
            self.pending_lyric_click_seek = None;
            return -1;
        };
        let content_bottom = scene.player.as_ref().map_or(scene.config.content_bottom, |player| {
            self.player_interaction
                .bottom_chrome_sample(self.player_screen, Instant::now())
                .content_bottom(player.layout)
        });

        if x < 0.0 || y < 0.0 || x > scene.config.width as f32 || y > scene.config.height as f32 {
            self.pending_lyric_click_seek = None;
            return -1;
        }
        // Match the renderer's four-sided lyrics clip. In Wide mode the left side
        // is the cover/metadata pane, so it must never select a lyric sharing its y.
        if x < scene.config.lyrics_clip_left
            || x > scene.config.width as f32 - scene.config.lyrics_clip_right
            || y < scene.config.content_top
            || y > scene.config.height as f32 - content_bottom
        {
            self.pending_lyric_click_seek = None;
            return -1;
        }

        let dynamic_layouts = scene.dynamic_line_layouts(current_time_ms);
        let auto_scroll_y = scene.scroll_y_for_time_with_layouts(current_time_ms, &dynamic_layouts);
        let scroll_y = self.manual_scroll_projected_scroll(
            auto_scroll_y,
            scene.max_scroll_for_layouts(&dynamic_layouts),
        );
        // Prefer the on-screen spring layouts when available (match what the user
        // sees). Fall back to content-space targets + scroll for the first frame.
        let hit = if self.frame_layouts.len() == scene.lines.len() {
            scene.lines.iter().enumerate().find(|(index, _)| {
                self.frame_layouts.get(*index).is_some_and(|layout| {
                    layout.text_visibility > 0.001
                        && y >= layout.top
                        && y <= layout.top + layout.height
                })
            })
        } else {
            let content_y = y + scroll_y;
            scene.lines.iter().enumerate().find(|(index, _)| {
                dynamic_layouts.get(*index).is_some_and(|layout| {
                    layout.text_visibility > 0.001
                        && content_y >= layout.top
                        && content_y <= layout.top + layout.height
                })
            })
        };
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

    /// Start time (ms) of the scene line with the given `source_index`, if present.
    pub fn line_start_ms(&self, source_index: usize) -> Option<i32> {
        self.scene
            .as_ref()?
            .lines
            .iter()
            .find(|line| line.source_index == source_index)
            .map(|line| line.start)
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

        let falloff = self.config.focus_dim_falloff_ms;
        let progress = if current_time_ms < line.start {
            1.0 - (line.start - current_time_ms) as f32 / falloff
        } else {
            1.0 - (current_time_ms - line.effective_end) as f32 / falloff
        };
        focus_alpha_from_progress(progress, self.config.focus_dim_min_alpha)
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

    /// Extends the focused range to cover every line whose singing genuinely
    /// overlaps it — so a run of lines with overlapping timelines (duet trades, a
    /// main line and its nested accompaniment) is treated as ONE scroll batch.
    /// Accompaniment lines are ordinary entries in `self.lines`, so they
    /// participate in the overlap chain and are never ignored. The scroll anchor
    /// holds on the group's first line, and the cascade's rigid block ends at the
    /// group's last line, until the whole batch has finished.
    ///
    /// The overlap test is STRICT (`>`, not `>=`): a line that ends exactly when
    /// the next begins is a clean sequential hand-off, not an overlap. Most TTML
    /// (e.g. Apple Music exports) times lyrics perfectly back-to-back, so treating
    /// `end == next.start` as overlap would chain an entire gapless run into one
    /// group and freeze auto-scroll on its first line for the whole run.
    fn focus_group_range(&self, current_time_ms: i32) -> (usize, usize) {
        let (mut first, mut last) = self.focus_index_range(current_time_ms);
        // Cap the batch at `MAX_SCROLL_GROUP_ROWS` wrapped rows so a long run of
        // overlapping lines (a duet trade, or a main line plus a tall
        // accompaniment) doesn't chain into one block that freezes auto-scroll on
        // its first line for the whole run — split the oversized batch instead. The
        // base focused range is always kept; only the overlap extension is capped.
        let mut rows: usize = (first..=last)
            .map(|index| self.lines[index].text_row_count())
            .sum();
        // Walk backward while the previous line was still being sung when this one
        // began (their timelines truly overlap — an exact touch does not count).
        while first > 0 && self.lines[first - 1].effective_end > self.lines[first].start {
            let candidate_rows = self.lines[first - 1].text_row_count();
            if rows + candidate_rows > MAX_SCROLL_GROUP_ROWS {
                break;
            }
            rows += candidate_rows;
            first -= 1;
        }
        // Walk forward while the next line begins before this one finishes.
        while last + 1 < self.lines.len()
            && self.lines[last].effective_end > self.lines[last + 1].start
        {
            let candidate_rows = self.lines[last + 1].text_row_count();
            if rows + candidate_rows > MAX_SCROLL_GROUP_ROWS {
                break;
            }
            rows += candidate_rows;
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
        let mut prev_cluster_index: Option<usize> = None;

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
            // Lines that share a cluster — a main line and its nested
            // accompaniment(s) — are separated by `accompaniment_gap` instead of the
            // normal inter-line spacing. That normal spacing is `2 * padding_y`
            // (each line box bakes in `padding_y` top and bottom), so shift the
            // cursor by the difference to *replace* it rather than add to it: the
            // gap between a main line and its accompaniment is now driven solely by
            // `accompaniment_gap`, not by `padding_y` (`linePadding`). Scaled by the
            // line's own visibility so the gap collapses as the accompaniment fades,
            // which also restores the full padding between separate clusters.
            if prev_cluster_index == Some(line.cluster_index) {
                let normal_gap = self.config.padding_y * 2.0;
                cursor_y += (self.config.accompaniment_gap - normal_gap) * text_visibility;
            }
            layouts.push(DynamicLineLayout {
                top: cursor_y,
                height,
                text_visibility,
                interlude_visibility,
            });
            cursor_y += height;
            prev_cluster_index = Some(line.cluster_index);
        }

        layouts
    }

    fn text_visibility_for_line(&self, line: &PreparedLine, current_time_ms: i32) -> f32 {
        if line.cluster_role.is_nested_accompaniment() {
            // Enter from the main line's start (so it blooms in with the main), but
            // exit on its own end (it leaves after its own singing finishes).
            accompaniment_visibility(line.entrance_start, line.end, current_time_ms)
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
        let blur_lines = (lines_away - self.config.blur_sharp_radius_lines).clamp(0.0, 10.0);
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

    /// Number of wrapped rows the main lyric text occupies (translation/phonetic
    /// excluded) — used to size a scroll cluster for the oversized-cluster split.
    fn text_row_count(&self) -> usize {
        match &self.kind {
            PreparedLineKind::Karaoke { text, .. } => text.rows.len().max(1),
            PreparedLineKind::Synced { text } => text.rows.len().max(1),
        }
    }

    fn is_accompaniment(&self) -> bool {
        matches!(
            self.kind,
            PreparedLineKind::Karaoke {
                is_accompaniment: true,
                ..
            }
        )
    }

    /// Gap between this line's body and its translation/phonetic detail rows.
    fn detail_gap(&self, config: &SceneConfig) -> f32 {
        if self.is_accompaniment() {
            config.accompaniment_translation_gap
        } else {
            config.translation_gap
        }
    }
}

fn prepared_text_width(text: &PreparedText) -> f32 {
    text.rows.iter().map(|row| row.width).fold(0.0, f32::max)
}

fn focus_alpha_from_progress(progress: f32, min_alpha: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    min_alpha + (1.0 - min_alpha) * eased
}

/// Smoothstep ease-in-out for a `[0, 1]` progress. Used to smooth otherwise-linear
/// fades (e.g. the background artwork bloom) so they accelerate and settle.
#[inline]
fn phase_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smooth_mesh_loudness(current: f32, target: f32, dt: f32) -> f32 {
    let current = current.clamp(0.0, 1.0);
    let target = target.clamp(0.0, 1.0);
    let time_constant = if target > current { 0.24 } else { 0.70 };
    let blend = 1.0 - (-dt.max(0.0) / time_constant).exp();
    current + (target - current) * blend
}

fn mesh_reactive_parameters(loudness: f32, reactive: bool) -> (f32, f32) {
    if !reactive {
        return (0.0, 0.2);
    }
    let loudness = loudness.clamp(0.0, 1.0);
    let shaped = loudness * loudness * (3.0 - 2.0 * loudness);
    // Keep the zoom in a narrow, always-positive range and cap pacing at 2.5x the
    // idle rate. Transients now change momentum rather than teleporting phase.
    (0.07 - shaped * 0.04, 0.2 + shaped * 0.30)
}

/// The karaoke "unsung" multiplier, chosen so the unsung ("unplayed") text sits
/// at a CONSTANT alpha for a line's whole life — the dimmed floor `dim_min`, which
/// is also where a dimmed already-sung line sits, so played and unplayed lines are
/// unified when dimmed. The caller multiplies the result by the line's
/// `focus_alpha`, so returning `dim_min / focus_alpha` keeps that product pinned
/// at `dim_min`: a line therefore never dims as it becomes active (no "darken then
/// the brush appears" dip) — its words just fill from `dim_min` up to full. At
/// full focus this yields `dim_min` (< 1.0), so the sung→unsung sweep still reads.
/// `dim_min.max(inactive)` keeps at least the configured contrast if a scene sets
/// `inactive` brighter than the dim floor.
fn unified_inactive_karaoke_alpha(focus_alpha: f32, dim_min: f32, inactive: f32) -> f32 {
    let target = dim_min.max(inactive);
    if focus_alpha <= f32::EPSILON {
        return 1.0;
    }
    (target / focus_alpha).clamp(0.0, 1.0)
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
            landscape_player: false,
            normal_font_size: 34.0,
            normal_line_height: 42.0,
            normal_attrs: TextAttrs::default(),
            accompaniment_font_size: 20.0,
            accompaniment_line_height: 26.0,
            accompaniment_attrs: TextAttrs::default(),
            translation_font_size: 16.0,
            translation_line_height: 21.0,
            translation_attrs: TextAttrs::default(),
            accompaniment_translation_font_size: 14.0,
            accompaniment_translation_line_height: 18.0,
            accompaniment_translation_attrs: TextAttrs::default(),
            phonetic_font_size: 13.0,
            phonetic_line_height: 16.0,
            phonetic_attrs: TextAttrs::default(),
            phonetic_gap: 4.0,
            translation_gap: ROW_GAP,
            accompaniment_translation_gap: ROW_GAP,
            padding_x: 16.0,
            content_left: 0.0,
            content_right: 0.0,
            lyrics_clip_left: 0.0,
            lyrics_clip_right: 0.0,
            padding_y: 8.0,
            content_top: 0.0,
            content_bottom: 0.0,
            keep_alive: 32.0,
            text_color: 0xffff_ffff,
            show_translation: true,
            show_phonetic: true,
            use_blur_effect: true,
            blur_delta: 3.0,
            accompaniment_gap: 0.0,
            blur_sharp_radius_lines: BLUR_SHARP_RADIUS_LINES,
            inactive_karaoke_alpha: KARAOKE_INACTIVE_ALPHA,
            focus_dim_min_alpha: FOCUS_ALPHA_MIN,
            focus_dim_falloff_ms: FOCUS_ALPHA_FALLOFF_MS,
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
            scroll_params: ScrollParams::default(),
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
            entrance_start: start,
            height,
            right_aligned: false,
            x_offset: 0.0,
            interlude: None,
            kind: PreparedLineKind::Synced {
                text: test_text(height),
            },
            translation: None,
            phonetic: None,
        }
    }

    fn test_top_bar() -> PreparedTopBar {
        PreparedTopBar {
            thumb_left: 20.0,
            thumb_top: 20.0,
            thumb_size: 68.0,
            thumb_clip: crate::capsule::continuous_rounded_rect(
                skia_safe::Rect::new(20.0, 20.0, 88.0, 88.0),
                14.0,
            ),
            thumb_border_width: 0.0,
            text_left: 96.0,
            text_max_width: 160.0,
            title_top: 30.0,
            artist_top: 52.0,
            artist_alpha: 0.4,
            button_cx: 300.0,
            button_cy: 54.0,
            button_radius: 14.0,
            title: test_text(20.0),
            artist: test_text(18.0),
        }
    }

    fn indexed_line(index: usize, start: i32, end: i32, height: f32) -> PreparedLine {
        let mut line = test_line(ClusterRole::Standalone, start, end, height);
        line.source_index = index;
        line.cluster_index = index;
        line
    }

    // Most TTML (Apple Music exports especially) times lyrics perfectly
    // back-to-back: line N ends exactly when line N+1 begins. Such a hand-off is
    // NOT an overlap, so each line must stay its own focus group — otherwise the
    // whole gapless run chains into one group anchored on its first line and
    // auto-scroll freezes there for the entire run (the havent-rain bug).
    #[test]
    fn exactly_touching_lines_do_not_chain_into_one_focus_group() {
        let scene = PreparedScene {
            config: test_config(),
            lines: vec![
                indexed_line(0, 0, 1_000, 50.0),
                indexed_line(1, 1_000, 2_000, 50.0),
                indexed_line(2, 2_000, 3_000, 50.0),
                indexed_line(3, 3_000, 4_000, 50.0),
                indexed_line(4, 4_000, 5_000, 50.0),
            ],
            content_height: 0.0,
            top_bar: None,
            player: None,
        };

        // A line deep in the gapless run is its own group, not swallowed by the
        // lines before/after it.
        assert_eq!(scene.focus_group_range(2_500), (2, 2));
        assert_eq!(scene.focus_group_range(500), (0, 0));

        // The scroll anchor therefore advances as playback moves line to line.
        let layouts = scene.dynamic_line_layouts(2_500);
        let s0 = scene.scroll_y_for_time_with_layouts(500, &layouts);
        let s1 = scene.scroll_y_for_time_with_layouts(1_500, &layouts);
        let s2 = scene.scroll_y_for_time_with_layouts(2_500, &layouts);
        assert!(
            s0 < s1 && s1 < s2,
            "auto-scroll should advance across touching lines, got {s0} -> {s1} -> {s2}"
        );
    }

    // Genuinely overlapping timelines (a duet trade or a main line and its nested
    // accompaniment still being sung together) MUST still be one scroll batch so
    // the anchor doesn't hop forward while the earlier line is still audible.
    #[test]
    fn genuinely_overlapping_lines_still_group() {
        let scene = PreparedScene {
            config: test_config(),
            lines: vec![
                indexed_line(0, 0, 2_000, 50.0),
                indexed_line(1, 1_500, 3_500, 50.0),
            ],
            content_height: 0.0,
            top_bar: None,
            player: None,
        };

        // At t=1000 only line 0 is focused, but line 1 begins before it ends, so
        // the forward walk pulls line 1 into the group.
        assert_eq!(scene.focus_group_range(1_000), (0, 1));
    }

    // A long run of lines that each overlap the next (a duet trade, or a main line
    // plus a tall accompaniment) would otherwise chain into one batch and freeze
    // auto-scroll on its first line. The row cap splits the oversized batch.
    #[test]
    fn oversized_overlapping_batch_is_capped_at_row_limit() {
        let scene = PreparedScene {
            config: test_config(),
            lines: vec![
                indexed_line(0, 0, 2_000, 50.0),
                indexed_line(1, 1_500, 3_500, 50.0),
                indexed_line(2, 3_000, 5_000, 50.0),
                indexed_line(3, 4_500, 6_500, 50.0),
                indexed_line(4, 6_000, 8_000, 50.0),
            ],
            content_height: 0.0,
            top_bar: None,
            player: None,
        };

        // Every line overlaps the next, so without a cap the whole run chains into
        // one batch. Each `indexed_line` is one row, so the batch stops at 3 rows.
        let (first, last) = scene.focus_group_range(1_600);
        assert_eq!((first, last), (0, 2));
        assert!(last - first + 1 <= MAX_SCROLL_GROUP_ROWS);
    }

    // Every newly-exposed knob must survive the Kotlin JSON keys → `SceneConfig`
    // resolution (this is the wire contract the config data classes serialize to).
    #[test]
    fn scene_json_overrides_new_config_knobs() {
        let mut renderer = LyricsRenderer::new();
        let json = r#"{
            "width": 300, "height": 200,
            "style": {
                "typography": {
                    "accompanimentTranslationFontSize": 12.0,
                    "accompanimentTranslationLineHeight": 15.0
                },
                "spacing": {
                    "accompanimentGap": 40.0,
                    "translationGap": 24.0,
                    "accompanimentTranslationGap": 10.0
                },
                "blur": { "sharpRadiusLines": 9.0 },
                "focus": {
                    "inactiveKaraokeAlpha": 0.5,
                    "dimMinAlpha": 0.7,
                    "dimFalloffMs": 1234.0
                },
                "autoScrollSpring": {
                    "stiffness": 250.0,
                    "damping": 30.0,
                    "chainCoupling": 0.9,
                    "distanceFalloff": 0.5,
                    "minResponse": 0.2
                },
                "manualScroll": {
                    "maxFlingVelocity": 9999.0,
                    "decelerationRate": 0.99,
                    "overscrollStiffness": 88.0,
                    "overscrollDamping": 11.0,
                    "rubberBandLimit": 90.0,
                    "rubberBandCoefficient": 0.3,
                    "blurRestoreMs": 1500,
                    "blurFadeInRate": 5.0,
                    "blurFadeOutRate": 9.0
                }
            },
            "lines": []
        }"#;
        let scene: LyricsScene = serde_json::from_str(json).unwrap();
        let config = renderer.prepare_scene(scene).unwrap().config;

        assert_eq!(config.accompaniment_gap, 40.0);
        assert_eq!(config.translation_gap, 24.0);
        assert_eq!(config.accompaniment_translation_gap, 10.0);
        assert_eq!(config.accompaniment_translation_font_size, 12.0);
        assert_eq!(config.accompaniment_translation_line_height, 15.0);
        assert_eq!(config.blur_sharp_radius_lines, 9.0);
        assert_eq!(config.inactive_karaoke_alpha, 0.5);
        assert_eq!(config.focus_dim_min_alpha, 0.7);
        assert_eq!(config.focus_dim_falloff_ms, 1234.0);
        // Groups omitted from the JSON fall back to the wire `Default`s.
        assert_eq!(config.normal_font_size, DEFAULT_NORMAL_FONT_SIZE);
        let sp = config.scroll_params;
        assert_eq!(sp.spring_stiffness, 250.0);
        assert_eq!(sp.spring_damping, 30.0);
        assert_eq!(sp.chain_coupling, 0.9);
        assert_eq!(sp.distance_falloff, 0.5);
        assert_eq!(sp.min_response, 0.2);
        assert_eq!(sp.max_fling_velocity, 9999.0);
        assert_eq!(sp.rubber_band_limit, 90.0);
        assert_eq!(sp.rubber_band_coefficient, 0.3);
        assert_eq!(sp.blur_restore_ms, 1500);
        assert_eq!(sp.blur_fade_out_rate, 9.0);
    }

    #[test]
    fn rtl_uses_opposite_physical_start_and_gradient_without_reordering() {
        let mut renderer = LyricsRenderer::new();
        renderer.load_system_fonts();
        let json = r#"{
            "width": 320, "height": 240,
            "style": { "showTranslation": false, "showPhonetic": false },
            "lines": [
                {
                    "kind": "karaoke", "start": 0, "end": 1000,
                    "isAccompaniment": false, "alignment": "start",
                    "translation": null, "phonetic": null,
                    "syllables": [
                        {"content":"שלום", "start":0, "end":500, "phonetic":null},
                        {"content":"עולם", "start":500, "end":1000, "phonetic":null}
                    ]
                },
                {
                    "kind": "karaoke", "start": 1000, "end": 2000,
                    "isAccompaniment": false, "alignment": "end",
                    "translation": null, "phonetic": null,
                    "syllables": [{"content":"עולם", "start":1000, "end":2000, "phonetic":null}]
                },
                {
                    "kind": "synced", "start": 2000, "end": 3000,
                    "content": "مرحبا", "translation": null
                }
            ]
        }"#;
        let scene: LyricsScene = serde_json::from_str(json).unwrap();
        let prepared = renderer.prepare_scene(scene).unwrap();

        assert!(!prepared.lines[0].right_aligned);
        assert!(prepared.lines[1].right_aligned);
        assert!(!prepared.lines[2].right_aligned);
        let PreparedLineKind::Karaoke {
            gradient_is_rtl,
            syllables,
            ..
        } = &prepared.lines[0].kind
        else {
            panic!("expected karaoke line");
        };
        assert!(!gradient_is_rtl);
        assert!(syllables[0].layout_x < syllables[1].layout_x);
    }

    #[test]
    fn main_line_focus_scale_uses_asymmetric_tweens() {
        let start = Instant::now();
        let mut state = FocusScaleState::default();

        assert!(!state.update(false, start));
        assert_eq!(state.value, MAIN_LINE_UNFOCUSED_SCALE);
        assert!(state.update(true, start));
        assert!(state.sample(start + std::time::Duration::from_millis(300)));
        assert!(state.value > MAIN_LINE_UNFOCUSED_SCALE && state.value < 1.0);
        assert!(!state.sample(start + std::time::Duration::from_millis(600)));
        assert_eq!(state.value, 1.0);

        assert!(state.update(false, start + std::time::Duration::from_millis(600)));
        assert!(!state.sample(start + std::time::Duration::from_millis(900)));
        assert_eq!(state.value, MAIN_LINE_UNFOCUSED_SCALE);
    }

    #[test]
    fn interlude_alignment_follows_next_main_past_before_accompaniment() {
        let json = r#"{
            "lines": [
                {
                    "kind": "karaoke", "start": 0, "end": 1000,
                    "isAccompaniment": false, "alignment": "start",
                    "translation": null, "phonetic": null, "syllables": []
                },
                {
                    "kind": "karaoke", "start": 8000, "end": 9000,
                    "isAccompaniment": true, "alignment": "start",
                    "translation": null, "phonetic": null, "syllables": []
                },
                {
                    "kind": "karaoke", "start": 9000, "end": 10000,
                    "isAccompaniment": false, "alignment": "end",
                    "translation": null, "phonetic": null, "syllables": []
                }
            ]
        }"#;
        let scene: LyricsScene = serde_json::from_str(json).unwrap();

        assert_eq!(
            layout::interlude_right_alignments(&scene.lines),
            vec![false, true, true]
        );
    }

    // The blur sharp-band knob must actually change the blur: a row 5 line-heights
    // from the anchor blurs under the default 2.5-line band but stays crisp under a
    // wide one.
    #[test]
    fn blur_sharp_radius_config_widens_the_sharp_band() {
        let far = 5.0 * test_config().normal_line_height;

        let mut narrow = test_config();
        narrow.blur_sharp_radius_lines = 2.5;
        let narrow_scene = PreparedScene {
            config: narrow,
            lines: vec![],
            content_height: 0.0,
            top_bar: None,
            player: None,
        };
        assert!(narrow_scene.blur_radius_for_screen_y(far, 0.0) > 0.0);

        let mut wide = test_config();
        wide.blur_sharp_radius_lines = 8.0;
        let wide_scene = PreparedScene {
            config: wide,
            lines: vec![],
            content_height: 0.0,
            top_bar: None,
            player: None,
        };
        assert_eq!(wide_scene.blur_radius_for_screen_y(far, 0.0), 0.0);
    }

    #[test]
    fn focus_alpha_is_symmetric_for_past_and_future_lines() {
        let scene = PreparedScene {
            config: test_config(),
            lines: vec![test_line(ClusterRole::Standalone, 1_000, 2_000, 50.0)],
            content_height: 0.0,
            top_bar: None,
            player: None,
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
        assert_eq!(
            scene.focus_alpha(line, line.effective_end + 800),
            FOCUS_ALPHA_MIN
        );
    }

    #[test]
    fn hit_test_records_pending_lyric_click_seek() {
        let mut renderer = LyricsRenderer::new();
        renderer.scene = Some(PreparedScene {
            config: test_config(),
            lines: vec![test_line(ClusterRole::Standalone, 1_000, 2_000, 50.0)],
            content_height: 0.0,
            top_bar: None,
            player: None,
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
    fn hit_test_excludes_player_top_bar_even_if_a_line_animates_below_it() {
        let mut renderer = LyricsRenderer::new();
        let mut config = test_config();
        config.content_top = 100.0;
        renderer.scene = Some(PreparedScene {
            config,
            lines: vec![test_line(ClusterRole::Standalone, 1_000, 2_000, 50.0)],
            content_height: 0.0,
            top_bar: Some(test_top_bar()),
            player: None,
        });
        renderer.frame_layouts = vec![DynamicLineLayout {
            top: 40.0,
            height: 50.0,
            text_visibility: 1.0,
            interlude_visibility: 0.0,
        }];

        assert!(renderer.hit_test_top_bar_region(20.0, 60.0));
        assert_eq!(renderer.hit_test_line(20.0, 60.0, 1_000), -1);
        assert!(renderer.pending_lyric_click_seek.is_none());
    }

    #[test]
    fn wide_lyrics_scale_grows_with_viewport_and_caps_at_one_point_four() {
        assert!((layout::wide_lyrics_layout_scale(1024.0, 512.0) - 1.0).abs() < 0.001);
        assert!((layout::wide_lyrics_layout_scale(1312.0, 756.0) - 1.2).abs() < 0.001);
        assert!((layout::wide_lyrics_layout_scale(1600.0, 1000.0) - 1.4).abs() < 0.001);
        assert!((layout::wide_lyrics_layout_scale(2400.0, 1080.0) - 1.4).abs() < 0.001);
        assert!((layout::wide_lyrics_layout_scale(2048.0, 512.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn android_landscape_without_top_bar_uses_dynamic_scale_and_focus() {
        let json = r#"{
            "width":1312,"height":756,
            "contentTop":24,"contentBottom":16,
            "contentLeft":10,"contentRight":20,
            "lines":[]
        }"#;
        let mut scene: LyricsScene = serde_json::from_str(json).expect("Android landscape scene");
        let chrome = layout::resolve_player_chrome(&mut scene);
        let visible_focus_y = layout::resolve_keep_alive(&chrome, &scene.style.spacing)
            + scene.style.spacing.line_padding * chrome.lyrics_layout_scale;

        assert!(!chrome.landscape_player, "no desktop two-pane chrome without top bar");
        assert!((chrome.lyrics_layout_scale - 1.2).abs() < 0.001);
        assert!((chrome.focus_y.expect("landscape focus") - 302.4).abs() < 0.01);
        assert!((visible_focus_y - 302.4).abs() < 0.01);
        assert!((chrome.content_left - 10.0).abs() < 0.01);
        assert!((chrome.content_right - 20.0).abs() < 0.01);
    }

    #[test]
    fn android_phone_uses_dp_scale_basis_instead_of_physical_pixels() {
        // Current ADB device: 2400×1080 @ 440dpi. Android caps the render target at
        // 2.2 MP, yielding about 2211×995 and an effective render density of 2.533.
        let json = r#"{
            "width":2211,"height":995,"layoutDensity":2.533,
            "topBar":{
                "title":"Title","artist":"Artist",
                "thumbLeft":71,"thumbTop":71,"thumbSize":172.244,"thumbRadius":35,
                "textLeft":263,"textMaxWidth":1800,
                "titleTop":96,"titleFontSize":43,"titleLineHeight":56,"titleWeight":600,
                "artistTop":152,"artistFontSize":38,"artistLineHeight":49,"artistAlpha":0.4,
                "buttonCx":2105,"buttonCy":157,"buttonRadius":35
            },
            "lines":[]
        }"#;
        let mut scene: LyricsScene = serde_json::from_str(json).expect("ADB landscape scene");
        let chrome = layout::resolve_player_chrome(&mut scene);

        assert!(chrome.landscape_player);
        assert!((chrome.lyrics_layout_scale - 1.0).abs() < 0.001);
        assert!((chrome.focus_y.expect("landscape focus") - 398.0).abs() < 0.01);
        assert!(chrome.lyrics_clip_left < 2211.0 * 0.5 - 18.0);
        assert!(chrome.lyrics_clip_right < 2211.0 * 0.04);
        let line_padding = scene.style.spacing.horizontal_padding * chrome.lyrics_layout_scale;
        let text_left = chrome.content_left + line_padding;
        let text_right = 2211.0 - chrome.content_right - line_padding;
        assert!(text_left >= 2211.0 * 0.5 - 18.0);
        assert!(text_right <= 2211.0 * 0.96);
    }

    #[test]
    fn wide_player_scene_uses_decompiled_default_geometry() {
        let json = r#"{
            "width":1600,"height":1000,"contentTop":140,"contentBottom":16,
            "contentLeft":10,"contentRight":20,
            "topBar":{
                "title":"Title","artist":"Artist",
                "thumbLeft":38,"thumbTop":52,"thumbSize":68,"thumbRadius":14,
                "textLeft":114,"textMaxWidth":1360,
                "titleTop":62,"titleFontSize":17,"titleLineHeight":22.1,"titleWeight":600,
                "artistTop":84.1,"artistFontSize":15,"artistLineHeight":19.5,"artistAlpha":0.4,
                "buttonCx":1558,"buttonCy":86,"buttonRadius":14
            },
            "lines":[]
        }"#;
        let mut scene: LyricsScene = serde_json::from_str(json).expect("landscape scene");
        let chrome = layout::resolve_player_chrome(&mut scene);
        let visible_focus_y = layout::resolve_keep_alive(&chrome, &scene.style.spacing)
            + scene.style.spacing.line_padding * chrome.lyrics_layout_scale;
        let bar = scene.top_bar.as_ref().expect("resolved player chrome");

        assert!(chrome.landscape_player);
        assert!((chrome.content_top - 24.0).abs() < 0.01);
        assert!((chrome.lyrics_clip_left - 782.0).abs() < 0.01);
        assert!((chrome.lyrics_clip_right - 64.0).abs() < 0.01);
        assert!((chrome.content_left - 836.6).abs() < 0.01);
        assert!((chrome.content_right - 118.6).abs() < 0.01);
        assert!((chrome.lyrics_layout_scale - 1.4).abs() < 0.01);
        assert!((chrome.focus_y.expect("wide focus y") - 400.0).abs() < 0.01);
        assert!((visible_focus_y - 400.0).abs() < 0.01);
        assert!((bar.thumb_left - 146.0).abs() < 0.01);
        assert!((bar.thumb_top - 208.2).abs() < 0.01);
        assert!((bar.thumb_size - 500.0).abs() < 0.01);
        assert!((chrome.thumb_border_width - 1.0).abs() < 0.01);
        assert!((bar.title_top - 758.2).abs() < 0.01);
    }

    #[test]
    fn short_ultrawide_player_centers_cover_group_and_keeps_metadata_below_it() {
        let json = r#"{
            "width":2048,"height":512,"contentTop":152,
            "topBar":{
                "title":"Title","artist":"Artist",
                "thumbLeft":28,"thumbTop":64,"thumbSize":68,"thumbRadius":14,
                "textLeft":104,"textMaxWidth":1800,
                "titleTop":74,"titleFontSize":17,"titleLineHeight":22.1,"titleWeight":600,
                "artistTop":96.1,"artistFontSize":15,"artistLineHeight":19.5,"artistAlpha":0.4,
                "buttonCx":2006,"buttonCy":98,"buttonRadius":14
            },
            "lines":[]
        }"#;
        let mut scene: LyricsScene = serde_json::from_str(json).expect("short wide scene");
        let chrome = layout::resolve_player_chrome(&mut scene);
        let visible_focus_y = layout::resolve_keep_alive(&chrome, &scene.style.spacing)
            + scene.style.spacing.line_padding * chrome.lyrics_layout_scale;
        let bar = scene.top_bar.as_ref().expect("short wide player chrome");

        assert!(chrome.landscape_player);
        assert!((chrome.lyrics_layout_scale - 1.0).abs() < 0.01);
        assert!((chrome.focus_y.expect("short wide focus y") - 204.8).abs() < 0.01);
        assert!((visible_focus_y - 204.8).abs() < 0.01);
        assert!((bar.thumb_left - 375.0).abs() < 0.01);
        assert!((bar.thumb_top - 112.4).abs() < 0.01);
        assert!((bar.thumb_size - 256.0).abs() < 0.01);
        assert!((bar.title_top - 394.0).abs() < 0.01);
        assert!(bar.title_top >= bar.thumb_top + bar.thumb_size);
    }

    #[test]
    fn wide_player_requires_decompiled_minimum_width() {
        let json = r#"{
            "width":1000,"height":500,
            "topBar":{
                "title":"Title","artist":"Artist",
                "thumbLeft":28,"thumbTop":28,"thumbSize":68,"thumbRadius":14,
                "textLeft":104,"textMaxWidth":800,
                "titleTop":38,"titleFontSize":17,"titleLineHeight":22.1,"titleWeight":600,
                "artistTop":60.1,"artistFontSize":15,"artistLineHeight":19.5,"artistAlpha":0.4,
                "buttonCx":958,"buttonCy":62,"buttonRadius":14
            },
            "lines":[]
        }"#;
        let mut scene: LyricsScene = serde_json::from_str(json).expect("narrow wide scene");
        let chrome = layout::resolve_player_chrome(&mut scene);

        assert!(!chrome.landscape_player);
        assert!((chrome.lyrics_layout_scale - 1.0).abs() < 0.01);
        assert!(chrome.focus_y.is_none());
    }

    #[test]
    fn portrait_player_scene_keeps_host_top_bar_geometry() {
        let json = r#"{
            "width":400,"height":800,"contentTop":140,"contentBottom":16,
            "contentLeft":10,"contentRight":20,
            "topBar":{
                "title":"Title","artist":"Artist",
                "thumbLeft":38,"thumbTop":52,"thumbSize":68,"thumbRadius":14,
                "textLeft":114,"textMaxWidth":160,
                "titleTop":62,"titleFontSize":17,"titleLineHeight":22.1,"titleWeight":600,
                "artistTop":84.1,"artistFontSize":15,"artistLineHeight":19.5,"artistAlpha":0.4,
                "buttonCx":358,"buttonCy":86,"buttonRadius":14
            },
            "lines":[]
        }"#;
        let mut scene: LyricsScene = serde_json::from_str(json).expect("portrait scene");
        let chrome = layout::resolve_player_chrome(&mut scene);
        let bar = scene.top_bar.as_ref().expect("portrait top bar");

        assert!(!chrome.landscape_player);
        assert!((chrome.lyrics_layout_scale - 1.0).abs() < 0.01);
        assert!(chrome.focus_y.is_none());
        assert!((chrome.content_top - 140.0).abs() < 0.01);
        assert!((chrome.content_left - 10.0).abs() < 0.01);
        assert!((chrome.content_right - 20.0).abs() < 0.01);
        assert!((chrome.lyrics_clip_left - 10.0).abs() < 0.01);
        assert!((chrome.lyrics_clip_right - 20.0).abs() < 0.01);
        assert!((chrome.thumb_border_width - 0.0).abs() < 0.01);
        assert!((bar.thumb_left - 38.0).abs() < 0.01);
        assert!((bar.thumb_top - 52.0).abs() < 0.01);
        assert!((bar.thumb_size - 68.0).abs() < 0.01);
    }

    #[test]
    fn landscape_player_hit_test_rejects_left_pane() {
        let mut renderer = LyricsRenderer::new();
        let mut config = test_config();
        config.width = 400;
        config.landscape_player = true;
        config.content_left = 160.0;
        config.content_right = 20.0;
        config.lyrics_clip_left = 160.0;
        config.lyrics_clip_right = 20.0;
        renderer.scene = Some(PreparedScene {
            config,
            lines: vec![test_line(ClusterRole::Standalone, 1_000, 2_000, 50.0)],
            content_height: 0.0,
            top_bar: Some(test_top_bar()),
            player: None,
        });
        renderer.frame_layouts = vec![DynamicLineLayout {
            top: 40.0,
            height: 50.0,
            text_visibility: 1.0,
            interlude_visibility: 0.0,
        }];

        assert!(renderer.hit_test_top_bar_region(80.0, 60.0));
        assert!(!renderer.hit_test_top_bar_region(200.0, 60.0));
        assert_eq!(renderer.hit_test_line(80.0, 60.0, 1_000), -1);
        assert_eq!(renderer.hit_test_line(200.0, 60.0, 1_000), 0);
    }

    #[test]
    fn top_bar_button_uses_a_48px_square_click_target() {
        let mut config = test_config();
        config.width = 400;
        config.content_top = 100.0;
        let mut renderer = LyricsRenderer::new();
        renderer.scene = Some(PreparedScene {
            config,
            lines: vec![],
            content_height: 0.0,
            top_bar: Some(test_top_bar()),
            player: None,
        });

        assert!(renderer.hit_test_top_bar_button(323.0, 77.0));
        assert!(!renderer.hit_test_top_bar_button(325.0, 79.0));
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
                allow_break_before: false,
                char_offset_in_syllable: 0,
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
        );

        assert_eq!(text.rows.len(), 1);
        assert_eq!(text.rows[0].min_x, -40.0);
        assert_eq!(text.rows[0].max_x, 100.0);
        assert_eq!(syllables[0].layout_x, -40.0);
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn synced_line_wrap_height_matches_prepared_rows() {
        let mut renderer = LyricsRenderer::new();
        renderer.font_system.db_mut().load_system_fonts();
        if renderer.font_system.db().is_empty() {
            return;
        }

        let json = r#"{
            "width": 170,
            "height": 220,
            "style": {
                "typography": {
                    "normalFontSize": 20.0,
                    "normalLineHeight": 28.0
                },
                "spacing": {
                    "horizontalPadding": 10.0,
                    "linePadding": 6.0,
                    "focusTopOffset": 20.0
                },
                "blur": { "enabled": false },
                "showTranslation": false,
                "showPhonetic": false
            },
            "lines": [{
                "kind": "synced",
                "sourceIndex": 0,
                "clusterIndex": 0,
                "clusterRole": "standalone",
                "start": 0,
                "end": 4000,
                "content": "one two three four five six seven eight nine ten eleven twelve",
                "translation": null
            }]
        }"#;
        let scene: LyricsScene = serde_json::from_str(json).unwrap();
        let prepared = renderer.prepare_scene(scene).unwrap();
        let line = prepared.lines.first().unwrap();
        let PreparedLineKind::Synced { text } = &line.kind else {
            panic!("expected synced line");
        };

        assert!(
            text.rows.len() > 1,
            "expected wrapped synced text rows, got {:?}",
            text.rows
        );
        assert!((text.height - text.rows.len() as f32 * 28.0).abs() < 0.01);
        assert!((line.height - (text.height + 12.0)).abs() < 0.01);
        for (index, row) in text.rows.iter().enumerate() {
            assert!(row.width <= 150.5, "row overflowed: {row:?}");
            assert!((row.y - index as f32 * 28.0).abs() < 0.01);
        }
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn oversized_syllable_wraps_inside_karaoke_layout() {
        let mut renderer = LyricsRenderer::new();
        renderer.font_system.db_mut().load_system_fonts();
        if renderer.font_system.db().is_empty() {
            return;
        }

        // Apple line-timed TTML (itunes:timing="Line") reaches the renderer as
        // one karaoke syllable spanning the complete parent line, as in Lie - NF.
        let json = r#"{
            "width": 190,
            "height": 220,
            "style": {
                "typography": {
                    "normalFontSize": 20.0,
                    "normalLineHeight": 28.0
                },
                "spacing": {
                    "horizontalPadding": 10.0,
                    "linePadding": 6.0,
                    "focusTopOffset": 20.0
                },
                "blur": { "enabled": false },
                "showTranslation": false,
                "showPhonetic": false
            },
            "lines": [{
                "kind": "karaoke",
                "sourceIndex": 0,
                "clusterIndex": 0,
                "clusterRole": "main",
                "start": 686,
                "end": 5110,
                "isAccompaniment": false,
                "alignment": "start",
                "translation": null,
                "phonetic": null,
                "syllables": [{
                    "content": "I heard you told your friends that I'm just not your type",
                    "start": 686,
                    "end": 5110,
                    "phonetic": null
                }]
            }, {
                "kind": "karaoke",
                "sourceIndex": 1,
                "clusterIndex": 1,
                "clusterRole": "main",
                "start": 6000,
                "end": 7000,
                "isAccompaniment": false,
                "alignment": "start",
                "translation": null,
                "phonetic": null,
                "syllables": [{
                    "content": "Ohh",
                    "start": 6000,
                    "end": 7000,
                    "phonetic": null
                }]
            }, {
                "kind": "karaoke",
                "sourceIndex": 2,
                "clusterIndex": 2,
                "clusterRole": "main",
                "start": 8000,
                "end": 10000,
                "isAccompaniment": false,
                "alignment": "start",
                "translation": null,
                "phonetic": null,
                "syllables": [{
                    "content": "antidisestablishmentarianism",
                    "start": 8000,
                    "end": 10000,
                    "phonetic": null
                }]
            }]
        }"#;
        let scene: LyricsScene = serde_json::from_str(json).unwrap();
        let prepared = renderer.prepare_scene(scene).unwrap();
        let line = prepared.lines.first().unwrap();
        let PreparedLineKind::Karaoke {
            text, syllables, ..
        } = &line.kind
        else {
            panic!("oversized syllable should remain karaoke text");
        };

        assert_eq!(syllables.len(), 1, "source timing must remain one syllable");
        assert!(
            text.rows.len() > 1,
            "expected wrapped rows, got {:?}",
            text.rows
        );
        assert!((text.height - text.rows.len() as f32 * 28.0).abs() < 0.01);
        for row in &text.rows {
            assert!(row.width <= 170.5, "row overflowed: {row:?}");
            assert!(
                row.syllable_segments
                    .iter()
                    .all(|segment| segment.syllable_index == 0),
                "wrapped fragments must retain the source syllable timing"
            );
        }
        let segments = text
            .rows
            .iter()
            .flat_map(|row| row.syllable_segments.iter())
            .collect::<Vec<_>>();
        assert_eq!(segments.first().map(|segment| segment.progress_start), Some(0.0));
        assert_eq!(segments.last().map(|segment| segment.progress_end), Some(1.0));
        assert!(segments.windows(2).all(|pair| {
            (pair[0].progress_end - pair[1].progress_start).abs() < 0.001
        }));

        let PreparedLineKind::Karaoke { text: short, .. } = &prepared.lines[1].kind else {
            panic!("short syllable should remain karaoke text");
        };
        assert_eq!(short.rows.len(), 1, "Ohh should not be split unnecessarily");

        let PreparedLineKind::Karaoke {
            text: unspaced, ..
        } = &prepared.lines[2].kind
        else {
            panic!("unspaced syllable should remain karaoke text");
        };
        assert!(
            unspaced.rows.len() > 1,
            "oversized unspaced syllable should wrap at grapheme boundaries"
        );
        for row in &unspaced.rows {
            assert!(row.width <= 170.5, "unspaced row overflowed: {row:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn desktop_cjk_scene_uses_a_ui_sans_fallback() {
        let mut renderer = LyricsRenderer::new();
        renderer.load_system_fonts();
        let json = r#"{
            "width": 420,
            "height": 220,
            "style": {
                "blur": { "enabled": false },
                "showTranslation": false,
                "showPhonetic": false
            },
            "lines": [{
                "kind": "synced",
                "sourceIndex": 0,
                "clusterIndex": 0,
                "clusterRole": "standalone",
                "start": 0,
                "end": 4000,
                "content": "中文歌词测试",
                "translation": null
            }]
        }"#;
        let scene: LyricsScene = serde_json::from_str(json).unwrap();
        let prepared = renderer.prepare_scene(scene).unwrap();
        let PreparedLineKind::Synced { text } = &prepared.lines[0].kind else {
            panic!("expected synced line");
        };
        let font_ids = text
            .rows
            .iter()
            .flat_map(|row| row.glyphs.iter())
            .map(|glyph| glyph.physical.cache_key.font_id)
            .collect::<Vec<_>>();
        assert!(!font_ids.is_empty(), "CJK text produced no glyphs");
        for id in font_ids {
            let face = renderer.font_system.db().face(id).expect("font face");
            let family = face
                .families
                .first()
                .map(|(name, _)| name.as_str())
                .unwrap_or(face.post_script_name.as_str())
                .to_ascii_lowercase();
            assert!(
                !family.contains("serif") && !family.contains("song"),
                "desktop CJK unexpectedly resolved to {family}"
            );
        }
    }

    #[test]
    fn nested_accompaniment_collapses_outside_compose_visibility_window() {
        let scene = PreparedScene {
            config: test_config(),
            lines: vec![
                test_line(ClusterRole::Main, 1_000, 2_000, 50.0),
                test_line(ClusterRole::AfterAccompaniment, 2_100, 3_000, 30.0),
            ],
            content_height: 0.0,
            top_bar: None,
            player: None,
        };

        // Collapsed before it starts, and still collapsed at the instant it starts
        // (the bloom now grows *from* the start rather than pre-rolling before it).
        assert_eq!(scene.dynamic_line_layouts(1_050)[1].height, 0.0);
        assert_eq!(scene.dynamic_line_layouts(2_100)[1].height, 0.0);

        // Blooms open over the enter window (start .. start + 500).
        let entering = scene.dynamic_line_layouts(2_350)[1];
        assert!(entering.height > 0.0 && entering.height < 30.0);

        // Fully open once the enter window completes, through the line and linger.
        assert_eq!(scene.dynamic_line_layouts(2_700)[1].height, 30.0);

        // Exit window is end(3000) + linger(200) .. + fade(400) = 3200..3600.
        let exiting = scene.dynamic_line_layouts(3_400)[1];
        assert!(exiting.height > 0.0 && exiting.height < 30.0);

        assert_eq!(scene.dynamic_line_layouts(3_700)[1].height, 0.0);
    }

    // The accompaniment's appearance is anchored to its MAIN line's start (via
    // `entrance_start`), so it blooms in with the main — already open well before
    // its own singing begins — while still exiting on its own end.
    #[test]
    fn accompaniment_blooms_in_with_its_main_line() {
        let mut after = test_line(ClusterRole::AfterAccompaniment, 2_100, 3_000, 30.0);
        after.entrance_start = 1_000; // the main line's start
        let scene = PreparedScene {
            config: test_config(),
            lines: vec![test_line(ClusterRole::Main, 1_000, 2_000, 50.0), after],
            content_height: 0.0,
            top_bar: None,
            player: None,
        };

        // Collapsed before the main appears.
        assert_eq!(scene.dynamic_line_layouts(900)[1].height, 0.0);
        // Fully open by main.start + enter window — long before its own start 2100.
        assert_eq!(scene.dynamic_line_layouts(1_600)[1].height, 30.0);
    }

    // A nested accompaniment must NOT run its own scroll spring: it rides the same
    // scroll as the rest of its cluster, so a main line and its accompaniment move
    // as one rigid block through the cascade instead of shearing apart. Regression
    // guard for "伴唱行不应单独使用弹簧，应该和主歌词作为一个整体计算弹簧".
    #[test]
    fn nested_accompaniment_rides_its_main_line_spring() {
        let mut renderer = LyricsRenderer::new();
        let mut lines = vec![
            indexed_line(0, 0, 1_000, 100.0),
            indexed_line(1, 1_000, 2_000, 100.0),
            indexed_line(2, 2_000, 3_000, 100.0),
            indexed_line(3, 3_000, 4_000, 100.0),
            indexed_line(4, 3_000, 4_000, 100.0),
            indexed_line(5, 5_000, 6_000, 100.0),
        ];
        // Rows 3 (main) and 4 (accompaniment) share one scroll cluster.
        lines[3].cluster_role = ClusterRole::Main;
        lines[4].cluster_role = ClusterRole::AfterAccompaniment;
        lines[4].cluster_index = lines[3].cluster_index;
        renderer.scene = Some(PreparedScene {
            config: test_config(),
            lines,
            content_height: 0.0,
            top_bar: None,
            player: None,
        });

        let content: Vec<DynamicLineLayout> = (0..6)
            .map(|i| DynamicLineLayout {
                top: i as f32 * 100.0,
                height: 100.0,
                text_visibility: 1.0,
                interlude_visibility: 0.0,
            })
            .collect();
        let viewport = 600.0;
        let scroll_of =
            |renderer: &LyricsRenderer, i: usize| content[i].top - renderer.frame_layouts[i].top;

        // Settle at the top so the cluster (rows 3-4) sits below the focus.
        for _ in 0..200 {
            renderer.animate_frame_layout(1_000, &content, 0.0, viewport, 0);
        }

        // Advance the target so the rows below the focus cascade toward it. Across
        // the whole transient the two cluster rows must keep exactly one scroll,
        // while the cascade genuinely lags the other rows (else the test is moot).
        let mut saw_cascade_spread = false;
        for step in 0..40 {
            let t = 1_000 + step * 30;
            renderer.animate_frame_layout(t, &content, 300.0, viewport, 1);
            let main_scroll = scroll_of(&renderer, 3);
            let acc_scroll = scroll_of(&renderer, 4);
            assert!(
                (main_scroll - acc_scroll).abs() < LINE_LAYOUT_EPSILON,
                "accompaniment scroll {acc_scroll} must track its main line's {main_scroll}"
            );
            if (scroll_of(&renderer, 2) - main_scroll).abs() > 1.0 {
                saw_cascade_spread = true;
            }
        }
        assert!(
            saw_cascade_spread,
            "cascade should lag rows relative to each other, otherwise the test proves nothing"
        );
    }

    // The unsung ("unplayed") text alpha (= focus_alpha * the karaoke multiplier)
    // must stay pinned at the dimmed floor for a line's whole focus range: that
    // both unifies it with a dimmed already-sung line (also `dim_min`) AND means a
    // line never darkens as it becomes active — it just fills in. Regression guard
    // for "从未播放到正在播放会先变暗，然后笔刷才出现".
    #[test]
    fn unplayed_alpha_is_constant_so_activating_a_line_never_darkens_it() {
        let dim_min = 0.4;
        let inactive = 0.2;
        let mut focus_alpha = dim_min;
        while focus_alpha <= 1.0 + 1e-6 {
            let unplayed =
                focus_alpha * unified_inactive_karaoke_alpha(focus_alpha, dim_min, inactive);
            assert!(
                (unplayed - dim_min).abs() < 1e-4,
                "focus_alpha {focus_alpha}: unplayed {unplayed} should stay at {dim_min}"
            );
            focus_alpha += 0.05;
        }
        // The active line still has karaoke contrast: sung (1.0) brighter than unsung.
        let active_unsung = 1.0 * unified_inactive_karaoke_alpha(1.0, dim_min, inactive);
        assert!(
            active_unsung < 1.0 && (active_unsung - dim_min).abs() < 1e-6,
            "active unsung {active_unsung}"
        );
    }

    #[test]
    fn mesh_reactivity_is_bounded_and_never_changes_phase_with_negative_amp() {
        let quiet = mesh_reactive_parameters(0.0, true);
        let loud = mesh_reactive_parameters(1.0, true);
        assert_eq!(mesh_reactive_parameters(0.5, false), (0.0, 0.2));
        assert!(quiet.0 >= 0.03 && quiet.0 <= 0.07);
        assert!(loud.0 >= 0.03 && loud.0 <= 0.07);
        assert!(loud.0 < quiet.0);
        assert!(quiet.1 >= 0.2 && loud.1 <= 0.5);
        assert!(loud.1 > quiet.1);
    }

    #[test]
    fn mesh_loudness_filter_is_frame_rate_independent() {
        let at_60_hz = (0..60).fold(0.0, |value, _| {
            smooth_mesh_loudness(value, 1.0, 1.0 / 60.0)
        });
        let at_30_hz = (0..30).fold(0.0, |value, _| {
            smooth_mesh_loudness(value, 1.0, 1.0 / 30.0)
        });
        assert!((at_60_hz - at_30_hz).abs() < 1.0e-5);

        let released = smooth_mesh_loudness(at_60_hz, 0.0, 0.1);
        let attacked = smooth_mesh_loudness(0.0, at_60_hz, 0.1);
        assert!(at_60_hz - released < attacked);
    }

    #[test]
    fn easing_functions_match_compose_control_points() {
        assert!((syllable_lift_easing(0.0) - 0.0).abs() < 0.0001);
        assert!(syllable_lift_easing(0.5) > 0.0);
        assert!(syllable_lift_easing(0.5) < 0.5);
        assert!((syllable_lift_easing(1.0) - 1.0).abs() < 0.0001);

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
        assert!(mid_effect.offset_y > 0.0);
        assert!(mid_effect.offset_y < start_effect.offset_y);
        assert!(end_effect.offset_y.abs() < 0.0001);
    }

    #[test]
    fn rust_page_selection_is_authoritative_after_first_player_scene() {
        let mut renderer = LyricsRenderer::new();
        assert!(!renderer.player_screen_initialized);
        assert_eq!(renderer.player_screen, PlayerScreenInput::Artwork);
        renderer.player_screen_initialized = true;
        renderer.select_player_screen(PlayerScreenInput::Lyrics);
        assert_eq!(renderer.player_screen, PlayerScreenInput::Lyrics);
        // A subsequent host wire still advertising artwork must not win once
        // ownership is established (mirrors set_scene_json retention logic).
        let wire_screen = PlayerScreenInput::Artwork;
        let retained = if renderer.player_screen_initialized {
            renderer.player_screen
        } else {
            wire_screen
        };
        assert_eq!(retained, PlayerScreenInput::Lyrics);
    }

    #[test]
    fn lyrics_and_queue_mode_buttons_toggle_back_to_artwork() {
        let mut renderer = LyricsRenderer::new();
        renderer.player_screen_initialized = true;
        renderer.player_screen = PlayerScreenInput::Artwork;

        // Open lyrics, then toggle closed.
        renderer.select_player_screen(PlayerScreenInput::Lyrics);
        assert_eq!(renderer.player_screen, PlayerScreenInput::Lyrics);
        let close_lyrics = if renderer.player_screen == PlayerScreenInput::Lyrics {
            PlayerScreenInput::Artwork
        } else {
            PlayerScreenInput::Lyrics
        };
        renderer.select_player_screen(close_lyrics);
        assert_eq!(renderer.player_screen, PlayerScreenInput::Artwork);

        // Open queue, then toggle closed.
        renderer.select_player_screen(PlayerScreenInput::Queue);
        assert_eq!(renderer.player_screen, PlayerScreenInput::Queue);
        let close_queue = if renderer.player_screen == PlayerScreenInput::Queue {
            PlayerScreenInput::Artwork
        } else {
            PlayerScreenInput::Queue
        };
        renderer.select_player_screen(close_queue);
        assert_eq!(renderer.player_screen, PlayerScreenInput::Artwork);
    }

    #[test]
    fn live_playback_overrides_are_stored_for_frame_apply() {
        let mut renderer = LyricsRenderer::new();
        renderer.set_player_live_playback(true, 243_000);
        assert_eq!(renderer.player_live_playing, Some(true));
        assert_eq!(renderer.player_live_duration_ms, Some(243_000));
        assert!(renderer.playback_active);
        renderer.clear_player_live_playback();
        assert_eq!(renderer.player_live_playing, None);
        assert_eq!(renderer.player_live_duration_ms, None);
    }
}
