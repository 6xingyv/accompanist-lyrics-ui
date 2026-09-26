package com.mocharealm.accompanist.sample.ui.playback

import androidx.compose.runtime.Composable
import androidx.compose.runtime.IntState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.withFrameNanos
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest

data class PlaybackSnapshot(
    val isPlaying: Boolean = false,
    val position: Long = 0L,
    val duration: Long = 0L,
    val sampledAtMillis: Long = 0L,
    val speed: Float = 1f,
    /** Increment for an explicit seek, loop or media transition, never for an ordinary sample. */
    val discontinuity: Long = 0L,
) {
    fun positionAt(nowMillis: Long): Int {
        val elapsed =
            if (isPlaying) ((nowMillis - sampledAtMillis).coerceAtLeast(0) * speed).toLong() else 0L
        return (position + elapsed)
            .coerceIn(
                0L,
                duration.takeIf { it > 0L }?.coerceAtMost(Int.MAX_VALUE.toLong())
                    ?: Int.MAX_VALUE.toLong(),
            )
            .toInt()
    }
}

/** Ordinary player corrections may hold the clock briefly, but never reverse animation. */
internal class PlaybackPositionCursor {
    private var revision: Long? = null
    private var previous = 0

    fun position(sample: PlaybackSnapshot, now: Long): Int {
        val candidate = sample.positionAt(now)
        // A controller may publish the repeat event before its first position sample.
        // Only accept an unmarked reset across opposite ends of a known-duration track;
        // ordinary backward sampling corrections still hold the clock monotonically.
        val boundaryWindow = minOf(1000L, sample.duration / 4)
        val restarted =
            sample.duration > 0 &&
                boundaryWindow > 0 &&
                previous.toLong() >= sample.duration - boundaryWindow &&
                candidate.toLong() <= boundaryWindow
        previous =
            if (revision != sample.discontinuity || restarted) candidate
            else maxOf(previous, candidate)
        revision = sample.discontinuity
        return previous
    }
}

/**
 * Collect controller samples outside composition. Consumers read the returned state during draw.
 */
@Composable
fun rememberPlaybackPosition(
    playback: StateFlow<PlaybackSnapshot>,
    monotonicMillis: () -> Long,
): IntState {
    val clock = rememberUpdatedState(monotonicMillis)
    val position =
        remember(playback) { mutableIntStateOf(playback.value.positionAt(monotonicMillis())) }
    LaunchedEffect(playback) {
        val cursor = PlaybackPositionCursor()
        playback.collectLatest { sample ->
            position.intValue = cursor.position(sample, clock.value())
            if (sample.isPlaying) {
                while (
                    position.intValue <
                        (sample.duration.takeIf { it > 0L }?.coerceAtMost(Int.MAX_VALUE.toLong())
                            ?: Int.MAX_VALUE.toLong())
                ) {
                    withFrameNanos { position.intValue = cursor.position(sample, clock.value()) }
                }
            }
        }
    }
    return position
}
