//! Native portrait player chrome.
//!
//! Geometry is resolved from the 393×852 Penpot reference as a vertical flex
//! stack. Fixed rows scale with the surface width; the lyrics/artwork/queue body
//! receives the remaining height. All interaction and press feedback lives in
//! Rust so a host only forwards pointer events and consumes action codes.

use super::*;
use skia_safe::{
    canvas::SaveLayerRec, BlendMode, ClipOp, Color4f, Contains, Font, Image, Paint, Path,
    PathBuilder, Point, Rect, SamplingOptions,
};

const DESIGN_WIDTH: f32 = 393.0;
const TOP_INSET: f32 = 44.0;
const HANDLE_ROW: f32 = 36.0;
const COMPACT_HEADER: f32 = 92.0;
const PROGRESS_ROW: f32 = 60.0;
const TRANSPORT_ROW: f32 = 142.0;
const MODE_NAV_ROW: f32 = 90.0;

// Player chrome is pure white + Plus over the mesh (not the mock's pink fills).
// Alphas sampled from design exports (status bar ignored — system-drawn):
//   - title / progress played: solid #FFFFFF → 1.0
//   - secondary text (artist, time labels): ~0.60 effective opacity
//   - mode/filter chips: unselected ~0.40, selected ~0.60
//   - progress track: ~0.50
//   - drag handle: ~0.40
const WHITE: Color4f = Color4f::new(1.0, 1.0, 1.0, 1.0);
const WHITE_HANDLE: Color4f = Color4f::new(1.0, 1.0, 1.0, 0.40);
const WHITE_BTN: Color4f = Color4f::new(1.0, 1.0, 1.0, 0.40);
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
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct PlayerInput {
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
}

