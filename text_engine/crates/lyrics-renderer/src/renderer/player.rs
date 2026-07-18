//! Native portrait player chrome.
//!
//! Geometry is resolved from the 393×852 Penpot reference as a vertical flex
//! stack. Fixed rows scale with the surface width; the lyrics/artwork/queue body
//! receives the remaining height. All interaction and press feedback lives in
//! Rust so a host only forwards pointer events and consumes action codes.

use super::scroll::{advance_fling, spring_step};
use super::*;
use skia_safe::{
    canvas::SaveLayerRec, BlendMode, ClipOp, Color4f, Contains, Image, Paint, Path, PathBuilder,
    Point, Rect, SamplingOptions,
};

const DESIGN_WIDTH: f32 = 393.0;
const TOP_INSET: f32 = 44.0;
const HANDLE_ROW: f32 = 36.0;
const COMPACT_HEADER: f32 = 92.0;
const PROGRESS_ROW: f32 = 60.0;
const TRANSPORT_ROW: f32 = 142.0;
const MODE_NAV_ROW: f32 = 90.0;
const ARTWORK_METADATA_ROW: f32 = 96.0;
const QUEUE_FILTER_ROW: f32 = 54.0;
const QUEUE_METADATA_ROW: f32 = 56.0;
const BOTTOM_CHROME_IDLE_SECONDS: f32 = 3.0;
const BOTTOM_CHROME_EXIT_SECONDS: f32 = 0.36;
const RUNTIME_LABEL_GLYPHS: &str = "0123456789:−";

