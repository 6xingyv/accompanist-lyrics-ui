package com.mocharealm.accompanist.sample

import android.os.SystemClock
import android.os.Trace
import androidx.media3.common.Player
import com.mocharealm.accompanist.sample.ui.playback.PlaybackSnapshot

/** All calls run on the main/player application looper. Counters exist only during system tracing. */
internal object PlaybackTimingTrace {
    var player: Player? = null
    private var lastReadNanos = 0L

    fun read(controller: Player?, lyricsPosition: Int, sample: PlaybackSnapshot) {
        if (!Trace.isEnabled()) return
        val nowNanos = System.nanoTime()
        // Several rows and scroll/layout can read the provider in the same frame.
        if (nowNanos - lastReadNanos < 8_000_000L) return
        lastReadNanos = nowNanos
        Trace.beginSection("Playback.readClock")
        try {
            val now = SystemClock.uptimeMillis()
            val controllerPosition = controller?.currentPosition
            val playerPosition = player?.currentPosition
            AudioOutputTimingTrace.read(lyricsPosition, controllerPosition, sample.speed)
            Trace.setCounter("Playback.lyricsPositionMs", lyricsPosition.toLong())
            Trace.setCounter("Playback.snapshotAgeMs", now - sample.sampledAtMillis)
            Trace.setCounter("Playback.extrapolatedMinusLyricsMs",
                sample.positionAt(now).toLong() - lyricsPosition)
            Trace.setCounter("Playback.playing", if (sample.isPlaying) 1L else 0L)
            Trace.setCounter("Playback.discontinuity", sample.discontinuity)
            if (controllerPosition != null) {
                Trace.setCounter("Playback.controllerPositionMs", controllerPosition)
                Trace.setCounter("Playback.controllerMinusLyricsMs", controllerPosition - lyricsPosition)
            }
            if (playerPosition != null) {
                Trace.setCounter("Playback.playerPositionMs", playerPosition)
                Trace.setCounter("Playback.playerMinusLyricsMs", playerPosition - lyricsPosition)
                if (controllerPosition != null)
                    Trace.setCounter("Playback.playerMinusControllerMs", playerPosition - controllerPosition)
            }
        } finally {
            Trace.endSection()
        }
    }
}