#[derive(Debug)]
pub(super) struct PreparedPlayer {
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
    pub layout: PlayerLayout,
    icons: PlayerIcons,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PlayerLayout {
    pub scale: f32,
    pub width: f32,
    pub height: f32,
    pub header_top: f32,
    pub body_top: f32,
    pub progress_top: f32,
    pub transport_top: f32,
    pub nav_top: f32,
    pub artwork_metadata_top: f32,
}

impl PlayerLayout {
    pub(super) fn resolve(width: f32, height: f32) -> Self {
        let scale = (width / DESIGN_WIDTH).max(0.25);
        let header_top = (TOP_INSET + HANDLE_ROW) * scale;
        let body_top = header_top + COMPACT_HEADER * scale;
        let nav_top = (height - MODE_NAV_ROW * scale).max(body_top);
        let transport_top = (nav_top - TRANSPORT_ROW * scale).max(body_top);
        let progress_top = (transport_top - PROGRESS_ROW * scale).max(body_top);
        let artwork_metadata_top = (progress_top - 96.0 * scale).max(header_top);
        Self {
            scale,
            width,
            height,
            header_top,
            body_top,
            progress_top,
            transport_top,
            nav_top,
            artwork_metadata_top,
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

    fn bottom_rect(self, x: f32, top: f32, width: f32, height: f32) -> Rect {
        Rect::from_xywh(x * self.scale, top, width * self.scale, height * self.scale)
    }

    fn button_rect(self, button: PlayerButton, screen: PlayerScreenInput) -> Rect {
        match button {
            PlayerButton::Favorite => {
                let top = if screen == PlayerScreenInput::Artwork {
                    self.artwork_metadata_top + 24.0 * self.scale
                } else {
                    102.0 * self.scale
                };
                self.bottom_rect(277.0, top, 48.0, 48.0)
            }
            PlayerButton::More => {
                let top = if screen == PlayerScreenInput::Artwork {
                    self.artwork_metadata_top + 24.0 * self.scale
                } else {
                    102.0 * self.scale
                };
                self.bottom_rect(321.0, top, 48.0, 48.0)
            }
            PlayerButton::Previous => {
                self.bottom_rect(69.5, self.transport_top + 35.0 * self.scale, 64.0, 72.0)
            }
            PlayerButton::PlayPause => {
                self.bottom_rect(164.5, self.transport_top + 35.0 * self.scale, 64.0, 72.0)
            }
            PlayerButton::Next => {
                self.bottom_rect(259.5, self.transport_top + 35.0 * self.scale, 64.0, 72.0)
            }
            PlayerButton::Lyrics => {
                self.bottom_rect(48.0, self.nav_top + 13.0 * self.scale, 64.0, 56.0)
            }
            PlayerButton::Output => {
                self.bottom_rect(164.5, self.nav_top + 13.0 * self.scale, 64.0, 56.0)
            }
            PlayerButton::Queue => {
                self.bottom_rect(281.0, self.nav_top + 13.0 * self.scale, 64.0, 56.0)
            }
            PlayerButton::QueueUpNext => self.rect(32.0, 180.0, 72.0, 38.0),
            PlayerButton::QueueShuffle => self.rect(117.0, 180.0, 72.0, 38.0),
            PlayerButton::QueueRepeatOne => self.rect(202.0, 180.0, 72.0, 38.0),
            PlayerButton::QueueAlbum => self.rect(287.0, 180.0, 72.0, 38.0),
        }
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
        let progress = (now
            .saturating_duration_since(started_at)
            .as_secs_f32()
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
}

impl PlayerButton {
    fn is_queue_filter(self) -> bool {
        matches!(
            self,
            Self::QueueUpNext | Self::QueueShuffle | Self::QueueRepeatOne | Self::QueueAlbum
        )
    }
}

const BUTTONS: [PlayerButton; 12] = [
    PlayerButton::Favorite,
    PlayerButton::More,
    PlayerButton::Previous,
    PlayerButton::PlayPause,
    PlayerButton::Next,
    PlayerButton::Lyrics,
    PlayerButton::Output,
    PlayerButton::Queue,
    PlayerButton::QueueUpNext,
    PlayerButton::QueueShuffle,
    PlayerButton::QueueRepeatOne,
    PlayerButton::QueueAlbum,
];

#[derive(Debug, Default)]
pub(super) struct PlayerInteractionState {
    pressed: Option<PlayerButton>,
    pressed_at: Option<Instant>,
    released: Option<(PlayerButton, Instant, f32)>,
}

impl PlayerInteractionState {
    pub(super) fn press(
        &mut self,
        layout: PlayerLayout,
        screen: PlayerScreenInput,
        x: f32,
        y: f32,
    ) -> i32 {
        let hit = BUTTONS.into_iter().find(|button| {
            (!button.is_queue_filter() || screen == PlayerScreenInput::Queue)
                && layout
                    .button_rect(*button, screen)
                    .contains(Point::new(x, y))
        });
        self.pressed = hit;
        self.pressed_at = hit.map(|_| Instant::now());
        self.released = None;
        hit.map_or(0, |button| button as i32)
    }

    pub(super) fn release(
        &mut self,
        layout: PlayerLayout,
        screen: PlayerScreenInput,
        x: f32,
        y: f32,
    ) -> i32 {
        let Some(button) = self.pressed else {
            return 0;
        };
        let accepted = layout
            .button_rect(button, screen)
            .contains(Point::new(x, y));
        let scale = self.scale_for(button, Instant::now()).0;
        self.pressed = None;
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
            let scale = self.scale_for(button, Instant::now()).0;
            self.pressed = None;
            self.released = Some((button, Instant::now(), scale));
        }
        self.pressed_at = None;
    }

    fn scale_for(&mut self, button: PlayerButton, now: Instant) -> (f32, bool) {
        if self.pressed == Some(button) {
            let elapsed = self
                .pressed_at
                .map_or(1.0, |at| {
                    now.saturating_duration_since(at).as_secs_f32() / 0.08
                })
                .clamp(0.0, 1.0);
            return (1.0 - 0.1 * ease_out_cubic(elapsed), elapsed < 1.0);
        }
        if let Some((released, at, from)) = self.released {
            if released == button {
                let elapsed = (now.saturating_duration_since(at).as_secs_f32() / 0.18)
                    .clamp(0.0, 1.0);
                if elapsed >= 1.0 {
                    self.released = None;
                    return (1.0, false);
                }
                return (from + (1.0 - from) * ease_out_back(elapsed), true);
            }
        }
        (1.0, false)
    }
}

#[derive(Debug)]
struct PlayerIcons {
    star: SvgIcon,
    ellipsis: SvgIcon,
    previous: SvgIcon,
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
}

impl SvgIcon {
    fn new(data: &str, view_width: f32, view_height: f32) -> Self {
        Self {
            // Never panic on icon parse: a missing glyph is preferable to
            // taking down the player when a scene is rebuilt at track change.
            path: Path::from_svg(data).unwrap_or_default(),
            view_width,
            view_height,
        }
    }
}

impl PlayerIcons {
    fn new() -> Self {
        Self {
            star: SvgIcon::new(STAR_PATH, 2316.92, 2209.92),
            ellipsis: SvgIcon::new(ELLIPSIS_PATH, 1947.92, 460.92),
            previous: SvgIcon::new(PREVIOUS_PATH, 1931.92, 1609.92),
            pause: SvgIcon::new(PAUSE_PATH, 1301.92, 1735.92),
            next: SvgIcon::new(NEXT_PATH, 1931.92, 1609.92),
            lyrics: SvgIcon::new(LYRICS_PATH, 2285.92, 2156.92),
            list: SvgIcon::new(LIST_PATH, 2096.92, 1542.92),
            shuffle: SvgIcon::new(SHUFFLE_PATH, 2379.92, 1893.92),
            repeat_one: SvgIcon::new(REPEAT_ONE_PATH, 2220.92, 1889.92),
            album: SvgIcon::new(ALBUM_PATH, 2439.92, 1922.92),
        }
    }
}

impl LyricsRenderer {
    pub(super) fn prepare_player(
        &mut self,
        input: Option<&PlayerInput>,
        width: f32,
        height: f32,
    ) -> Option<PreparedPlayer> {
        let input = input?;
        let layout = PlayerLayout::resolve(width, height);
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
            });
        }
        self.text_attrs = saved;
        Some(PreparedPlayer {
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
            layout,
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
    player: &PreparedPlayer,
    interaction: &mut PlayerInteractionState,
    transition: PlayerTransitionSample,
    current_time_ms: i32,
) -> bool {
    let layout = player.layout;
    let scale = layout.scale;
    let now = Instant::now();
    let mut animating = false;

    // Drag handle — soft white pill (Plus).
    {
        let mut paint = plus_paint();
        paint.set_color4f(WHITE_HANDLE, None);
        canvas.draw_round_rect(
            Rect::from_xywh(166.5 * scale, 59.5 * scale, 60.0 * scale, 5.0 * scale),
            2.5 * scale,
            2.5 * scale,
            &paint,
        );
    }

    let page_bounds = Rect::from_xywh(
        0.0,
        layout.header_top,
        layout.width,
        layout.progress_top - layout.header_top,
    );
    let (lyrics_alpha, lyrics_scale) = transition.content_transform(PlayerScreenInput::Lyrics);
    if lyrics_alpha > 0.001 {
        draw_content_layer(canvas, page_bounds, lyrics_alpha, lyrics_scale, |canvas| {
            draw_compact_header(canvas, typefaces, player, interaction, now, &mut animating);
        });
    }
    let (artwork_alpha, artwork_scale) = transition.content_transform(PlayerScreenInput::Artwork);
    if artwork_alpha > 0.001 {
        draw_content_layer(
            canvas,
            page_bounds,
            artwork_alpha,
            artwork_scale,
            |canvas| {
                draw_artwork_metadata(canvas, typefaces, player, interaction, now, &mut animating);
            },
        );
    }
    let (queue_alpha, queue_scale) = transition.content_transform(PlayerScreenInput::Queue);
    if queue_alpha > 0.001 {
        draw_content_layer(canvas, page_bounds, queue_alpha, queue_scale, |canvas| {
            draw_queue_page(
                canvas,
                typefaces,
                thumbnail,
                player,
                interaction,
                now,
                &mut animating,
            );
        });
    }

    // One image participates in the transition. Its geometry is interpolated;
    // compact and full cover copies are never cross-faded over one another.
    // Normal blend (not Plus) so the cover keeps true colour.
    let artwork_progress = transition.artwork_progress();
    let shared_art = lerp_rect(
        layout.compact_artwork_rect(),
        layout.full_artwork_rect(),
        artwork_progress,
    );
    let radius = lerp(12.0 * scale, 18.0 * scale, artwork_progress);
    draw_artwork(canvas, thumbnail, shared_art, radius);

    draw_progress(canvas, typefaces, player, current_time_ms);
    draw_transport(canvas, player, interaction, now, &mut animating);
    draw_mode_navigation(
        canvas,
        player,
        interaction,
        transition.to,
        now,
        &mut animating,
    );
    animating || transition.active
}

/// Anti-aliased paint with additive (Plus) blend for white chrome over the mesh.
fn plus_paint() -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_blend_mode(BlendMode::Plus);
    paint
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

fn draw_compact_header(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    player: &PreparedPlayer,
    interaction: &mut PlayerInteractionState,
    now: Instant,
    animating: &mut bool,
) {
    let s = player.layout.scale;
    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(116.0 * s, 90.0 * s, 157.0 * s, 72.0 * s),
        ClipOp::Intersect,
        true,
    );
    draw_plus_text(
        canvas,
        typefaces,
        &player.title,
        116.0 * s,
        102.5 * s,
        TEXT_PRIMARY_ALPHA,
    );
    draw_plus_text(
        canvas,
        typefaces,
        &player.artist,
        116.0 * s,
        128.5 * s,
        TEXT_SECONDARY_ALPHA,
    );
    canvas.restore();

    let (favorite_scale, favorite_animating) = interaction.scale_for(PlayerButton::Favorite, now);
    let (more_scale, more_animating) = interaction.scale_for(PlayerButton::More, now);
    *animating |= favorite_animating || more_animating;
    draw_action_button(
        canvas,
        &player.icons.star,
        Point::new(301.0 * s, 126.0 * s),
        32.0 * s,
        20.0 * s,
        favorite_scale,
        player.liked,
    );
    draw_action_button(
        canvas,
        &player.icons.ellipsis,
        Point::new(345.0 * s, 126.0 * s),
        32.0 * s,
        20.0 * s,
        more_scale,
        false,
    );
}

fn draw_artwork_metadata(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    player: &PreparedPlayer,
    interaction: &mut PlayerInteractionState,
    now: Instant,
    animating: &mut bool,
) {
    let layout = player.layout;
    let scale = layout.scale;
    let top = layout.artwork_metadata_top;
    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(32.0 * scale, top + 8.0 * scale, 241.0 * scale, 72.0 * scale),
        ClipOp::Intersect,
        true,
    );
    draw_plus_text(
        canvas,
        typefaces,
        &player.artwork_title,
        32.0 * scale,
        top + 24.0 * scale,
        TEXT_PRIMARY_ALPHA,
    );
    draw_plus_text(
        canvas,
        typefaces,
        &player.artwork_artist,
        32.0 * scale,
        top + 51.0 * scale,
        TEXT_SECONDARY_ALPHA,
    );
    canvas.restore();

    let (favorite_scale, favorite_animating) = interaction.scale_for(PlayerButton::Favorite, now);
    let (more_scale, more_animating) = interaction.scale_for(PlayerButton::More, now);
    *animating |= favorite_animating || more_animating;
    let center_y = top + 48.0 * scale;
    draw_action_button(
        canvas,
        &player.icons.star,
        Point::new(301.0 * scale, center_y),
        32.0 * scale,
        20.0 * scale,
        favorite_scale,
        player.liked,
    );
    draw_action_button(
        canvas,
        &player.icons.ellipsis,
        Point::new(345.0 * scale, center_y),
        32.0 * scale,
        20.0 * scale,
        more_scale,
        false,
    );
}

fn draw_queue_page(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    thumbnail: Option<&Image>,
    player: &PreparedPlayer,
    interaction: &mut PlayerInteractionState,
    now: Instant,
    animating: &mut bool,
) {
    draw_compact_header(canvas, typefaces, player, interaction, now, animating);

    let layout = player.layout;
    let scale = layout.scale;
    for (button, icon, filter, x, icon_size) in [
        (
            PlayerButton::QueueUpNext,
            &player.icons.list,
            QueueFilterInput::UpNext,
            68.0,
            (18.0, 14.0),
        ),
        (
            PlayerButton::QueueShuffle,
            &player.icons.shuffle,
            QueueFilterInput::Shuffle,
            153.0,
            (18.0, 14.0),
        ),
        (
            PlayerButton::QueueRepeatOne,
            &player.icons.repeat_one,
            QueueFilterInput::RepeatOne,
            238.0,
            (18.0, 15.0),
        ),
        (
            PlayerButton::QueueAlbum,
            &player.icons.album,
            QueueFilterInput::Album,
            323.0,
            (18.0, 15.0),
        ),
    ] {
        let (button_scale, is_animating) = interaction.scale_for(button, now);
        *animating |= is_animating;
        let center = Point::new(x * scale, 199.0 * scale);
        canvas.save();
        canvas.translate(center);
        canvas.scale((button_scale, button_scale));
        canvas.translate((-center.x, -center.y));
        let selected = player.queue_filter == filter;
        let mut paint = plus_paint();
        paint.set_color4f(if selected { WHITE_BTN_ACTIVE } else { WHITE_BTN }, None);
        canvas.draw_round_rect(
            Rect::from_xywh(
                (x - 36.0) * scale,
                180.0 * scale,
                72.0 * scale,
                38.0 * scale,
            ),
            19.0 * scale,
            19.0 * scale,
            &paint,
        );
        draw_icon(
            canvas,
            icon,
            center,
            icon_size.0 * scale,
            icon_size.1 * scale,
            if selected { WHITE_BTN_ACTIVE } else { WHITE_BTN },
            1.0,
        );
        canvas.restore();
    }

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(
            32.0 * scale,
            226.0 * scale,
            (layout.width - 64.0 * scale).max(1.0),
            (layout.progress_top - 226.0 * scale).max(1.0),
        ),
        ClipOp::Intersect,
        true,
    );
    draw_plus_text(
        canvas,
        typefaces,
        &player.queue_title,
        32.0 * scale,
        234.0 * scale,
        TEXT_PRIMARY_ALPHA,
    );
    draw_plus_text(
        canvas,
        typefaces,
        &player.queue_source,
        32.0 * scale,
        260.0 * scale,
        TEXT_SECONDARY_ALPHA,
    );