// Player chrome is pure white + Plus over the mesh (not the mock's pink fills).
// Alphas sampled from design exports (status bar ignored — system-drawn):
//   - title / progress played: solid #FFFFFF → 1.0
//   - secondary text (artist, time labels): ~0.60 effective opacity
//   - mode/filter chips: unselected ~0.40, selected ~0.60
//   - progress track: ~0.50
//   - drag handle: ~0.40
const WHITE: Color4f = Color4f::new(1.0, 1.0, 1.0, 1.0);
const WHITE_HANDLE: Color4f = Color4f::new(1.0, 1.0, 1.0, 0.40);
const WHITE_BTN: Color4f = Color4f::new(1.0, 1.0, 1.0, 0.10);
const WHITE_BTN_ACTIVE: Color4f = Color4f::new(1.0, 1.0, 1.0, 0.60);
const WHITE_SECONDARY: Color4f = Color4f::new(1.0, 1.0, 1.0, 0.60);
const WHITE_TRACK: Color4f = Color4f::new(1.0, 1.0, 1.0, 0.50);
const WHITE_FILL: Color4f = Color4f::new(1.0, 1.0, 1.0, 1.0);
const ARTWORK_PLACEHOLDER: Color4f = Color4f::new(1.0, 1.0, 1.0, 0.12);
const TEXT_PRIMARY_ALPHA: f32 = 1.0;
const TEXT_SECONDARY_ALPHA: f32 = 0.60;

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PlayerScreenInput {
    Lyrics,
    /// Resting full-artwork page. Mode-nav chips are unselected here.
    #[default]
    Artwork,
    Queue,
}

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PlayerPresentationInput {
    Mini,
    #[default]
    Full,
}

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum QueueFilterInput {
    #[default]
    UpNext,
    Shuffle,
    RepeatOne,
    Album,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct PlayerQueueItemInput {
    pub title: String,
    pub artist: String,
    pub artwork_key: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct PlayerInput {
    pub presentation: PlayerPresentationInput,
    pub viewport_width: Option<f32>,
    pub viewport_height: Option<f32>,
    pub screen: PlayerScreenInput,
    pub title: String,
    pub artist: String,
    pub duration_ms: i32,
    pub is_playing: bool,
    pub liked: bool,
    pub queue_title: String,
    pub queue_source: String,
    pub queue_filter: QueueFilterInput,
    pub queue_items: Vec<PlayerQueueItemInput>,
}

#[derive(Debug)]
struct PreparedQueueItem {
    title: PreparedText,
    artist: PreparedText,
    artwork_key: String,
}

/// Small, fixed glyph repertoire used by the live progress labels.
///
/// Preparing these glyphs through cosmic-text keeps the whole player on the
/// supplied primary font (SF Pro in Clef) and preserves the normal per-cluster
/// fallback path if a future primary font is missing the Unicode minus sign.
/// It also avoids reshaping the two labels on every rendered frame.
#[derive(Debug)]
struct PreparedRuntimeLabelFont {
    glyphs: Vec<(char, PreparedText)>,
}

impl PreparedRuntimeLabelFont {
    fn glyph(&self, ch: char) -> Option<&PreparedText> {
        self.glyphs
            .iter()
            .find_map(|(candidate, text)| (*candidate == ch).then_some(text))
    }

    fn text_width(&self, text: &str) -> f32 {
        text.chars()
            .filter_map(|ch| self.glyph(ch))
            .map(prepared_text_width)
            .sum()
    }
}

#[derive(Debug)]
pub(super) struct PreparedPlayer {
    pub presentation: PlayerPresentationInput,
    pub screen: PlayerScreenInput,
    pub duration_ms: i32,
    pub is_playing: bool,
    pub liked: bool,
    pub title: PreparedText,
    pub artist: PreparedText,
    pub artwork_title: PreparedText,
    pub artwork_artist: PreparedText,
    queue_title: PreparedText,
    queue_source: PreparedText,
    queue_filter: QueueFilterInput,
    queue_items: Vec<PreparedQueueItem>,
    runtime_label_font: PreparedRuntimeLabelFont,
    pub layout: PlayerLayout,
    mini_layout: PlayerLayout,
    icons: PlayerIcons,
}

/// A small CSS-like flex axis used by the native player scene.
///
/// Call sites only declare row/column intent. The shared resolver owns free-space
/// distribution and constrained shrinking, so responsive geometry is not rebuilt
/// from unrelated `height - row - row` expressions in every drawing path.
#[derive(Debug, Clone, Copy)]
struct FlexItem {
    basis: f32,
    min: f32,
    grow: f32,
    shrink: f32,
}

impl FlexItem {
    fn fixed(size: f32) -> Self {
        let size = size.max(0.0);
        Self {
            basis: size,
            min: size * 0.7,
            grow: 0.0,
            shrink: 1.0,
        }
    }

    fn grow(weight: f32) -> Self {
        Self {
            basis: 0.0,
            min: 0.0,
            grow: weight.max(0.0),
            shrink: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct FlexSpan {
    start: f32,
    end: f32,
}

impl FlexSpan {
    fn size(self) -> f32 {
        self.end - self.start
    }

    fn center(self) -> f32 {
        (self.start + self.end) * 0.5
    }
}

fn resolve_flex_axis<const N: usize>(extent: f32, items: [FlexItem; N]) -> [FlexSpan; N] {
    let extent = extent.max(0.0);
    let mut sizes: [f32; N] = std::array::from_fn(|index| items[index].basis);
    let basis_sum = sizes.iter().sum::<f32>();
    let free_space = extent - basis_sum;

    if free_space >= 0.0 {
        let total_grow = items.iter().map(|item| item.grow).sum::<f32>();
        if total_grow > 0.0 {
            for (size, item) in sizes.iter_mut().zip(items) {
                *size += free_space * item.grow / total_grow;
            }
        }
    } else {
        let mut deficit = -free_space;
        for _ in 0..N {
            let total_weight = items
                .iter()
                .enumerate()
                .filter(|(index, item)| sizes[*index] > item.min && item.shrink > 0.0)
                .map(|(index, item)| item.shrink * sizes[index])
                .sum::<f32>();
            if total_weight <= f32::EPSILON || deficit <= f32::EPSILON {
                break;
            }
            let mut consumed = 0.0;
            for (index, item) in items.iter().enumerate() {
                if sizes[index] <= item.min || item.shrink <= 0.0 {
                    continue;
                }
                let share = deficit * item.shrink * sizes[index] / total_weight;
                let reduction = share.min(sizes[index] - item.min);
                sizes[index] -= reduction;
                consumed += reduction;
            }
            if consumed <= f32::EPSILON {
                break;
            }
            deficit -= consumed;
        }
    }

    let mut cursor = 0.0;
    std::array::from_fn(|index| {
        let start = cursor;
        cursor += sizes[index];
        FlexSpan { start, end: cursor }
    })
}

macro_rules! flex_axis {
    ($extent:expr; $( $name:ident => $item:expr ),+ $(,)?) => {
        let [$($name),+] = resolve_flex_axis($extent, [$($item),+]);
    };
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PlayerLayout {
    pub scale: f32,
    pub width: f32,
    pub height: f32,
    pub header_top: f32,
    pub body_top: f32,
    pub progress_top: f32,
    pub artwork_metadata_top: f32,
    pub queue_metadata_top: f32,
    pub queue_content_top: f32,
    action_center_y: f32,
    artwork_action_center_y: f32,
    queue_filter_center_y: f32,
    transport_center_y: f32,
    nav_center_y: f32,
}

impl PlayerLayout {
    pub(super) fn resolve(width: f32, height: f32) -> Self {
        let scale = (width / DESIGN_WIDTH).max(0.25);
        Self::resolve_with_scale(width, height, scale)
    }

    fn resolve_with_scale(width: f32, height: f32, scale: f32) -> Self {
        let scale = scale.max(0.25);
        flex_axis!(height;
            top => FlexItem::fixed((TOP_INSET + HANDLE_ROW) * scale),
            header => FlexItem::fixed(COMPACT_HEADER * scale),
            body => FlexItem::grow(1.0),
            _progress => FlexItem::fixed(PROGRESS_ROW * scale),
            transport => FlexItem::fixed(TRANSPORT_ROW * scale),
            nav => FlexItem::fixed(MODE_NAV_ROW * scale),
        );
        flex_axis!(body.end - top.end;
            artwork => FlexItem::grow(1.0),
            artwork_metadata => FlexItem::fixed(ARTWORK_METADATA_ROW * scale),
        );
        flex_axis!(body.size();
            queue_filter => FlexItem::fixed(QUEUE_FILTER_ROW * scale),
            queue_metadata => FlexItem::fixed(QUEUE_METADATA_ROW * scale),
            _queue_list => FlexItem::grow(1.0),
        );
        let header_top = top.end;
        let body_top = header.end;
        let progress_top = body.end;
        Self {
            scale,
            width,
            height,
            header_top,
            body_top,
            progress_top,
            artwork_metadata_top: header_top + artwork.end,
            queue_metadata_top: body_top + queue_metadata.start,
            queue_content_top: body_top + queue_metadata.end,
            action_center_y: header.center(),
            artwork_action_center_y: header_top + artwork_metadata.center(),
            queue_filter_center_y: body_top + queue_filter.center(),
            transport_center_y: transport.center(),
            nav_center_y: nav.center(),
        }
    }

    pub(super) fn lyrics_content_top(self) -> f32 {
        self.body_top
    }

    pub(super) fn lyrics_content_bottom(self) -> f32 {
        self.height - self.progress_top
    }

    fn rect(self, x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::from_xywh(
            x * self.scale,
            y * self.scale,
            width * self.scale,
            height * self.scale,
        )
    }

    fn compact_artwork_rect(self) -> Rect {
        self.rect(32.0, 90.0, 72.0, 72.0)
    }

    fn full_artwork_rect(self) -> Rect {
        let region_top = self.header_top;
        let region_height = (self.artwork_metadata_top - region_top).max(1.0);
        let size = (self.width - 48.0 * self.scale)
            .min(region_height - 24.0 * self.scale)
            .max(1.0);
        Rect::from_xywh(
            (self.width - size) * 0.5,
            region_top + (region_height - size) * 0.5,
            size,
            size,
        )
    }

    fn collapsed_artwork_rect(self) -> Rect {
        MiniPlayerLayout::resolve(self).artwork
    }
}

#[derive(Debug, Clone, Copy)]
struct MiniPlayerLayout {
    artwork: Rect,
    text: Rect,
    play_center: Point,
    next_center: Point,
}

impl MiniPlayerLayout {
    fn resolve(layout: PlayerLayout) -> Self {
        let s = layout.scale;
        flex_axis!(layout.width;
            _leading => FlexItem::fixed(8.0 * s),
            artwork => FlexItem::fixed(44.0 * s),
            artwork_gap => FlexItem::fixed(8.0 * s),
            _metadata => FlexItem::grow(1.0),
            control_gap => FlexItem::fixed(4.0 * s),
            play => FlexItem::fixed(44.0 * s),
            _play_gap => FlexItem::fixed(4.0 * s),
            next => FlexItem::fixed(44.0 * s),
            _trailing => FlexItem::fixed(14.0 * s),
        );
        let center_y = layout.height * 0.5;
        let artwork_size = artwork.size().min((layout.height - 16.0 * s).max(1.0));
        let artwork_left = artwork.start + (artwork.size() - artwork_size) * 0.5;
        Self {
            artwork: Rect::from_xywh(
                artwork_left,
                center_y - artwork_size * 0.5,
                artwork_size,
                artwork_size,
            ),
            text: Rect::new(artwork_gap.end, 0.0, control_gap.start, layout.height),
            play_center: Point::new(play.center(), center_y),
            next_center: Point::new(next.center(), center_y),
        }
    }
}

const SCREEN_TRANSITION_SECONDS: f32 = 0.42;

#[derive(Debug, Clone, Copy)]
pub(super) struct PlayerTransitionSample {
    pub from: PlayerScreenInput,
    pub to: PlayerScreenInput,
    pub progress: f32,
    pub active: bool,
}

impl PlayerTransitionSample {
    pub(super) fn settled(screen: PlayerScreenInput) -> Self {
        Self {
            from: screen,
            to: screen,
            progress: 1.0,
            active: false,
        }
    }

    pub(super) fn content_transform(self, screen: PlayerScreenInput) -> (f32, f32) {
        if !self.active {
            return if screen == self.to {
                (1.0, 1.0)
            } else {
                (0.0, 0.94)
            };
        }
        if screen == self.from {
            let progress = (self.progress / 0.58).clamp(0.0, 1.0);
            let eased = ease_out_cubic(progress);
            return (1.0 - eased, 1.0 - 0.06 * eased);
        }
        if screen == self.to {
            let progress = ((self.progress - 0.30) / 0.70).clamp(0.0, 1.0);
            let eased = smooth_step(progress);
            return (eased, 0.94 + 0.06 * eased);
        }
        (0.0, 0.94)
    }

    pub(super) fn artwork_progress(self) -> f32 {
        let progress = smooth_step(self.progress);
        match (self.from, self.to) {
            (PlayerScreenInput::Artwork, PlayerScreenInput::Artwork) => 1.0,
            (PlayerScreenInput::Artwork, _) => 1.0 - progress,
            (_, PlayerScreenInput::Artwork) => progress,
            _ => 0.0,
        }
    }

    fn shares_compact_header(self) -> bool {
        self.from != PlayerScreenInput::Artwork && self.to != PlayerScreenInput::Artwork
    }
}

#[derive(Debug)]
pub(super) struct PlayerTransitionState {
    current: PlayerScreenInput,
    from: PlayerScreenInput,
    to: PlayerScreenInput,
    started_at: Option<Instant>,
}

impl Default for PlayerTransitionState {
    fn default() -> Self {
        Self {
            current: PlayerScreenInput::Artwork,
            from: PlayerScreenInput::Artwork,
            to: PlayerScreenInput::Artwork,
            started_at: None,
        }
    }
}

impl PlayerTransitionState {
    pub(super) fn set_target(
        &mut self,
        previous: Option<PlayerScreenInput>,
        target: Option<PlayerScreenInput>,
    ) {
        let Some(target) = target else {
            self.started_at = None;
            return;
        };
        if self.started_at.is_some() && target == self.to {
            return;
        }
        let from = previous.unwrap_or(target);
        if from == target {
            self.current = target;
            self.from = target;
            self.to = target;
            self.started_at = None;
            return;
        }
        self.current = from;
        self.from = from;
        self.to = target;
        self.started_at = Some(Instant::now());
    }

    pub(super) fn sample(&mut self, now: Instant) -> PlayerTransitionSample {
        let Some(started_at) = self.started_at else {
            return PlayerTransitionSample::settled(self.current);
        };
        let progress = (now.saturating_duration_since(started_at).as_secs_f32()
            / SCREEN_TRANSITION_SECONDS)
            .clamp(0.0, 1.0);
        if progress >= 1.0 {
            self.current = self.to;
            self.started_at = None;
            return PlayerTransitionSample::settled(self.current);
        }
        PlayerTransitionSample {
            from: self.from,
            to: self.to,
            progress,
            active: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub(super) enum PlayerButton {
    Favorite = 1,
    More = 2,
    Previous = 3,
    PlayPause = 4,
    Next = 5,
    Lyrics = 6,
    Output = 7,
    Queue = 8,
    QueueUpNext = 9,
    QueueShuffle = 10,
    QueueRepeatOne = 11,
    QueueAlbum = 12,
    Open = 13,
}

impl PlayerButton {
    fn is_bottom_chrome(self) -> bool {
        matches!(
            self,
            Self::Previous
                | Self::PlayPause
                | Self::Next
                | Self::Lyrics
                | Self::Output
                | Self::Queue
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum PlayerControlVisual {
    Open,
    Action {
        diameter: f32,
        icon_size: f32,
    },
    Transport {
        width: f32,
        height: f32,
    },
    Mode {
        diameter: f32,
        width: f32,
        height: f32,
    },
    Filter {
        pill: Rect,
        width: f32,
        height: f32,
    },
}

#[derive(Debug, Clone, Copy)]
struct PlayerControl {
    button: PlayerButton,
    hit_rect: Rect,
    center: Point,
    visual: PlayerControlVisual,
}

impl PlayerControl {
    fn open(hit_rect: Rect) -> Self {
        Self {
            button: PlayerButton::Open,
            center: Point::new(hit_rect.center_x(), hit_rect.center_y()),
            hit_rect,
            visual: PlayerControlVisual::Open,
        }
    }

    fn action_geometry(self) -> (f32, f32) {
        match self.visual {
            PlayerControlVisual::Action {
                diameter,
                icon_size,
            } => (diameter, icon_size),
            _ => unreachable!("player control visual is not an action"),
        }
    }

    fn transport_size(self) -> (f32, f32) {
        match self.visual {
            PlayerControlVisual::Transport { width, height } => (width, height),
            _ => unreachable!("player control visual is not transport"),
        }
    }

    fn mode_geometry(self) -> (f32, f32, f32) {
        match self.visual {
            PlayerControlVisual::Mode {
                diameter,
                width,
                height,
            } => (diameter, width, height),
            _ => unreachable!("player control visual is not mode navigation"),
        }
    }

    fn filter_geometry(self) -> (Rect, f32, f32) {
        match self.visual {
            PlayerControlVisual::Filter {
                pill,
                width,
                height,
            } => (pill, width, height),
            _ => unreachable!("player control visual is not a queue filter"),
        }
    }
}

#[derive(Debug)]
pub(super) struct PlayerUiLayout {
    layout: PlayerLayout,
    screen: PlayerScreenInput,
    controls: Vec<PlayerControl>,
    mini: Option<MiniPlayerLayout>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlayerHit {
    Control(PlayerButton),
    Scroll,
}

macro_rules! push_player_controls {
    ($controls:ident, $s:expr, $y:expr, action; $( $button:ident, $x:expr, $icon:expr );+ $(;)?) => {
        $(
            $controls.push(PlayerControl {
                button: PlayerButton::$button,
                hit_rect: Rect::from_xywh(($x - 24.0) * $s, $y - 24.0 * $s, 48.0 * $s, 48.0 * $s),
                center: Point::new($x * $s, $y),
                visual: PlayerControlVisual::Action {
                    diameter: 32.0 * $s,
                    icon_size: $icon * $s,
                },
            });
        )+
    };
    ($controls:ident, $s:expr, $y:expr, transport; $( $button:ident, $x:expr, $width:expr, $height:expr );+ $(;)?) => {
        $(
            $controls.push(PlayerControl {
                button: PlayerButton::$button,
                hit_rect: Rect::from_xywh(($x - 32.0) * $s, $y - 36.0 * $s, 64.0 * $s, 72.0 * $s),
                center: Point::new($x * $s, $y),
                visual: PlayerControlVisual::Transport {
                    width: $width * $s,
                    height: $height * $s,
                },
            });
        )+
    };
    ($controls:ident, $s:expr, $y:expr, mode; $( $button:ident, $x:expr, $width:expr, $height:expr );+ $(;)?) => {
        $(
            $controls.push(PlayerControl {
                button: PlayerButton::$button,
                hit_rect: Rect::from_xywh(($x - 32.0) * $s, $y - 32.0 * $s, 64.0 * $s, 56.0 * $s),
                center: Point::new($x * $s, $y),
                visual: PlayerControlVisual::Mode {
                    diameter: 32.0 * $s,
                    width: $width * $s,
                    height: $height * $s,
                },
            });
        )+
    };
}

impl PlayerUiLayout {
    pub(super) fn resolve(
        layout: PlayerLayout,
        screen: PlayerScreenInput,
        presentation: PlayerPresentationInput,
    ) -> Self {
        let s = layout.scale;
        if presentation == PlayerPresentationInput::Mini {
            let mini = MiniPlayerLayout::resolve(layout);
            let mut controls = vec![PlayerControl::open(Rect::from_xywh(
                0.0,
                0.0,
                layout.width,
                layout.height,
            ))];
            for (button, center) in [
                (PlayerButton::PlayPause, mini.play_center),
                (PlayerButton::Next, mini.next_center),
            ] {
                controls.push(PlayerControl {
                    button,
                    hit_rect: Rect::from_xywh(center.x - 22.0 * s, 0.0, 44.0 * s, layout.height),
                    center,
                    visual: PlayerControlVisual::Transport {
                        width: 32.0 * s,
                        height: 32.0 * s,
                    },
                });
            }
            return Self {
                layout,
                screen,
                controls,
                mini: Some(mini),
            };
        }
        let action_y = if screen == PlayerScreenInput::Artwork {
            layout.artwork_action_center_y
        } else {
            layout.action_center_y
        };
        let transport_y = layout.transport_center_y;
        let nav_y = layout.nav_center_y;
        let mut controls = Vec::with_capacity(12);
        push_player_controls!(controls, s, action_y, action;
            Favorite, 301.0, 20.0;
            More, 345.0, 20.0;
        );
        push_player_controls!(controls, s, transport_y, transport;
            Previous, 101.5, 42.0, 35.0;
            PlayPause, 196.5, 42.0, 42.0;
            Next, 291.5, 42.0, 35.0;
        );
        push_player_controls!(controls, s, nav_y, mode;
            Lyrics, 80.0, 18.0, 18.0;
            Output, 196.5, 18.0, 18.0;
            Queue, 313.0, 18.0, 15.0;
        );
        if screen == PlayerScreenInput::Queue {
            for (button, x, width, height) in [
                (PlayerButton::QueueUpNext, 68.0, 18.0, 14.0),
                (PlayerButton::QueueShuffle, 153.0, 18.0, 14.0),
                (PlayerButton::QueueRepeatOne, 238.0, 18.0, 15.0),
                (PlayerButton::QueueAlbum, 323.0, 18.0, 15.0),
            ] {
                let pill = Rect::from_xywh(
                    (x - 36.0) * s,
                    layout.queue_filter_center_y - 19.0 * s,
                    72.0 * s,
                    38.0 * s,
                );
                controls.push(PlayerControl {
                    button,
                    hit_rect: pill,
                    center: Point::new(x * s, layout.queue_filter_center_y),
                    visual: PlayerControlVisual::Filter {
                        pill,
                        width: width * s,
                        height: height * s,
                    },
                });
            }
        }
        Self {
            layout,
            screen,
            controls,
            mini: None,
        }
    }

    fn control(&self, button: PlayerButton) -> PlayerControl {
        self.controls
            .iter()
            .find(|control| control.button == button)
            .copied()
            .expect("visible player control must exist in resolved UI")
    }

    fn hit_test(&self, point: Point, bottom_chrome: BottomChromeSample) -> Option<PlayerHit> {
        if let Some(button) = self
            .controls
            .iter()
            .rev()
            .find(|control| {
                let mut rect = control.hit_rect;
                if control.button.is_bottom_chrome() {
                    if bottom_chrome.visibility <= 0.001 {
                        return false;
                    }
                    let offset = bottom_chrome.slide_offset(self.layout);
                    rect = Rect::new(
                        rect.left,
                        rect.top + offset,
                        rect.right,
                        rect.bottom + offset,
                    );
                }
                rect.contains(point)
            })
            .map(|control| control.button)
        {
            return Some(PlayerHit::Control(button));
        }

        let scroll_top = match self.screen {
            PlayerScreenInput::Lyrics => self.layout.body_top,
            PlayerScreenInput::Queue => self.layout.queue_content_top,
            PlayerScreenInput::Artwork => return None,
        };
        Rect::new(
            0.0,
            scroll_top,
            self.layout.width,
            bottom_chrome.content_end(self.layout),
        )
        .contains(point)
        .then_some(PlayerHit::Scroll)
    }
}

#[derive(Debug, Clone, Copy)]
enum DrawBlend {
    Plus,
    SourceOver,
    DestinationOut,
}

enum DrawCommand<'a> {
    RoundRect {
        rect: Rect,
        radius: f32,
        color: Color4f,
        blend: DrawBlend,
    },
    Circle {
        center: Point,
        radius: f32,
        color: Color4f,
        blend: DrawBlend,
    },
    Icon {
        icon: &'a SvgIcon,
        center: Point,
        width: f32,
        height: f32,
        color: Color4f,
        alpha: f32,
        blend: DrawBlend,
    },
    Airplay {
        center: Point,
        size: f32,
        color: Color4f,
        blend: DrawBlend,
    },
}

impl DrawCommand<'_> {
    fn draw(self, canvas: &skia_safe::Canvas) {
        match self {
            Self::RoundRect {
                rect,
                radius,
                color,
                blend,
            } => {
                let mut paint = command_paint(color, blend);
                paint.set_anti_alias(true);
                let path = crate::capsule::continuous_rounded_rect(rect, radius);
                canvas.draw_path(&path, &paint);
            }
            Self::Circle {
                center,
                radius,
                color,
                blend,
            } => {
                let mut paint = command_paint(color, blend);
                paint.set_anti_alias(true);
                canvas.draw_circle(center, radius, &paint);
            }
            Self::Icon {
                icon,
                center,
                width,
                height,
                color,
                alpha,
                blend,
            } => {
                let scale = (width / icon.view_width).min(height / icon.view_height);
                let draw_w = icon.view_width * scale;
                let draw_h = icon.view_height * scale;
                let paint = command_paint(
                    Color4f::new(color.r, color.g, color.b, color.a * alpha),
                    blend,
                );
                canvas.save();
                let origin_x = if icon.mirror_x {
                    center.x + draw_w * 0.5
                } else {
                    center.x - draw_w * 0.5
                };
                canvas.translate((origin_x, center.y - draw_h * 0.5));
                canvas.scale((if icon.mirror_x { -scale } else { scale }, scale));
                canvas.draw_path(&icon.path, &paint);
                canvas.restore();
            }
            Self::Airplay {
                center,
                size,
                color,
                blend,
            } => {
                draw_airplay_command(canvas, center, size, color, blend);
            }
        }
    }
}

fn command_paint(color: Color4f, blend: DrawBlend) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(color, None);
    paint.set_blend_mode(match blend {
        DrawBlend::Plus => BlendMode::Plus,
        DrawBlend::SourceOver => BlendMode::SrcOver,
        DrawBlend::DestinationOut => BlendMode::DstOut,
    });
    paint
}

#[derive(Debug)]
pub(super) struct PlayerInteractionState {
    pointer_hit: Option<PlayerHit>,
    pressed: Option<PlayerButton>,
    pressed_at: Option<Instant>,
    released: Option<(PlayerButton, Instant, f32)>,
    last_activity_at: Instant,
}

impl Default for PlayerInteractionState {
    fn default() -> Self {
        Self {
            pointer_hit: None,
            pressed: None,
            pressed_at: None,
            released: None,
            last_activity_at: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BottomChromeSample {
    pub visibility: f32,
    pub active: bool,
}

#[derive(Debug, Default)]
pub(super) struct QueueScrollState {
    offset: f32,
    velocity: f32,
    pub(super) dragging: bool,
    last_frame_at: Option<Instant>,
}

#[derive(Debug, Default)]
pub(super) struct QueueReorderState {
    active: bool,
    from: usize,
    to: usize,
    pointer_y: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct QueueReorderSample {
    pub active: bool,
    pub from: usize,
    pub to: usize,
    pub pointer_y: f32,
}

impl QueueReorderState {
    pub(super) fn sample(&self) -> QueueReorderSample {
        QueueReorderSample {
            active: self.active,
            from: self.from,
            to: self.to,
            pointer_y: self.pointer_y,
        }
    }
}

impl BottomChromeSample {
    pub(super) fn content_bottom(self, layout: PlayerLayout) -> f32 {
        (layout.height - layout.progress_top).max(0.0) * self.visibility
    }

    fn content_end(self, layout: PlayerLayout) -> f32 {
        layout.height - self.content_bottom(layout)
    }

    fn slide_offset(self, layout: PlayerLayout) -> f32 {
        64.0 * layout.scale * (1.0 - self.visibility)
    }
}

#[derive(Debug, Clone, Copy)]
struct ControlAnimation {
    scale: f32,
    press: f32,
    active: bool,
}

impl PlayerInteractionState {
    pub(super) fn reveal(&mut self) {
        self.last_activity_at = Instant::now();
    }

    pub(super) fn bottom_chrome_sample(
        &self,
        screen: PlayerScreenInput,
        now: Instant,
    ) -> BottomChromeSample {
        if screen == PlayerScreenInput::Artwork {
            return BottomChromeSample {
                visibility: 1.0,
                active: false,
            };
        }
        let elapsed = now
            .saturating_duration_since(self.last_activity_at)
            .as_secs_f32();
        if elapsed < BOTTOM_CHROME_IDLE_SECONDS {
            return BottomChromeSample {
                visibility: 1.0,
                active: true,
            };
        }
        let progress =
            ((elapsed - BOTTOM_CHROME_IDLE_SECONDS) / BOTTOM_CHROME_EXIT_SECONDS).clamp(0.0, 1.0);
        BottomChromeSample {
            visibility: 1.0 - smooth_step(progress),
            active: progress < 1.0,
        }
    }

    pub(super) fn press(&mut self, ui: &PlayerUiLayout, x: f32, y: f32) -> i32 {
        let now = Instant::now();
        let bottom_chrome = self.bottom_chrome_sample(ui.screen, now);
        self.last_activity_at = now;
        let hit = ui.hit_test(Point::new(x, y), bottom_chrome);
        self.pointer_hit = hit;
        self.pressed = match hit {
            Some(PlayerHit::Control(button)) => Some(button),
            _ => None,
        };
        self.pressed_at = self.pressed.map(|_| now);
        self.released = None;
        self.pressed.map_or(0, |button| button as i32)
    }

    pub(super) fn release(&mut self, ui: &PlayerUiLayout, x: f32, y: f32) -> i32 {
        let Some(button) = self.pressed else {
            self.pointer_hit = None;
            return 0;
        };
        let bottom_chrome = self.bottom_chrome_sample(ui.screen, Instant::now());
        let accepted =
            ui.hit_test(Point::new(x, y), bottom_chrome) == Some(PlayerHit::Control(button));
        let scale = self.animation_for(button, Instant::now()).scale;
        self.pressed = None;
        self.pointer_hit = None;
        self.pressed_at = None;
        self.released = Some((button, Instant::now(), scale));
        if accepted {
            button as i32
        } else {
            0
        }
    }

    pub(super) fn cancel(&mut self) {
        if let Some(button) = self.pressed {
            let scale = self.animation_for(button, Instant::now()).scale;
            self.pressed = None;
            self.released = Some((button, Instant::now(), scale));
        }
        self.pointer_hit = None;
        self.pressed_at = None;
    }

    pub(super) fn begin_scroll(&mut self) -> bool {
        let accepted = self.pointer_hit == Some(PlayerHit::Scroll);
        if accepted {
            self.pressed = None;
            self.pressed_at = None;
            self.released = None;
            self.pointer_hit = None;
        } else {
            self.cancel();
        }
        accepted
    }

    fn animation_for(&mut self, button: PlayerButton, now: Instant) -> ControlAnimation {
        if self.pressed == Some(button) {
            let elapsed = self
                .pressed_at
                .map_or(1.0, |at| {
                    now.saturating_duration_since(at).as_secs_f32() / 0.08
                })
                .clamp(0.0, 1.0);
            let eased = ease_out_cubic(elapsed);
            return ControlAnimation {
                scale: 1.0 - 0.1 * eased,
                press: eased,
                active: elapsed < 1.0,
            };
        }
        if let Some((released, at, from)) = self.released {
            if released == button {
                let elapsed =
                    (now.saturating_duration_since(at).as_secs_f32() / 0.18).clamp(0.0, 1.0);
                if elapsed >= 1.0 {
                    self.released = None;
                    return ControlAnimation {
                        scale: 1.0,
                        press: 0.0,
                        active: false,
                    };
                }
                return ControlAnimation {
                    scale: from + (1.0 - from) * ease_out_back(elapsed),
                    press: 1.0 - ease_out_cubic(elapsed),
                    active: true,
                };
            }
        }
        ControlAnimation {
            scale: 1.0,
            press: 0.0,
            active: false,
        }
    }
}

impl LyricsRenderer {
    fn queue_scroll_max(&self, bottom_chrome: BottomChromeSample) -> f32 {
        let Some(player) = self.scene.as_ref().and_then(|scene| scene.player.as_ref()) else {
            return 0.0;
        };
        let list_top = player.layout.queue_content_top;
        let content_height = player.queue_items.len() as f32 * 56.0 * player.layout.scale;
        (list_top + content_height - bottom_chrome.content_end(player.layout)).max(0.0)
    }

    pub(super) fn begin_queue_scroll(&mut self) {
        self.queue_scroll.dragging = true;
        self.queue_scroll.velocity = 0.0;
        self.queue_scroll.last_frame_at = Some(Instant::now());
        self.player_interaction.reveal();
    }

    pub(super) fn scroll_queue_by(&mut self, delta_y: f32) {
        if !delta_y.is_finite() {
            return;
        }
        let max = self.queue_scroll_max(BottomChromeSample {
            visibility: 1.0,
            active: true,
        });
        let limit = self
            .scene
            .as_ref()
            .and_then(|scene| scene.player.as_ref())
            .map_or(80.0, |player| 80.0 * player.layout.scale);
        self.queue_scroll.offset = (self.queue_scroll.offset + delta_y).clamp(-limit, max + limit);
        self.queue_scroll.velocity = 0.0;
        self.queue_scroll.dragging = true;
        self.queue_scroll.last_frame_at = Some(Instant::now());
        self.player_interaction.reveal();
    }

    pub(super) fn end_queue_scroll(&mut self, velocity_y: f32) {
        self.queue_scroll.dragging = false;
        self.queue_scroll.velocity = velocity_y.clamp(
            -self.scroll_params().max_fling_velocity,
            self.scroll_params().max_fling_velocity,
        );
        self.queue_scroll.last_frame_at = Some(Instant::now());
        self.player_interaction.reveal();
    }

    pub(super) fn cancel_queue_scroll(&mut self) {
        self.queue_scroll.dragging = false;
        self.queue_scroll.velocity = 0.0;
        self.queue_scroll.last_frame_at = Some(Instant::now());
    }

    pub(super) fn sample_queue_scroll(
        &mut self,
        now: Instant,
        bottom_chrome: BottomChromeSample,
    ) -> (f32, bool) {
        let max = self.queue_scroll_max(bottom_chrome);
        let dt = self
            .queue_scroll
            .last_frame_at
            .map(|last| now.saturating_duration_since(last).as_secs_f32())
            .unwrap_or(0.0)
            .clamp(0.0, 0.05);
        self.queue_scroll.last_frame_at = Some(now);

        if !self.queue_scroll.dragging && dt > 0.0 {
            let params = self.scroll_params();
            advance_fling(
                &mut self.queue_scroll.offset,
                &mut self.queue_scroll.velocity,
                params.deceleration_rate,
                dt,
            );
            let bound = self.queue_scroll.offset.clamp(0.0, max);
            let displacement = bound - self.queue_scroll.offset;
            if displacement.abs() > LINE_LAYOUT_EPSILON {
                spring_step(
                    &mut self.queue_scroll.offset,
                    &mut self.queue_scroll.velocity,
                    bound,
                    params.overscroll_stiffness,
                    params.overscroll_damping,
                    dt,
                );
            }
            if self.queue_scroll.velocity.abs() < MANUAL_SCROLL_VELOCITY_EPSILON
                && displacement.abs() < LINE_LAYOUT_EPSILON
            {
                self.queue_scroll.offset = bound;
                self.queue_scroll.velocity = 0.0;
            }
        }

        let active = self.queue_scroll.dragging
            || self.queue_scroll.velocity.abs() >= MANUAL_SCROLL_VELOCITY_EPSILON
            || self.queue_scroll.offset < -LINE_LAYOUT_EPSILON
            || self.queue_scroll.offset > max + LINE_LAYOUT_EPSILON;
        (self.queue_scroll.offset, active)
    }

    pub fn begin_queue_reorder(&mut self, x: f32, y: f32) -> i32 {
        if self.player_screen != PlayerScreenInput::Queue {
            return -1;
        }
        let Some(player) = self.scene.as_ref().and_then(|scene| scene.player.as_ref()) else {
            return -1;
        };
        let scale = player.layout.scale;
        let list_top = player.layout.queue_content_top;
        let list_bottom = self
            .player_interaction
            .bottom_chrome_sample(self.player_screen, Instant::now())
            .content_end(player.layout);
        if x < 326.0 * scale || x > 377.0 * scale || y < list_top || y > list_bottom {
            return -1;
        }
        let row_height = 56.0 * scale;
        let index = ((y + self.queue_scroll.offset - list_top) / row_height).floor() as usize;
        if index >= player.queue_items.len() {
            return -1;
        }
        self.queue_reorder = QueueReorderState {
            active: true,
            from: index,
            to: index,
            pointer_y: y,
        };
        self.queue_scroll.dragging = false;
        self.queue_scroll.velocity = 0.0;
        self.player_interaction.cancel();
        self.player_interaction.reveal();
        index as i32
    }

    pub fn update_queue_reorder(&mut self, y: f32) {
        if !self.queue_reorder.active || !y.is_finite() {
            return;
        }
        let Some(player) = self.scene.as_ref().and_then(|scene| scene.player.as_ref()) else {
            self.queue_reorder = QueueReorderState::default();
            return;
        };
        let scale = player.layout.scale;
        let row_height = 56.0 * scale;
        let list_top = player.layout.queue_content_top;
        let list_bottom = self
            .player_interaction
            .bottom_chrome_sample(self.player_screen, Instant::now())
            .content_end(player.layout);
        let edge = 36.0 * scale;
        let max = self.queue_scroll_max(BottomChromeSample {
            visibility: 1.0,
            active: true,
        });
        if y < list_top + edge {
            self.queue_scroll.offset = (self.queue_scroll.offset - 12.0 * scale).max(0.0);
        } else if y > list_bottom - edge {
            self.queue_scroll.offset = (self.queue_scroll.offset + 12.0 * scale).min(max);
        }
        let content_y = y.clamp(list_top, list_bottom) + self.queue_scroll.offset;
        let target = ((content_y - list_top) / row_height).floor() as usize;
        self.queue_reorder.to = target.min(player.queue_items.len().saturating_sub(1));
        self.queue_reorder.pointer_y =
            y.clamp(list_top + row_height * 0.5, list_bottom - row_height * 0.5);
        self.player_interaction.reveal();
    }

    pub fn finish_queue_reorder(&mut self) -> i64 {
        if !self.queue_reorder.active {
            return -1;
        }
        let from = self.queue_reorder.from;
        let to = self.queue_reorder.to;
        if from != to {
            if let Some(player) = self.scene.as_mut().and_then(|scene| scene.player.as_mut()) {
                if from < player.queue_items.len() && to < player.queue_items.len() {
                    let item = player.queue_items.remove(from);
                    player.queue_items.insert(to, item);
                }
            }
        }
        self.queue_reorder = QueueReorderState::default();
        ((from as i64) << 32) | to as i64
    }

    pub fn cancel_queue_reorder(&mut self) {
        self.queue_reorder = QueueReorderState::default();
    }
}

#[derive(Debug)]
struct PlayerIcons {
    star: SvgIcon,
    ellipsis: SvgIcon,
    previous: SvgIcon,
    play: SvgIcon,
    pause: SvgIcon,
    next: SvgIcon,
    lyrics: SvgIcon,
    list: SvgIcon,
    shuffle: SvgIcon,
    repeat_one: SvgIcon,
    album: SvgIcon,
}

#[derive(Debug)]
struct SvgIcon {
    path: Path,
    view_width: f32,
    view_height: f32,
    mirror_x: bool,
}

impl SvgIcon {
    fn new(data: &str, view_width: f32, view_height: f32) -> Self {
        Self {
            // Never panic on icon parse: a missing glyph is preferable to
            // taking down the player when a scene is rebuilt at track change.
            path: Path::from_svg(data).unwrap_or_default(),
            view_width,
            view_height,
            mirror_x: false,
        }
    }

    fn mirrored(mut self) -> Self {
        self.mirror_x = true;
        self
    }
}

impl PlayerIcons {
    fn new() -> Self {
        Self {
            star: SvgIcon::new(STAR_PATH, 2316.92, 2209.92),
            ellipsis: SvgIcon::new(ELLIPSIS_PATH, 1947.92, 460.92),
            previous: SvgIcon::new(COMPOSE_FORWARD_PATH, 32.0, 32.0).mirrored(),
            play: SvgIcon::new(COMPOSE_PLAY_PATH, 32.0, 32.0),
            pause: SvgIcon::new(COMPOSE_PAUSE_PATH, 32.0, 32.0),
            next: SvgIcon::new(COMPOSE_FORWARD_PATH, 32.0, 32.0),
            lyrics: SvgIcon::new(LYRICS_PATH, 2285.92, 2156.92),
            list: SvgIcon::new(LIST_PATH, 2096.92, 1542.92),
            shuffle: SvgIcon::new(SHUFFLE_PATH, 2379.92, 1893.92),
            repeat_one: SvgIcon::new(REPEAT_ONE_PATH, 2220.92, 1889.92),
            album: SvgIcon::new(ALBUM_PATH, 2439.92, 1922.92),
        }
    }
}

impl LyricsRenderer {
    pub(super) fn lyrics_input_enabled(&self) -> bool {
        self.scene
            .as_ref()
            .and_then(|scene| scene.player.as_ref())
            .is_none()
            || self.player_screen == PlayerScreenInput::Lyrics
    }

    pub(super) fn prepare_player(
        &mut self,
        input: Option<&PlayerInput>,
        width: f32,
        height: f32,
    ) -> Option<PreparedPlayer> {
        let input = input?;
        let viewport_width = input
            .viewport_width
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(width)
            .clamp(1.0, width);
        let viewport_height = input
            .viewport_height
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(height)
            .clamp(1.0, height);
        let mini_layout = PlayerLayout::resolve_with_scale(
            viewport_width,
            viewport_height,
            (width / DESIGN_WIDTH).max(0.25),
        );
        let layout = if input.presentation == PlayerPresentationInput::Mini {
            mini_layout
        } else {
            PlayerLayout::resolve(width, height)
        };
        let saved = self.text_attrs;
        self.text_attrs = TextAttrs {
            weight: 700,
            italic: false,
        };
        let title = self.prepare_plain_text(
            &input.title,
            17.0 * layout.scale,
            24.0 * layout.scale,
            1_000_000.0,
            false,
        );
        self.text_attrs = TextAttrs {
            weight: 400,
            italic: false,
        };
        let artist = self.prepare_plain_text(
            &input.artist,
            15.0 * layout.scale,
            21.0 * layout.scale,
            1_000_000.0,
            false,
        );
        self.text_attrs = TextAttrs {
            weight: 700,
            italic: false,
        };
        let artwork_title = self.prepare_plain_text(
            &input.title,
            19.0 * layout.scale,
            25.0 * layout.scale,
            1_000_000.0,
            false,
        );
        self.text_attrs = TextAttrs {
            weight: 400,
            italic: false,
        };
        let artwork_artist = self.prepare_plain_text(
            &input.artist,
            15.0 * layout.scale,
            21.0 * layout.scale,
            1_000_000.0,
            false,
        );
        self.text_attrs = TextAttrs {
            weight: 700,
            italic: false,
        };
        let queue_title = self.prepare_plain_text(
            if input.queue_title.is_empty() {
                "Up Next"
            } else {
                &input.queue_title
            },
            18.0 * layout.scale,
            23.0 * layout.scale,
            1_000_000.0,
            false,
        );
        self.text_attrs = TextAttrs {
            weight: 600,
            italic: false,
        };
        let queue_source = self.prepare_plain_text(
            &input.queue_source,
            14.0 * layout.scale,
            19.0 * layout.scale,
            1_000_000.0,
            false,
        );
        let mut queue_items = Vec::with_capacity(input.queue_items.len());
        for item in &input.queue_items {
            self.text_attrs = TextAttrs {
                weight: 600,
                italic: false,
            };
            let item_title = self.prepare_plain_text(
                &item.title,
                15.0 * layout.scale,
                20.0 * layout.scale,
                1_000_000.0,
                false,
            );
            self.text_attrs = TextAttrs {
                weight: 400,
                italic: false,
            };
            let item_artist = self.prepare_plain_text(
                &item.artist,
                13.0 * layout.scale,
                18.0 * layout.scale,
                1_000_000.0,
                false,
            );
            queue_items.push(PreparedQueueItem {
                title: item_title,
                artist: item_artist,
                artwork_key: item.artwork_key.clone(),
            });
        }
        self.text_attrs = TextAttrs {
            weight: 400,
            italic: false,
        };
        let runtime_label_font = PreparedRuntimeLabelFont {
            glyphs: RUNTIME_LABEL_GLYPHS
                .chars()
                .map(|ch| {
                    let text = self.prepare_plain_text(
                        &ch.to_string(),
                        11.0 * layout.scale,
                        14.0 * layout.scale,
                        1_000_000.0,
                        false,
                    );
                    (ch, text)
                })
                .collect(),
        };
        self.text_attrs = saved;
        Some(PreparedPlayer {
            presentation: input.presentation,
            screen: input.screen,
            duration_ms: input.duration_ms.max(0),
            is_playing: input.is_playing,
            liked: input.liked,
            title,
            artist,
            artwork_title,
            artwork_artist,
            queue_title,
            queue_source,
            queue_filter: input.queue_filter,
            queue_items,
            runtime_label_font,
            layout,
            mini_layout,
            icons: PlayerIcons::new(),
        })
    }
}

pub(super) fn collect_player_font_usage(
    player: &PreparedPlayer,
    ids: &mut Vec<fontdb::ID>,
) -> usize {
    collect_text_font_usage(&player.title, ids)
        + collect_text_font_usage(&player.artist, ids)
        + collect_text_font_usage(&player.artwork_title, ids)
        + collect_text_font_usage(&player.artwork_artist, ids)
        + collect_text_font_usage(&player.queue_title, ids)
        + collect_text_font_usage(&player.queue_source, ids)
        + player
            .runtime_label_font
            .glyphs
            .iter()
            .map(|(_, text)| collect_text_font_usage(text, ids))
            .sum::<usize>()
        + player
            .queue_items
            .iter()
            .map(|item| {
                collect_text_font_usage(&item.title, ids)
                    + collect_text_font_usage(&item.artist, ids)
            })
            .sum::<usize>()
}

pub(super) fn draw_player(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    thumbnail: Option<&Image>,
    queue_artworks: &HashMap<String, Image>,
    player: &PreparedPlayer,
    interaction: &mut PlayerInteractionState,
    transition: PlayerTransitionSample,
    bottom_chrome: BottomChromeSample,
    queue_scroll_y: f32,
    queue_reorder: QueueReorderSample,
    current_time_ms: i32,
    expansion_progress: f32,
) -> bool {
    let layout = player.layout;
    let scale = layout.scale;
    let now = Instant::now();
    let mut animating = false;
    let lyrics_ui = PlayerUiLayout::resolve(layout, PlayerScreenInput::Lyrics, player.presentation);
    let artwork_ui =
        PlayerUiLayout::resolve(layout, PlayerScreenInput::Artwork, player.presentation);
    let queue_ui = PlayerUiLayout::resolve(layout, PlayerScreenInput::Queue, player.presentation);
    let active_ui = PlayerUiLayout::resolve(layout, transition.to, player.presentation);
    if player.presentation == PlayerPresentationInput::Mini {
        return draw_mini_player(
            canvas,
            typefaces,
            thumbnail,
            player,
            &active_ui,
            interaction,
            now,
            current_time_ms,
            1.0,
        );
    }
    let expansion = smooth_step(expansion_progress.clamp(0.0, 1.0));
    if expansion < 0.999 {
        let mini_ui = PlayerUiLayout::resolve(
            player.mini_layout,
            transition.to,
            PlayerPresentationInput::Mini,
        );
        animating |= draw_mini_player(
            canvas,
            typefaces,
            thumbnail,
            player,
            &mini_ui,
            interaction,
            now,
            current_time_ms,
            1.0 - expansion,
        );
    }

    // Drag handle — soft white pill (Plus).
    DrawCommand::RoundRect {
        rect: Rect::from_xywh(166.5 * scale, 59.5 * scale, 60.0 * scale, 5.0 * scale),
        radius: 2.5 * scale,
        color: multiplied_alpha(WHITE_HANDLE, expansion),
        blend: DrawBlend::Plus,
    }
    .draw(canvas);

    let page_bounds = Rect::from_xywh(
        0.0,
        layout.header_top,
        layout.width,
        bottom_chrome.content_end(layout) - layout.header_top,
    );
    let shares_compact_header = transition.shares_compact_header();
    let (lyrics_alpha, lyrics_scale) = transition.content_transform(PlayerScreenInput::Lyrics);
    if lyrics_alpha > 0.001 && !shares_compact_header {
        draw_content_layer(
            canvas,
            page_bounds,
            lyrics_alpha * expansion,
            lyrics_scale,
            |canvas, alpha| {
                draw_compact_header(
                    canvas,
                    typefaces,
                    player,
                    &lyrics_ui,
                    interaction,
                    now,
                    current_time_ms,
                    alpha,
                    &mut animating,
                );
            },
        );
    }
    let (artwork_alpha, artwork_scale) = transition.content_transform(PlayerScreenInput::Artwork);
    if artwork_alpha > 0.001 {
        draw_content_layer(
            canvas,
            page_bounds,
            artwork_alpha * expansion,
            artwork_scale,
            |canvas, alpha| {
                draw_artwork_metadata(
                    canvas,
                    typefaces,
                    player,
                    &artwork_ui,
                    interaction,
                    now,
                    current_time_ms,
                    alpha,
                    &mut animating,
                );
            },
        );
    }
    let (queue_alpha, queue_scale) = transition.content_transform(PlayerScreenInput::Queue);
    if queue_alpha > 0.001 {
        draw_content_layer(
            canvas,
            page_bounds,
            queue_alpha * expansion,
            queue_scale,
            |canvas, alpha| {
                if !shares_compact_header {
                    draw_compact_header(
                        canvas,
                        typefaces,
                        player,
                        &queue_ui,
                        interaction,
                        now,
                        current_time_ms,
                        alpha,
                        &mut animating,
                    );
                }
                draw_queue_body(
                    canvas,
                    typefaces,
                    thumbnail,
                    queue_artworks,
                    player,
                    &queue_ui,
                    bottom_chrome,
                    queue_scroll_y,
                    queue_reorder,
                    interaction,
                    now,
                    alpha,
                    &mut animating,
                );
            },
        );
    }
    if shares_compact_header {
        draw_compact_header(
            canvas,
            typefaces,
            player,
            &active_ui,
            interaction,
            now,
            current_time_ms,
            expansion,
            &mut animating,
        );
    }

    // One image participates in the transition. Its geometry is interpolated;
    // compact and full cover copies are never cross-faded over one another.
    // Normal blend (not Plus) so the cover keeps true colour.
    let artwork_progress = transition.artwork_progress();
    let expanded_art = lerp_rect(
        layout.compact_artwork_rect(),
        layout.full_artwork_rect(),
        artwork_progress,
    );
    let shared_art = lerp_rect(layout.collapsed_artwork_rect(), expanded_art, expansion);
    let expanded_radius = lerp(12.0 * scale, 18.0 * scale, artwork_progress);
    let radius = lerp(6.0 * scale, expanded_radius, expansion);
    draw_artwork(canvas, thumbnail, shared_art, radius, 1.0);

    if bottom_chrome.visibility > 0.001 {
        canvas.save();
        canvas.translate((0.0, bottom_chrome.slide_offset(layout)));
        draw_progress(
            canvas,
            typefaces,
            player,
            current_time_ms,
            bottom_chrome.visibility * expansion,
        );
        draw_transport(
            canvas,
            player,
            &active_ui,
            interaction,
            now,
            bottom_chrome.visibility * expansion,
            &mut animating,
        );
        draw_mode_navigation(
            canvas,
            player,
            &active_ui,
            interaction,
            transition.to,
            now,
            bottom_chrome.visibility * expansion,
            &mut animating,
        );
        canvas.restore();
    }
    animating || transition.active || bottom_chrome.active
}

fn draw_mini_player(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    thumbnail: Option<&Image>,
    player: &PreparedPlayer,
    ui: &PlayerUiLayout,
    interaction: &mut PlayerInteractionState,
    now: Instant,
    current_time_ms: i32,
    alpha: f32,
) -> bool {
    let layout = ui.layout;
    let s = layout.scale;
    let mini = ui
        .mini
        .expect("mini presentation must resolve mini flex geometry");
    DrawCommand::RoundRect {
        rect: Rect::from_xywh(0.0, 0.0, layout.width, layout.height),
        radius: 16.0 * s,
        color: Color4f::new(0.08, 0.08, 0.08, 0.82 * alpha),
        blend: DrawBlend::SourceOver,
    }
    .draw(canvas);
    draw_artwork(canvas, thumbnail, mini.artwork, 6.0 * s, alpha);

    let text_left = mini.text.left;
    let text_width = mini.text.width().max(1.0);
    let mut animating = false;
    animating |= draw_plus_marquee_text(
        canvas,
        typefaces,
        &player.title,
        text_left,
        7.0 * s,
        text_width,
        0.0,
        8.0 * s,
        TEXT_PRIMARY_ALPHA * alpha,
        current_time_ms,
    );
    animating |= draw_plus_marquee_text(
        canvas,
        typefaces,
        &player.artist,
        text_left,
        31.0 * s,
        text_width,
        0.0,
        8.0 * s,
        TEXT_SECONDARY_ALPHA * alpha,
        current_time_ms,
    );

    for (button, icon) in [
        (
            PlayerButton::PlayPause,
            if player.is_playing {
                &player.icons.pause
            } else {
                &player.icons.play
            },
        ),
        (PlayerButton::Next, &player.icons.next),
    ] {
        let control = ui.control(button);
        let (width, height) = control.transport_size();
        let animation = interaction.animation_for(button, now);
        animating |= animation.active;
        draw_icon(
            canvas,
            icon,
            control.center,
            width * animation.scale,
            height * animation.scale,
            WHITE,
            alpha,
        );
    }
    animating
}

/// Draw prepared text through a Plus layer so white metadata matches Compose.
fn draw_plus_text(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    text: &PreparedText,
    origin_x: f32,
    origin_y: f32,
    alpha: f32,
) {
    let mut layer = Paint::default();
    layer.set_blend_mode(BlendMode::Plus);
    canvas.save_layer(&SaveLayerRec::default().paint(&layer));
    draw_prepared_text_skia(
        canvas,
        typefaces,
        text,
        origin_x,
        origin_y,
        (255, 255, 255, 255),
        alpha,
        0.0,
        None,
    );
    canvas.restore();
}

#[allow(clippy::too_many_arguments)]
fn draw_plus_marquee_text(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    text: &PreparedText,
    left: f32,
    top: f32,
    max_width: f32,
    left_fade_width: f32,
    right_fade_width: f32,
    alpha: f32,
    current_time_ms: i32,
) -> bool {
    let mut layer = Paint::default();
    layer.set_blend_mode(BlendMode::Plus);
    canvas.save_layer(&SaveLayerRec::default().paint(&layer));
    let animating = super::draw::draw_top_bar_marquee_line(
        canvas,
        typefaces,
        text,
        left,
        top,
        max_width,
        left_fade_width,
        right_fade_width,
        (255, 255, 255, 255),
        alpha,
        current_time_ms,
    );
    canvas.restore();
    animating
}

fn draw_compact_header(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    player: &PreparedPlayer,
    ui: &PlayerUiLayout,
    interaction: &mut PlayerInteractionState,
    now: Instant,
    current_time_ms: i32,
    page_alpha: f32,
    animating: &mut bool,
) {
    let s = player.layout.scale;
    *animating |= draw_plus_marquee_text(
        canvas,
        typefaces,
        &player.title,
        116.0 * s,
        102.5 * s,
        157.0 * s,
        12.0 * s,
        4.0 * s,
        TEXT_PRIMARY_ALPHA * page_alpha,
        current_time_ms,
    );
    *animating |= draw_plus_marquee_text(
        canvas,
        typefaces,
        &player.artist,
        116.0 * s,
        128.5 * s,
        157.0 * s,
        12.0 * s,
        4.0 * s,
        TEXT_SECONDARY_ALPHA * page_alpha,
        current_time_ms,
    );

    let favorite_animation = interaction.animation_for(PlayerButton::Favorite, now);
    let more_animation = interaction.animation_for(PlayerButton::More, now);
    *animating |= favorite_animation.active || more_animation.active;
    let favorite = ui.control(PlayerButton::Favorite);
    let more = ui.control(PlayerButton::More);
    let (favorite_diameter, favorite_icon_size) = favorite.action_geometry();
    let (more_diameter, more_icon_size) = more.action_geometry();
    draw_action_button(
        canvas,
        &player.icons.star,
        favorite.center,
        favorite_diameter,
        favorite_icon_size,
        favorite_animation.scale,
        player.liked,
        page_alpha,
    );
    draw_action_button(
        canvas,
        &player.icons.ellipsis,
        more.center,
        more_diameter,
        more_icon_size,
        more_animation.scale,
        false,
        page_alpha,
    );
}

fn draw_artwork_metadata(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    player: &PreparedPlayer,
    ui: &PlayerUiLayout,
    interaction: &mut PlayerInteractionState,
    now: Instant,
    current_time_ms: i32,
    page_alpha: f32,
    animating: &mut bool,
) {
    let layout = player.layout;
    let scale = layout.scale;
    let top = layout.artwork_metadata_top;
    *animating |= draw_plus_marquee_text(
        canvas,
        typefaces,
        &player.artwork_title,
        32.0 * scale,
        top + 24.0 * scale,
        241.0 * scale,
        8.0 * scale,
        4.0 * scale,
        TEXT_PRIMARY_ALPHA * page_alpha,
        current_time_ms,
    );
    *animating |= draw_plus_marquee_text(
        canvas,
        typefaces,
        &player.artwork_artist,
        32.0 * scale,
        top + 51.0 * scale,
        241.0 * scale,
        8.0 * scale,
        4.0 * scale,
        TEXT_SECONDARY_ALPHA * page_alpha,
        current_time_ms,
    );

    let favorite_animation = interaction.animation_for(PlayerButton::Favorite, now);
    let more_animation = interaction.animation_for(PlayerButton::More, now);
    *animating |= favorite_animation.active || more_animation.active;
    let favorite = ui.control(PlayerButton::Favorite);
    let more = ui.control(PlayerButton::More);
    let (favorite_diameter, favorite_icon_size) = favorite.action_geometry();
    let (more_diameter, more_icon_size) = more.action_geometry();
    draw_action_button(
        canvas,
        &player.icons.star,
        favorite.center,
        favorite_diameter,
        favorite_icon_size,
        favorite_animation.scale,
        player.liked,
        page_alpha,
    );
    draw_action_button(
        canvas,
        &player.icons.ellipsis,
        more.center,
        more_diameter,
        more_icon_size,
        more_animation.scale,
        false,
        page_alpha,
    );
}

fn draw_queue_body(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    thumbnail: Option<&Image>,
    queue_artworks: &HashMap<String, Image>,
    player: &PreparedPlayer,
    ui: &PlayerUiLayout,
    bottom_chrome: BottomChromeSample,
    queue_scroll_y: f32,
    queue_reorder: QueueReorderSample,
    interaction: &mut PlayerInteractionState,
    now: Instant,
    page_alpha: f32,
    animating: &mut bool,
) {
    let layout = player.layout;
    let scale = layout.scale;
    for (button, icon, filter) in [
        (
            PlayerButton::QueueUpNext,
            &player.icons.list,
            QueueFilterInput::UpNext,
        ),
        (
            PlayerButton::QueueShuffle,
            &player.icons.shuffle,
            QueueFilterInput::Shuffle,
        ),
        (
            PlayerButton::QueueRepeatOne,
            &player.icons.repeat_one,
            QueueFilterInput::RepeatOne,
        ),
        (
            PlayerButton::QueueAlbum,
            &player.icons.album,
            QueueFilterInput::Album,
        ),
    ] {
        let control = ui.control(button);
        let (pill, icon_width, icon_height) = control.filter_geometry();
        let animation = interaction.animation_for(button, now);
        *animating |= animation.active;
        canvas.save();
        canvas.translate(control.center);
        canvas.scale((animation.scale, animation.scale));
        canvas.translate((-control.center.x, -control.center.y));
        let selected = player.queue_filter == filter;
        draw_inverse_button(
            canvas,
            icon,
            control.center,
            icon_width,
            icon_height,
            0.0,
            Some((pill, pill.height() * 0.5)),
            selected,
            page_alpha,
        );
        canvas.restore();
    }

    draw_plus_text(
        canvas,
        typefaces,
        &player.queue_title,
        32.0 * scale,
        layout.queue_metadata_top + 8.0 * scale,
        TEXT_PRIMARY_ALPHA * page_alpha,
    );
    draw_plus_text(
        canvas,
        typefaces,
        &player.queue_source,
        32.0 * scale,
        layout.queue_metadata_top + 34.0 * scale,
        TEXT_SECONDARY_ALPHA * page_alpha,
    );

    let list_top = layout.queue_content_top;
    let list_bottom = bottom_chrome.content_end(layout);
    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(
            32.0 * scale,
            list_top,
            (layout.width - 64.0 * scale).max(1.0),
            (list_bottom - list_top).max(1.0),
        ),
        ClipOp::Intersect,
        true,
    );
    canvas.translate((0.0, -queue_scroll_y));

    for visual_index in 0..player.queue_items.len() {
        let item_index = reordered_item_index(visual_index, queue_reorder);
        let item = &player.queue_items[item_index];
        let mut top = list_top + visual_index as f32 * 56.0 * scale;
        if queue_reorder.active && item_index == queue_reorder.from {
            top = queue_reorder.pointer_y + queue_scroll_y - 28.0 * scale;
        }
        if top - queue_scroll_y >= list_bottom {
            break;
        }
        if top + 56.0 * scale - queue_scroll_y <= list_top {
            continue;
        }
        draw_artwork(
            canvas,
            if item.artwork_key.is_empty() {
                thumbnail
            } else {
                queue_artworks.get(&item.artwork_key)
            },
            Rect::from_xywh(32.0 * scale, top + 4.0 * scale, 48.0 * scale, 48.0 * scale),
            8.0 * scale,
            page_alpha,
        );
        canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(92.0 * scale, top, 238.0 * scale, 56.0 * scale),
            ClipOp::Intersect,
            true,
        );
        draw_plus_text(
            canvas,
            typefaces,
            &item.title,
            92.0 * scale,
            top + 8.0 * scale,
            TEXT_PRIMARY_ALPHA * page_alpha,
        );
        draw_plus_text(
            canvas,
            typefaces,
            &item.artist,
            92.0 * scale,
            top + 30.0 * scale,
            TEXT_SECONDARY_ALPHA * page_alpha,
        );
        canvas.restore();
        draw_reorder_handle(
            canvas,
            Point::new(349.0 * scale, top + 28.0 * scale),
            scale,
            page_alpha,
        );
    }
    canvas.restore();
}

fn reordered_item_index(visual_index: usize, reorder: QueueReorderSample) -> usize {
    if !reorder.active || reorder.from == reorder.to {
        return visual_index;
    }
    if reorder.from < reorder.to {
        if visual_index == reorder.to {
            reorder.from
        } else if visual_index >= reorder.from && visual_index < reorder.to {
            visual_index + 1
        } else {
            visual_index
        }
    } else if visual_index == reorder.to {
        reorder.from
    } else if visual_index > reorder.to && visual_index <= reorder.from {
        visual_index - 1
    } else {
        visual_index
    }
}

fn draw_reorder_handle(canvas: &skia_safe::Canvas, center: Point, scale: f32, alpha: f32) {
    for offset in [-4.0, 0.0, 4.0] {
        DrawCommand::RoundRect {
            rect: Rect::from_xywh(
                center.x - 7.0 * scale,
                center.y + offset * scale - scale,
                14.0 * scale,
                2.0 * scale,
            ),
            radius: scale,
            color: multiplied_alpha(WHITE_SECONDARY, alpha),
            blend: DrawBlend::Plus,
        }
        .draw(canvas);
    }
}

fn draw_progress(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    player: &PreparedPlayer,
    current_time_ms: i32,
    chrome_alpha: f32,
) {
    let l = player.layout;
    let s = l.scale;
    let left = 32.0 * s;
    let width = (l.width - 64.0 * s).max(1.0);
    let top = l.progress_top + 14.0 * s;
    let ratio = if player.duration_ms > 0 {
        (current_time_ms.max(0) as f32 / player.duration_ms as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    DrawCommand::RoundRect {
        rect: Rect::from_xywh(left, top, width, 4.0 * s),
        radius: 2.0 * s,
        color: multiplied_alpha(WHITE_TRACK, chrome_alpha),
        blend: DrawBlend::Plus,
    }
    .draw(canvas);
    DrawCommand::RoundRect {
        rect: Rect::from_xywh(left, top, width * ratio, 4.0 * s),
        radius: 2.0 * s,
        color: multiplied_alpha(WHITE_FILL, chrome_alpha),
        blend: DrawBlend::Plus,
    }
    .draw(canvas);

    let elapsed = format_duration(current_time_ms.max(0));
    let remaining = format!(
        "−{}",
        format_duration((player.duration_ms - current_time_ms).max(0))
    );
    draw_runtime_label(
        canvas,
        typefaces,
        &player.runtime_label_font,
        &elapsed,
        left,
        l.progress_top + 28.0 * s,
        TEXT_SECONDARY_ALPHA * chrome_alpha,
        false,
    );
    draw_runtime_label(
        canvas,
        typefaces,
        &player.runtime_label_font,
        &remaining,
        left + width,
        l.progress_top + 28.0 * s,
        TEXT_SECONDARY_ALPHA * chrome_alpha,
        true,
    );
}

fn draw_transport(
    canvas: &skia_safe::Canvas,
    player: &PreparedPlayer,
    ui: &PlayerUiLayout,
    interaction: &mut PlayerInteractionState,
    now: Instant,
    chrome_alpha: f32,
    animating: &mut bool,
) {
    for (button, icon) in [
        (PlayerButton::Previous, &player.icons.previous),
        (PlayerButton::Next, &player.icons.next),
    ] {
        let control = ui.control(button);
        let (width, height) = control.transport_size();
        let animation = interaction.animation_for(button, now);
        *animating |= animation.active;
        draw_icon(
            canvas,
            icon,
            control.center,
            width * animation.scale,
            height * animation.scale,
            WHITE,
            chrome_alpha,
        );
    }
    let play = ui.control(PlayerButton::PlayPause);
    let (play_width, play_height) = play.transport_size();
    let play_animation = interaction.animation_for(PlayerButton::PlayPause, now);
    *animating |= play_animation.active;
    if player.is_playing {
        draw_icon(
            canvas,
            &player.icons.pause,
            play.center,
            play_width * (32.0 / 42.0) * play_animation.scale,
            play_height * play_animation.scale,
            WHITE,
            chrome_alpha,
        );
    } else {
        draw_icon(
            canvas,
            &player.icons.play,
            play.center,
            play_width * play_animation.scale,
            play_height * play_animation.scale,
            WHITE,
            chrome_alpha,
        );
    }
}

fn draw_mode_navigation(
    canvas: &skia_safe::Canvas,
    player: &PreparedPlayer,
    ui: &PlayerUiLayout,
    interaction: &mut PlayerInteractionState,
    selected_screen: PlayerScreenInput,
    now: Instant,
    chrome_alpha: f32,
    animating: &mut bool,
) {
    for (button, glyph, active_on) in [
        (
            PlayerButton::Lyrics,
            ModeGlyph::Icon(&player.icons.lyrics),
            Some(PlayerScreenInput::Lyrics),
        ),
        (PlayerButton::Output, ModeGlyph::Airplay, None),
        (
            PlayerButton::Queue,
            ModeGlyph::Icon(&player.icons.list),
            Some(PlayerScreenInput::Queue),
        ),
    ] {
        let control = ui.control(button);
        let animation = interaction.animation_for(button, now);
        *animating |= animation.active;
        let selected = active_on == Some(selected_screen);
        draw_mode_control(canvas, control, glyph, selected, animation, chrome_alpha);
    }
}

#[derive(Clone, Copy)]
enum ModeGlyph<'a> {
    Icon(&'a SvgIcon),
    Airplay,
}

fn draw_mode_control(
    canvas: &skia_safe::Canvas,
    control: PlayerControl,
    glyph: ModeGlyph<'_>,
    selected: bool,
    animation: ControlAnimation,
    chrome_alpha: f32,
) {
    let (diameter, icon_width, icon_height) = control.mode_geometry();
    canvas.save();
    canvas.translate(control.center);
    canvas.scale((animation.scale, animation.scale));
    canvas.translate((-control.center.x, -control.center.y));

    let fill = if selected { 1.0 } else { animation.press };
    draw_mode_glyph(
        canvas,
        glyph,
        control.center,
        icon_width,
        icon_height,
        multiplied_alpha(WHITE, chrome_alpha * (1.0 - fill)),
        DrawBlend::Plus,
    );
    if fill > 0.001 {
        let bounds = Rect::from_xywh(
            control.center.x - diameter * 0.5,
            control.center.y - diameter * 0.5,
            diameter,
            diameter,
        );
        let mut layer = Paint::default();
        layer.set_blend_mode(BlendMode::Plus);
        layer.set_alpha_f(fill * chrome_alpha);
        canvas.save_layer(&SaveLayerRec::default().bounds(&bounds).paint(&layer));
        DrawCommand::Circle {
            center: control.center,
            radius: diameter * 0.5,
            color: WHITE_BTN_ACTIVE,
            blend: DrawBlend::SourceOver,
        }
        .draw(canvas);
        draw_mode_glyph(
            canvas,
            glyph,
            control.center,
            icon_width,
            icon_height,
            WHITE,
            DrawBlend::DestinationOut,
        );
        canvas.restore();
    }
    canvas.restore();
}

fn draw_mode_glyph(
    canvas: &skia_safe::Canvas,
    glyph: ModeGlyph<'_>,
    center: Point,
    width: f32,
    height: f32,
    color: Color4f,
    blend: DrawBlend,
) {
    match glyph {
        ModeGlyph::Icon(icon) => DrawCommand::Icon {
            icon,
            center,
            width,
            height,
            color,
            alpha: 1.0,
            blend,
        }
        .draw(canvas),
        ModeGlyph::Airplay => DrawCommand::Airplay {
            center,
            size: width,
            color,
            blend,
        }
        .draw(canvas),
    }
}

fn draw_content_layer(
    canvas: &skia_safe::Canvas,
    bounds: Rect,
    alpha: f32,
    scale: f32,
    draw: impl FnOnce(&skia_safe::Canvas, f32),
) {
    canvas.save();
    canvas.clip_rect(bounds, ClipOp::Intersect, true);
    let center = Point::new(
        (bounds.left + bounds.right) * 0.5,
        (bounds.top + bounds.bottom) * 0.5,
    );
    canvas.translate(center);
    canvas.scale((scale, scale));
    canvas.translate((-center.x, -center.y));
    draw(canvas, alpha.clamp(0.0, 1.0));
    canvas.restore();
}

fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress.clamp(0.0, 1.0)
}

fn multiplied_alpha(color: Color4f, alpha: f32) -> Color4f {
    Color4f::new(color.r, color.g, color.b, color.a * alpha.clamp(0.0, 1.0))
}

fn lerp_rect(from: Rect, to: Rect, progress: f32) -> Rect {
    Rect::new(
        lerp(from.left, to.left, progress),
        lerp(from.top, to.top, progress),
        lerp(from.right, to.right, progress),
        lerp(from.bottom, to.bottom, progress),
    )
}

fn draw_action_button(
    canvas: &skia_safe::Canvas,
    icon: &SvgIcon,
    center: Point,
    diameter: f32,
    icon_size: f32,
    scale: f32,
    filled: bool,
    alpha: f32,
) {
    canvas.save();
    canvas.translate(center);
    canvas.scale((scale, scale));
    canvas.translate((-center.x, -center.y));
    draw_inverse_button(
        canvas,
        icon,
        center,
        icon_size,
        if icon.view_height < icon.view_width * 0.5 {
            icon_size * 0.3
        } else {
            icon_size
        },
        diameter,
        None,
        filled,
        alpha,
    );
    canvas.restore();
}

#[allow(clippy::too_many_arguments)]
fn draw_inverse_button(
    canvas: &skia_safe::Canvas,
    icon: &SvgIcon,
    center: Point,
    icon_width: f32,
    icon_height: f32,
    circle_diameter: f32,
    pill: Option<(Rect, f32)>,
    selected: bool,
    alpha: f32,
) {
    let bounds = pill.map_or_else(
        || {
            let diameter = circle_diameter.max(1.0);
            Rect::from_xywh(
                center.x - diameter * 0.5,
                center.y - diameter * 0.5,
                diameter,
                diameter,
            )
        },
        |(rect, _)| rect,
    );
    if !selected {
        match pill {
            Some((rect, radius)) => DrawCommand::RoundRect {
                rect,
                radius,
                color: multiplied_alpha(WHITE_BTN, alpha),
                blend: DrawBlend::Plus,
            }
            .draw(canvas),
            None => DrawCommand::Circle {
                center,
                radius: bounds.width() * 0.5,
                color: multiplied_alpha(WHITE_BTN, alpha),
                blend: DrawBlend::Plus,
            }
            .draw(canvas),
        }
        DrawCommand::Icon {
            icon,
            center,
            width: icon_width,
            height: icon_height,
            color: WHITE,
            alpha,
            blend: DrawBlend::Plus,
        }
        .draw(canvas);
        return;
    }

    let mut layer = Paint::default();
    layer.set_blend_mode(BlendMode::Plus);
    layer.set_alpha_f(alpha.clamp(0.0, 1.0));
    canvas.save_layer(&SaveLayerRec::default().bounds(&bounds).paint(&layer));
    match pill {
        Some((rect, radius)) => DrawCommand::RoundRect {
            rect,
            radius,
            color: WHITE_BTN_ACTIVE,
            blend: DrawBlend::SourceOver,
        }
        .draw(canvas),
        None => DrawCommand::Circle {
            center,
            radius: bounds.width() * 0.5,
            color: WHITE_BTN_ACTIVE,
            blend: DrawBlend::SourceOver,
        }
        .draw(canvas),
    }
    DrawCommand::Icon {
        icon,
        center,
        width: icon_width,
        height: icon_height,
        color: WHITE,
        alpha: 1.0,
        blend: DrawBlend::DestinationOut,
    }
    .draw(canvas);
    canvas.restore();
}

fn draw_artwork(
    canvas: &skia_safe::Canvas,
    thumbnail: Option<&Image>,
    rect: Rect,
    radius: f32,
    alpha: f32,
) {
    let clip = crate::capsule::continuous_rounded_rect(rect, radius);
    canvas.save();
    canvas.clip_path(&clip, ClipOp::Intersect, true);
    if let Some(image) = thumbnail {
        let mut paint = Paint::default();
        paint.set_alpha_f(alpha.clamp(0.0, 1.0));
        canvas.draw_image_rect_with_sampling_options(
            image,
            None,
            rect,
            SamplingOptions::from(skia_safe::sampling_options::FilterMode::Linear),
            &paint,
        );
    } else {
        DrawCommand::RoundRect {
            rect,
            radius: 0.0,
            color: multiplied_alpha(ARTWORK_PLACEHOLDER, alpha),
            blend: DrawBlend::SourceOver,
        }
        .draw(canvas);
    }
    canvas.restore();
}

fn draw_icon(
    canvas: &skia_safe::Canvas,
    icon: &SvgIcon,
    center: Point,
    width: f32,
    height: f32,
    color: Color4f,
    alpha: f32,
) {
    DrawCommand::Icon {
        icon,
        center,
        width,
        height,
        color,
        alpha,
        blend: DrawBlend::Plus,
    }
    .draw(canvas);
}

fn draw_airplay_command(
    canvas: &skia_safe::Canvas,
    center: Point,
    size: f32,
    color: Color4f,
    blend: DrawBlend,
) {
    let mut paint = command_paint(color, blend);
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width((size * 0.075).max(1.0));
    paint.set_stroke_cap(skia_safe::paint::Cap::Round);
    for radius in [0.19, 0.34, 0.49] {
        let r = size * radius;
        canvas.draw_arc(
            Rect::from_xywh(center.x - r, center.y - r, r * 2.0, r * 2.0),
            205.0,
            130.0,
            false,
            &paint,
        );
    }
    paint.set_style(skia_safe::paint::Style::Fill);
    let mut triangle = PathBuilder::new();
    triangle.move_to((center.x, center.y + size * 0.05));
    triangle.line_to((center.x - size * 0.34, center.y + size * 0.47));
    triangle.line_to((center.x + size * 0.34, center.y + size * 0.47));
    triangle.close();
    canvas.draw_path(&triangle.detach(), &paint);
}

fn draw_runtime_label(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    runtime_font: &PreparedRuntimeLabelFont,
    text: &str,
    x: f32,
    top: f32,
    alpha: f32,
    right_aligned: bool,
) {
    let measured = runtime_font.text_width(text);
    let mut cursor = if right_aligned { x - measured } else { x };
    let mut layer = Paint::default();
    layer.set_blend_mode(BlendMode::Plus);
    canvas.save_layer(&SaveLayerRec::default().paint(&layer));
    for ch in text.chars() {
        let Some(glyph) = runtime_font.glyph(ch) else {
            continue;
        };
        draw_prepared_text_skia(
            canvas,
            typefaces,
            glyph,
            cursor,
            top,
            (255, 255, 255, 255),
            alpha,
            0.0,
            None,
        );
        cursor += prepared_text_width(glyph);
    }
    canvas.restore();
}

fn format_duration(milliseconds: i32) -> String {
    let total_seconds = (milliseconds.max(0) / 1000) as u32;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

fn ease_out_cubic(value: f32) -> f32 {
    1.0 - (1.0 - value).powi(3)
}

fn smooth_step(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn ease_out_back(value: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (value - 1.0).powi(3) + c1 * (value - 1.0).powi(2)
}

// Exact vectors used by Clef's Compose mini player. Full and mini transports
// intentionally share these paths so the icon cannot change during expansion.
const COMPOSE_PLAY_PATH: &str = "M7.988 23.977V7.711C7.988 7.125 8.133 6.695 8.422 6.422C8.711 6.141 9.055 6 9.453 6C9.805 6 10.164 6.102 10.531 6.305L24.184 14.285C24.668 14.566 25.004 14.82 25.191 15.047C25.387 15.266 25.484 15.531 25.484 15.844C25.484 16.148 25.387 16.414 25.191 16.641C25.004 16.867 24.668 17.121 24.184 17.402L10.531 25.383C10.164 25.586 9.805 25.688 9.453 25.688C9.055 25.688 8.711 25.547 8.422 25.266C8.133 24.984 7.988 24.555 7.988 23.977Z";
const COMPOSE_PAUSE_PATH: &str = "M10.559 25.383C10.043 25.383 9.652 25.25 9.387 24.984C9.129 24.719 9 24.328 9 23.813V7.559C9 7.043 9.129 6.656 9.387 6.398C9.652 6.133 10.043 6 10.559 6H13.231C13.738 6 14.125 6.125 14.391 6.375C14.656 6.625 14.789 7.02 14.789 7.559V23.813C14.789 24.328 14.656 24.719 14.391 24.984C14.125 25.25 13.738 25.383 13.231 25.383H10.559ZM19.078 25.383C18.563 25.383 18.172 25.25 17.906 24.984C17.641 24.719 17.508 24.328 17.508 23.813V7.559C17.508 7.043 17.641 6.656 17.906 6.398C18.172 6.133 18.563 6 19.078 6H21.738C22.254 6 22.641 6.125 22.898 6.375C23.164 6.625 23.297 7.02 23.297 7.559V23.813C23.297 24.328 23.164 24.719 22.898 24.984C22.641 25.25 22.254 25.383 21.738 25.383H19.078Z";
const COMPOSE_FORWARD_PATH: &str = "M0 23.277V8.617C0 8.07 0.137 7.664 0.41 7.398C0.691 7.133 1.02 7 1.395 7C1.738 7 2.078 7.098 2.414 7.293L14.731 14.465C15.168 14.723 15.481 14.961 15.668 15.18C15.856 15.398 15.949 15.656 15.949 15.953C15.949 16.25 15.856 16.508 15.668 16.727C15.481 16.945 15.168 17.184 14.731 17.441L2.414 24.613C2.078 24.809 1.738 24.906 1.395 24.906C1.02 24.906 0.691 24.773 0.41 24.508C0.137 24.242 0 23.832 0 23.277ZM15.891 23.277V8.617C15.891 8.07 16.027 7.664 16.301 7.398C16.574 7.133 16.902 7 17.285 7C17.629 7 17.969 7.098 18.305 7.293L30.609 14.465C31.055 14.723 31.371 14.961 31.559 15.18C31.746 15.398 31.84 15.656 31.84 15.953C31.84 16.25 31.746 16.508 31.559 16.727C31.371 16.945 31.055 17.184 30.609 17.441L18.305 24.613C17.969 24.809 17.629 24.906 17.285 24.906C16.902 24.906 16.574 24.773 16.301 24.508C16.027 24.242 15.891 23.832 15.891 23.277Z";

// Paths extracted from E:/interest/sf-pro-extracor/SF-Pro.ttf via sf_map.json.
// Retained while older design snapshots still reference their extracted names.
#[allow(dead_code)]
fn legacy_transport_paths() -> [&'static str; 3] {
    [PREVIOUS_PATH, PAUSE_PATH, NEXT_PATH]
}
const STAR_PATH: &str = "M473.960 2144.960Q442.960 2121.960 436.460 2082.960Q429.960 2043.960 448.960 1989.960L657.960 1367.960L123.960 983.960Q76.960 950.960 58.960 915.960Q40.960 880.960 52.960 843.960Q64.960 807.960 99.960 789.960Q134.960 771.960 192.960 772.960L847.960 776.960L1046.960 151.960Q1064.960 96.960 1092.460 68.960Q1119.960 40.960 1157.960 40.960Q1196.960 40.960 1224.460 68.960Q1251.960 96.960 1269.960 151.960L1468.960 776.960L2123.960 772.960Q2181.960 771.960 2216.960 789.960Q2251.960 807.960 2263.960 843.960Q2275.960 880.960 2257.960 915.960Q2239.960 950.960 2192.960 983.960L1658.960 1367.960L1867.960 1989.960Q1886.960 2043.960 1880.460 2082.960Q1873.960 2121.960 1842.960 2144.960Q1811.960 2168.960 1772.960 2161.460Q1733.960 2153.960 1687.960 2120.960L1157.960 1731.960L628.960 2120.960Q582.960 2153.960 543.960 2161.460Q504.960 2168.960 473.960 2144.960ZM617.960 1946.960Q619.960 1949.960 626.960 1944.960L1107.960 1577.960Q1131.960 1558.960 1158.460 1558.960Q1184.960 1558.960 1208.960 1577.960L1689.960 1944.960Q1696.960 1949.960 1698.960 1946.960Q1699.960 1944.960 1698.960 1937.960L1499.960 1365.960Q1492.960 1346.960 1493.460 1329.960Q1493.960 1312.960 1502.960 1298.460Q1511.960 1283.960 1528.960 1271.960L2026.960 927.960Q2033.960 923.960 2031.960 919.960Q2030.960 916.960 2022.960 916.960L1417.960 927.960Q1386.960 928.960 1366.960 915.460Q1346.960 901.960 1337.960 870.960L1163.960 291.960Q1161.960 283.960 1157.960 283.960Q1154.960 283.960 1152.960 291.960L978.960 870.960Q969.960 901.960 949.960 915.460Q929.960 928.960 898.960 927.960L293.960 916.960Q285.960 916.960 284.960 919.960Q282.960 923.960 289.960 927.960L787.960 1271.960Q804.960 1283.960 813.960 1298.460Q822.960 1312.960 823.460 1329.960Q823.960 1346.960 816.960 1365.960L617.960 1937.960Q615.960 1944.960 617.960 1946.960Z";
const ELLIPSIS_PATH: &str = "M230.960 419.960Q177.960 419.960 134.960 394.460Q91.960 368.960 66.460 325.960Q40.960 282.960 40.960 230.960Q40.960 177.960 66.460 134.960Q91.960 91.960 134.960 66.460Q177.960 40.960 230.960 40.960Q282.960 40.960 325.960 66.460Q368.960 91.960 394.460 134.960Q419.960 177.960 419.960 230.960Q419.960 282.960 394.460 325.960Q368.960 368.960 325.960 394.460Q282.960 419.960 230.960 419.960ZM973.960 419.960Q920.960 419.960 877.960 394.460Q834.960 368.960 809.460 325.960Q783.960 282.960 783.960 230.960Q783.960 177.960 809.460 134.960Q834.960 91.960 877.960 66.460Q920.960 40.960 973.960 40.960Q1025.960 40.960 1068.960 66.460Q1111.960 91.960 1137.460 134.960Q1162.960 177.960 1162.960 230.960Q1162.960 282.960 1137.460 325.960Q1111.960 368.960 1068.960 394.460Q1025.960 419.960 973.960 419.960ZM1716.960 419.960Q1663.960 419.960 1620.960 394.460Q1577.960 368.960 1552.460 325.960Q1526.960 282.960 1526.960 230.960Q1526.960 177.960 1552.460 134.960Q1577.960 91.960 1620.960 66.460Q1663.960 40.960 1716.960 40.960Q1768.960 40.960 1811.960 66.460Q1854.960 91.960 1880.960 134.960Q1906.960 177.960 1906.960 230.960Q1906.960 282.960 1880.960 325.960Q1854.960 368.960 1811.960 394.460Q1768.960 419.960 1716.960 419.960Z";
const PREVIOUS_PATH: &str = "M1890.960 1429.960Q1890.960 1500.960 1855.460 1534.960Q1819.960 1568.960 1771.960 1568.960Q1727.960 1568.960 1684.960 1543.960L633.960 931.960Q577.960 898.960 553.960 870.960Q529.960 842.960 529.960 804.960Q529.960 766.960 553.960 738.960Q577.960 710.960 633.960 677.960L1684.960 65.960Q1727.960 40.960 1771.960 40.960Q1819.960 40.960 1855.460 74.960Q1890.960 108.960 1890.960 178.960L1890.960 1429.960ZM401.960 1562.960L173.960 1562.960Q107.960 1562.960 74.460 1528.960Q40.960 1494.960 40.960 1428.960L40.960 179.960Q40.960 110.960 74.460 78.460Q107.960 45.960 173.960 45.960L401.960 45.960Q466.960 45.960 500.960 79.960Q534.960 113.960 534.960 179.960L534.960 1428.960Q534.960 1494.960 500.960 1528.960Q466.960 1562.960 401.960 1562.960Z";
const PAUSE_PATH: &str = "M173.960 1694.960Q107.960 1694.960 74.460 1660.960Q40.960 1626.960 40.960 1560.960L40.960 173.960Q40.960 107.960 74.460 74.460Q107.960 40.960 173.960 40.960L401.960 40.960Q466.960 40.960 500.960 72.960Q534.960 104.960 534.960 173.960L534.960 1560.960Q534.960 1626.960 500.960 1660.960Q466.960 1694.960 401.960 1694.960L173.960 1694.960ZM900.960 1694.960Q834.960 1694.960 800.960 1660.960Q766.960 1626.960 766.960 1560.960L766.960 173.960Q766.960 107.960 800.960 74.460Q834.960 40.960 900.960 40.960L1127.960 40.960Q1193.960 40.960 1227.460 72.960Q1260.960 104.960 1260.960 173.960L1260.960 1560.960Q1260.960 1626.960 1227.460 1660.960Q1193.960 1694.960 1127.960 1694.960L900.960 1694.960Z";
const NEXT_PATH: &str = "M40.960 1429.960L40.960 178.960Q40.960 108.960 75.960 74.960Q110.960 40.960 159.960 40.960Q203.960 40.960 246.960 65.960L1296.960 677.960Q1353.960 710.960 1377.960 738.960Q1401.960 766.960 1401.960 804.960Q1401.960 842.960 1377.960 870.960Q1353.960 898.960 1296.960 931.960L246.960 1543.960Q203.960 1568.960 159.960 1568.960Q110.960 1568.960 75.960 1534.960Q40.960 1500.960 40.960 1429.960ZM1529.960 1562.960Q1463.960 1562.960 1430.460 1528.960Q1396.960 1494.960 1396.960 1428.960L1396.960 179.960Q1396.960 113.960 1430.460 79.960Q1463.960 45.960 1529.960 45.960L1756.960 45.960Q1822.960 45.960 1856.960 78.460Q1890.960 110.960 1890.960 179.960L1890.960 1428.960Q1890.960 1494.960 1856.960 1528.960Q1822.960 1562.960 1756.960 1562.960L1529.960 1562.960Z";
const LYRICS_PATH: &str = "M634.960 2115.960Q591.960 2115.960 568.960 2087.960Q545.960 2059.960 545.960 2012.960L545.960 1721.960L498.960 1721.960Q349.960 1721.960 247.960 1668.960Q145.960 1615.960 93.460 1513.960Q40.960 1411.960 40.960 1264.960L40.960 498.960Q40.960 351.960 93.460 249.960Q145.960 147.960 247.960 94.460Q349.960 40.960 498.960 40.960L1786.960 40.960Q1935.960 40.960 2037.960 94.460Q2139.960 147.960 2192.460 249.960Q2244.960 351.960 2244.960 498.960L2244.960 1264.960Q2244.960 1411.960 2192.460 1513.960Q2139.960 1615.960 2037.960 1668.960Q1935.960 1721.960 1786.960 1721.960L1110.960 1721.960L749.960 2051.960Q713.960 2084.960 689.460 2100.460Q664.960 2115.960 634.960 2115.960ZM644.960 772.960Q644.960 860.960 696.460 920.960Q747.960 980.960 835.960 980.960Q867.960 980.960 897.960 972.960Q927.960 964.960 947.960 939.960L955.960 939.960Q936.960 982.960 906.960 1014.960Q876.960 1046.960 841.460 1067.960Q805.960 1088.960 772.960 1097.960Q743.960 1104.960 733.460 1117.460Q722.960 1129.960 722.960 1148.960Q722.960 1168.960 737.460 1182.960Q751.960 1196.960 773.960 1196.960Q812.960 1196.960 865.960 1173.460Q918.960 1149.960 969.460 1101.960Q1019.960 1053.960 1053.460 981.460Q1086.960 908.960 1086.960 810.960Q1086.960 740.960 1058.460 684.960Q1029.960 628.960 979.460 596.460Q928.960 563.960 862.960 563.960Q769.960 563.960 707.460 622.460Q644.960 680.960 644.960 772.960ZM1201.960 772.960Q1201.960 860.960 1252.960 920.960Q1303.960 980.960 1391.960 980.960Q1423.960 980.960 1454.460 972.960Q1484.960 964.960 1504.960 939.960L1512.960 939.960Q1493.960 982.960 1463.960 1014.960Q1433.960 1046.960 1398.460 1067.960Q1362.960 1088.960 1328.960 1097.960Q1299.960 1104.960 1289.960 1117.460Q1279.960 1129.960 1279.960 1148.960Q1279.960 1168.960 1293.960 1182.960Q1307.960 1196.960 1330.960 1196.960Q1369.960 1196.960 1422.460 1173.460Q1474.960 1149.960 1525.960 1101.960Q1576.960 1053.960 1610.460 981.460Q1643.960 908.960 1643.960 810.960Q1643.960 740.960 1615.460 684.960Q1586.960 628.960 1535.960 596.460Q1484.960 563.960 1418.960 563.960Q1325.960 563.960 1263.960 622.460Q1201.960 680.960 1201.960 772.960Z";
const LIST_PATH: &str = "M166.960 292.960Q114.960 292.960 77.960 256.460Q40.960 219.960 40.960 166.960Q40.960 114.960 77.960 77.960Q114.960 40.960 166.960 40.960Q218.960 40.960 255.960 77.960Q292.960 114.960 292.960 166.960Q292.960 219.960 255.960 256.460Q218.960 292.960 166.960 292.960ZM608.960 247.960Q574.960 247.960 551.460 224.460Q527.960 200.960 527.960 166.960Q527.960 132.960 551.460 109.960Q574.960 86.960 608.960 86.960L1973.960 86.960Q2007.960 86.960 2031.960 109.960Q2055.960 132.960 2055.960 166.960Q2055.960 200.960 2031.960 224.460Q2007.960 247.960 1973.960 247.960L608.960 247.960ZM166.960 897.960Q114.960 897.960 77.960 860.960Q40.960 823.960 40.960 771.960Q40.960 719.960 77.960 682.960Q114.960 645.960 166.960 645.960Q218.960 645.960 255.960 682.960Q292.960 719.960 292.960 771.960Q292.960 823.960 255.960 860.960Q218.960 897.960 166.960 897.960ZM608.960 851.960Q574.960 851.960 551.460 828.960Q527.960 805.960 527.960 771.960Q527.960 737.960 551.460 714.460Q574.960 690.960 608.960 690.960L1973.960 690.960Q2007.960 690.960 2031.960 714.460Q2055.960 737.960 2055.960 771.960Q2055.960 805.960 2031.960 828.960Q2007.960 851.960 1973.960 851.960L608.960 851.960ZM166.960 1501.960Q114.960 1501.960 77.960 1465.460Q40.960 1428.960 40.960 1375.960Q40.960 1323.960 77.960 1287.460Q114.960 1250.960 166.960 1250.960Q218.960 1250.960 255.960 1287.460Q292.960 1323.960 292.960 1375.960Q292.960 1428.960 255.960 1465.460Q218.960 1501.960 166.960 1501.960ZM608.960 1456.960Q574.960 1456.960 551.460 1433.460Q527.960 1409.960 527.960 1375.960Q527.960 1341.960 551.460 1318.960Q574.960 1295.960 608.960 1295.960L1973.960 1295.960Q2007.960 1295.960 2031.960 1318.960Q2055.960 1341.960 2055.960 1375.960Q2055.960 1409.960 2031.960 1433.460Q2007.960 1456.960 1973.960 1456.960L608.960 1456.960Z";
const SHUFFLE_PATH: &str = "M1820.960 102.960Q1820.960 72.960 1837.460 56.960Q1853.960 40.960 1884.960 40.960Q1898.960 40.960 1912.460 45.460Q1925.960 49.960 1936.960 58.960L2314.960 372.960Q2338.960 393.960 2338.960 420.960Q2338.960 447.960 2314.960 467.960L1936.960 780.960Q1925.960 789.960 1912.460 794.960Q1898.960 799.960 1884.960 799.960Q1853.960 799.960 1837.460 783.460Q1820.960 766.960 1820.960 736.960L1820.960 102.960ZM40.960 1458.960Q40.960 1421.960 66.460 1398.960Q91.960 1375.960 132.960 1375.960L365.960 1375.960Q441.960 1375.960 500.460 1344.960Q558.960 1313.960 623.960 1237.960L1228.960 530.960Q1318.960 425.960 1402.960 384.960Q1486.960 343.960 1611.960 343.960L1967.960 343.960Q2002.960 343.960 2027.460 368.460Q2051.960 392.960 2051.960 427.960Q2051.960 461.960 2027.460 486.460Q2002.960 510.960 1967.960 510.960L1616.960 510.960Q1561.960 510.960 1517.460 524.460Q1472.960 537.960 1432.460 567.960Q1391.960 597.960 1348.960 647.960L742.960 1354.960Q651.960 1459.960 568.460 1500.960Q484.960 1541.960 360.960 1541.960L132.960 1541.960Q91.960 1541.960 66.460 1518.960Q40.960 1495.960 40.960 1458.960ZM1820.960 1790.960L1820.960 1156.960Q1820.960 1126.960 1837.460 1110.460Q1853.960 1093.960 1884.960 1093.960Q1898.960 1093.960 1912.460 1098.960Q1925.960 1103.960 1936.960 1112.960L2314.960 1425.960Q2338.960 1445.960 2338.960 1472.960Q2338.960 1499.960 2314.960 1520.960L1936.960 1834.960Q1925.960 1843.960 1912.460 1848.460Q1898.960 1852.960 1884.960 1852.960Q1853.960 1852.960 1837.460 1836.960Q1820.960 1820.960 1820.960 1790.960ZM40.960 434.960Q40.960 397.960 66.460 374.960Q91.960 351.960 132.960 351.960L360.960 351.960Q484.960 351.960 568.460 392.960Q651.960 433.960 742.960 538.960L1348.960 1245.960Q1391.960 1295.960 1432.460 1325.960Q1472.960 1355.960 1517.460 1369.460Q1561.960 1382.960 1616.960 1382.960L1967.960 1382.960Q2002.960 1382.960 2027.460 1407.460Q2051.960 1431.960 2051.960 1465.960Q2051.960 1500.960 2027.460 1525.460Q2002.960 1549.960 1967.960 1549.960L1611.960 1549.960Q1486.960 1549.960 1402.960 1508.960Q1318.960 1467.960 1228.960 1362.960L623.960 655.960Q558.960 579.960 500.460 548.960Q441.960 517.960 365.960 517.960L132.960 517.960Q91.960 517.960 66.460 494.960Q40.960 471.960 40.960 434.960Z";
const REPEAT_ONE_PATH: &str = "M1074.960 132.960Q1074.960 101.960 1091.460 85.960Q1107.960 69.960 1138.960 69.960Q1152.960 69.960 1166.460 74.460Q1179.960 78.960 1190.960 87.960L1568.960 401.960Q1593.960 422.960 1592.960 449.960Q1591.960 476.960 1568.960 496.960L1190.960 809.960Q1179.960 818.960 1166.460 823.960Q1152.960 828.960 1138.960 828.960Q1107.960 828.960 1091.460 812.460Q1074.960 795.960 1074.960 765.960L1074.960 132.960ZM124.960 994.960Q89.960 994.960 65.460 970.460Q40.960 945.960 40.960 909.960L40.960 802.960Q40.960 667.960 99.960 569.960Q158.960 471.960 267.460 418.460Q375.960 364.960 524.960 364.960L1221.960 364.960Q1256.960 364.960 1281.460 389.460Q1305.960 413.960 1305.960 447.960Q1305.960 481.960 1281.460 506.460Q1256.960 530.960 1221.960 530.960L508.960 530.960Q372.960 530.960 291.460 608.460Q209.960 685.960 209.960 814.960L209.960 909.960Q209.960 945.960 185.460 970.460Q160.960 994.960 124.960 994.960ZM926.960 1785.960Q926.960 1815.960 910.460 1832.460Q893.960 1848.960 862.960 1848.960Q848.960 1848.960 835.460 1843.960Q821.960 1838.960 810.960 1829.960L432.960 1515.960Q407.960 1495.960 408.460 1468.460Q408.960 1440.960 432.960 1420.960L810.960 1107.960Q821.960 1098.960 835.460 1094.460Q848.960 1089.960 862.960 1089.960Q893.960 1089.960 910.460 1105.960Q926.960 1121.960 926.960 1152.960L926.960 1785.960ZM2090.960 923.960Q2126.960 923.960 2150.960 948.460Q2174.960 972.960 2174.960 1008.960L2174.960 1115.960Q2174.960 1249.960 2115.960 1348.460Q2056.960 1446.960 1948.460 1500.460Q1839.960 1553.960 1690.960 1553.960L779.960 1553.960Q744.960 1553.960 720.460 1529.460Q695.960 1504.960 695.960 1469.960Q695.960 1435.960 720.460 1411.460Q744.960 1386.960 779.960 1386.960L1706.960 1386.960Q1842.960 1386.960 1924.960 1309.460Q2006.960 1231.960 2006.960 1103.960L2006.960 1008.960Q2006.960 972.960 2030.960 948.460Q2054.960 923.960 2090.960 923.960ZM2091.960 707.960Q2049.960 707.960 2025.960 683.960Q2001.960 659.960 2001.960 617.960L2001.960 196.960L1991.960 196.960L1884.960 281.960Q1865.960 296.960 1843.960 296.960Q1817.960 296.960 1801.960 281.960Q1785.960 266.960 1785.960 243.960Q1785.960 228.960 1791.460 216.460Q1796.960 203.960 1812.960 191.960L1946.960 89.960Q1977.960 65.960 2005.460 53.460Q2032.960 40.960 2071.960 40.960Q2119.960 40.960 2149.960 70.960Q2179.960 100.960 2179.960 151.960L2179.960 617.960Q2179.960 659.960 2156.460 683.960Q2132.960 707.960 2091.960 707.960Z";
const ALBUM_PATH: &str = "M492.960 577.960Q533.960 577.960 561.960 549.460Q589.960 520.960 589.960 481.960Q589.960 440.960 561.960 413.460Q533.960 385.960 492.960 385.960Q454.960 385.960 426.460 413.960Q397.960 441.960 397.960 481.960Q397.960 519.960 426.460 548.960Q454.960 577.960 492.960 577.960ZM492.960 892.960Q533.960 892.960 561.960 864.960Q589.960 836.960 589.960 796.960Q589.960 756.960 561.960 728.960Q533.960 700.960 492.960 700.960Q454.960 700.960 426.460 729.460Q397.960 757.960 397.960 796.960Q397.960 835.960 426.460 864.460Q454.960 892.960 492.960 892.960ZM492.960 1206.960Q533.960 1206.960 561.960 1178.960Q589.960 1150.960 589.960 1110.960Q589.960 1071.960 561.960 1043.960Q533.960 1015.960 492.960 1015.960Q453.960 1015.960 425.960 1044.460Q397.960 1072.960 397.960 1110.960Q397.960 1150.960 425.960 1178.960Q453.960 1206.960 492.960 1206.960ZM492.960 1537.960Q533.960 1537.960 561.960 1509.960Q589.960 1481.960 589.960 1441.960Q589.960 1401.960 561.960 1373.460Q533.960 1344.960 492.960 1344.960Q454.960 1344.960 426.460 1373.960Q397.960 1402.960 397.960 1441.960Q397.960 1480.960 426.460 1509.460Q454.960 1537.960 492.960 1537.960ZM803.960 544.960L1982.960 544.960Q2010.960 544.960 2029.460 526.460Q2047.960 507.960 2047.960 481.960Q2047.960 453.960 2028.960 434.960Q2009.960 415.960 1982.960 415.960L803.960 415.960Q776.960 415.960 757.960 434.960Q738.960 453.960 738.960 481.960Q738.960 507.960 757.960 526.460Q776.960 544.960 803.960 544.960ZM803.960 860.960L1456.960 860.960Q1482.960 860.960 1501.960 841.960Q1520.960 822.960 1520.960 796.960Q1520.960 769.960 1501.960 750.960Q1482.960 731.960 1456.960 731.960L803.960 731.960Q776.960 731.960 757.960 750.960Q738.960 769.960 738.960 796.960Q738.960 822.960 757.960 841.960Q776.960 860.960 803.960 860.960ZM803.960 1176.960L1982.960 1176.960Q2010.960 1176.960 2029.460 1157.960Q2047.960 1138.960 2047.960 1110.960Q2047.960 1084.960 2029.460 1066.460Q2010.960 1047.960 1982.960 1047.960L803.960 1047.960Q776.960 1047.960 757.960 1066.460Q738.960 1084.960 738.960 1110.960Q738.960 1138.960 757.960 1157.960Q776.960 1176.960 803.960 1176.960ZM803.960 1506.960L1456.960 1506.960Q1482.960 1506.960 1501.960 1487.960Q1520.960 1468.960 1520.960 1441.960Q1520.960 1414.960 1502.460 1396.460Q1483.960 1377.960 1456.960 1377.960L803.960 1377.960Q776.960 1377.960 757.960 1396.460Q738.960 1414.960 738.960 1441.960Q738.960 1468.960 757.960 1487.960Q776.960 1506.960 803.960 1506.960ZM354.960 1881.960Q197.960 1881.960 119.460 1804.460Q40.960 1726.960 40.960 1572.960L40.960 350.960Q40.960 195.960 119.460 118.460Q197.960 40.960 354.960 40.960L2084.960 40.960Q2242.960 40.960 2320.960 118.960Q2398.960 196.960 2398.960 350.960L2398.960 1572.960Q2398.960 1726.960 2320.960 1804.460Q2242.960 1881.960 2084.960 1881.960L354.960 1881.960Z";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn penpot_reference_layout_matches_393_by_852() {
        let layout = PlayerLayout::resolve(393.0, 852.0);
        assert_eq!(layout.header_top, 80.0);
        assert_eq!(layout.body_top, 172.0);
        assert_eq!(layout.progress_top, 560.0);
        assert_eq!(layout.transport_center_y, 691.0);
        assert_eq!(layout.nav_center_y, 807.0);
        assert_eq!(layout.queue_metadata_top, 226.0);
        assert_eq!(layout.queue_content_top, 282.0);
        assert_eq!(layout.lyrics_content_bottom(), 292.0);
    }

    #[test]
    fn compressed_flex_rows_remain_ordered() {
        let layout = PlayerLayout::resolve(393.0, 400.0);
        assert!(layout.header_top <= layout.body_top);
        assert!(layout.body_top <= layout.progress_top);
        assert!(layout.progress_top <= layout.transport_center_y);
        assert!(layout.transport_center_y <= layout.nav_center_y);
        assert!(layout.nav_center_y <= layout.height);
    }

    #[test]
    fn mini_player_uses_horizontal_flex_slots() {
        let layout = PlayerLayout::resolve(393.0, 60.0);
        let mini = MiniPlayerLayout::resolve(layout);
        assert_eq!(mini.artwork, Rect::from_xywh(8.0, 8.0, 44.0, 44.0));
        assert_eq!(mini.text.left, 60.0);
        assert_eq!(mini.text.right, 283.0);
        assert_eq!(mini.play_center, Point::new(309.0, 30.0));
        assert_eq!(mini.next_center, Point::new(357.0, 30.0));
    }

    #[test]
    fn artwork_geometry_matches_penpot_reference() {
        let layout = PlayerLayout::resolve(393.0, 852.0);
        assert_eq!(layout.artwork_metadata_top, 464.0);
        let artwork = layout.full_artwork_rect();
        assert_eq!(artwork, Rect::from_xywh(24.0, 99.5, 345.0, 345.0));
    }

    #[test]
    fn flexible_body_absorbs_extra_height() {
        let short = PlayerLayout::resolve(393.0, 852.0);
        let tall = PlayerLayout::resolve(393.0, 932.0);
        assert_eq!(short.body_top, tall.body_top);
        assert_eq!(tall.progress_top - short.progress_top, 80.0);
    }

    #[test]
    fn transport_hit_targets_follow_visual_order() {
        let layout = PlayerLayout::resolve(393.0, 852.0);
        let ui = PlayerUiLayout::resolve(
            layout,
            PlayerScreenInput::Lyrics,
            PlayerPresentationInput::Full,
        );
        let mut state = PlayerInteractionState::default();
        assert_eq!(
            state.press(&ui, 101.5, 691.0),
            PlayerButton::Previous as i32
        );
        state.cancel();
        assert_eq!(
            state.press(&ui, 196.5, 691.0),
            PlayerButton::PlayPause as i32
        );
        state.cancel();
        assert_eq!(state.press(&ui, 291.5, 691.0), PlayerButton::Next as i32);
    }

    #[test]
    fn mode_button_press_animates_from_plain_icon_to_inverse_fill() {
        let now = Instant::now();
        let mut state = PlayerInteractionState {
            pointer_hit: Some(PlayerHit::Control(PlayerButton::Lyrics)),
            pressed: Some(PlayerButton::Lyrics),
            pressed_at: Some(now - std::time::Duration::from_millis(80)),
            released: None,
            last_activity_at: now,
        };
        let pressed = state.animation_for(PlayerButton::Lyrics, now);
        assert!(pressed.press > 0.999);
        assert!((pressed.scale - 0.9).abs() < 0.001);

        let idle = state.animation_for(PlayerButton::Queue, now);
        assert_eq!(idle.press, 0.0);
        assert_eq!(idle.scale, 1.0);
    }

    #[test]
    fn lyrics_and_queue_bottom_chrome_hide_after_three_seconds() {
        let layout = PlayerLayout::resolve(393.0, 852.0);
        let now = Instant::now();
        let state = PlayerInteractionState {
            last_activity_at: now - std::time::Duration::from_millis(3_180),
            ..PlayerInteractionState::default()
        };
        let lyrics = state.bottom_chrome_sample(PlayerScreenInput::Lyrics, now);
        assert!(lyrics.visibility > 0.0 && lyrics.visibility < 1.0);
        assert!(lyrics.content_bottom(layout) < layout.lyrics_content_bottom());

        let hidden = state.bottom_chrome_sample(
            PlayerScreenInput::Queue,
            now + std::time::Duration::from_secs(1),
        );
        assert_eq!(hidden.visibility, 0.0);
        assert!(!hidden.active);

        let artwork = state.bottom_chrome_sample(PlayerScreenInput::Artwork, now);
        assert_eq!(artwork.visibility, 1.0);
    }

    #[test]
    fn page_transition_shrinks_out_and_grows_in() {
        let start = PlayerTransitionSample {
            from: PlayerScreenInput::Lyrics,
            to: PlayerScreenInput::Artwork,
            progress: 0.0,
            active: true,
        };
        assert_eq!(
            start.content_transform(PlayerScreenInput::Lyrics),
            (1.0, 1.0)
        );
        assert_eq!(
            start.content_transform(PlayerScreenInput::Artwork),
            (0.0, 0.94)
        );
        assert_eq!(start.artwork_progress(), 0.0);

        let end = PlayerTransitionSample::settled(PlayerScreenInput::Artwork);
        assert_eq!(
            end.content_transform(PlayerScreenInput::Lyrics),
            (0.0, 0.94)
        );
        assert_eq!(
            end.content_transform(PlayerScreenInput::Artwork),
            (1.0, 1.0)
        );
        assert_eq!(end.artwork_progress(), 1.0);

        let artwork_to_queue = PlayerTransitionSample {
            from: PlayerScreenInput::Artwork,
            to: PlayerScreenInput::Queue,
            progress: 0.5,
            active: true,
        };
        assert!(artwork_to_queue.artwork_progress() > 0.0);
        assert!(artwork_to_queue.artwork_progress() < 1.0);
    }

    #[test]
    fn lyrics_and_queue_share_the_compact_metadata_header() {
        let lyrics_to_queue = PlayerTransitionSample {
            from: PlayerScreenInput::Lyrics,
            to: PlayerScreenInput::Queue,
            progress: 0.5,
            active: true,
        };
        assert!(lyrics_to_queue.shares_compact_header());
        assert!(PlayerTransitionSample::settled(PlayerScreenInput::Lyrics).shares_compact_header());
        assert!(PlayerTransitionSample::settled(PlayerScreenInput::Queue).shares_compact_header());
        assert!(
            !PlayerTransitionSample::settled(PlayerScreenInput::Artwork).shares_compact_header()
        );
    }

    #[test]
    fn rebuilding_the_same_target_does_not_cancel_an_active_transition() {
        let mut transition = PlayerTransitionState::default();
        transition.set_target(
            Some(PlayerScreenInput::Lyrics),
            Some(PlayerScreenInput::Artwork),
        );
        let started_at = transition.started_at;

        transition.set_target(
            Some(PlayerScreenInput::Artwork),
            Some(PlayerScreenInput::Artwork),
        );

        assert_eq!(transition.started_at, started_at);
        assert_eq!(transition.from, PlayerScreenInput::Lyrics);
        assert_eq!(transition.to, PlayerScreenInput::Artwork);
    }

    #[test]
    fn queue_filter_hit_targets_only_exist_on_queue_screen() {
        let layout = PlayerLayout::resolve(393.0, 852.0);
        let mut state = PlayerInteractionState::default();
        let lyrics_ui = PlayerUiLayout::resolve(
            layout,
            PlayerScreenInput::Lyrics,
            PlayerPresentationInput::Full,
        );
        let queue_ui = PlayerUiLayout::resolve(
            layout,
            PlayerScreenInput::Queue,
            PlayerPresentationInput::Full,
        );
        assert_eq!(state.press(&lyrics_ui, 153.0, 199.0), 0);
        assert_eq!(
            state.press(&queue_ui, 153.0, 199.0),
            PlayerButton::QueueShuffle as i32
        );
    }

    #[test]
    fn queue_ui_hit_test_matches_every_resolved_control_center() {
        let layout = PlayerLayout::resolve(393.0, 852.0);
        let ui = PlayerUiLayout::resolve(
            layout,
            PlayerScreenInput::Queue,
            PlayerPresentationInput::Full,
        );
        let visible = BottomChromeSample {
            visibility: 1.0,
            active: false,
        };
        assert_eq!(ui.controls.len(), 12);
        for control in &ui.controls {
            assert_eq!(
                ui.hit_test(control.center, visible),
                Some(PlayerHit::Control(control.button)),
                "center must hit {:?}",
                control.button,
            );
        }
    }

    #[test]
    fn scroll_regions_share_the_player_hit_test_and_controls_take_precedence() {
        let layout = PlayerLayout::resolve(393.0, 852.0);
        let visible = BottomChromeSample {
            visibility: 1.0,
            active: false,
        };
        let lyrics = PlayerUiLayout::resolve(
            layout,
            PlayerScreenInput::Lyrics,
            PlayerPresentationInput::Full,
        );
        assert_eq!(
            lyrics.hit_test(Point::new(196.5, 300.0), visible),
            Some(PlayerHit::Scroll),
        );
        assert_eq!(
            lyrics.hit_test(lyrics.control(PlayerButton::PlayPause).center, visible),
            Some(PlayerHit::Control(PlayerButton::PlayPause)),
        );

        let artwork = PlayerUiLayout::resolve(
            layout,
            PlayerScreenInput::Artwork,
            PlayerPresentationInput::Full,
        );
        assert_eq!(artwork.hit_test(Point::new(196.5, 300.0), visible), None);
    }

    #[test]
    fn mini_player_hit_test_exposes_open_behind_transport_controls() {
        let layout = PlayerLayout::resolve(393.0, 60.0);
        let ui = PlayerUiLayout::resolve(
            layout,
            PlayerScreenInput::Artwork,
            PlayerPresentationInput::Mini,
        );
        let visible = BottomChromeSample {
            visibility: 1.0,
            active: false,
        };
        assert_eq!(
            ui.hit_test(Point::new(120.0, 30.0), visible),
            Some(PlayerHit::Control(PlayerButton::Open)),
        );
        assert_eq!(
            ui.hit_test(ui.control(PlayerButton::PlayPause).center, visible),
            Some(PlayerHit::Control(PlayerButton::PlayPause)),
        );
    }

    #[test]
    fn duration_labels_match_design_format() {
        assert_eq!(format_duration(47_500), "0:47");
        assert_eq!(format_duration(243_999), "4:03");
    }

    #[test]
    fn player_wire_deserializes_lyrics_screen() {
        let player: PlayerInput = serde_json::from_str(
            r#"{"screen":"lyrics","title":"Jupiter","artist":"Coldplay","durationMs":243000,"isPlaying":true,"liked":false}"#,
        )
        .unwrap();
        assert_eq!(player.screen, PlayerScreenInput::Lyrics);
        assert_eq!(player.duration_ms, 243_000);
        assert!(player.is_playing);
    }

    #[test]
    fn player_wire_deserializes_artwork_screen() {
        let player: PlayerInput =
            serde_json::from_str(r#"{"screen":"artwork","title":"Jupiter","artist":"Coldplay"}"#)
                .unwrap();
        assert_eq!(player.screen, PlayerScreenInput::Artwork);
    }

    #[test]
    fn player_wire_deserializes_mini_viewport() {
        let player: PlayerInput = serde_json::from_str(
            r#"{"presentation":"mini","viewportWidth":361.0,"viewportHeight":60.0,"title":"Jupiter","artist":"Coldplay"}"#,
        )
        .unwrap();
        assert_eq!(player.presentation, PlayerPresentationInput::Mini);
        assert_eq!(player.viewport_width, Some(361.0));
        assert_eq!(player.viewport_height, Some(60.0));
    }

    #[test]
    fn player_wire_defaults_to_artwork_when_screen_omitted() {
        let player: PlayerInput =
            serde_json::from_str(r#"{"title":"Jupiter","artist":"Coldplay"}"#).unwrap();
        assert_eq!(player.screen, PlayerScreenInput::Artwork);
    }

    #[test]
    fn player_wire_deserializes_queue_screen_and_items() {
        let player: PlayerInput = serde_json::from_str(
            r#"{"screen":"queue","title":"Jupiter","artist":"Coldplay","queueTitle":"Up Next","queueSource":"From Jupiter","queueFilter":"repeatOne","queueItems":[{"title":"Moon Music","artist":"Coldplay","artworkKey":"content://cover/1"}]}"#,
        )
        .unwrap();
        assert_eq!(player.screen, PlayerScreenInput::Queue);
        assert_eq!(player.queue_filter, QueueFilterInput::RepeatOne);
        assert_eq!(player.queue_items.len(), 1);
        assert_eq!(player.queue_items[0].title, "Moon Music");
        assert_eq!(player.queue_items[0].artwork_key, "content://cover/1");
    }

    #[test]
    fn queue_reorder_mapping_moves_one_row_and_closes_the_gap() {
        let down = QueueReorderSample {
            active: true,
            from: 1,
            to: 3,
            pointer_y: 0.0,
        };
        assert_eq!(
            (0..5)
                .map(|index| reordered_item_index(index, down))
                .collect::<Vec<_>>(),
            vec![0, 2, 3, 1, 4],
        );
        let up = QueueReorderSample {
            active: true,
            from: 3,
            to: 1,
            pointer_y: 0.0,
        };
        assert_eq!(
            (0..5)
                .map(|index| reordered_item_index(index, up))
                .collect::<Vec<_>>(),
            vec![0, 3, 1, 2, 4],
        );
    }
}
