//! Per-app WASAPI process loopback → `lyrics_renderer::audio`.
//!
//! When SMTC reports a playing session, we resolve the session's
//! `SourceAppUserModelId` to a process id and open
//! [`AudioClient::new_application_loopback_client`] (Windows 10 2004+).
//! That captures **only that process tree** — not the system mix.

#![cfg(windows)]

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use wasapi::{initialize_mta, AudioClient, Direction, SampleType, StreamMode, WaveFormat};

const CAPTURE_RATE: usize = 48_000;
const CAPTURE_CHANNELS: usize = 2;
const CAPTURE_BITS: usize = 32;

/// Shared switches for the capture thread.
#[derive(Debug)]
pub struct CaptureControl {
    capturing: AtomicBool,
    stop: AtomicBool,
    /// Target process for loopback (0 = unresolved).
    target_pid: AtomicU32,
    /// SMTC SourceAppUserModelId used to (re)resolve `target_pid`.
    source_app_id: Mutex<String>,
}

impl CaptureControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            capturing: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            target_pid: AtomicU32::new(0),
            source_app_id: Mutex::new(String::new()),
        })
    }

    /// Update capture target from SMTC and whether analysis should run.
    pub fn update_session(&self, source_app_id: &str, is_playing: bool) {
        {
            let mut guard = self.source_app_id.lock().unwrap_or_else(|e| e.into_inner());
            if guard.as_str() != source_app_id {
                *guard = source_app_id.to_string();
                self.target_pid.store(0, Ordering::SeqCst);
            }
        }
        if is_playing && !source_app_id.is_empty() {
            if self.target_pid.load(Ordering::SeqCst) == 0 {
                if let Some(pid) = resolve_process_id(source_app_id) {
                    eprintln!("[audio] resolved app `{source_app_id}` → pid {pid}");
                    self.target_pid.store(pid, Ordering::SeqCst);
                } else {
                    eprintln!("[audio] could not resolve process for app `{source_app_id}`");
                }
            }
        }
        let was = self.capturing.swap(is_playing, Ordering::SeqCst);
        if was && !is_playing {
            lyrics_renderer::audio::reset();
        }
        if is_playing && self.target_pid.load(Ordering::SeqCst) == 0 {
            // Keep trying to resolve while playing.
            if let Some(pid) = resolve_process_id(source_app_id) {
                eprintln!("[audio] resolved app `{source_app_id}` → pid {pid}");
                self.target_pid.store(pid, Ordering::SeqCst);
            }
        }
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.capturing.store(false, Ordering::SeqCst);
    }

    fn should_stop(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn is_capturing(&self) -> bool {
        self.capturing.load(Ordering::Relaxed)
    }

    fn pid(&self) -> u32 {
        self.target_pid.load(Ordering::SeqCst)
    }

    fn aumid(&self) -> String {
        self.source_app_id
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

pub fn spawn(control: Arc<CaptureControl>) {
    thread::Builder::new()
        .name("app-loopback".into())
        .spawn(move || {
            if let Err(error) = capture_loop(control) {
                eprintln!("[audio] capture thread exited: {error}");
            }
        })
        .ok();
}

fn capture_loop(control: Arc<CaptureControl>) -> Result<(), String> {
    // HRESULT: S_OK / S_FALSE both fine for COM apartment init.
    let hr = initialize_mta();
    if hr.is_err() {
        return Err(format!("initialize_mta failed: {hr:?}"));
    }
    lyrics_renderer::audio::set_sample_rate(CAPTURE_RATE as f32);

    while !control.should_stop() {
        if !control.is_capturing() {
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        let mut pid = control.pid();
        if pid == 0 {
            let aumid = control.aumid();
            if let Some(resolved) = resolve_process_id(&aumid) {
                eprintln!("[audio] resolved app `{aumid}` → pid {resolved}");
                control.target_pid.store(resolved, Ordering::SeqCst);
                pid = resolved;
            } else {
                thread::sleep(Duration::from_millis(400));
                continue;
            }
        }

        match run_process_loopback(pid, &control) {
            Ok(()) => {}
            Err(error) => {
                eprintln!("[audio] process loopback pid={pid} failed: {error}");
                // Force re-resolve next time (process may have restarted).
                control.target_pid.store(0, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
    Ok(())
}

fn run_process_loopback(process_id: u32, control: &CaptureControl) -> Result<(), String> {
    // Float stereo @ 48 kHz — analyser is rate-agnostic enough; we set sample rate above.
    let desired = WaveFormat::new(
        CAPTURE_BITS,
        CAPTURE_BITS,
        &SampleType::Float,
        CAPTURE_RATE,
        CAPTURE_CHANNELS,
        None,
    );
    let block_align = desired.get_blockalign() as usize;
    let include_tree = true;

    let mut client = AudioClient::new_application_loopback_client(process_id, include_tree)
        .map_err(|e| format!("new_application_loopback_client: {e}"))?;

    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 0,
    };
    client
        .initialize_client(&desired, &Direction::Capture, &mode)
        .map_err(|e| format!("initialize_client: {e}"))?;

    let event = client
        .set_get_eventhandle()
        .map_err(|e| format!("set_get_eventhandle: {e}"))?;
    let capture = client
        .get_audiocaptureclient()
        .map_err(|e| format!("get_audiocaptureclient: {e}"))?;

    client
        .start_stream()
        .map_err(|e| format!("start_stream: {e}"))?;

    eprintln!(
        "[audio] app process loopback started (pid={process_id}, tree={include_tree}, {CAPTURE_RATE} Hz float stereo)"
    );

    let mut queue: VecDeque<u8> = VecDeque::with_capacity(CAPTURE_RATE * block_align / 10);
    let mut mono = Vec::with_capacity(2048);
    let active_pid = process_id;

    while control.is_capturing() && !control.should_stop() {
        // Retarget if SMTC pointed us at another process.
        if control.pid() != 0 && control.pid() != active_pid {
            break;
        }

        let new_frames = capture
            .get_next_packet_size()
            .map_err(|e| format!("get_next_packet_size: {e}"))?
            .unwrap_or(0);
        if new_frames > 0 {
            let need = new_frames as usize * block_align;
            let extra = need.saturating_sub(queue.capacity().saturating_sub(queue.len()));
            queue.reserve(extra);
            capture
                .read_from_device_to_deque(&mut queue)
                .map_err(|e| format!("read_from_device_to_deque: {e}"))?;
        }

        // Drain complete frames → mono f32 → analyser.
        while queue.len() >= block_align * 256 {
            let frames = 256;
            let bytes = frames * block_align;
            mono.clear();
            for frame in 0..frames {
                let base = frame * block_align;
                // 32-bit float LE stereo
                let mut sum = 0.0f32;
                for ch in 0..CAPTURE_CHANNELS {
                    let i = base + ch * 4;
                    if i + 4 > queue.len() {
                        break;
                    }
                    let sample =
                        f32::from_le_bytes([queue[i], queue[i + 1], queue[i + 2], queue[i + 3]]);
                    sum += sample;
                }
                mono.push(sum / CAPTURE_CHANNELS as f32);
            }
            for _ in 0..bytes {
                queue.pop_front();
            }
            if !mono.is_empty() {
                lyrics_renderer::audio::push_pcm(&mono);
            }
        }

        if event.wait_for_event(200).is_err() {
            // Timeout: still loop while capturing (app may be silent).
            continue;
        }
    }

    let _ = client.stop_stream();
    eprintln!("[audio] app process loopback stopped (pid={active_pid})");
    Ok(())
}

/// Map SMTC `SourceAppUserModelId` → a process id suitable for process loopback.
fn resolve_process_id(source_app_id: &str) -> Option<u32> {
    if source_app_id.is_empty() {
        return None;
    }

    let hints = process_name_hints(source_app_id);
    if hints.is_empty() {
        return None;
    }

    let refreshes = RefreshKind::nothing().with_processes(ProcessRefreshKind::everything());
    let mut system = System::new_with_specifics(refreshes);
    system.refresh_processes(ProcessesToUpdate::All, true);

    // Prefer exact executable name match (e.g. Spotify.exe).
    for hint in &hints {
        let hint_os = OsStr::new(hint.as_str());
        for process in system.processes_by_name(hint_os) {
            // Capture the process tree from the parent when available (wasapi sample).
            let pid = process
                .parent()
                .map(|p| p.as_u32())
                .unwrap_or_else(|| process.pid().as_u32());
            if pid != 0 {
                return Some(pid);
            }
        }
    }

    // Fallback: substring match on process name (UWP / package AUMIDs).
    for (_pid, process) in system.processes() {
        let name = process.name().to_string_lossy().to_ascii_lowercase();
        for hint in &hints {
            let h = hint.to_ascii_lowercase();
            let h_stem = h.trim_end_matches(".exe");
            if name == h || name == h_stem || name.contains(h_stem) {
                let pid = process
                    .parent()
                    .map(|p| p.as_u32())
                    .unwrap_or_else(|| process.pid().as_u32());
                if pid != 0 {
                    return Some(pid);
                }
            }
        }
    }

    None
}

/// Derive process-name candidates from an AUMID.
///
/// Examples:
/// - `Spotify.exe` → `["Spotify.exe", "Spotify"]`
/// - `Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic` → package / app tokens
fn process_name_hints(aumid: &str) -> Vec<String> {
    let mut hints = Vec::new();
    let push = |hints: &mut Vec<String>, s: &str| {
        let s = s.trim();
        if s.is_empty() {
            return;
        }
        if !hints.iter().any(|h| h.eq_ignore_ascii_case(s)) {
            hints.push(s.to_string());
        }
    };

    push(&mut hints, aumid);

    if let Some((pkg, app)) = aumid.split_once('!') {
        // package family!AppId
        let family = pkg.split('_').next().unwrap_or(pkg);
        if let Some(short) = family.rsplit('.').next() {
            push(&mut hints, short);
            push(&mut hints, &format!("{short}.exe"));
        }
        push(&mut hints, app);
        push(&mut hints, &format!("{app}.exe"));
    } else if aumid.ends_with(".exe") {
        push(&mut hints, aumid.trim_end_matches(".exe"));
    } else {
        push(&mut hints, &format!("{aumid}.exe"));
    }

    hints
}
