#![cfg(not(target_os = "android"))]

#[cfg(windows)]
mod audio_capture;
mod frame_timing;
mod gpu;
#[cfg(windows)]
mod gpu_preference;

use frame_timing::{frame_timing_enabled, FrameSample, FrameTimingLogger};
use gpu::DesktopGpuRenderer;
use lyrics_parser::parser::{auto_parser::AutoParser, lyrics_parser::LyricsParser};
use lyrics_parser::SceneBuildParams;
use lyrics_renderer::TextEngine;
use skia_safe::{
    paint, Color, Color4f, Font, FontMgr, FontStyle, Paint, Point, Rect, TextBlob, Typeface,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tao::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use tao::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::{CursorIcon, Window, WindowBuilder};
use unicode_segmentation::UnicodeSegmentation;

const DEFAULT_WIDTH: u32 = 420;
const DEFAULT_HEIGHT: u32 = 520;
const SMTC_POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Used only when refresh-rate detection fails entirely.
const DEFAULT_REFRESH_HZ: f64 = 60.0;
const CLOCK_SEEK_SNAP_MS: f64 = 500.0;
const CLOCK_RECONCILE_MS: f64 = 350.0;
const CLOCK_MAX_RATE: f64 = 2.5;
const CLOCK_MAX_FRAME_MS: f64 = 64.0;
const APPLE_MUSIC_CLOCK_SAMPLE_COUNT: usize = 8;
const APPLE_MUSIC_SEEK_SNAP_MS: f64 = 1500.0;
const SEEK_ACK_TOLERANCE_MS: u32 = 1_000;
const SEEK_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Manual SMTC publishers are commonly updated on a ~5 s cadence. Once the
/// seek API accepts a request, allow three publication intervals for its
/// timeline snapshot to catch up without overwriting the optimistic clock.
const SEEK_ACCEPTED_ACK_TIMEOUT: Duration = Duration::from_secs(15);
/// Difference between the Windows (1601) and Unix (1970) epochs in 100 ns ticks.
const WINDOWS_TO_UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

/// Logical (density-independent) caption bar height — matches a compact Win11 title bar.
const CAPTION_HEIGHT_DP: f32 = 32.0;
const CAPTION_BUTTON_WIDTH_DP: f32 = 46.0;
const CAPTION_FADE_IN_MS: f32 = 180.0;
const CAPTION_FADE_OUT_MS: f32 = 240.0;
/// Physical travel per OS-configured wheel "line". Tao already multiplies a
/// wheel notch by the user's Windows scroll-lines setting (normally three).
const WHEEL_LINE_STEP_DP: f32 = 18.0;
/// Max edge when feeding album art into the renderer (mesh only needs 32²; thumb ≤512).
const ARTWORK_MAX_EDGE: u32 = 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PlaybackSnapshot {
    title: String,
    artist: String,
    position_ms: i32,
    duration_ms: i32,
    is_playing: bool,
    /// SMTC `SourceAppUserModelId` (e.g. `Spotify.exe`) for per-app audio capture.
    source_app_id: String,
    /// Raw SMTC timeline-update identity used by the Apple Music clock adapter.
    smtc_update_ticks: i64,
    artwork: Option<Arc<Artwork>>,
}

#[derive(Clone, Debug)]
struct Artwork {
    pixels: Vec<u32>,
    width: usize,
    height: usize,
    hash: u64,
}

impl PartialEq for Artwork {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height && self.hash == other.hash
    }
}

impl Eq for Artwork {}

#[derive(Clone, Debug)]
struct AppConfig {
    lyrics_dir: PathBuf,
    recursive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaptionHit {
    None,
    Drag,
    /// Toggle always-on-top (pin).
    AlwaysOnTop,
    Minimize,
    Close,
}

pub fn run() -> Result<(), String> {
    env_logger::try_init().ok();

    // Must run before any window / WGL context so hybrid GPU drivers see the
    // OS-assigned adapter (Settings → Graphics) for this executable.
    #[cfg(windows)]
    gpu_preference::apply_windows_gpu_preference();

    let config = AppConfig::from_env_args()?;
    let (playback_tx, playback_rx) = mpsc::channel();
    spawn_smtc_listener(playback_tx);
    let (seek_tx, seek_result_rx) = spawn_smtc_seek_worker();

    #[cfg(windows)]
    let audio_capture = {
        let control = audio_capture::CaptureControl::new();
        audio_capture::spawn(Arc::clone(&control));
        eprintln!(
            "[audio] per-app WASAPI process loopback armed (targets SMTC SourceAppUserModelId)"
        );
        control
    };

    let event_loop = EventLoop::<()>::new();
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Accompanist Desktop Lyrics")
            .with_inner_size(LogicalSize::new(
                DEFAULT_WIDTH as f64,
                DEFAULT_HEIGHT as f64,
            ))
            .with_min_inner_size(LogicalSize::new(320.0, 240.0))
            .with_resizable(true)
            .with_decorations(false)
            .with_transparent(false)
            .with_always_on_top(true)
            .build(&event_loop)
            .map_err(|error| format!("failed to create tao window: {error}"))?,
    );
    // Default pinned; caption pin button can toggle this off.
    let mut always_on_top = true;

    let mut renderer = DesktopGpuRenderer::new(&window)?;
    let vsync = renderer.vsync_enabled();
    let mut display_timing = detect_display_timing(window.as_ref());
    eprintln!(
        "[frame] display refresh: prefer {:.0} Hz (monitor max {:.0} Hz, system max {:.0} Hz); vsync={}",
        display_timing.prefer_hz,
        display_timing.monitor_max_hz,
        display_timing.system_max_hz,
        if vsync { "on" } else { "off" }
    );

    let timing_enabled = frame_timing_enabled();
    if timing_enabled {
        eprintln!(
            "[frame] timing enabled (ACCOMPANIST_FRAME_TIMING); logging 1s averages to stderr"
        );
    }

    let mut app = DesktopLyricsApp::new(
        config,
        playback_rx,
        seek_tx,
        seek_result_rx,
        window.scale_factor() as f32,
        FrameTimingLogger::new(timing_enabled),
        always_on_top,
        #[cfg(windows)]
        audio_capture,
    );
    app.install_placeholder_scene(window.inner_size());

    let window_for_loop = Arc::clone(&window);
    let mut next_frame_deadline = Instant::now();
    // Once true, never go back to Poll/redraw — otherwise continuous frames
    // overwrite ControlFlow::Exit and the Windows loop can hang in GetMessage.
    let mut exiting = false;
    event_loop.run(move |event, _, control_flow| {
        if exiting {
            // begin_exit already calls process::exit; this is a safety net if
            // we re-enter the handler with the flag set.
            std::process::exit(0);
        }

        // Playing / animating: stay in Poll so redraw requests are not throttled by
        // the SMTC 500ms idle wake. Idle: sleep until the next SMTC poll.
        if app.wants_continuous_frames() {
            *control_flow = ControlFlow::Poll;
        } else {
            *control_flow = ControlFlow::WaitUntil(Instant::now() + SMTC_POLL_INTERVAL);
        }

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    begin_exit(
                        &mut exiting,
                        control_flow,
                        &window_for_loop,
                        #[cfg(windows)]
                        &app.audio_capture,
                    );
                }
                WindowEvent::Resized(size) => {
                    app.resize(size, window_for_loop.scale_factor() as f32);
                    if let Err(error) = renderer.resize(size) {
                        eprintln!("{error}");
                    }
                    let next = detect_display_timing(window_for_loop.as_ref());
                    if (next.prefer_hz - display_timing.prefer_hz).abs() >= 0.5 {
                        eprintln!(
                            "[frame] display refresh updated: prefer {:.0} Hz (monitor max {:.0})",
                            next.prefer_hz, next.monitor_max_hz
                        );
                    }
                    display_timing = next;
                    window_for_loop.request_redraw();
                }
                WindowEvent::Moved(_) => {
                    // Window may have crossed onto another monitor with a different max Hz.
                    let next = detect_display_timing(window_for_loop.as_ref());
                    if (next.prefer_hz - display_timing.prefer_hz).abs() >= 0.5 {
                        eprintln!(
                            "[frame] display refresh updated: prefer {:.0} Hz (monitor max {:.0})",
                            next.prefer_hz, next.monitor_max_hz
                        );
                        display_timing = next;
                    }
                }
                WindowEvent::ScaleFactorChanged {
                    scale_factor,
                    new_inner_size,
                } => {
                    app.resize(*new_inner_size, scale_factor as f32);
                    if let Err(error) = renderer.resize(*new_inner_size) {
                        eprintln!("{error}");
                    }
                    display_timing = detect_display_timing(window_for_loop.as_ref());
                    window_for_loop.request_redraw();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    app.on_cursor_moved(&window_for_loop, position);
                }
                WindowEvent::CursorLeft { .. } => {
                    app.on_cursor_left(&window_for_loop);
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    if app.on_mouse_wheel(delta) {
                        window_for_loop.request_redraw();
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    if button == MouseButton::Left {
                        match state {
                            ElementState::Pressed => {
                                // Drag / pin / minimize act on press; close + lyric seek
                                // wait for release so a press-drag doesn't fire them.
                                let hit = app.on_mouse_pressed(&window_for_loop);
                                if hit == CaptionHit::AlwaysOnTop {
                                    always_on_top = !always_on_top;
                                    window_for_loop.set_always_on_top(always_on_top);
                                    app.set_always_on_top(always_on_top);
                                    window_for_loop.request_redraw();
                                }
                            }
                            ElementState::Released => {
                                if app.on_mouse_released_close() {
                                    begin_exit(
                                        &mut exiting,
                                        control_flow,
                                        &window_for_loop,
                                        #[cfg(windows)]
                                        &app.audio_capture,
                                    );
                                } else if app.try_lyric_tap_seek() {
                                    window_for_loop.request_redraw();
                                }
                            }
                            _ => {}
                        }
                    }
                }
                WindowEvent::Focused(false) => {
                    app.on_mouse_released();
                }
                WindowEvent::Destroyed => {
                    std::process::exit(0);
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                if exiting {
                    std::process::exit(0);
                }
                let dirty = app.drain_playback_updates();
                if dirty || app.wants_continuous_frames() {
                    window_for_loop.request_redraw();
                }
            }
            Event::RedrawRequested(_) => {
                if exiting {
                    std::process::exit(0);
                }
                // Software frame pace when vsync is unavailable: aim for the
                // preferred (max) refresh interval so we don't spin at hundreds of fps.
                if !vsync {
                    let now = Instant::now();
                    if now < next_frame_deadline {
                        // Short sleep keeps Poll from burning a core while still
                        // waking in time for the target cadence.
                        thread::sleep(next_frame_deadline.saturating_duration_since(now));
                    }
                }

                let frame_started = Instant::now();
                match app.render(&mut renderer, window_for_loop.inner_size()) {
                    Ok(need_more) => {
                        // Always re-request while we need continuous frames. With
                        // vsync, swap_buffers blocks on vblank so this paces to the
                        // *active* display mode; without vsync we pace to prefer_hz.
                        if !exiting && (need_more || app.wants_continuous_frames()) {
                            window_for_loop.request_redraw();
                        }
                    }
                    Err(error) => eprintln!("{error}"),
                }

                if !vsync {
                    let budget = display_timing.frame_interval;
                    next_frame_deadline = frame_started + budget;
                    // If we overran the budget, schedule from now so we don't
                    // accumulate permanent debt after a hitch.
                    let now = Instant::now();
                    if next_frame_deadline < now {
                        next_frame_deadline = now;
                    }
                }
            }
            Event::LoopDestroyed => {
                std::process::exit(0);
            }
            _ => {}
        }
    });
}

