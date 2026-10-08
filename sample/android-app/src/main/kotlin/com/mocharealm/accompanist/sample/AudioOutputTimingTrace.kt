package com.mocharealm.accompanist.sample

import android.content.Context
import android.media.AudioTimestamp
import android.media.AudioTrack
import android.os.Trace
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.DefaultAudioSink
import androidx.media3.exoplayer.audio.ForwardingAudioSink
import java.nio.ByteBuffer

/** Observe the default output without changing its format, buffer size or routing. */
@OptIn(UnstableApi::class)
@Suppress("DEPRECATION")
internal object AudioOutputTimingTrace {
    @Volatile private var track: AudioTrack? = null
    @Volatile private var sinkSample: SinkSample? = null
    private val timestamp = AudioTimestamp() // Main-thread reads only.
    private data class SinkSample(
        val positionUs: Long, val sampledAtNanos: Long,
        val trackStartMediaUs: Long?,
    )

    fun createSink(context: Context, floatOutput: Boolean, playbackParameters: Boolean): AudioSink {
        val sink = DefaultAudioSink.Builder(context)
            .setEnableFloatOutput(floatOutput)
            .setEnableAudioOutputPlaybackParameters(playbackParameters)
            .setAudioTrackProvider { config, attributes, sessionId, trackContext ->
                DefaultAudioSink.AudioTrackProvider.DEFAULT
                    .getAudioTrack(config, attributes, sessionId, trackContext)
                    .also { track = it; sinkSample = null }
            }
            .build()
        return object : ForwardingAudioSink(sink) {
            private var streamOffsetUs = 0L
            private var trackStartRendererUs: Long? = null

            override fun setOutputStreamOffsetUs(outputStreamOffsetUs: Long) {
                streamOffsetUs = outputStreamOffsetUs
                super.setOutputStreamOffsetUs(outputStreamOffsetUs)
            }

            override fun handleBuffer(buffer: ByteBuffer, presentationTimeUs: Long,
                encodedAccessUnitCount: Int): Boolean {
                if (trackStartRendererUs == null && buffer.hasRemaining())
                    trackStartRendererUs = presentationTimeUs
                return super.handleBuffer(buffer, presentationTimeUs, encodedAccessUnitCount)
            }

            override fun flush() {
                trackStartRendererUs = null
                sinkSample = null
                super.flush()
            }

            override fun getCurrentPositionUs(sourceEnded: Boolean): Long {
                val position = super.getCurrentPositionUs(sourceEnded)
                if (Trace.isEnabled() && position != AudioSink.CURRENT_POSITION_NOT_SET) {
                    // Sink timestamps are in renderer time, which includes Media3's large
                    // stream offset. Convert to media time before comparing with Player/UI.
                    sinkSample = SinkSample(position - streamOffsetUs, System.nanoTime(),
                        trackStartRendererUs?.minus(streamOffsetUs))
                    Trace.setCounter("Audio.streamOffsetUs", streamOffsetUs)
                    Trace.setCounter("Audio.sinkPositionUs", position - streamOffsetUs)
                }
                return position
            }
        }
    }

    fun read(lyricsPosition: Int, controllerPosition: Long?, speed: Float) {
        val output = track ?: return
        Trace.beginSection("Audio.readTrackClock")
        try {
            if (output.state != AudioTrack.STATE_INITIALIZED) return
            val rate = output.sampleRate
            if (rate <= 0) return
            val headFrames = output.playbackHeadPosition.toLong() and 0xffffffffL
            val valid = output.getTimestamp(timestamp)
            val now = System.nanoTime()
            Trace.setCounter("Audio.timestampValid", if (valid) 1L else 0L)
            Trace.setCounter("Audio.trackPlaying", if (output.playState == AudioTrack.PLAYSTATE_PLAYING) 1L else 0L)
            Trace.setCounter("Audio.sampleRate", rate.toLong())
            Trace.setCounter("Audio.bufferDurationUs", output.bufferSizeInFrames * 1_000_000L / rate)
            Trace.setCounter("Audio.underruns", output.underrunCount.toLong())
            Trace.setCounter("Audio.outputDeviceType", output.routedDevice?.type?.toLong() ?: 0L)
            Trace.setCounter("Audio.trackIdentity", System.identityHashCode(output).toLong())
            Trace.setCounter("Audio.playbackHeadUs", headFrames * 1_000_000L / rate)
            Trace.setCounter("Audio.speedPermille", (speed * 1000).toLong())
            if (!valid || output.playState != AudioTrack.PLAYSTATE_PLAYING) return
            val ageUs = (now - timestamp.nanoTime) / 1000L
            val hardwarePositionUs = timestamp.framePosition * 1_000_000L / rate + (ageUs * speed).toLong()
            Trace.setCounter("Audio.timestampAgeUs", ageUs)
            Trace.setCounter("Audio.hardwarePositionUs", hardwarePositionUs)
            Trace.setCounter("Audio.headAheadOfHardwareUs", headFrames * 1_000_000L / rate - hardwarePositionUs)
            sinkSample?.let { sample ->
                val elapsedUs = (now - sample.sampledAtNanos) / 1000L
                val sinkPositionUs = sample.positionUs + (elapsedUs * speed).toLong()
                Trace.setCounter("Audio.sinkSampleAgeUs", elapsedUs)
                Trace.setCounter("Audio.sinkExtrapolatedPositionUs", sinkPositionUs)
                sample.trackStartMediaUs?.let { start ->
                    Trace.setCounter("Audio.trackStartMediaUs", start)
                    val hardwareMediaUs = hardwarePositionUs + start
                    Trace.setCounter("Audio.hardwareMediaPositionUs", hardwareMediaUs)
                    Trace.setCounter("Audio.sinkMinusHardwareUs", sinkPositionUs - hardwareMediaUs)
                    Trace.setCounter("Audio.hardwareMinusLyricsUs", hardwareMediaUs - lyricsPosition * 1000L)
                    controllerPosition?.let {
                        Trace.setCounter("Audio.hardwareMinusControllerUs", hardwareMediaUs - it * 1000L)
                    }
                }
                Trace.setCounter("Audio.sinkMinusLyricsUs", sinkPositionUs - lyricsPosition * 1000L)
                controllerPosition?.let {
                    Trace.setCounter("Audio.sinkMinusControllerUs", sinkPositionUs - it * 1000L)
                }
            }
        } catch (_: IllegalStateException) {
            // An output may be released by the playback thread while a UI sample is taken.
        } finally {
            Trace.endSection()
        }
    }

    fun clear() { track = null; sinkSample = null }
}
