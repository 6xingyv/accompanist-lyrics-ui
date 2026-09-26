package com.mocharealm.accompanist.sample

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.IntSize
import com.mocharealm.accompanist.sample.ui.composable.background.BackgroundVisualState
import com.mocharealm.accompanist.sample.ui.composable.background.FlowingLightBackground
import kotlin.test.*
import kotlinx.coroutines.Dispatchers

@OptIn(InternalComposeUiApi::class, InternalComposeTracingApi::class)
class BackgroundRecompositionTest {
    @Test
    fun playbackAndUnrelatedUiUpdatesDoNotRecomposeBackground() {
        val background = BackgroundVisualState(ImageBitmap(32, 32), 0.7f)
        val uiState = mutableStateOf(background to 0)
        var backgroundCompositions = 0
        var siblingCompositions = 0
        Composer.setTracer(
            object : CompositionTracer {
                override fun isTraceInProgress() = true

                override fun traceEventStart(key: Int, dirty1: Int, dirty2: Int, info: String) {
                    if (
                        info.startsWith(
                            "com.mocharealm.accompanist.sample.ui.composable.background.FlowingLightBackground ("
                        )
                    )
                        backgroundCompositions++
                }

                override fun traceEventEnd() {}
            }
        )
        val recomposer = FrameRecomposer(Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(frameRecomposer = recomposer, size = IntSize(400, 600))
        var nanos = 0L
        fun frame() {
            Snapshot.sendApplyNotifications()
            nanos += 16_666_667L
            recomposer.performFrame(nanos)
            scene.measureAndLayout()
        }
        try {
            scene.setContent {
                Box {
                    // Same derived state and background call as PlayerScreen.
                    val visual by remember { derivedStateOf { uiState.value.first } }
                    FlowingLightBackground(visual, Modifier.fillMaxSize())
                    Box {
                        val tick = uiState.value.second
                        SideEffect { siblingCompositions = tick }
                    }
                }
            }
            repeat(80) { frame() }
            assertTrue(backgroundCompositions > 0)
            backgroundCompositions = 0
            repeat(120) { tick ->
                uiState.value = background to (tick + 1)
                frame()
            }
            assertEquals(120, siblingCompositions)
            assertEquals(0, backgroundCompositions)
            // Positive control: a real cover change must be observed by the tracer.
            uiState.value = BackgroundVisualState(ImageBitmap(32, 32), 0.4f) to 120
            repeat(60) { frame() }
            assertTrue(backgroundCompositions > 0)
        } finally {
            scene.close()
            recomposer.close()
            Composer.setTracer(null)
        }
    }
}