fn begin_exit(
    exiting: &mut bool,
    control_flow: &mut ControlFlow,
    window: &Window,
    #[cfg(windows)] audio_capture: &audio_capture::CaptureControl,
) {
    if *exiting {
        // Second attempt (e.g. stuck Exit path) — force kill.
        std::process::exit(0);
    }
    *exiting = true;
    *control_flow = ControlFlow::Exit;
    // Hide first so the user sees the window go away immediately.
    window.set_visible(false);
    #[cfg(windows)]
    audio_capture.request_stop();
    lyrics_renderer::audio::reset();

    // tao's Windows event loop can leave the process alive after ControlFlow::Exit
    // when continuous Poll/redraw or background SMTC/WASAPI threads keep the
    // message pump from draining cleanly. Hard-exit here; destructors are skipped
    // on purpose so GL/COM teardown cannot hang the process.
    std::process::exit(0);
}

/// Preferred present / pacing rate derived from the OS display mode list.
#[derive(Clone, Copy, Debug)]
struct DisplayTiming {
    /// Hz we try to run at (current monitor's max mode, else system max, else 60).
    prefer_hz: f64,
    /// Max mode Hz on the window's current monitor.
    monitor_max_hz: f64,
    /// Max mode Hz across all connected monitors.
    system_max_hz: f64,
    /// `1 / prefer_hz` for software pacing when vsync is off.
    frame_interval: Duration,
}

fn detect_display_timing(window: &Window) -> DisplayTiming {
    let mut system_max_hz = 0.0_f64;
    for monitor in window.available_monitors() {
        for mode in monitor.video_modes() {
            system_max_hz = system_max_hz.max(mode.refresh_rate() as f64);
        }
    }

    let mut monitor_max_hz = 0.0_f64;
    if let Some(monitor) = window
        .current_monitor()
        .or_else(|| window.primary_monitor())
    {
        for mode in monitor.video_modes() {
            monitor_max_hz = monitor_max_hz.max(mode.refresh_rate() as f64);
        }
    }

    // Prefer the monitor the window is on; fall back to any attached display.
    let prefer_hz = if monitor_max_hz >= 30.0 {
        monitor_max_hz
    } else if system_max_hz >= 30.0 {
        system_max_hz
    } else {
        DEFAULT_REFRESH_HZ
    }
    .clamp(30.0, 500.0);

    DisplayTiming {
        prefer_hz,
        monitor_max_hz: if monitor_max_hz >= 30.0 {
            monitor_max_hz
        } else {
            prefer_hz
        },
        system_max_hz: if system_max_hz >= 30.0 {
            system_max_hz
        } else {
            prefer_hz
        },
        frame_interval: Duration::from_secs_f64(1.0 / prefer_hz),
    }
}

#[derive(Debug, Clone, Copy)]
struct CaptionFade {
    alpha: f32,
    start_alpha: f32,
    target_alpha: f32,
    started_at: Option<Instant>,
    duration_ms: f32,
}

impl Default for CaptionFade {
    fn default() -> Self {
        Self {
            alpha: 0.0,
            start_alpha: 0.0,
            target_alpha: 0.0,
            started_at: None,
            duration_ms: 0.0,
        }
    }
}

impl CaptionFade {
    fn sample(&mut self, now: Instant) -> (f32, bool) {
        let Some(started_at) = self.started_at else {
            self.alpha = self.target_alpha;
            return (self.alpha, false);
        };
        let progress = (now.saturating_duration_since(started_at).as_secs_f32() * 1000.0
            / self.duration_ms.max(1.0))
        .clamp(0.0, 1.0);
        let eased = progress * progress * (3.0 - 2.0 * progress);
        self.alpha = self.start_alpha + (self.target_alpha - self.start_alpha) * eased;
        if progress >= 1.0 {
            self.alpha = self.target_alpha;
            self.started_at = None;
            (self.alpha, false)
        } else {
            (self.alpha, true)
        }
    }

    fn set_visible(&mut self, visible: bool, now: Instant) {
        self.sample(now);
        let target = if visible { 1.0 } else { 0.0 };
        if (target - self.target_alpha).abs() <= f32::EPSILON {
            return;
        }
        self.start_alpha = self.alpha;
        self.target_alpha = target;
        self.duration_ms = if visible {
            CAPTION_FADE_IN_MS
        } else {
            CAPTION_FADE_OUT_MS
        };
        self.started_at = Some(now);
    }

    fn is_animating(&self) -> bool {
        self.started_at.is_some()
    }
}

struct DesktopLyricsApp {
    config: AppConfig,
    playback_rx: Receiver<PlaybackSnapshot>,
    seek_tx: Sender<SeekRequest>,
    seek_result_rx: Receiver<SeekResult>,
    engine: TextEngine,
    clock: PlaybackClock,
    pending_seek: Option<PendingSeek>,
    next_seek_id: u64,
    current_track_key: Option<String>,
    current_artwork: Option<Arc<Artwork>>,
    current_lyrics: Option<lyrics_parser::SyncedLyrics>,
    /// Live SMTC title/artist for the player top bar (Android `setTopBar`).
    top_bar_title: String,
    top_bar_artist: String,
    last_size: PhysicalSize<u32>,
    density: f32,
    cursor: PhysicalPosition<f64>,
    cursor_inside: bool,
    caption_pressed: Option<CaptionHit>,
    /// Press position for tap-to-seek (ignored if the pointer moves too far).
    pointer_down: Option<PhysicalPosition<f64>>,
    /// Window always-on-top (pin) state — mirrored for caption icon drawing.
    always_on_top: bool,
    /// Last render return value — whether the engine asked for another frame.
    last_animating: bool,
    caption_fade: CaptionFade,
    caption_font_tower: Option<CaptionFontTower>,
    caption_text: Option<CaptionTextLayout>,
    caption_text_key: Option<CaptionTextKey>,
    frame_timing: FrameTimingLogger,
    #[cfg(windows)]
    audio_capture: Arc<audio_capture::CaptureControl>,
}

/// Max pointer travel (physical px) to still count as a lyric tap, not a drag.
const TAP_SLOP_PX: f64 = 8.0;

