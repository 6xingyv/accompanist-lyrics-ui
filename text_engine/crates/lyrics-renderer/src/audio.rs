//! Real-time audio analysis (loudness / pitch / BPM), ported from the reference
//! `clef` `audio-tools` crate. PCM float samples are pushed in from the Kotlin side
//! (an ExoPlayer `TeeAudioProcessor`, same process) and analysed with a 1024-point
//! FFT: A-weighted RMS loudness with peak-decay, HPS pitch, and autocorrelation BPM.
//! The result lives in a process-global so the in-process mesh-gradient renderer can
//! read the latest metrics every frame without a JNI round-trip.

use once_cell::sync::Lazy;
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, Default)]
pub struct Metrics {
    pub pitch: f32,
    pub bpm: f32,
    pub loudness: f32,
}

struct AnalysisState {
    ring_buffer: Vec<f32>,
    fft_size: usize,
    sample_rate: f32,
    pitch: f32,
    bpm_ema: f32,
    loudness: f32,
    energy_history: Vec<f32>,
    max_history: usize,
    alpha: f32,
    fft: Arc<dyn Fft<f32>>,
    complex_buffer: Vec<Complex<f32>>,
    decay_factor: f32,
}

impl AnalysisState {
    fn new() -> Self {
        let fft_size = 1024;
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);
        Self {
            ring_buffer: Vec::new(),
            fft_size,
            sample_rate: 44100.0,
            pitch: 0.0,
            bpm_ema: 120.0,
            loudness: 0.0,
            energy_history: Vec::new(),
            max_history: 1024,
            alpha: 0.1,
            fft,
            complex_buffer: vec![Complex::new(0.0, 0.0); fft_size],
            decay_factor: 0.70,
        }
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    fn push_and_analyze(&mut self, new_data: &[f32]) {
        self.ring_buffer.extend_from_slice(new_data);
        // Cap the ring buffer so a stalled reader can't grow it unbounded.
        let cap = self.fft_size * 8;
        if self.ring_buffer.len() > cap {
            let drop = self.ring_buffer.len() - cap;
            self.ring_buffer.drain(0..drop);
        }
        while self.ring_buffer.len() >= self.fft_size {
            let window: Vec<f32> = self.ring_buffer[0..self.fft_size].to_vec();
            self.run_fft_logic(&window);
            // 50% overlap (hop = fft_size / 2).
            self.ring_buffer.drain(0..self.fft_size / 2);
        }
    }

    fn run_fft_logic(&mut self, window: &[f32]) {
        for i in 0..self.fft_size {
            let w = 0.5
                * (1.0
                    - (2.0 * std::f32::consts::PI * i as f32 / (self.fft_size - 1) as f32).cos());
            self.complex_buffer[i] = Complex::new(window[i] * w, 0.0);
        }
        self.fft.process(&mut self.complex_buffer);
        let magnitudes: Vec<f32> = self.complex_buffer.iter().map(|c| c.norm()).collect();

        // Loudness: A-weighted RMS with peak decay.
        let mut weighted_energy = 0.0;
        for i in 0..self.fft_size / 2 {
            let freq = i as f32 * self.sample_rate / self.fft_size as f32;
            let weight = a_weighting(freq);
            weighted_energy += magnitudes[i] * magnitudes[i] * weight;
        }
        let instantaneous_loudness = (weighted_energy / (self.fft_size / 2) as f32).sqrt();
        if instantaneous_loudness > self.loudness {
            self.loudness = instantaneous_loudness;
        } else {
            self.loudness *= self.decay_factor;
        }

        // Pitch: Harmonic Product Spectrum.
        let mut hps = magnitudes.clone();
        for h in 2..=4 {
            for i in 0..magnitudes.len() / h {
                hps[i] *= magnitudes[i * h];
            }
        }
        if let Some((max_idx, _)) = hps
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        {
            self.pitch = max_idx as f32 * self.sample_rate / self.fft_size as f32;
        }

        // BPM: low-frequency energy autocorrelation.
        let low_freq_bins = self.fft_size / 16;
        let energy = magnitudes[0..low_freq_bins]
            .iter()
            .map(|&x| x * x)
            .sum::<f32>()
            .sqrt();
        self.energy_history.push(energy);
        if self.energy_history.len() > self.max_history {
            self.energy_history.remove(0);
        }
        if self.energy_history.len() > 256 {
            let mut max_corr = 0.0;
            let mut best_lag = 0;
            let frame_rate = self.sample_rate / self.fft_size as f32;
            let min_lag = (frame_rate * 60.0 / 180.0) as usize;
            let max_lag = (frame_rate * 60.0 / 60.0) as usize;
            for lag in min_lag..=max_lag {
                let corr = autocorrelation(&self.energy_history, lag);
                if corr > max_corr {
                    max_corr = corr;
                    best_lag = lag;
                }
            }
            if best_lag > 0 {
                let bpm = 60.0 * frame_rate / best_lag as f32;
                self.bpm_ema = self.alpha * bpm + (1.0 - self.alpha) * self.bpm_ema;
            }
        }
    }

    fn metrics(&self) -> Metrics {
        Metrics {
            pitch: self.pitch,
            bpm: self.bpm_ema,
            loudness: self.loudness,
        }
    }

    fn reset(&mut self) {
        self.ring_buffer.clear();
        self.energy_history.clear();
        self.pitch = 0.0;
        self.loudness = 0.0;
        self.bpm_ema = 120.0;
    }
}

static ANALYSIS_STATE: Lazy<Mutex<AnalysisState>> = Lazy::new(|| Mutex::new(AnalysisState::new()));

fn a_weighting(freq: f32) -> f32 {
    let f2 = freq * freq;
    let num = 12194.0f32.powi(2) * f2.powi(2);
    let den1 = f2 + 20.6f32.powi(2);
    let den2 = ((f2 + 107.7f32.powi(2)) * (f2 + 737.9f32.powi(2))).sqrt();
    let den3 = f2 + 12194.0f32.powi(2);
    num / (den1 * den2 * den3)
}

fn autocorrelation(data: &[f32], lag: usize) -> f32 {
    let mut sum = 0.0;
    for i in 0..data.len().saturating_sub(lag) {
        sum += data[i] * data[i + lag];
    }
    sum
}

/// Push interleaved/mono PCM float samples (called from the audio thread).
pub fn push_pcm(samples: &[f32]) {
    if samples.is_empty() {
        return;
    }
    if let Ok(mut state) = ANALYSIS_STATE.lock() {
        state.push_and_analyze(samples);
    }
}

/// Configure the source sample rate (defaults to 44100).
pub fn set_sample_rate(sample_rate: f32) {
    if let Ok(mut state) = ANALYSIS_STATE.lock() {
        state.set_sample_rate(sample_rate);
    }
}

/// Feed loudness from a native audio engine that already owns decoded PCM.
pub fn set_external_loudness(loudness: f32) {
    if let Ok(mut state) = ANALYSIS_STATE.lock() {
        state.loudness = loudness.max(0.0);
    }
}

/// Latest analysis metrics (read on the render thread each frame).
pub fn current_metrics() -> Metrics {
    ANALYSIS_STATE
        .lock()
        .map(|state| state.metrics())
        .unwrap_or_default()
}

/// Clear accumulated state (e.g. on track change / stop).
pub fn reset() {
    if let Ok(mut state) = ANALYSIS_STATE.lock() {
        state.reset();
    }
}
