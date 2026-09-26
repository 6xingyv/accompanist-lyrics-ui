package com.mocharealm.accompanist.sample

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.IntSize
import com.mocharealm.accompanist.sample.ui.playback.PlaybackPositionCursor
import com.mocharealm.accompanist.sample.ui.playback.PlaybackSnapshot
import com.mocharealm.accompanist.sample.ui.playback.rememberPlaybackPosition
import kotlin.test.*
import kotlinx.coroutines.flow.MutableStateFlow

@OptIn(InternalComposeUiApi::class, InternalComposeTracingApi::class)
class PlaybackClockTest {
    @Test
    fun repeatPositionCanArriveAfterItsDiscontinuityEvent() {
        val cursor = PlaybackPositionCursor()
        val end = PlaybackSnapshot(true, 10000, 10000, 1000, discontinuity = 1)
        assertEquals(10000, cursor.position(end, 1000))
        assertEquals(10000, cursor.position(end.copy(discontinuity = 2), 1000))
        val repeated = end.copy(position = 0, sampledAtMillis = 1001, discontinuity = 2)
        assertEquals(0, cursor.position(repeated, 1001))
        assertEquals(100, cursor.position(repeated, 1101))
        assertEquals(
            100,
            cursor.position(repeated.copy(position = 80, sampledAtMillis = 1101), 1101),
        )
        assertEquals(
            10000,
            cursor.position(end.copy(sampledAtMillis = 2000, discontinuity = 3), 2000),
        )
        assertEquals(
            0,
            cursor.position(
                repeated.copy(isPlaying = false, sampledAtMillis = 2001, discontinuity = 3),
                2001,
            ),
        )
    }

    @Test
    fun playerCorrectionsNeverReverseButExplicitSeeksAndLoopsDo() {
        val cursor = PlaybackPositionCursor()
        val sample = PlaybackSnapshot(true, 1000, 10000, 0)
        assertEquals(1300, cursor.position(sample, 300))
        val correction = sample.copy(position = 1250, sampledAtMillis = 300)
        assertEquals(1300, cursor.position(correction, 300))
        assertEquals(1350, cursor.position(correction, 400))
        assertEquals(
            1350,
            cursor.position(correction.copy(isPlaying = false, position = 1320), 500),
        )
        assertEquals(
            500,
            cursor.position(
                sample.copy(position = 500, sampledAtMillis = 500, discontinuity = 1),
                500,
            ),
        )
        assertEquals(
            0,
            cursor.position(
                sample.copy(position = 0, sampledAtMillis = 600, discontinuity = 2),
                600,
            ),
        )
    }

    @Test
    fun interpolationRespectsPauseSpeedAndBounds() {
        val sample = PlaybackSnapshot(true, 1000, 5000, 100, 2f)
        assertEquals(1400, sample.positionAt(300))
        assertEquals(1400, sample.copy(duration = 0).positionAt(300))
        assertEquals(1000, sample.positionAt(0))
        assertEquals(5000, sample.positionAt(10000))
        assertEquals(1000, sample.copy(isPlaying = false).positionAt(10000))
    }

    @Test
    fun samplesAndFramesDoNotRecomposeClockOwnerAndPausedSeeksUpdateDrawing() {
        val samples = MutableStateFlow(PlaybackSnapshot(true, 100, 20000, 0))
        var millis = 0L
        var drawnPosition = -1
        var ownerCompositions = 0
        var draws = 0
        val events = mutableListOf<String>()
        Composer.setTracer(
            object : CompositionTracer {
                override fun isTraceInProgress() = true

                override fun traceEventStart(key: Int, dirty1: Int, dirty2: Int, info: String) {
                    if (info.contains("rememberPlaybackPosition")) events.add(info)
                }

                override fun traceEventEnd() {}
            }
        )
        val recomposer = FrameRecomposer(kotlinx.coroutines.Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(frameRecomposer = recomposer, size = IntSize(100, 100))
        val canvas = Canvas(ImageBitmap(100, 100))
        fun frame() {
            millis += 16
            Snapshot.sendApplyNotifications()
            recomposer.performFrame(millis * 1_000_000)
            scene.measureAndLayout()
            scene.draw(canvas)
        }
        try {
            scene.setContent {
                val position = rememberPlaybackPosition(samples) { millis }
                SideEffect { ownerCompositions++ }
                Canvas(Modifier.fillMaxSize()) {
                    drawnPosition = position.intValue
                    draws++
                    drawRect(Color.White, alpha = drawnPosition / 20000f)
                }
            }
            repeat(10) { frame() }
            assertTrue(events.isNotEmpty(), "Tracer must observe the clock composition")
            events.clear()
            val compositionsBefore = ownerCompositions
            val drawsBefore = draws
            var previousDrawn = drawnPosition
            repeat(120) { index ->
                if (index % 15 == 0)
                    samples.value =
                        samples.value.copy(position = millis + 80, sampledAtMillis = millis)
                frame()
                assertTrue(
                    drawnPosition >= previousDrawn,
                    "Ordinary sample correction cannot reverse a drawn frame",
                )
                previousDrawn = drawnPosition
            }
            assertTrue(draws > drawsBefore)
            assertEquals(compositionsBefore, ownerCompositions)
            assertTrue(events.isEmpty(), "Sampling and frames must not recompose: $events")
            samples.value = PlaybackSnapshot(false, 900, 20000, millis, discontinuity = 1)
            repeat(4) { frame() }
            assertEquals(900, drawnPosition)
            samples.value =
                samples.value.copy(position = 875, sampledAtMillis = millis, discontinuity = 2)
            repeat(4) { frame() }
            assertEquals(875, drawnPosition, "Small backward seeks while paused must be applied")
            assertEquals(compositionsBefore, ownerCompositions)
        } finally {
            scene.close()
            recomposer.close()
            Composer.setTracer(null)
        }
    }
}