impl DesktopLyricsApp {
    fn new(
        config: AppConfig,
        playback_rx: Receiver<PlaybackSnapshot>,
        seek_tx: Sender<SeekRequest>,
        seek_result_rx: Receiver<SeekResult>,
        density: f32,
        frame_timing: FrameTimingLogger,
        always_on_top: bool,
        #[cfg(windows)] audio_capture: Arc<audio_capture::CaptureControl>,
    ) -> Self {
        let mut engine = TextEngine::new(2048, 2048);
        engine.load_system_fonts();

        Self {
            config,
            playback_rx,
            seek_tx,
            seek_result_rx,
            engine,
            clock: PlaybackClock::default(),
            pending_seek: None,
            next_seek_id: 0,
            current_track_key: None,
            current_artwork: None,
            current_lyrics: None,
            top_bar_title: String::new(),
            top_bar_artist: String::new(),
            last_size: PhysicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT),
            density: density.max(0.5),
            cursor: PhysicalPosition::new(0.0, 0.0),
            cursor_inside: false,
            caption_pressed: None,
            pointer_down: None,
            always_on_top,
            last_animating: true,
            caption_fade: CaptionFade::default(),
            caption_font_tower: make_caption_font_tower(density.max(0.5)),
            caption_text: None,
            caption_text_key: None,
            frame_timing,
            #[cfg(windows)]
            audio_capture,
        }
    }

    fn set_always_on_top(&mut self, pinned: bool) {
        self.always_on_top = pinned;
    }

    fn caption_height_px(&self) -> f32 {
        CAPTION_HEIGHT_DP * self.density
    }

    fn caption_button_width_px(&self) -> f32 {
        CAPTION_BUTTON_WIDTH_DP * self.density
    }

    fn install_placeholder_scene(&mut self, size: PhysicalSize<u32>) {
        self.last_size = normalized_size(size);
        let lyrics = lyrics_parser::SyncedLyrics {
            lines: vec![lyrics_parser::SyncedLine::new(
                "等待系统媒体会话".to_string(),
                None,
                0,
                24 * 60 * 60 * 1000,
            )
            .into()],
            ..Default::default()
        };
        self.set_lyrics_scene(&lyrics);
        self.current_lyrics = Some(lyrics);
    }

    fn resize(&mut self, size: PhysicalSize<u32>, density: f32) {
        let size = normalized_size(size);
        let density = density.max(0.5);
        let density_changed = (self.density - density).abs() > 0.001;
        if self.last_size == size && !density_changed {
            return;
        }
        self.last_size = size;
        self.density = density;
        if density_changed {
            self.caption_font_tower = make_caption_font_tower(density);
            self.caption_text = None;
            self.caption_text_key = None;
        }
        if let Some(lyrics) = self.current_lyrics.clone() {
            self.set_lyrics_scene(&lyrics);
        }
    }

    /// Drain SMTC snapshots. Returns true when something visual changed.
    fn drain_playback_updates(&mut self) -> bool {
        let mut dirty = false;
        while let Ok(result) = self.seek_result_rx.try_recv() {
            if self
                .pending_seek
                .is_some_and(|pending| pending.request_id == result.request_id)
            {
                if result.accepted {
                    if let Some(pending) = self.pending_seek.as_mut() {
                        pending.api_accepted = true;
                    }
                } else {
                    // The provider rejected the request. Release the optimistic
                    // anchor immediately; the next periodic snapshot restores
                    // the authoritative player position.
                    self.pending_seek = None;
                }
                dirty = true;
            }
        }

        let mut latest = None;
        while let Ok(snapshot) = self.playback_rx.try_recv() {
            latest = Some(snapshot);
        }

        let Some(snapshot) = latest else {
            return dirty;
        };

        let track_key = track_key(&snapshot);
        let track_changed = self.current_track_key.as_deref() != Some(track_key.as_str());
        let top_bar_changed = self.top_bar_title != snapshot.title
            || self.top_bar_artist != snapshot.artist;
        let accept_clock_sample = if track_changed {
            self.pending_seek = None;
            true
        } else if let Some(pending) = self.pending_seek {
            if pending.accepts(snapshot.position_ms, snapshot.is_playing, Instant::now()) {
                self.pending_seek = None;
                true
            } else {
                false
            }
        } else {
            true
        };
        if accept_clock_sample {
            if track_changed {
                self.clock.force_smtc_sample(
                    snapshot.position_ms,
                    snapshot.is_playing,
                    &snapshot.source_app_id,
                    snapshot.smtc_update_ticks,
                );
            } else {
                self.clock.publish_smtc_sample(
                    snapshot.position_ms,
                    snapshot.is_playing,
                    &snapshot.source_app_id,
                    snapshot.smtc_update_ticks,
                );
            }
        } else {
            // A poll that was already in flight when the click happened may still
            // contain the old position. Preserve the optimistic seek anchor, but
            // do not let the playback state itself become stale.
            self.clock.set_playing(snapshot.is_playing);
        }

        // Reactive mesh when playing (loudness from per-app process loopback)
        // or when we at least have artwork for a static mesh.
        let reactive = snapshot.is_playing || snapshot.artwork.is_some();
        self.engine
            .set_playback_state(snapshot.is_playing, reactive);
        #[cfg(windows)]
        self.audio_capture
            .update_session(&snapshot.source_app_id, snapshot.is_playing);
        if track_changed {
            lyrics_renderer::audio::reset();
        }
        let art_changed = self.update_background_art(&snapshot, &track_key);

        self.top_bar_title = snapshot.title.clone();
        self.top_bar_artist = snapshot.artist.clone();

        if track_changed {
            self.current_track_key = Some(track_key);
            let lyrics = find_matching_lyrics(&self.config, &snapshot)
                .and_then(|path| parse_lyrics_file_with_auto_parser(&path).ok())
                .unwrap_or_else(|| missing_lyrics(&snapshot));
            self.set_lyrics_scene(&lyrics);
            self.current_lyrics = Some(lyrics);
            return true;
        }

        // Same track, but title/artist (or density-driven top bar) may still need a
        // scene rebuild so the in-surface top bar stays in sync with SMTC.
        if top_bar_changed {
            if let Some(lyrics) = self.current_lyrics.clone() {
                self.set_lyrics_scene(&lyrics);
            }
            return true;
        }

        dirty || art_changed
    }

    fn update_background_art(&mut self, snapshot: &PlaybackSnapshot, track_key: &str) -> bool {
        if self.current_artwork == snapshot.artwork {
            return false;
        }

        if let Some(artwork) = &snapshot.artwork {
            self.engine.set_background_art(
                &artwork.pixels,
                artwork.width,
                artwork.height,
                stable_seed(track_key),
            );
        } else {
            self.engine.clear_background();
        }
        self.current_artwork = snapshot.artwork.clone();
        true
    }

    fn set_lyrics_scene(&mut self, lyrics: &lyrics_parser::SyncedLyrics) {
        let caption = self.caption_height_px();
        let mut params = SceneBuildParams::new(self.last_size.width, self.last_size.height)
            .with_density(self.density)
            .with_insets(caption, 0.0, 0.0, 0.0);
        if !self.top_bar_title.trim().is_empty() {
            params = params.with_top_bar(&self.top_bar_title, &self.top_bar_artist);
        }
        let json = lyrics_parser::scene_json_with(lyrics, &params);
        let result = self.engine.set_lyrics_scene_json(&json);
        if result.contains("\"error\"") {
            eprintln!("failed to set lyrics scene: {result}");
        }
    }

    fn wants_continuous_frames(&self) -> bool {
        // Keep the present loop hot while media is playing or the engine still
        // has in-flight animation (springs, marquee, mesh fade, etc.).
        self.last_animating || self.clock.is_playing || self.caption_fade.is_animating()
    }

    fn on_cursor_moved(&mut self, window: &Window, position: PhysicalPosition<f64>) {
        let previous_hit = if self.cursor_inside {
            self.hit_test_caption(self.cursor)
        } else {
            CaptionHit::None
        };
        self.cursor = position;
        self.cursor_inside = true;
        let hit = self.hit_test_caption(position);
        let icon = match hit {
            CaptionHit::Minimize | CaptionHit::Close | CaptionHit::AlwaysOnTop => CursorIcon::Hand,
            CaptionHit::Drag => CursorIcon::Arrow,
            CaptionHit::None => CursorIcon::Default,
        };
        window.set_cursor_icon(icon);
        // Redraw on both enter and leave. Without the leave transition, a paused
        // renderer could retain the last visible caption after the pointer moved
        // from the caption into the lyrics area.
        if hit != previous_hit {
            window.request_redraw();
        }
    }

    fn on_cursor_left(&mut self, window: &Window) {
        self.cursor_inside = false;
        window.set_cursor_icon(CursorIcon::Default);
        window.request_redraw();
    }

    fn on_mouse_wheel(&mut self, delta: MouseScrollDelta) -> bool {
        let delta_y = mouse_wheel_delta_px(delta, self.density);
        if !delta_y.is_finite() || delta_y.abs() < 0.01 {
            return false;
        }
        self.engine.begin_lyrics_scroll();
        self.engine.scroll_lyrics_by(delta_y);
        // A wheel has discrete impulses rather than a reliable pointer velocity.
        // Release immediately so the existing hold + return-to-auto physics owns
        // the rest of the interaction.
        self.engine.end_lyrics_scroll(0.0);
        true
    }

    /// Handle caption chrome on mouse down. Returns the hit target.
    fn on_mouse_pressed(&mut self, window: &Window) -> CaptionHit {
        let hit = self.hit_test_caption(self.cursor);
        self.caption_pressed = Some(hit);
        self.pointer_down = Some(self.cursor);
        match hit {
            CaptionHit::Drag => {
                // drag_window runs a nested modal loop until the button is released.
                let _ = window.drag_window();
                self.caption_pressed = None;
                self.pointer_down = None;
            }
            CaptionHit::Minimize => {
                window.set_minimized(true);
                self.caption_pressed = None;
                self.pointer_down = None;
            }
            CaptionHit::AlwaysOnTop => {
                // Toggled by the event loop (needs `window.set_always_on_top`).
                self.caption_pressed = None;
                self.pointer_down = None;
            }
            CaptionHit::Close | CaptionHit::None => {}
        }
        hit
    }

    fn on_mouse_released(&mut self) {
        self.caption_pressed = None;
        self.pointer_down = None;
    }

    /// Complete a close click only if press and release both hit the close button.
    fn on_mouse_released_close(&mut self) -> bool {
        let pressed = self.caption_pressed;
        let close = pressed == Some(CaptionHit::Close)
            && self.hit_test_caption(self.cursor) == CaptionHit::Close;
        if close {
            self.caption_pressed = None;
            self.pointer_down = None;
        }
        close
    }

    /// Tap a lyric line → seek SMTC timeline to that line's start.
    /// Returns true when a seek was issued (optimistic local clock updated).
    fn try_lyric_tap_seek(&mut self) -> bool {
        // Don't steal clicks that started on the caption bar.
        if self.caption_pressed.is_some_and(|hit| hit != CaptionHit::None) {
            self.caption_pressed = None;
            self.pointer_down = None;
            return false;
        }
        let Some(down) = self.pointer_down.take() else {
            self.caption_pressed = None;
            return false;
        };
        self.caption_pressed = None;

        let dx = self.cursor.x - down.x;
        let dy = self.cursor.y - down.y;
        if dx * dx + dy * dy > TAP_SLOP_PX * TAP_SLOP_PX {
            return false;
        }
        // Caption strip is chrome, not lyrics.
        if self.hit_test_caption(self.cursor) != CaptionHit::None {
            return false;
        }

        let time_ms = self.clock.compute_display_time_ms();
        let x = self.cursor.x as f32;
        let y = self.cursor.y as f32;
        if self.engine.hit_test_top_bar_region(x, y) {
            return false;
        }
        let source_index = self.engine.hit_test_lyrics_line(x, y, time_ms);
        if source_index < 0 {
            return false;
        }
        let Some(start_ms) = self.engine.lyrics_line_start_ms(source_index as usize) else {
            return false;
        };

        // Optimistic local clock so the karaoke sweep jumps immediately; SMTC will
        // reconcile on the next poll once the player acknowledges the seek.
        self.next_seek_id = self.next_seek_id.wrapping_add(1);
        let request = SeekRequest {
            request_id: self.next_seek_id,
            position_ms: start_ms,
        };
        if self.seek_tx.send(request).is_err() {
            return false;
        }
        self.pending_seek = Some(PendingSeek {
            request_id: request.request_id,
            target_position_ms: start_ms,
            issued_at: Instant::now(),
            api_accepted: false,
        });
        self.clock.force_sample(start_ms, self.clock.is_playing);
        true
    }

    fn hit_test_caption(&self, position: PhysicalPosition<f64>) -> CaptionHit {
        let x = position.x as f32;
        let y = position.y as f32;
        let height = self.caption_height_px();
        if y < 0.0 || y > height {
            return CaptionHit::None;
        }
        let width = self.last_size.width as f32;
        let button_w = self.caption_button_width_px();
        // Right → left: Close | Minimize | Pin (always-on-top)
        if x >= width - button_w {
            return CaptionHit::Close;
        }
        if x >= width - button_w * 2.0 {
            return CaptionHit::Minimize;
        }
        if x >= width - button_w * 3.0 {
            return CaptionHit::AlwaysOnTop;
        }
        CaptionHit::Drag
    }

    fn render(
        &mut self,
        renderer: &mut DesktopGpuRenderer,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        let size = normalized_size(size);
        if self.last_size != size {
            self.resize(size, self.density);
        }
        renderer.resize(size)?;
        let current_time_ms = self.clock.compute_display_time_ms();
        let hover = if self.cursor_inside {
            self.hit_test_caption(self.cursor)
        } else {
            CaptionHit::None
        };
        let caption_now = Instant::now();
        self.caption_fade
            .set_visible(caption_bar_visible(hover), caption_now);
        let (caption_alpha, caption_animating) = self.caption_fade.sample(caption_now);
        let density = self.density;
        let caption_h = self.caption_height_px();
        let button_w = self.caption_button_width_px();
        let always_on_top = self.always_on_top;
        let title = if self.top_bar_title.trim().is_empty() {
            "Accompanist".to_string()
        } else if self.top_bar_artist.trim().is_empty() {
            self.top_bar_title.clone()
        } else {
            format!("{} — {}", self.top_bar_title, self.top_bar_artist)
        };
        let caption_inset = 12.0 * density;
        let caption_max_text_width =
            (size.width as f32 - button_w * 3.0 - caption_inset * 2.0).max(0.0);
        let caption_text_key = CaptionTextKey {
            text: title.clone(),
            max_width_bits: caption_max_text_width.to_bits(),
        };
        if self.caption_text_key.as_ref() != Some(&caption_text_key) {
            self.caption_text = self
                .caption_font_tower
                .as_ref()
                .map(|tower| tower.layout_ellipsized(&title, caption_max_text_width));
            self.caption_text_key = Some(caption_text_key);
        }
        let caption_text = self.caption_text.clone();
        let collect_timing = self.frame_timing.enabled();
        let mut caption_ms = 0.0;

        let drawn = renderer.draw_frame(|canvas| {
            canvas.clear(Color::from_argb(255, 12, 12, 14));
            let engine_result = self
                .engine
                .render_lyrics_frame_to_canvas(current_time_ms, canvas);
            let caption_start = Instant::now();
            draw_caption_bar(
                canvas,
                size.width as f32,
                caption_h,
                button_w,
                density,
                caption_text.as_ref(),
                hover,
                always_on_top,
                caption_alpha,
            );
            if collect_timing {
                caption_ms = caption_start.elapsed().as_secs_f64() * 1000.0;
            }
            engine_result
        })?;

        if collect_timing {
            self.frame_timing.record(FrameSample {
                engine: self.engine.last_engine_frame_timing(),
                caption_ms,
                gpu: drawn.timing,
            });
        }

        self.last_animating = drawn.engine_result != 0;
        Ok(self.last_animating || self.clock.is_playing || caption_animating)
    }
}

