#![cfg(not(target_os = "android"))]

mod gpu;

use gpu::DesktopGpuRenderer;
use lyrics_parser::parser::{auto_parser::AutoParser, lyrics_parser::LyricsParser};
use lyrics_renderer::TextEngine;
use skia_safe::Color;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tao::dpi::{LogicalSize, PhysicalSize};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;

const DEFAULT_WIDTH: u32 = 900;
const DEFAULT_HEIGHT: u32 = 260;
const SMTC_POLL_INTERVAL: Duration = Duration::from_millis(500);
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const CLOCK_SEEK_SNAP_MS: f64 = 1000.0;
const CLOCK_RECONCILE_MS: f64 = 350.0;
const CLOCK_MAX_RATE: f64 = 2.5;
const CLOCK_MAX_FRAME_MS: f64 = 64.0;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PlaybackSnapshot {
    title: String,
    artist: String,
    position_ms: i32,
    duration_ms: i32,
    is_playing: bool,
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

pub fn run() -> Result<(), String> {
    env_logger::try_init().ok();

    let config = AppConfig::from_env_args()?;
    let (playback_tx, playback_rx) = mpsc::channel();
    spawn_smtc_listener(playback_tx);

    let event_loop = EventLoop::<()>::new();
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Accompanist Desktop Lyrics")
            .with_inner_size(LogicalSize::new(
                DEFAULT_WIDTH as f64,
                DEFAULT_HEIGHT as f64,
            ))
            .with_min_inner_size(LogicalSize::new(360.0, 120.0))
            .with_resizable(true)
            .with_decorations(false)
            .with_transparent(false)
            .with_always_on_top(true)
            .build(&event_loop)
            .map_err(|error| format!("failed to create tao window: {error}"))?,
    );

    let mut renderer = DesktopGpuRenderer::new(&window)?;

    let mut app = DesktopLyricsApp::new(config, playback_rx);
    app.install_placeholder_scene(window.inner_size());

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + FRAME_INTERVAL);

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized(size) => {
                    app.resize(size);
                    if let Err(error) = renderer.resize(size) {
                        eprintln!("{error}");
                    }
                    window.request_redraw();
                }
                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    app.resize(*new_inner_size);
                    if let Err(error) = renderer.resize(*new_inner_size) {
                        eprintln!("{error}");
                    }
                    window.request_redraw();
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                app.drain_playback_updates();
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                if let Err(error) = app.render(&mut renderer, window.inner_size()) {
                    eprintln!("{error}");
                }
            }
            _ => {}
        }
    });
}

struct DesktopLyricsApp {
    config: AppConfig,
    playback_rx: Receiver<PlaybackSnapshot>,
    engine: TextEngine,
    clock: PlaybackClock,
    current_track_key: Option<String>,
    current_artwork: Option<Arc<Artwork>>,
    current_lyrics: Option<lyrics_parser::SyncedLyrics>,
    last_size: PhysicalSize<u32>,
}

impl DesktopLyricsApp {
    fn new(config: AppConfig, playback_rx: Receiver<PlaybackSnapshot>) -> Self {
        let mut engine = TextEngine::new(2048, 2048);
        engine.load_system_fonts();

        Self {
            config,
            playback_rx,
            engine,
            clock: PlaybackClock::default(),
            current_track_key: None,
            current_artwork: None,
            current_lyrics: None,
            last_size: PhysicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT),
        }
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

    fn resize(&mut self, size: PhysicalSize<u32>) {
        let size = normalized_size(size);
        if self.last_size == size {
            return;
        }
        self.last_size = size;
        if let Some(lyrics) = self.current_lyrics.clone() {
            self.set_lyrics_scene(&lyrics);
        }
    }

    fn drain_playback_updates(&mut self) {
        let mut latest = None;
        while let Ok(snapshot) = self.playback_rx.try_recv() {
            latest = Some(snapshot);
        }

        let Some(snapshot) = latest else {
            return;
        };

        let track_key = track_key(&snapshot);
        let track_changed = self.current_track_key.as_deref() != Some(track_key.as_str());
        self.clock
            .publish_sample(snapshot.position_ms, snapshot.is_playing);

        self.engine
            .set_playback_state(snapshot.is_playing, snapshot.artwork.is_some());
        self.update_background_art(&snapshot, &track_key);

        if !track_changed {
            return;
        }

        self.current_track_key = Some(track_key);
        let lyrics = find_matching_lyrics(&self.config, &snapshot)
            .and_then(|path| parse_lyrics_file_with_auto_parser(&path).ok())
            .unwrap_or_else(|| missing_lyrics(&snapshot));
        self.set_lyrics_scene(&lyrics);
        self.current_lyrics = Some(lyrics);
    }