    for (index, item) in player.queue_items.iter().enumerate() {
        let top = (282.0 + index as f32 * 56.0) * scale;
        if top >= layout.progress_top {
            break;
        }
        draw_artwork(
            canvas,
            thumbnail,
            Rect::from_xywh(32.0 * scale, top + 4.0 * scale, 48.0 * scale, 48.0 * scale),
            8.0 * scale,
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
            TEXT_PRIMARY_ALPHA,
        );
        draw_plus_text(
            canvas,
            typefaces,
            &item.artist,
            92.0 * scale,
            top + 30.0 * scale,
            TEXT_SECONDARY_ALPHA,
        );
        canvas.restore();
        draw_reorder_handle(canvas, Point::new(349.0 * scale, top + 28.0 * scale), scale);
    }
    canvas.restore();
}

fn draw_reorder_handle(canvas: &skia_safe::Canvas, center: Point, scale: f32) {
    let mut paint = plus_paint();
    paint.set_color4f(WHITE_SECONDARY, None);
    for offset in [-4.0, 0.0, 4.0] {
        canvas.draw_round_rect(
            Rect::from_xywh(
                center.x - 7.0 * scale,
                center.y + offset * scale - scale,
                14.0 * scale,
                2.0 * scale,
            ),
            scale,
            scale,
            &paint,
        );
    }
}

fn draw_progress(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    player: &PreparedPlayer,
    current_time_ms: i32,
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
    let mut paint = plus_paint();
    paint.set_color4f(WHITE_TRACK, None);
    canvas.draw_round_rect(
        Rect::from_xywh(left, top, width, 4.0 * s),
        2.0 * s,
        2.0 * s,
        &paint,
    );
    paint.set_color4f(WHITE_FILL, None);
    canvas.draw_round_rect(
        Rect::from_xywh(left, top, width * ratio, 4.0 * s),
        2.0 * s,
        2.0 * s,
        &paint,
    );

    let elapsed = format_duration(current_time_ms.max(0));
    let remaining = format!(
        "−{}",
        format_duration((player.duration_ms - current_time_ms).max(0))
    );
    draw_runtime_label(
        canvas,
        typefaces,
        &elapsed,
        left,
        l.progress_top + 28.0 * s,
        11.0 * s,
        TEXT_SECONDARY_ALPHA,
        false,
    );
    draw_runtime_label(
        canvas,
        typefaces,
        &remaining,
        left + width,
        l.progress_top + 28.0 * s,
        11.0 * s,
        TEXT_SECONDARY_ALPHA,
        true,
    );
}

fn draw_transport(
    canvas: &skia_safe::Canvas,
    player: &PreparedPlayer,
    interaction: &mut PlayerInteractionState,
    now: Instant,
    animating: &mut bool,
) {
    let l = player.layout;
    let s = l.scale;
    let y = l.transport_top + 71.0 * s;
    for (button, icon, center, size) in [
        (
            PlayerButton::Previous,
            &player.icons.previous,
            Point::new(101.5 * s, y),
            (42.0 * s, 35.0 * s),
        ),
        (
            PlayerButton::Next,
            &player.icons.next,
            Point::new(291.5 * s, y),
            (42.0 * s, 35.0 * s),
        ),
    ] {
        let (button_scale, active) = interaction.scale_for(button, now);
        *animating |= active;
        draw_icon(
            canvas,
            icon,
            center,
            size.0 * button_scale,
            size.1 * button_scale,
            WHITE,
            1.0,
        );
    }
    let (play_scale, active) = interaction.scale_for(PlayerButton::PlayPause, now);
    *animating |= active;
    if player.is_playing {
        draw_icon(
            canvas,
            &player.icons.pause,
            Point::new(196.5 * s, y),
            32.0 * s * play_scale,
            42.0 * s * play_scale,
            WHITE,
            1.0,
        );
    } else {
        draw_play_icon(canvas, Point::new(196.5 * s, y), 42.0 * s * play_scale);
    }
}

fn draw_mode_navigation(
    canvas: &skia_safe::Canvas,
    player: &PreparedPlayer,
    interaction: &mut PlayerInteractionState,
    selected_screen: PlayerScreenInput,
    now: Instant,
    animating: &mut bool,
) {
    let l = player.layout;
    let s = l.scale;
    let y = l.nav_top + 45.0 * s;
    // Only Lyrics and Queue carry a selected chip. Artwork is the resting page
    // with every chip inactive; Output is never a selected destination.
    for (button, icon, x, active_on, icon_size) in [
        (
            PlayerButton::Lyrics,
            &player.icons.lyrics,
            80.0,
            PlayerScreenInput::Lyrics,
            (18.0, 18.0),
        ),
        (
            PlayerButton::Queue,
            &player.icons.list,
            313.0,
            PlayerScreenInput::Queue,
            (18.0, 15.0),
        ),
    ] {
        let active = selected_screen == active_on;
        let (button_scale, is_animating) = interaction.scale_for(button, now);
        *animating |= is_animating;
        canvas.save();
        canvas.translate((x * s, y));
        canvas.scale((button_scale, button_scale));
        canvas.translate((-x * s, -y));
        let rect = Rect::from_xywh((x - 16.0) * s, y - 16.0 * s, 32.0 * s, 32.0 * s);
        let mut paint = plus_paint();
        paint.set_color4f(if active { WHITE_BTN_ACTIVE } else { WHITE_BTN }, None);
        canvas.draw_round_rect(rect, 16.0 * s, 16.0 * s, &paint);
        draw_icon(
            canvas,
            icon,
            Point::new(x * s, y),
            icon_size.0 * s,
            icon_size.1 * s,
            if active { WHITE_BTN_ACTIVE } else { WHITE_BTN },
            1.0,
        );
        canvas.restore();
    }

    // Output: press scale only — never active/selected styling (always 0.4).
    let output_x = 196.5;
    let (output_scale, output_animating) = interaction.scale_for(PlayerButton::Output, now);
    *animating |= output_animating;
    canvas.save();
    canvas.translate((output_x * s, y));
    canvas.scale((output_scale, output_scale));
    canvas.translate((-output_x * s, -y));
    let output_rect = Rect::from_xywh((output_x - 16.0) * s, y - 16.0 * s, 32.0 * s, 32.0 * s);
    let mut output_paint = plus_paint();
    output_paint.set_color4f(WHITE_BTN, None);
    canvas.draw_round_rect(output_rect, 16.0 * s, 16.0 * s, &output_paint);
    // AirPlay/audio output icon: three radio arcs and the lower triangle. It is
    // drawn as Skia primitives so it stays crisp at every render scale.
    draw_airplay_overlay(canvas, Point::new(output_x * s, y), 18.0 * s, WHITE_BTN);
    canvas.restore();
}

fn draw_content_layer(
    canvas: &skia_safe::Canvas,
    bounds: Rect,
    alpha: f32,
    scale: f32,
    draw: impl FnOnce(&skia_safe::Canvas),
) {
    let mut paint = Paint::default();
    paint.set_alpha_f(alpha.clamp(0.0, 1.0));
    canvas.save_layer(&SaveLayerRec::default().bounds(&bounds).paint(&paint));
    let center = Point::new(
        (bounds.left + bounds.right) * 0.5,
        (bounds.top + bounds.bottom) * 0.5,
    );
    canvas.translate(center);
    canvas.scale((scale, scale));
    canvas.translate((-center.x, -center.y));
    draw(canvas);
    canvas.restore();
}

fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress.clamp(0.0, 1.0)
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
) {
    canvas.save();
    canvas.translate(center);
    canvas.scale((scale, scale));
    canvas.translate((-center.x, -center.y));
    let mut paint = plus_paint();
    paint.set_color4f(if filled { WHITE_BTN_ACTIVE } else { WHITE_BTN }, None);
    canvas.draw_circle(center, diameter * 0.5, &paint);
    draw_icon(
        canvas,
        icon,
        center,
        icon_size,
        if icon.view_height < icon.view_width * 0.5 {
            icon_size * 0.3
        } else {
            icon_size
        },
        if filled { WHITE_BTN_ACTIVE } else { WHITE_BTN },
        1.0,
    );
    canvas.restore();
}