fn draw_caption_bar(
    canvas: &skia_safe::Canvas,
    width: f32,
    height: f32,
    button_w: f32,
    density: f32,
    title: Option<&CaptionTextLayout>,
    hover: CaptionHit,
    always_on_top: bool,
    alpha: f32,
) {
    // Caption chrome is discoverable on hover: when the pointer is outside its
    // strip, leave the mesh player completely unobstructed.
    if alpha <= 0.001 {
        return;
    }

    let bounds = Rect::from_xywh(0.0, 0.0, width, height);
    let mut opacity = Paint::default();
    opacity.set_alpha_f(alpha.clamp(0.0, 1.0));
    canvas.save_layer(
        &skia_safe::canvas::SaveLayerRec::default()
            .bounds(&bounds)
            .paint(&opacity),
    );

    // Semi-opaque strip so the mesh/background still reads through lightly.
    let mut bar_paint = Paint::default();
    bar_paint.set_anti_alias(true);
    bar_paint.set_color4f(Color4f::new(0.05, 0.05, 0.07, 0.72), None);
    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, width, height), &bar_paint);

    // Bottom hairline separator.
    let mut line = Paint::default();
    line.set_anti_alias(true);
    line.set_color4f(Color4f::new(1.0, 1.0, 1.0, 0.08), None);
    line.set_stroke_width(1.0_f32.max(density * 0.5));
    line.set_style(paint::Style::Stroke);
    canvas.draw_line(
        Point::new(0.0, height - 0.5),
        Point::new(width, height - 0.5),
        &line,
    );

    // Title — left-aligned with a small inset matching the player top bar padding.
    if let Some(title) = title {
        let mut text_paint = Paint::default();
        text_paint.set_anti_alias(true);
        text_paint.set_color4f(Color4f::new(1.0, 1.0, 1.0, 0.72), None);
        let inset = 12.0 * density;
        // All fallback runs share one baseline. Use the tower-wide extrema so CJK
        // and Latin stay vertically aligned even when their typefaces have
        // different ascent/descent metrics.
        let baseline_y = height * 0.5 - (title.ascent + title.descent) * 0.5;
        let mut x = inset;
        for run in &title.runs {
            if let Some(blob) = TextBlob::from_str(&run.text, &run.font) {
                canvas.draw_text_blob(blob, (x, baseline_y), &text_paint);
            }
            x += run.width;
        }
    }

    draw_caption_button(
        canvas,
        width - button_w * 3.0,
        0.0,
        button_w,
        height,
        density,
        hover == CaptionHit::AlwaysOnTop,
        CaptionGlyph::Pin { active: always_on_top },
    );
    draw_caption_button(
        canvas,
        width - button_w * 2.0,
        0.0,
        button_w,
        height,
        density,
        hover == CaptionHit::Minimize,
        CaptionGlyph::Minimize,
    );
    draw_caption_button(
        canvas,
        width - button_w,
        0.0,
        button_w,
        height,
        density,
        hover == CaptionHit::Close,
        CaptionGlyph::Close,
    );
    canvas.restore();
}

fn caption_bar_visible(hover: CaptionHit) -> bool {
    hover != CaptionHit::None
}

#[derive(Clone, Copy)]
enum CaptionGlyph {
    /// Pushpin: filled when always-on-top is active.
    Pin { active: bool },
    Minimize,
    Close,
}

