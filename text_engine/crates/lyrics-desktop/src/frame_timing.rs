//! Optional frame-phase timing for diagnosing CPU record vs flush vs swap costs.
//!
//! Enable with `ACCOMPANIST_FRAME_TIMING=1` (or `true`). Logs a rolling 1s average
//! to stderr:
//!
//! ```text
//! [frame] n=60 total=16.2 (max 22.1) | record=9.4 [typefaces=0.0 layout=0.8 bg=4.1 lyrics=4.2 top=0.1 caption=0.1] flush=1.2 swap=5.4
//! ```
//!
//! Interpretation tips:
//! - **swap large & stable (~16ms)**: mostly waiting on vsync (healthy if total≈16)
//! - **record large**: CPU-side Skia recording / mesh breathe / blur layers
//! - **flush large, GPU% low**: submit/sync bubbles (pipeline stall)
//! - **bg or lyrics dominate record**: target those phases first

use lyrics_renderer::EngineFrameTiming;
use std::time::{Duration, Instant};

/// True when `ACCOMPANIST_FRAME_TIMING` is `1` / `true` / `yes`.
pub fn frame_timing_enabled() -> bool {
    std::env::var_os("ACCOMPANIST_FRAME_TIMING")
        .and_then(|value| value.into_string().ok())
        .is_some_and(|value| {
            value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
        })
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GpuFrameTiming {
    /// Host draw callback (clear + engine + caption) — Skia command recording.
    pub record_ms: f64,
    /// `flush_and_submit_surface` — push recorded ops to the GPU.
    pub flush_ms: f64,
    /// `swap_buffers` — present; blocks on vblank when vsync is on.
    pub swap_ms: f64,
    pub total_ms: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameSample {
    pub engine: EngineFrameTiming,
    pub caption_ms: f64,
    pub gpu: GpuFrameTiming,
}

/// Accumulates samples and prints a 1 Hz summary.
#[derive(Debug)]
pub struct FrameTimingLogger {
    enabled: bool,
    window_start: Instant,
    samples: Vec<FrameSample>,
}

impl FrameTimingLogger {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            window_start: Instant::now(),
            samples: Vec::with_capacity(128),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn record(&mut self, sample: FrameSample) {
        if !self.enabled {
            return;
        }
        self.samples.push(sample);
        if self.window_start.elapsed() >= Duration::from_secs(1) {
            self.flush_window();
        }
    }

    fn flush_window(&mut self) {
        let n = self.samples.len();
        if n == 0 {
            self.window_start = Instant::now();
            return;
        }

        let mut sum = FrameAccum::default();
        let mut max_total = 0.0_f64;
        for sample in &self.samples {
            sum.add(sample);
            max_total = max_total.max(sample.gpu.total_ms);
        }
        let inv = 1.0 / n as f64;
        let avg = sum.scale(inv);

        eprintln!(
            "[frame] n={n} fps≈{:.0} total={:.2} (max {:.2}) | record={:.2} [tf={:.2} layout={:.2} bg={:.2} lyrics={:.2} top={:.2} caption={:.2}] flush={:.2} swap={:.2}",
            n as f64 / self.window_start.elapsed().as_secs_f64().max(0.001),
            avg.gpu.total_ms,
            max_total,
            avg.gpu.record_ms,
            avg.engine.typefaces_ms,
            avg.engine.layout_ms,
            avg.engine.background_ms,
            avg.engine.lyrics_ms,
            avg.engine.top_bar_ms,
            avg.caption_ms,
            avg.gpu.flush_ms,
            avg.gpu.swap_ms,
        );

        self.samples.clear();
        self.window_start = Instant::now();
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct FrameAccum {
    engine: EngineFrameTiming,
    caption_ms: f64,
    gpu: GpuFrameTiming,
}

impl FrameAccum {
    fn add(&mut self, sample: &FrameSample) {
        self.engine.typefaces_ms += sample.engine.typefaces_ms;
        self.engine.layout_ms += sample.engine.layout_ms;
        self.engine.background_ms += sample.engine.background_ms;
        self.engine.lyrics_ms += sample.engine.lyrics_ms;
        self.engine.top_bar_ms += sample.engine.top_bar_ms;
        self.engine.total_ms += sample.engine.total_ms;
        self.caption_ms += sample.caption_ms;
        self.gpu.record_ms += sample.gpu.record_ms;
        self.gpu.flush_ms += sample.gpu.flush_ms;
        self.gpu.swap_ms += sample.gpu.swap_ms;
        self.gpu.total_ms += sample.gpu.total_ms;
    }

    fn scale(self, inv: f64) -> FrameSample {
        FrameSample {
            engine: EngineFrameTiming {
                typefaces_ms: self.engine.typefaces_ms * inv,
                layout_ms: self.engine.layout_ms * inv,
                background_ms: self.engine.background_ms * inv,
                lyrics_ms: self.engine.lyrics_ms * inv,
                top_bar_ms: self.engine.top_bar_ms * inv,
                total_ms: self.engine.total_ms * inv,
            },
            caption_ms: self.caption_ms * inv,
            gpu: GpuFrameTiming {
                record_ms: self.gpu.record_ms * inv,
                flush_ms: self.gpu.flush_ms * inv,
                swap_ms: self.gpu.swap_ms * inv,
                total_ms: self.gpu.total_ms * inv,
            },
        }
    }
}
