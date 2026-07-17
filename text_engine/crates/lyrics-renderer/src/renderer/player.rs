//! Native portrait player chrome.
//!
//! Geometry is resolved from the 393×852 Penpot reference as a vertical flex
//! stack. Fixed rows scale with the surface width; the lyrics/artwork/queue body
//! receives the remaining height. All interaction and press feedback lives in
//! Rust so a host only forwards pointer events and consumes action codes.

use super::*;
use skia_safe::{
    canvas::SaveLayerRec, ClipOp, Color4f, Contains, Font, Image, Paint, Path, PathBuilder, Point,
    Rect, SamplingOptions,
};

const DESIGN_WIDTH: f32 = 393.0;
const TOP_INSET: f32 = 44.0;
const HANDLE_ROW: f32 = 36.0;
const COMPACT_HEADER: f32 = 92.0;
const PROGRESS_ROW: f32 = 60.0;
const TRANSPORT_ROW: f32 = 142.0;
const MODE_NAV_ROW: f32 = 90.0;

const WHITE: Color4f = Color4f::new(1.0, 1.0, 1.0, 1.0);
const ACTIVE_BG: Color4f = Color4f::new(0.949, 0.608, 0.761, 1.0);
const ACTIVE_FG: Color4f = Color4f::new(0.722, 0.169, 0.353, 1.0);
const INACTIVE_BG: Color4f = Color4f::new(0.847, 0.275, 0.451, 0.72);

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PlayerScreenInput {
    #[default]
    Lyrics,
    Artwork,
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
            (PlayerScreenInput::Lyrics, PlayerScreenInput::Artwork) => progress,
            (PlayerScreenInput::Artwork, PlayerScreenInput::Lyrics) => 1.0 - progress,
            (_, PlayerScreenInput::Artwork) => 1.0,
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
            current: PlayerScreenInput::Lyrics,
            from: PlayerScreenInput::Lyrics,
            to: PlayerScreenInput::Lyrics,
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
        let progress =
            ((now - started_at).as_secs_f32() / SCREEN_TRANSITION_SECONDS).clamp(0.0, 1.0);
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
}

const BUTTONS: [PlayerButton; 8] = [
    PlayerButton::Favorite,
    PlayerButton::More,
    PlayerButton::Previous,
    PlayerButton::PlayPause,
    PlayerButton::Next,
    PlayerButton::Lyrics,
    PlayerButton::Output,
    PlayerButton::Queue,
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
            layout
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
                .map_or(1.0, |at| (now - at).as_secs_f32() / 0.08)
                .clamp(0.0, 1.0);
            return (1.0 - 0.1 * ease_out_cubic(elapsed), elapsed < 1.0);
        }
        if let Some((released, at, from)) = self.released {
            if released == button {
                let elapsed = ((now - at).as_secs_f32() / 0.18).clamp(0.0, 1.0);
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
            path: Path::from_svg(data).expect("embedded SF Symbol path must parse"),
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

    // Empty system inset, then the Penpot drag handle row.
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(Color4f::new(1.0, 0.545, 0.769, 0.85), None);
    canvas.draw_round_rect(
        Rect::from_xywh(166.5 * scale, 59.5 * scale, 60.0 * scale, 5.0 * scale),
        2.5 * scale,
        2.5 * scale,
        &paint,
    );

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

    // One image participates in the transition. Its geometry is interpolated;
    // compact and full cover copies are never cross-faded over one another.
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
    draw_prepared_text_skia(
        canvas,
        typefaces,
        &player.title,
        116.0 * s,
        102.5 * s,
        (255, 255, 255, 255),
        1.0,
        0.0,
        None,
    );
    draw_prepared_text_skia(
        canvas,
        typefaces,
        &player.artist,
        116.0 * s,
        128.5 * s,
        (255, 139, 196, 255),
        0.85,
        0.0,
        None,
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
    draw_prepared_text_skia(
        canvas,
        typefaces,
        &player.artwork_title,
        32.0 * scale,
        top + 24.0 * scale,
        (255, 255, 255, 255),
        1.0,
        0.0,
        None,
    );
    draw_prepared_text_skia(
        canvas,
        typefaces,
        &player.artwork_artist,
        32.0 * scale,
        top + 51.0 * scale,
        (255, 139, 196, 255),
        0.78,
        0.0,
        None,
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
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(Color4f::new(1.0, 1.0, 1.0, 0.28), None);
    canvas.draw_round_rect(
        Rect::from_xywh(left, top, width, 4.0 * s),
        2.0 * s,
        2.0 * s,
        &paint,
    );
    paint.set_color4f(Color4f::new(1.0, 1.0, 1.0, 0.92), None);
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
        0.65,
        false,
    );
    draw_runtime_label(
        canvas,
        typefaces,
        &remaining,
        left + width,
        l.progress_top + 28.0 * s,
        11.0 * s,
        0.65,
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
    for (button, icon, x, screen, icon_size) in [
        (
            PlayerButton::Lyrics,
            &player.icons.lyrics,
            80.0,
            Some(PlayerScreenInput::Lyrics),
            (18.0, 18.0),
        ),
        (
            PlayerButton::Output,
            &player.icons.lyrics,
            196.5,
            Some(PlayerScreenInput::Artwork),
            (18.0, 18.0),
        ),
        (
            PlayerButton::Queue,
            &player.icons.list,
            313.0,
            None,
            (18.0, 15.0),
        ),
    ] {
        let active = screen == Some(selected_screen);
        let (button_scale, is_animating) = interaction.scale_for(button, now);
        *animating |= is_animating;
        canvas.save();
        canvas.translate((x * s, y));
        canvas.scale((button_scale, button_scale));
        canvas.translate((-x * s, -y));
        let rect = Rect::from_xywh((x - 16.0) * s, y - 16.0 * s, 32.0 * s, 32.0 * s);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(if active { ACTIVE_BG } else { INACTIVE_BG }, None);
        canvas.draw_round_rect(rect, 16.0 * s, 16.0 * s, &paint);
        if button != PlayerButton::Output {
            draw_icon(
                canvas,
                icon,
                Point::new(x * s, y),
                icon_size.0 * s,
                icon_size.1 * s,
                if active { ACTIVE_FG } else { WHITE },
                1.0,
            );
        }
        canvas.restore();
    }
    // AirPlay/audio output icon: three radio arcs and the lower triangle. It is
    // drawn as Skia primitives so it stays crisp at every render scale.
    let (output_scale, _) = interaction.scale_for(PlayerButton::Output, now);
    draw_airplay_overlay(
        canvas,
        Point::new(196.5 * s, y),
        18.0 * s * output_scale,
        if selected_screen == PlayerScreenInput::Artwork {
            ACTIVE_FG
        } else {
            WHITE
        },
    );
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
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(INACTIVE_BG, None);
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
        WHITE,
        if filled { 1.0 } else { 1.0 },
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
        paint.set_color4f(Color4f::new(0.8, 0.1, 0.45, 1.0), None);
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
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
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
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
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
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
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
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
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
}