fn draw_caption_button(
    canvas: &skia_safe::Canvas,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    density: f32,
    hovered: bool,
    glyph: CaptionGlyph,
) {
    let pin_active = matches!(glyph, CaptionGlyph::Pin { active: true });
    if hovered || pin_active {
        let mut bg = Paint::default();
        bg.set_anti_alias(true);
        match glyph {
            CaptionGlyph::Close => {
                bg.set_color4f(Color4f::new(0.91, 0.18, 0.22, 1.0), None);
            }
            CaptionGlyph::Pin { active: true } if !hovered => {
                // Subtle highlight so the pin reads as "on" even without hover.
                bg.set_color4f(Color4f::new(0.35, 0.55, 1.0, 0.28), None);
            }
            CaptionGlyph::Pin { .. } | CaptionGlyph::Minimize => {
                bg.set_color4f(Color4f::new(1.0, 1.0, 1.0, 0.10), None);
            }
        }
        canvas.draw_rect(Rect::from_xywh(x, y, w, h), &bg);
    }

    let mut icon = Paint::default();
    icon.set_anti_alias(true);
    let icon_alpha = if pin_active { 1.0 } else { 0.92 };
    icon.set_color4f(Color4f::new(1.0, 1.0, 1.0, icon_alpha), None);
    icon.set_style(paint::Style::Stroke);
    icon.set_stroke_width((1.25 * density).max(1.0));
    icon.set_stroke_cap(paint::Cap::Round);

    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let s = 4.5 * density;
    match glyph {
        CaptionGlyph::Pin { active } => {
            // Simple pushpin: head circle + short needle below.
            let head_r = s * 0.72;
            let head_cy = cy - s * 0.25;
            if active {
                let mut fill = Paint::default();
                fill.set_anti_alias(true);
                fill.set_color4f(Color4f::new(0.55, 0.75, 1.0, 0.95), None);
                fill.set_style(paint::Style::Fill);
                canvas.draw_circle(Point::new(cx, head_cy), head_r, &fill);
            }
            canvas.draw_circle(Point::new(cx, head_cy), head_r, &icon);
            canvas.draw_line(
                Point::new(cx, head_cy + head_r),
                Point::new(cx, cy + s * 0.95),
                &icon,
            );
        }
        CaptionGlyph::Minimize => {
            canvas.draw_line(
                Point::new(cx - s, cy),
                Point::new(cx + s, cy),
                &icon,
            );
        }
        CaptionGlyph::Close => {
            canvas.draw_line(
                Point::new(cx - s, cy - s),
                Point::new(cx + s, cy + s),
                &icon,
            );
            canvas.draw_line(
                Point::new(cx + s, cy - s),
                Point::new(cx - s, cy + s),
                &icon,
            );
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CaptionTextKey {
    text: String,
    max_width_bits: u32,
}

#[derive(Clone)]
struct CaptionTextRun {
    text: String,
    font: Font,
    width: f32,
}

#[derive(Clone, Default)]
struct CaptionTextLayout {
    runs: Vec<CaptionTextRun>,
    width: f32,
    ascent: f32,
    descent: f32,
}

struct CaptionFontTower {
    manager: FontMgr,
    primary: Typeface,
    style: FontStyle,
    size: f32,
}

impl CaptionFontTower {
    fn layout_ellipsized(&self, text: &str, max_width: f32) -> CaptionTextLayout {
        if max_width <= 0.0 {
            return CaptionTextLayout::default();
        }

        let full = self.layout(text);
        if full.width <= max_width {
            return full;
        }

        let ellipsis = self.layout("…");
        if ellipsis.width >= max_width {
            return ellipsis;
        }

        let graphemes = UnicodeSegmentation::graphemes(text, true).collect::<Vec<_>>();
        let mut low = 0usize;
        let mut high = graphemes.len();
        let mut best = ellipsis;
        while low <= high {
            let middle = low + (high - low) / 2;
            let candidate = graphemes[..middle].concat() + "…";
            let layout = self.layout(&candidate);
            if layout.width <= max_width {
                best = layout;
                low = middle + 1;
            } else if middle == 0 {
                break;
            } else {
                high = middle - 1;
            }
        }
        best
    }

    fn layout(&self, text: &str) -> CaptionTextLayout {
        let mut grouped = Vec::<(Typeface, String)>::new();
        for cluster in UnicodeSegmentation::graphemes(text, true) {
            let typeface = self.typeface_for_cluster(cluster);
            if let Some((last_typeface, last_text)) = grouped.last_mut() {
                if last_typeface.unique_id() == typeface.unique_id() {
                    last_text.push_str(cluster);
                    continue;
                }
            }
            grouped.push((typeface, cluster.to_string()));
        }

        let mut result = CaptionTextLayout {
            ascent: 0.0,
            descent: 0.0,
            ..CaptionTextLayout::default()
        };
        for (typeface, text) in grouped {
            let font = Font::from_typeface(typeface, self.size);
            let width = font.measure_str(&text, None).0;
            let (_, metrics) = font.metrics();
            result.ascent = result.ascent.min(metrics.ascent);
            result.descent = result.descent.max(metrics.descent);
            result.width += width;
            result.runs.push(CaptionTextRun { text, font, width });
        }
        result
    }

    fn typeface_for_cluster(&self, cluster: &str) -> Typeface {
        if typeface_supports_cluster(&self.primary, cluster) {
            return self.primary.clone();
        }

        let mut first_fallback = None;
        for ch in cluster.chars().filter(|ch| is_visible_font_character(*ch)) {
            let Some(typeface) = self.manager.match_family_style_character(
                self.primary.family_name(),
                self.style,
                &[],
                ch as i32,
            ) else {
                continue;
            };
            if typeface_supports_cluster(&typeface, cluster) {
                return typeface;
            }
            first_fallback.get_or_insert(typeface);
        }

        first_fallback.unwrap_or_else(|| self.primary.clone())
    }
}

fn is_visible_font_character(ch: char) -> bool {
    !ch.is_control()
        && ch != '\u{200d}'
        && !(('\u{fe00}'..='\u{fe0f}').contains(&ch))
        && !(('\u{e0100}'..='\u{e01ef}').contains(&ch))
}

fn typeface_supports_cluster(typeface: &Typeface, cluster: &str) -> bool {
    cluster
        .chars()
        .filter(|ch| is_visible_font_character(*ch))
        .all(|ch| typeface.unichar_to_glyph(ch as i32) != 0)
}

fn make_caption_font_tower(density: f32) -> Option<CaptionFontTower> {
    let mgr = FontMgr::new();
    let style = FontStyle::normal();
    let typeface = mgr
        .match_family_style("Segoe UI", style)
        .or_else(|| mgr.match_family_style("sans-serif", style))?;
    Some(CaptionFontTower {
        manager: mgr,
        primary: typeface,
        style,
        size: 12.0 * density,
    })
}

#[derive(Clone, Copy, Debug)]
struct SeekRequest {
    request_id: u64,
    position_ms: i32,
}

#[derive(Clone, Copy, Debug)]
struct SeekResult {
    request_id: u64,
    accepted: bool,
}

#[derive(Clone, Copy, Debug)]
struct PendingSeek {
    request_id: u64,
    target_position_ms: i32,
    issued_at: Instant,
    api_accepted: bool,
}

impl PendingSeek {
    fn accepts(self, sample_position_ms: i32, is_playing: bool, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.issued_at);
        let timeout = if self.api_accepted {
            SEEK_ACCEPTED_ACK_TIMEOUT
        } else {
            SEEK_REQUEST_TIMEOUT
        };
        if elapsed >= timeout {
            // The request worker or provider never produced a matching timeline.
            // Give the real session clock authority again instead of remaining
            // forever on the optimistic target (especially important while paused).
            return true;
        }

        let expected = if is_playing {
            self.target_position_ms
                .saturating_add(elapsed.as_millis().min(i32::MAX as u128) as i32)
        } else {
            self.target_position_ms
        };
        sample_position_ms.abs_diff(expected) <= SEEK_ACK_TOLERANCE_MS
    }
}

fn is_apple_music_source(source_app_id: &str) -> bool {
    source_app_id
        .to_ascii_lowercase()
        .starts_with("appleinc.applemusicwin_")
}

/// Stable source clock for Apple Music's unusual SMTC publisher. Every distinct
/// `LastUpdatedTime` yields an estimate of the position-at-reference-time; a short
/// moving average removes the independent timestamp/Position cadence while the
/// resulting clock advances monotonically at 1x between updates.
#[derive(Debug)]
struct AppleMusicClock {
    reference_clock: Instant,
    base_samples_ms: VecDeque<f64>,
    last_update_ticks: Option<i64>,
    is_playing: bool,
}

impl AppleMusicClock {
    fn new(position_ms: i32, update_ticks: i64, is_playing: bool, now: Instant) -> Self {
        let mut clock = Self {
            reference_clock: now,
            base_samples_ms: VecDeque::with_capacity(APPLE_MUSIC_CLOCK_SAMPLE_COUNT),
            last_update_ticks: None,
            is_playing,
        };
        clock.reset_at(position_ms, update_ticks, is_playing, now);
        clock
    }

    fn reset_at(&mut self, position_ms: i32, update_ticks: i64, is_playing: bool, now: Instant) {
        self.reference_clock = now;
        self.base_samples_ms.clear();
        self.base_samples_ms.push_back(position_ms as f64);
        self.last_update_ticks = (update_ticks > 0).then_some(update_ticks);
        self.is_playing = is_playing;
    }

    /// Returns `(averaged_position_now, reanchored)`.
    fn publish_at(
        &mut self,
        position_ms: i32,
        update_ticks: i64,
        is_playing: bool,
        now: Instant,
    ) -> (i32, bool) {
        let current = self.position_at(now);
        let state_changed = self.is_playing != is_playing;
        let discontinuity =
            (position_ms as f64 - current as f64).abs() >= APPLE_MUSIC_SEEK_SNAP_MS;
        if state_changed || !is_playing || discontinuity {
            self.reset_at(position_ms, update_ticks, is_playing, now);
            return (position_ms.max(0), true);
        }

        let is_new_update = update_ticks <= 0 || self.last_update_ticks != Some(update_ticks);
        if is_new_update {
            let elapsed_ms = now
                .saturating_duration_since(self.reference_clock)
                .as_secs_f64()
                * 1000.0;
            self.base_samples_ms
                .push_back(position_ms as f64 - elapsed_ms);
            while self.base_samples_ms.len() > APPLE_MUSIC_CLOCK_SAMPLE_COUNT {
                self.base_samples_ms.pop_front();
            }
            self.last_update_ticks = (update_ticks > 0).then_some(update_ticks);
        }
        (self.position_at(now), false)
    }

    fn position_at(&self, now: Instant) -> i32 {
        let average_base = self.base_samples_ms.iter().sum::<f64>()
            / self.base_samples_ms.len().max(1) as f64;
        let elapsed_ms = if self.is_playing {
            now.saturating_duration_since(self.reference_clock)
                .as_secs_f64()
                * 1000.0
        } else {
            0.0
        };
        (average_base + elapsed_ms)
            .round()
            .clamp(0.0, i32::MAX as f64) as i32
    }
}

#[derive(Debug)]
struct PlaybackClock {
    anchor_position_ms: i32,
    anchor_clock: Instant,
    display_ms: f64,
    last_clock: Option<Instant>,
    primed: bool,
    is_playing: bool,
    apple_music: Option<AppleMusicClock>,
}

impl Default for PlaybackClock {
    fn default() -> Self {
        Self {
            anchor_position_ms: 0,
            anchor_clock: Instant::now(),
            display_ms: 0.0,
            last_clock: None,
            primed: false,
            is_playing: false,
            apple_music: None,
        }
    }
}

impl PlaybackClock {
    fn publish_smtc_sample(
        &mut self,
        position_ms: i32,
        is_playing: bool,
        source_app_id: &str,
        update_ticks: i64,
    ) {
        self.publish_smtc_sample_at(
            position_ms,
            is_playing,
            source_app_id,
            update_ticks,
            Instant::now(),
        );
    }

    fn publish_smtc_sample_at(
        &mut self,
        position_ms: i32,
        is_playing: bool,
        source_app_id: &str,
        update_ticks: i64,
        now: Instant,
    ) {
        if is_apple_music_source(source_app_id) {
            let (target, _) = match self.apple_music.as_mut() {
                Some(clock) => clock.publish_at(position_ms, update_ticks, is_playing, now),
                None => {
                    self.apple_music = Some(AppleMusicClock::new(
                        position_ms,
                        update_ticks,
                        is_playing,
                        now,
                    ));
                    (position_ms, true)
                }
            };
            self.publish_sample_at(target, is_playing, now);
        } else {
            self.apple_music = None;
            self.publish_sample_at(position_ms, is_playing, now);
        }
    }

    fn publish_sample_at(&mut self, position_ms: i32, is_playing: bool, now: Instant) {
        self.set_playing_at(is_playing, now);
        if self.anchor_position_ms != position_ms {
            self.anchor_position_ms = position_ms;
            self.anchor_clock = now;
        }
    }

    fn force_smtc_sample(
        &mut self,
        position_ms: i32,
        is_playing: bool,
        source_app_id: &str,
        update_ticks: i64,
    ) {
        let now = Instant::now();
        self.apple_music = is_apple_music_source(source_app_id).then(|| {
            AppleMusicClock::new(position_ms, update_ticks, is_playing, now)
        });
        self.force_sample_at(position_ms, is_playing, now);
    }

    fn force_sample(&mut self, position_ms: i32, is_playing: bool) {
        let now = Instant::now();
        if let Some(clock) = self.apple_music.as_mut() {
            clock.reset_at(position_ms, 0, is_playing, now);
        }
        self.force_sample_at(position_ms, is_playing, now);
    }

    fn force_sample_at(&mut self, position_ms: i32, is_playing: bool, now: Instant) {
        self.anchor_position_ms = position_ms;
        self.anchor_clock = now;
        self.display_ms = position_ms.max(0) as f64;
        self.last_clock = Some(now);
        self.primed = true;
        self.is_playing = is_playing;
    }

    fn set_playing(&mut self, is_playing: bool) {
        self.set_playing_at(is_playing, Instant::now());
    }

    fn set_playing_at(&mut self, is_playing: bool, now: Instant) {
        if self.is_playing == is_playing {
            return;
        }
        if self.is_playing {
            self.anchor_position_ms = self.projected_anchor_at(now);
        }
        self.anchor_clock = now;
        self.is_playing = is_playing;
    }

    fn compute_display_time_ms(&mut self) -> i32 {
        self.compute_display_time_at(Instant::now())
    }

    fn compute_display_time_at(&mut self, now: Instant) -> i32 {
        let target = self.projected_anchor_at(now) as f64;
        if !self.primed {
            self.primed = true;
            self.last_clock = Some(now);
            self.display_ms = target;
            return self.display_ms.round() as i32;
        }

        let dt_ms = self
            .last_clock
            .map(|last| now.saturating_duration_since(last).as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
            .clamp(0.0, CLOCK_MAX_FRAME_MS);
        self.last_clock = Some(now);
        let gap = target - self.display_ms;
        if gap.abs() >= CLOCK_SEEK_SNAP_MS {
            self.display_ms = target;
        } else {
            let base_rate = if self.is_playing { 1.0 } else { 0.0 };
            let rate = (base_rate + gap / CLOCK_RECONCILE_MS).clamp(0.0, CLOCK_MAX_RATE);
            self.display_ms += rate * dt_ms;
            if gap > 0.0 && self.display_ms > target {
                self.display_ms = target;
            }
        }
        self.display_ms.round().clamp(0.0, i32::MAX as f64) as i32
    }

    fn projected_anchor_at(&self, now: Instant) -> i32 {
        if self.is_playing {
            self.anchor_position_ms.saturating_add(
                now.saturating_duration_since(self.anchor_clock)
                    .as_millis()
                    .min(i32::MAX as u128) as i32,
            )
        } else {
            self.anchor_position_ms
        }
    }
}

impl AppConfig {
    fn from_env_args() -> Result<Self, String> {
        let mut lyrics_dir = std::env::var_os("ACCOMPANIST_LYRICS_DIR").map(PathBuf::from);
        let mut recursive = std::env::var_os("ACCOMPANIST_LYRICS_RECURSIVE")
            .and_then(|value| value.into_string().ok())
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));

        let mut args = std::env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_string_lossy().as_ref() {
                "--lyrics-dir" => {
                    let value = args.next().ok_or("--lyrics-dir requires a folder path")?;
                    lyrics_dir = Some(PathBuf::from(value));
                }
                "--recursive" => recursive = true,
                "--help" | "-h" => return Err(help_text()),
                other => return Err(format!("unknown argument `{other}`\n{}", help_text())),
            }
        }

        let lyrics_dir =
            lyrics_dir.ok_or_else(|| format!("lyrics folder is required\n{}", help_text()))?;
        if !lyrics_dir.is_dir() {
            return Err(format!(
                "lyrics folder does not exist: {}",
                lyrics_dir.display()
            ));
        }

        Ok(Self {
            lyrics_dir,
            recursive,
        })
    }
}

