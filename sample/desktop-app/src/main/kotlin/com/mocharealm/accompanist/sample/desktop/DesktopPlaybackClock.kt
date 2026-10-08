package com.mocharealm.accompanist.sample.desktop

import androidx.compose.runtime.*
import com.mocharealm.accompanist.sample.ui.playback.PlaybackSnapshot
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlin.math.abs

/** Smooth ordinary corrections; a new revision means an actual seek or track transition. */
@Composable
internal fun rememberDesktopPosition(samples: StateFlow<PlaybackSnapshot>): IntState {
    val position = remember(samples) { mutableIntStateOf(0) }
    LaunchedEffect(samples) {
        var revision = Long.MIN_VALUE
        var previousFrame = monotonicMillis()
        var display = 0.0
        samples.collectLatest { sample ->
            if (!sample.isPlaying || revision != sample.discontinuity) {
                display = sample.positionAt(monotonicMillis()).toDouble()
                revision = sample.discontinuity
                position.intValue = display.toInt()
                previousFrame = monotonicMillis()
            }
            while (sample.isPlaying && (sample.duration <= 0 || display < sample.duration)) {
                withFrameNanos {
                val now = monotonicMillis()
                val target = sample.positionAt(now).toDouble()
                val elapsed = (now - previousFrame).coerceIn(0, 64)
                previousFrame = now
                val gap = target - display
                if (revision != sample.discontinuity || abs(gap) >= 500) display = target
                else if (!sample.isPlaying) display = target
                else {
                    val rate = (sample.speed + gap / 350.0).coerceIn(0.0, 2.5)
                    display += rate * elapsed
                    if (gap > 0 && display > target) display = target
                }
                revision = sample.discontinuity
                position.intValue = display.toLong().coerceIn(0, Int.MAX_VALUE.toLong()).toInt()
                }
            }
        }
    }
    return position
}

/** Only distinct SMTC publications contribute to Apple Music's averaged clock. */
internal class AppleMusicClock {
    private var reference = 0L
    private var token = 0L
    private var playing = false
    private val samples = ArrayDeque<Long>()
    @Synchronized fun reset(sample: MediaSnapshot) {
        reference = sample.sampledAt
        token = sample.timelineToken
        playing = sample.playing
        samples.clear()
        samples.addLast(sample.position)
    }
    @Synchronized fun accept(sample: MediaSnapshot): Long {
        if (samples.isEmpty() || playing != sample.playing || !sample.playing) {
            reset(sample)
        } else if (sample.timelineToken <= 0 || sample.timelineToken != token) {
            val current = positionAt(sample.sampledAt)
            if (abs(sample.position - current) >= 1500) reset(sample) else {
                samples.addLast(sample.position - (sample.sampledAt - reference))
                while (samples.size > 8) samples.removeFirst()
                token = sample.timelineToken
            }
        }
        return positionAt(sample.sampledAt)
    }
    private fun positionAt(now: Long) =
        (samples.sum() / samples.size.coerceAtLeast(1) + if (playing) now - reference else 0)
            .coerceAtLeast(0)
}
