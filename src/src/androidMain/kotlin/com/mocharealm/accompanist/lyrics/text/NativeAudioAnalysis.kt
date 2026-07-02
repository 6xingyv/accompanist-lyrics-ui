package com.mocharealm.accompanist.lyrics.text

import java.nio.ByteBuffer

/**
 * Process-global bridge to the Rust audio analyzer (loudness / pitch / BPM). PCM
 * float samples are pushed in from an ExoPlayer `TeeAudioProcessor` (same process)
 * and the analysis result is read in-process by the mesh-gradient renderer every
 * frame — so there is no per-frame JNI round-trip for the metrics, only this push.
 *
 * Backed by the same `libtext_engine.so` as [NativeTextEngine]; the analysis state
 * is a Rust `lazy_static`, independent of any engine handle.
 */
object NativeAudioAnalysis {

    init {
        System.loadLibrary("text_engine")
    }

    /**
     * Push [floatCount] PCM float samples from a **direct** [buffer] (native byte
     * order). Only the first `floatCount * 4` bytes are read. Safe to call from the
     * audio thread.
     */
    fun pushAudioData(buffer: ByteBuffer, floatCount: Int) {
        if (!buffer.isDirect || floatCount <= 0) return
        nativePushAudioData(buffer, floatCount)
    }

    /** Set the source sample rate (Hz). Defaults to 44100 until called. */
    fun setSampleRate(sampleRate: Float) {
        nativeSetSampleRate(sampleRate)
    }

    /** Clear accumulated analysis state (e.g. on track change / stop). */
    fun reset() {
        nativeReset()
    }

    private external fun nativePushAudioData(buffer: ByteBuffer, floatCount: Int)
    private external fun nativeSetSampleRate(sampleRate: Float)
    private external fun nativeReset()
}