fn help_text() -> String {
    "usage: cargo run -r -p lyrics-desktop --bin desktop_lyrics -- --lyrics-dir <folder> [--recursive]\n\
     or set ACCOMPANIST_LYRICS_DIR=<folder>\n\
     set ACCOMPANIST_FRAME_TIMING=1 to log 1s frame phase averages (record/flush/swap + engine)"
        .to_string()
}

fn normalized_size(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    PhysicalSize::new(size.width.max(1), size.height.max(1))
}

fn mouse_wheel_delta_px(delta: MouseScrollDelta, density: f32) -> f32 {
    let value = match delta {
        // Positive wheel values mean moving the wheel away from the user (up).
        // The renderer's positive offset moves toward later lyrics, so invert it.
        MouseScrollDelta::LineDelta(_, y) => -y * WHEEL_LINE_STEP_DP * density.max(0.5),
        // Pixel deltas are already in the physical coordinate system used by the
        // renderer and must not be density-scaled a second time.
        MouseScrollDelta::PixelDelta(position) => -(position.y as f32),
        _ => 0.0,
    };
    value.clamp(-480.0 * density.max(0.5), 480.0 * density.max(0.5))
}

fn track_key(snapshot: &PlaybackSnapshot) -> String {
    normalize_name(&format!("{} {}", snapshot.artist, snapshot.title))
}

fn stable_seed(value: &str) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in value.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

fn missing_lyrics(snapshot: &PlaybackSnapshot) -> lyrics_parser::SyncedLyrics {
    let title = snapshot
        .title
        .trim()
        .to_string()
        .if_empty("未找到当前曲目的同名歌词");
    let artists = if snapshot.artist.trim().is_empty() {
        Vec::new()
    } else {
        vec![lyrics_parser::Artist {
            kind: "Main".to_string(),
            name: snapshot.artist.clone(),
        }]
    };
    lyrics_parser::SyncedLyrics {
        title: title.clone(),
        artists,
        lines: vec![lyrics_parser::SyncedLine::new(
            format!("未找到歌词：{title}"),
            None,
            0,
            snapshot.duration_ms.max(24 * 60 * 60 * 1000),
        )
        .into()],
        ..Default::default()
    }
}

trait StringExt {
    fn if_empty(self, fallback: &str) -> String;
}

impl StringExt for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

fn find_matching_lyrics(config: &AppConfig, snapshot: &PlaybackSnapshot) -> Option<PathBuf> {
    let mut wanted = vec![snapshot.title.trim().to_string()];
    if !snapshot.artist.trim().is_empty() && !snapshot.title.trim().is_empty() {
        wanted.push(format!("{} - {}", snapshot.artist, snapshot.title));
        wanted.push(format!("{} - {}", snapshot.title, snapshot.artist));
    }
    let wanted: Vec<String> = wanted
        .into_iter()
        .map(|name| normalize_name(file_stem_like(&name)))
        .filter(|name| !name.is_empty())
        .collect();

    let mut best = None::<(u32, PathBuf)>;
    visit_lyrics_files(&config.lyrics_dir, config.recursive, &mut |path| {
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            return;
        };
        let normalized = normalize_name(stem);
        let score = wanted
            .iter()
            .map(|candidate| fuzzy_name_score(candidate, &normalized))
            .max()
            .unwrap_or(0);
        if score >= 65
            && best
                .as_ref()
                .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, path.to_path_buf()));
        }
    });
    best.map(|(_, path)| path)
}

fn parse_lyrics_file_with_auto_parser(path: &Path) -> Result<lyrics_parser::SyncedLyrics, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Ok(AutoParser::default().parse(&content))
}

fn visit_lyrics_files(path: &Path, recursive: bool, visitor: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                visit_lyrics_files(&path, recursive, visitor);
            }
            continue;
        }
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(is_supported_lyrics_extension)
        {
            visitor(&path);
        }
    }
}

fn is_supported_lyrics_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "lrc" | "ttml" | "xml" | "yrc" | "krc"
    )
}

fn file_stem_like(name: &str) -> &str {
    Path::new(name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(name)
}

fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter_map(|ch| {
            let ch = ch.to_ascii_lowercase();
            if ch.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&ch) {
                Some(ch)
            } else {
                None
            }
        })
        .collect()
}

fn fuzzy_name_score(expected: &str, candidate: &str) -> u32 {
    if expected.is_empty() || candidate.is_empty() {
        return 0;
    }
    if expected == candidate {
        return 100;
    }
    if candidate.contains(expected) || expected.contains(candidate) {
        let shorter = expected.len().min(candidate.len()) as f32;
        let longer = expected.len().max(candidate.len()) as f32;
        return (70.0 + 25.0 * shorter / longer).round() as u32;
    }

    let lcs = longest_common_subsequence_len(expected, candidate) as f32;
    let expected_len = expected.chars().count().max(1) as f32;
    let candidate_len = candidate.chars().count().max(1) as f32;
    let recall = lcs / expected_len;
    let precision = lcs / candidate_len;
    let length_balance = expected_len.min(candidate_len) / expected_len.max(candidate_len);
    (100.0 * (0.55 * recall + 0.35 * precision + 0.10 * length_balance)).round() as u32
}

fn longest_common_subsequence_len(left: &str, right: &str) -> usize {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    if left.is_empty() || right.is_empty() {
        return 0;
    }

    let mut previous = vec![0usize; right.len() + 1];
    let mut current = vec![0usize; right.len() + 1];
    for left_ch in &left {
        for (j, right_ch) in right.iter().enumerate() {
            current[j + 1] = if left_ch == right_ch {
                previous[j] + 1
            } else {
                previous[j + 1].max(current[j])
            };
        }
        std::mem::swap(&mut previous, &mut current);
        current.fill(0);
    }
    previous[right.len()]
}

