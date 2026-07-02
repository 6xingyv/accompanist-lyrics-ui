package com.mocharealm.accompanist.sample.service

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.DefaultAudioSink
import androidx.media3.exoplayer.audio.TeeAudioProcessor
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import com.mocharealm.accompanist.lyrics.text.NativeAudioAnalysis
import com.mocharealm.accompanist.sample.MainActivity
import java.nio.ByteBuffer
import java.nio.ByteOrder

class PlaybackService : MediaSessionService() {

    private var mediaSession: MediaSession? = null
    private lateinit var player: ExoPlayer

    override fun onCreate() {
        super.onCreate()
        initializePlayerAndSession()
    }

    @OptIn(UnstableApi::class)
    private fun initializePlayerAndSession() {
        // A TeeAudioProcessor taps the decoded PCM as it flows to the audio track and
        // forwards it (as float) to the process-global Rust analyzer, which the
        // lyrics surface reads each frame to drive the reactive mesh background. This
        // stays in-process (same as the renderer), needs no extra permission, and is
        // transparent to playback (the tee passes the buffer straight through).
        val renderersFactory = object : DefaultRenderersFactory(this) {
            override fun buildAudioSink(
                context: Context,
                enableFloatOutput: Boolean,
                enableAudioTrackPlaybackParams: Boolean
            ): AudioSink {
                return DefaultAudioSink.Builder(context)
                    .setEnableFloatOutput(enableFloatOutput)
                    .setEnableAudioTrackPlaybackParams(enableAudioTrackPlaybackParams)
                    .setAudioProcessors(
                        arrayOf<AudioProcessor>(TeeAudioProcessor(AnalysisAudioBufferSink()))
                    )
                    .build()
            }
        }.setExtensionRendererMode(DefaultRenderersFactory.EXTENSION_RENDERER_MODE_PREFER)

        player = ExoPlayer.Builder(this)
            .setRenderersFactory(renderersFactory)
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
                    .setUsage(C.USAGE_MEDIA)
                    .build(),
                true
            )
            .build()

        val sessionActivityIntent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP
        }

        val sessionActivityPendingIntent = PendingIntent.getActivity(
            this,
            0,
            sessionActivityIntent,
            PendingIntent.FLAG_IMMUTABLE
        )

        mediaSession = MediaSession.Builder(this, player)
            .setSessionActivity(sessionActivityPendingIntent)
            .build()
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? {
        return mediaSession
    }

    override fun onDestroy() {
        mediaSession?.run {
            player.release()
            release()
            mediaSession = null
        }
        super.onDestroy()
    }

    /**
     * Converts each tapped PCM buffer to mono-order float samples and pushes them to
     * the Rust analyzer. Handles both 16-bit and float source encodings (the sink may
     * fall back to 16-bit when float output is unavailable).
     */
    @OptIn(UnstableApi::class)
    private class AnalysisAudioBufferSink : TeeAudioProcessor.AudioBufferSink {
        private var encoding = C.ENCODING_PCM_16BIT
        // Reused direct float buffer (native order) handed to JNI.
        private var floatBuffer: ByteBuffer =
            ByteBuffer.allocateDirect(0).order(ByteOrder.nativeOrder())

        override fun flush(sampleRateHz: Int, channelCount: Int, encoding: Int) {
            this.encoding = encoding
            NativeAudioAnalysis.setSampleRate(sampleRateHz.toFloat())
        }

        override fun handleBuffer(buffer: ByteBuffer) {
            val remaining = buffer.remaining()
            if (remaining <= 0) return
            val sampleCount = when (encoding) {
                C.ENCODING_PCM_FLOAT -> remaining / 4
                C.ENCODING_PCM_16BIT -> remaining / 2
                else -> return
            }
            if (sampleCount <= 0) return

            val floats = ensureCapacity(sampleCount)
            floats.clear()
            val src = buffer.duplicate().order(ByteOrder.nativeOrder())
            when (encoding) {
                C.ENCODING_PCM_FLOAT -> {
                    val fb = src.asFloatBuffer()
                    for (i in 0 until sampleCount) floats.putFloat(fb.get())
                }

                C.ENCODING_PCM_16BIT -> {
                    val sb = src.asShortBuffer()
                    for (i in 0 until sampleCount) floats.putFloat(sb.get() / 32768f)
                }
            }
            NativeAudioAnalysis.pushAudioData(floatBuffer, sampleCount)
        }

        private fun ensureCapacity(sampleCount: Int): ByteBuffer {
            val needed = sampleCount * 4
            if (floatBuffer.capacity() < needed) {
                floatBuffer = ByteBuffer.allocateDirect(needed).order(ByteOrder.nativeOrder())
            }
            return floatBuffer
        }
    }
}