fn draw_artwork(canvas: &skia_safe::Canvas, thumbnail: Option<&Image>, rect: Rect, radius: f32) {
    let clip = crate::capsule::continuous_rounded_rect(rect, radius);
    canvas.save();
    canvas.clip_path(&clip, ClipOp::Intersect, true);
    if let Some(image) = thumbnail {
        let paint = Paint::default();
        canvas.draw_image_rect_with_sampling_options(
            image,
            None,
            rect,
            SamplingOptions::from(skia_safe::sampling_options::FilterMode::Linear),
            &paint,
        );
    } else {
        let mut paint = Paint::default();
        paint.set_color4f(ARTWORK_PLACEHOLDER, None);
        canvas.draw_rect(rect, &paint);
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
    let scale = (width / icon.view_width).min(height / icon.view_height);
    let draw_w = icon.view_width * scale;
    let draw_h = icon.view_height * scale;
    let mut paint = plus_paint();
    paint.set_color4f(
        Color4f::new(color.r, color.g, color.b, color.a * alpha),
        None,
    );
    canvas.save();
    canvas.translate((center.x - draw_w * 0.5, center.y - draw_h * 0.5));
    canvas.scale((scale, scale));
    canvas.draw_path(&icon.path, &paint);
    canvas.restore();
}

fn draw_airplay_overlay(canvas: &skia_safe::Canvas, center: Point, size: f32, color: Color4f) {
    let mut paint = plus_paint();
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width((size * 0.075).max(1.0));
    paint.set_stroke_cap(skia_safe::paint::Cap::Round);
    paint.set_color4f(color, None);
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

fn draw_play_icon(canvas: &skia_safe::Canvas, center: Point, size: f32) {
    let mut builder = PathBuilder::new();
    builder.move_to((center.x - size * 0.28, center.y - size * 0.43));
    builder.line_to((center.x + size * 0.43, center.y));
    builder.line_to((center.x - size * 0.28, center.y + size * 0.43));
    builder.close();
    let mut paint = plus_paint();
    paint.set_color4f(WHITE, None);
    canvas.draw_path(&builder.detach(), &paint);
}

fn draw_runtime_label(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    text: &str,
    x: f32,
    top: f32,
    size: f32,
    alpha: f32,
    right_aligned: bool,
) {
    let mut font = typefaces
        .values()
        .next()
        .cloned()
        .map(|typeface| Font::from_typeface(typeface, size))
        .unwrap_or_default();
    font.set_size(size);
    let mut paint = plus_paint();
    paint.set_color4f(Color4f::new(1.0, 1.0, 1.0, alpha), None);
    let measured = font.measure_str(text, Some(&paint)).0;
    let left = if right_aligned { x - measured } else { x };
    canvas.draw_str(text, (left, top + size), &font, &paint);
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

// Paths extracted from E:/interest/sf-pro-extracor/SF-Pro.ttf via sf_map.json.
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
        assert_eq!(layout.transport_top, 620.0);
        assert_eq!(layout.nav_top, 762.0);
        assert_eq!(layout.lyrics_content_bottom(), 292.0);
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
        let mut state = PlayerInteractionState::default();
        assert_eq!(
            state.press(layout, PlayerScreenInput::Lyrics, 101.5, 691.0),
            PlayerButton::Previous as i32
        );
        state.cancel();
        assert_eq!(
            state.press(layout, PlayerScreenInput::Lyrics, 196.5, 691.0),
            PlayerButton::PlayPause as i32
        );
        state.cancel();
        assert_eq!(
            state.press(layout, PlayerScreenInput::Lyrics, 291.5, 691.0),
            PlayerButton::Next as i32
        );
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
        assert_eq!(
            state.press(layout, PlayerScreenInput::Lyrics, 153.0, 199.0),
            0
        );
        assert_eq!(
            state.press(layout, PlayerScreenInput::Queue, 153.0, 199.0),
            PlayerButton::QueueShuffle as i32
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
    fn player_wire_defaults_to_artwork_when_screen_omitted() {
        let player: PlayerInput =
            serde_json::from_str(r#"{"title":"Jupiter","artist":"Coldplay"}"#).unwrap();
        assert_eq!(player.screen, PlayerScreenInput::Artwork);
    }

    #[test]
    fn player_wire_deserializes_queue_screen_and_items() {
        let player: PlayerInput = serde_json::from_str(
            r#"{"screen":"queue","title":"Jupiter","artist":"Coldplay","queueTitle":"Up Next","queueSource":"From Jupiter","queueFilter":"repeatOne","queueItems":[{"title":"Moon Music","artist":"Coldplay"}]}"#,
        )
        .unwrap();
        assert_eq!(player.screen, PlayerScreenInput::Queue);
        assert_eq!(player.queue_filter, QueueFilterInput::RepeatOne);
        assert_eq!(player.queue_items.len(), 1);
        assert_eq!(player.queue_items[0].title, "Moon Music");
    }
}