fn spawn_smtc_listener(sender: mpsc::Sender<PlaybackSnapshot>) {
    thread::spawn(move || {
        let mut cached_media_key = String::new();
        let mut cached_artwork = None::<Arc<Artwork>>;
        loop {
            if let Ok(snapshot) =
                current_playback_snapshot(&cached_media_key, cached_artwork.clone())
            {
                cached_media_key = media_identity_key(&snapshot);
                cached_artwork = snapshot.artwork.clone();
                // Publish every poll. A paused seek can be rejected without any
                // SMTC property changing; periodic samples let the UI's pending
                // seek guard time out and return to the authoritative position.
                let _ = sender.send(snapshot);
            }
            thread::sleep(SMTC_POLL_INTERVAL);
        }
    });
}

/// Serialize SMTC seeks so an older click can never finish after a newer one.
/// Pending clicks are coalesced before each API call; the final requested target
/// always remains last in the FIFO worker.
fn spawn_smtc_seek_worker() -> (Sender<SeekRequest>, Receiver<SeekResult>) {
    let (sender, receiver) = mpsc::channel::<SeekRequest>();
    let (result_sender, result_receiver) = mpsc::channel();
    thread::spawn(move || {
        while let Ok(mut request) = receiver.recv() {
            while let Ok(newer_request) = receiver.try_recv() {
                request = newer_request;
            }
            #[cfg(windows)]
            let accepted = match seek_smtc_position_ms_windows(request.position_ms) {
                Ok(()) => true,
                Err(error) => {
                    eprintln!("SMTC seek to {}ms failed: {error}", request.position_ms);
                    false
                }
            };
            #[cfg(not(windows))]
            let accepted = {
                eprintln!(
                    "SMTC seek to {}ms ignored: only available on Windows",
                    request.position_ms
                );
                false
            };
            let _ = result_sender.send(SeekResult {
                request_id: request.request_id,
                accepted,
            });
        }
    });
    (sender, result_receiver)
}

#[cfg(windows)]
fn seek_smtc_position_ms_windows(position_ms: i32) -> Result<(), String> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .map_err(|error| error.to_string())?
        .join()
        .map_err(|error| error.to_string())?;
    let session = manager
        .GetCurrentSession()
        .map_err(|error| error.to_string())?;

    let timeline = session
        .GetTimelineProperties()
        .map_err(|error| error.to_string())?;
    let timeline_start = timeline
        .StartTime()
        .map_err(|error| error.to_string())?
        .Duration;
    // Lyrics are relative to the media start, while SMTC positions live in the
    // session's timeline coordinates. Preserve a non-zero StartTime when seeking.
    // TimeSpan ticks are 100 ns; 1 ms = 10_000 ticks.
    let ticks = timeline_start.saturating_add((position_ms as i64).saturating_mul(10_000));
    let ok = session
        .TryChangePlaybackPositionAsync(ticks)
        .map_err(|error| error.to_string())?
        .join()
        .map_err(|error| error.to_string())?;
    if !ok {
        return Err("session rejected playback position change".into());
    }
    Ok(())
}

#[cfg(windows)]
fn current_playback_snapshot(
    cached_media_key: &str,
    cached_artwork: Option<Arc<Artwork>>,
) -> Result<PlaybackSnapshot, String> {
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    };

    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .map_err(|error| error.to_string())?
        .join()
        .map_err(|error| error.to_string())?;
    let session = manager
        .GetCurrentSession()
        .map_err(|error| error.to_string())?;
    let properties = session
        .TryGetMediaPropertiesAsync()
        .map_err(|error| error.to_string())?
        .join()
        .map_err(|error| error.to_string())?;
    let playback = session
        .GetPlaybackInfo()
        .map_err(|error| error.to_string())?;
    let timeline = session
        .GetTimelineProperties()
        .map_err(|error| error.to_string())?;

    let title = properties
        .Title()
        .map_err(|error| error.to_string())?
        .to_string_lossy();
    let artist = properties
        .Artist()
        .map_err(|error| error.to_string())?
        .to_string_lossy();
    let source_app_id = session
        .SourceAppUserModelId()
        .map(|id| id.to_string_lossy())
        .unwrap_or_default();
    let timeline_start_ticks = timeline
        .StartTime()
        .map_err(|error| error.to_string())?
        .Duration;
    let timeline_end_ticks = timeline
        .EndTime()
        .map_err(|error| error.to_string())?
        .Duration;
    let duration_ms = tick_delta_ms(timeline_end_ticks, timeline_start_ticks);
    let media_key = media_identity_key_from_parts(&artist, &title, duration_ms);
    // Always re-fetch when the track identity changes. For the same track, reuse a
    // successful cache, but retry when the previous attempt failed (late-arriving
    // thumbnails from some players).
    let artwork = if media_key == cached_media_key {
        if cached_artwork.is_some() {
            cached_artwork
        } else {
            media_properties_artwork(&properties).map(Arc::new)
        }
    } else {
        media_properties_artwork(&properties).map(Arc::new)
    };

    let is_playing = playback
        .PlaybackStatus()
        .map_err(|error| error.to_string())?
        == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
    let playback_rate = playback
        .PlaybackRate()
        .ok()
        .and_then(|rate| rate.Value().ok())
        .filter(|rate| rate.is_finite() && *rate > 0.0)
        .unwrap_or(1.0);
    let raw_position_ms = tick_delta_ms(
        timeline
            .Position()
            .map_err(|error| error.to_string())?
            .Duration,
        timeline_start_ticks,
    );
    let smtc_update_ticks = timeline
        .LastUpdatedTime()
        .ok()
        .map(|updated| updated.UniversalTime)
        .unwrap_or(0);
    let sample_age_ms = windows_datetime_age_ms(smtc_update_ticks)
        .unwrap_or(0);
    let position_ms = project_smtc_position_ms(
        raw_position_ms,
        sample_age_ms,
        is_playing,
        playback_rate,
        duration_ms,
    );

    Ok(PlaybackSnapshot {
        title,
        artist,
        position_ms,
        duration_ms,
        is_playing,
        source_app_id,
        smtc_update_ticks,
        artwork,
    })
}

fn tick_delta_ms(value_ticks: i64, origin_ticks: i64) -> i32 {
    (value_ticks.saturating_sub(origin_ticks) / 10_000).clamp(0, i32::MAX as i64) as i32
}

fn project_smtc_position_ms(
    position_ms: i32,
    sample_age_ms: i32,
    is_playing: bool,
    playback_rate: f64,
    duration_ms: i32,
) -> i32 {
    let projected = if is_playing {
        let advance = (sample_age_ms.max(0) as f64 * playback_rate.max(0.0))
            .round()
            .clamp(0.0, i32::MAX as f64) as i32;
        position_ms.saturating_add(advance)
    } else {
        position_ms
    };
    if duration_ms > 0 {
        projected.clamp(0, duration_ms)
    } else {
        projected.max(0)
    }
}

#[cfg(windows)]
fn windows_datetime_age_ms(last_updated_ticks: i64) -> Option<i32> {
    if last_updated_ticks <= 0 {
        return None;
    }
    let unix_ticks = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos()
        .checked_div(100)?
        .min(i64::MAX as u128) as i64;
    let now_ticks = WINDOWS_TO_UNIX_EPOCH_TICKS.saturating_add(unix_ticks);
    let age_ticks = now_ticks.checked_sub(last_updated_ticks)?;
    if age_ticks < 0 {
        return None;
    }
    Some((age_ticks / 10_000).min(i32::MAX as i64) as i32)
}

#[cfg(windows)]
fn media_properties_artwork(
    properties: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<Artwork> {
    let thumbnail = properties.Thumbnail().ok()?;
    let stream = thumbnail.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()?.min(32 * 1024 * 1024);
    if size == 0 {
        return None;
    }

    // Prefer the full buffer API when available; fall back to DataReader.
    let mut bytes = vec![0u8; size as usize];
    if let Ok(input) = stream.GetInputStreamAt(0) {
        if let Ok(reader) = windows::Storage::Streams::DataReader::CreateDataReader(&input) {
            let loaded = reader.LoadAsync(size as u32).ok()?.join().ok()?;
            if loaded == 0 {
                return None;
            }
            bytes.truncate(loaded as usize);
            reader.ReadBytes(&mut bytes).ok()?;
            return decode_artwork(&bytes);
        }
    }
    None
}

fn decode_artwork(bytes: &[u8]) -> Option<Artwork> {
    use image::GenericImageView;

    let image = match image::load_from_memory(bytes) {
        Ok(image) => image,
        Err(error) => {
            // Log once per failure path so missing codecs / corrupt streams are visible.
            eprintln!(
                "artwork decode failed ({} bytes, magic={:02x?}): {error}",
                bytes.len(),
                bytes.iter().take(8).cloned().collect::<Vec<_>>()
            );
            return None;
        }
    };

    // Downscale very large covers before ARGB conversion — mesh only needs 32² and
    // the top-bar thumb is capped at 512; this also avoids Skia upload failures.
    let image = {
        let (w, h) = image.dimensions();
        let max_edge = w.max(h);
        if max_edge > ARTWORK_MAX_EDGE {
            let scale = ARTWORK_MAX_EDGE as f32 / max_edge as f32;
            let nw = ((w as f32 * scale).round() as u32).max(1);
            let nh = ((h as f32 * scale).round() as u32).max(1);
            image.resize(nw, nh, image::imageops::FilterType::Triangle)
        } else {
            image
        }
    };

    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }

    let mut hash = 0xcbf29ce484222325u64;
    let pixels = rgba
        .pixels()
        .map(|pixel| {
            let [r, g, b, a] = pixel.0;
            let argb = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
            hash ^= argb as u64;
            hash = hash.wrapping_mul(0x100000001b3);
            argb
        })
        .collect();
    Some(Artwork {
        pixels,
        width: width as usize,
        height: height as usize,
        hash,
    })
}