    fn update_background_art(&mut self, snapshot: &PlaybackSnapshot, track_key: &str) {
        if self.current_artwork == snapshot.artwork {
            return;
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
    }

    fn set_lyrics_scene(&mut self, lyrics: &lyrics_parser::SyncedLyrics) {
        let json = lyrics_parser::scene_json(lyrics, self.last_size.width, self.last_size.height);
        let result = self.engine.set_lyrics_scene_json(&json);
        if result.contains("\"error\"") {
            eprintln!("failed to set lyrics scene: {result}");
        }
    }

    fn render(
        &mut self,
        renderer: &mut DesktopGpuRenderer,
        size: PhysicalSize<u32>,
    ) -> Result<(), String> {
        let size = normalized_size(size);
        if self.last_size != size {
            self.resize(size);
        }
        renderer.resize(size)?;
        let current_time_ms = self.clock.compute_display_time_ms();
        renderer.draw_frame(|canvas| {
            canvas.clear(Color::from_argb(255, 18, 18, 18));
            self.engine
                .render_lyrics_frame_to_canvas(current_time_ms, canvas)
        })?;
        Ok(())
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
        }
    }
}

impl PlaybackClock {
    fn publish_sample(&mut self, position_ms: i32, is_playing: bool) {
        let now = Instant::now();
        if self.is_playing != is_playing {
            if self.is_playing {
                self.anchor_position_ms = self.projected_anchor_at(now);
            }
            self.anchor_clock = now;
            self.is_playing = is_playing;
        }
        if self.anchor_position_ms != position_ms {
            self.anchor_position_ms = position_ms;
            self.anchor_clock = now;
        }
    }

    fn compute_display_time_ms(&mut self) -> i32 {
        let now = Instant::now();
        let target = self.projected_anchor_at(now) as f64;
        if !self.primed {
            self.primed = true;
            self.last_clock = Some(now);
            self.display_ms = target;
            return self.display_ms.round() as i32;
        }

        let dt_ms = self
            .last_clock
            .map(|last| now.duration_since(last).as_secs_f64() * 1000.0)
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
                now.duration_since(self.anchor_clock)
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
    "usage: cargo run --features desktop --bin desktop_lyrics -- --lyrics-dir <folder> [--recursive]\n\
     or set ACCOMPANIST_LYRICS_DIR=<folder>"
        .to_string()
}

fn normalized_size(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    PhysicalSize::new(size.width.max(1), size.height.max(1))
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
        let mut last = PlaybackSnapshot::default();
        let mut cached_media_key = String::new();
        let mut cached_artwork = None::<Arc<Artwork>>;
        loop {
            if let Ok(snapshot) =
                current_playback_snapshot(&cached_media_key, cached_artwork.clone())
            {
                cached_media_key = media_identity_key(&snapshot);
                cached_artwork = snapshot.artwork.clone();
                if snapshot != last {
                    last = snapshot.clone();
                    let _ = sender.send(snapshot);
                }
            }
            thread::sleep(SMTC_POLL_INTERVAL);
        }
    });
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
    let duration_ms = timespan_ms(timeline.EndTime().map_err(|error| error.to_string())?);
    let media_key = media_identity_key_from_parts(&artist, &title, duration_ms);
    let artwork = if media_key == cached_media_key && cached_artwork.is_some() {
        cached_artwork
    } else {
        media_properties_artwork(&properties).map(Arc::new)
    };

    Ok(PlaybackSnapshot {
        title,
        artist,
        position_ms: timespan_ms(timeline.Position().map_err(|error| error.to_string())?),
        duration_ms,
        is_playing: playback
            .PlaybackStatus()
            .map_err(|error| error.to_string())?
            == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing,
        artwork,
    })
}

#[cfg(windows)]
fn timespan_ms(value: windows::Foundation::TimeSpan) -> i32 {
    (value.Duration / 10_000).clamp(0, i32::MAX as i64) as i32
}

#[cfg(windows)]
fn media_properties_artwork(
    properties: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<Artwork> {
    let thumbnail = properties.Thumbnail().ok()?;
    let stream = thumbnail.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()?.min(16 * 1024 * 1024);
    if size == 0 {
        return None;
    }

    let input = stream.GetInputStreamAt(0).ok()?;
    let reader = windows::Storage::Streams::DataReader::CreateDataReader(&input).ok()?;
    let loaded = reader.LoadAsync(size as u32).ok()?.join().ok()?;
    if loaded == 0 {
        return None;
    }

    let mut bytes = vec![0u8; loaded as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    decode_artwork(&bytes)
}

fn decode_artwork(bytes: &[u8]) -> Option<Artwork> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return None;
    }

    let mut hash = 0xcbf29ce484222325u64;
    let pixels = image
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
}