#[cfg(not(windows))]
fn current_playback_snapshot(
    _cached_media_key: &str,
    _cached_artwork: Option<Arc<Artwork>>,
) -> Result<PlaybackSnapshot, String> {
    Err("SMTC is only available on Windows".to_string())
}

fn media_identity_key(snapshot: &PlaybackSnapshot) -> String {
    media_identity_key_from_parts(&snapshot.artist, &snapshot.title, snapshot.duration_ms)
}

fn media_identity_key_from_parts(artist: &str, title: &str, duration_ms: i32) -> String {
    format!(
        "{}:{}",
        normalize_name(&format!("{artist} {title}")),
        duration_ms
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_names_for_case_and_symbols() {
        assert_eq!(normalize_name("Artist - Song!.lrc"), "artistsonglrc");
        assert_eq!(normalize_name("周杰伦 - 稻香"), "周杰伦稻香");
    }

    #[test]
    fn fuzzy_score_accepts_common_filename_variants() {
        let expected = normalize_name("Artist - Song");
        assert_eq!(
            fuzzy_name_score(&expected, &normalize_name("Artist - Song")),
            100
        );
        assert!(fuzzy_name_score(&expected, &normalize_name("Artist - Song (Live)")) >= 65);
        assert!(fuzzy_name_score(&normalize_name("Song"), &normalize_name("Artist - Song")) >= 65);
        assert!(fuzzy_name_score(&expected, &normalize_name("Completely Different")) < 65);
    }

    #[test]
    fn stable_seed_is_repeatable() {
        assert_eq!(stable_seed("artistsong"), stable_seed("artistsong"));
        assert_ne!(stable_seed("artistsong"), stable_seed("othersong"));
    }

    #[test]
    fn caption_font_tower_resolves_cjk_to_real_glyphs() {
        let Some(tower) = make_caption_font_tower(1.0) else {
            return;
        };

        for cluster in ["中", "日", "한"] {
            let typeface = tower.typeface_for_cluster(cluster);
            let ch = cluster.chars().next().unwrap();
            assert_ne!(
                typeface.unichar_to_glyph(ch as i32),
                0,
                "caption fallback returned a missing glyph for {cluster}"
            );
        }

        let layout = tower.layout("Caption 中文 日本語 한국어");
        assert!(layout.width > 0.0);
        assert!(layout.ascent < 0.0);
        assert!(layout.runs.iter().all(|run| run
            .font
            .text_to_glyphs_vec(&run.text)
            .into_iter()
            .all(|glyph| glyph != 0)));
    }

    #[test]
    fn caption_ellipsize_measures_the_resolved_font_runs() {
        let Some(tower) = make_caption_font_tower(1.0) else {
            return;
        };
        let max_width = tower.layout("中文标题…").width;
        let layout = tower.layout_ellipsized("中文标题与很长的艺术家名称", max_width);

        assert!(layout.width <= max_width + 0.01);
        assert!(layout.runs.iter().any(|run| run.text.contains('…')));
        assert!(layout.runs.iter().all(|run| run
            .font
            .text_to_glyphs_vec(&run.text)
            .into_iter()
            .all(|glyph| glyph != 0)));
    }

    #[test]
    fn caption_chrome_is_hidden_without_hover() {
        assert!(!caption_bar_visible(CaptionHit::None));
        assert!(caption_bar_visible(CaptionHit::Drag));
        assert!(caption_bar_visible(CaptionHit::AlwaysOnTop));
        assert!(caption_bar_visible(CaptionHit::Minimize));
        assert!(caption_bar_visible(CaptionHit::Close));
    }

    #[test]
    fn caption_chrome_fades_in_and_out() {
        let start = Instant::now();
        let mut fade = CaptionFade::default();

        fade.set_visible(true, start);
        let (alpha, animating) = fade.sample(
            start + Duration::from_secs_f32(CAPTION_FADE_IN_MS / 2000.0),
        );
        assert!(animating);
        assert!(alpha > 0.0 && alpha < 1.0);
        let (alpha, animating) =
            fade.sample(start + Duration::from_secs_f32(CAPTION_FADE_IN_MS / 1000.0));
        assert!(!animating);
        assert_eq!(alpha, 1.0);

        let hide_at = start + Duration::from_millis(500);
        fade.set_visible(false, hide_at);
        let (alpha, animating) =
            fade.sample(hide_at + Duration::from_secs_f32(CAPTION_FADE_OUT_MS / 2000.0));
        assert!(animating);
        assert!(alpha > 0.0 && alpha < 1.0);
        let (alpha, animating) =
            fade.sample(hide_at + Duration::from_secs_f32(CAPTION_FADE_OUT_MS / 1000.0));
        assert!(!animating);
        assert_eq!(alpha, 0.0);
    }

    #[test]
    fn pending_seek_rejects_old_poll_then_accepts_acknowledgement() {
        let issued_at = Instant::now();
        let pending = PendingSeek {
            request_id: 1,
            target_position_ms: 30_000,
            issued_at,
            api_accepted: false,
        };

        assert!(!pending.accepts(10_000, true, issued_at + Duration::from_millis(500)));
        assert!(pending.accepts(30_500, true, issued_at + Duration::from_millis(500)));
        assert!(pending.accepts(10_000, false, issued_at + SEEK_REQUEST_TIMEOUT));

        let accepted = PendingSeek {
            api_accepted: true,
            ..pending
        };
        assert!(!accepted.accepts(
            10_000,
            false,
            issued_at + SEEK_ACCEPTED_ACK_TIMEOUT - Duration::from_millis(1)
        ));
        assert!(accepted.accepts(
            10_000,
            false,
            issued_at + SEEK_ACCEPTED_ACK_TIMEOUT
        ));
    }

    #[test]
    fn smtc_timeline_is_normalized_and_projected_from_last_update() {
        assert_eq!(tick_delta_ms(125_000_000, 25_000_000), 10_000);
        assert_eq!(
            project_smtc_position_ms(10_000, 750, true, 1.0, 60_000),
            10_750
        );
        assert_eq!(
            project_smtc_position_ms(59_800, 750, true, 1.0, 60_000),
            60_000
        );
        assert_eq!(
            project_smtc_position_ms(10_000, 750, false, 1.0, 60_000),
            10_000
        );
    }

    #[test]
    fn apple_music_clock_averages_distinct_smtc_update_times() {
        let start = Instant::now();
        let mut clock = AppleMusicClock::new(45_068, 1, true, start);

        // Captured from Apple Music: raw Position advances in one-second steps,
        // while LastUpdatedTime resets independently. The projected values are
        // therefore non-uniform and can even move backward. Repeated polls for
        // the same update do not add weight to the average.
        let (same_update, reanchored) =
            clock.publish_at(45_569, 1, true, start + Duration::from_millis(500));
        assert!(!reanchored);
        assert_eq!(same_update, 45_568);
        assert_eq!(clock.base_samples_ms.len(), 1);

        let (second_update, reanchored) =
            clock.publish_at(46_301, 2, true, start + Duration::from_millis(751));
        assert!(!reanchored);
        assert_eq!(second_update, 46_060);
        assert_eq!(clock.base_samples_ms.len(), 2);

        let (backward_sample, reanchored) =
            clock.publish_at(46_258, 3, true, start + Duration::from_millis(1_251));
        assert!(!reanchored);
        assert_eq!(backward_sample, 46_459);
        assert_eq!(clock.base_samples_ms.len(), 3);
    }

    #[test]
    fn apple_music_clock_reanchors_on_seek_and_playback_boundaries() {
        let start = Instant::now();
        let mut clock = AppleMusicClock::new(10_000, 1, true, start);

        let jitter_at = start + Duration::from_millis(500);
        let (_, reanchored) = clock.publish_at(11_300, 2, true, jitter_at);
        assert!(!reanchored, "Apple cadence jitter is not a seek");

        let seek_at = start + Duration::from_millis(1_000);
        let (position, reanchored) = clock.publish_at(20_000, 2, true, seek_at);
        assert!(reanchored);
        assert_eq!(position, 20_000);
        assert_eq!(clock.base_samples_ms.len(), 1);

        let pause_at = seek_at + Duration::from_millis(300);
        let (position, reanchored) = clock.publish_at(20_300, 3, false, pause_at);
        assert!(reanchored);
        assert_eq!(position, 20_300);
        assert_eq!(clock.position_at(pause_at + Duration::from_secs(2)), 20_300);

        let resume_at = pause_at + Duration::from_secs(2);
        let (position, reanchored) = clock.publish_at(20_300, 4, true, resume_at);
        assert!(reanchored);
        assert_eq!(position, 20_300);
        assert_eq!(clock.position_at(resume_at + Duration::from_millis(250)), 20_550);
    }

    #[test]
    fn non_apple_sources_keep_dynamic_clock_chasing() {
        assert!(is_apple_music_source(
            "AppleInc.AppleMusicWin_nzyj5cx40ttqa!App"
        ));
        assert!(!is_apple_music_source("Spotify.exe"));

        let start = Instant::now();
        let mut clock = PlaybackClock::default();
        clock.publish_smtc_sample_at(10_000, true, "Spotify.exe", 1, start);
        assert_eq!(clock.compute_display_time_at(start), 10_000);

        let update_at = start + Duration::from_millis(100);
        clock.publish_smtc_sample_at(10_400, true, "Spotify.exe", 2, update_at);
        let display = clock.compute_display_time_at(update_at);
        assert!(display > 10_100 && display < 10_400);
        assert!(clock.apple_music.is_none());
    }

    #[test]
    fn mouse_wheel_delta_matches_renderer_scroll_direction_and_units() {
        assert_eq!(
            mouse_wheel_delta_px(MouseScrollDelta::LineDelta(0.0, 3.0), 2.0),
            -108.0
        );
        assert_eq!(
            mouse_wheel_delta_px(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -24.0)),
                2.0,
            ),
            24.0
        );
    }
}
