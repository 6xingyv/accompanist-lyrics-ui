package com.mocharealm.accompanist.sample

import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
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
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.sample.ui.composable.MobileModalScaffold
import com.mocharealm.accompanist.sample.ui.composable.PadModalScaffold
import com.mocharealm.accompanist.sample.ui.utils.ScreenCornerDataDp
import kotlin.test.*

@OptIn(InternalComposeUiApi::class, InternalComposeTracingApi::class)
class ModalRecompositionTest {
    @Test fun mobileAnimationDoesNotRecomposeScaffold() = verifyAnimation(true)

    @Test fun padAnimationDoesNotRecomposeScaffold() = verifyAnimation(false)

    private fun verifyAnimation(mobile: Boolean) {
        val open = mutableStateOf(false)
        var events = 0
        val name = if (mobile) "MobileModalScaffold" else "PadModalScaffold"
        Composer.setTracer(
            object : CompositionTracer {
                override fun isTraceInProgress() = true

                override fun traceEventStart(key: Int, dirty1: Int, dirty2: Int, info: String) {
                    if (info.startsWith("com.mocharealm.accompanist.sample.ui.composable.$name ("))
                        events++
                }

                override fun traceEventEnd() {}
            }
        )
        val recomposer = FrameRecomposer(kotlinx.coroutines.Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(frameRecomposer = recomposer, size = IntSize(400, 600))
        val bitmap = ImageBitmap(400, 600)
        val canvas = Canvas(bitmap)
        var nanos = 0L
        fun frame() {
            Snapshot.sendApplyNotifications()
            nanos += 16_666_667L
            recomposer.performFrame(nanos)
            scene.measureAndLayout()
            scene.draw(canvas)
        }
        try {
            scene.setContent {
                if (mobile)
                    MobileModalScaffold(
                        isModalOpen = open.value,
                        onDismissRequest = { open.value = false },
                        screenCornerDataDp = ScreenCornerDataDp(0.dp, 0.dp, 0.dp, 0.dp),
                        animationSpec = tween(1000),
                        modalContent = { Box(Modifier.size(100.dp).background(Color.Red)) },
                        content = { Box(Modifier.fillMaxSize().background(Color.White)) },
                    )
                else
                    PadModalScaffold(
                        isModalOpen = open.value,
                        onDismissRequest = { open.value = false },
                        animationSpec = tween(1000),
                        modalContent = { Box(Modifier.size(100.dp).background(Color.Red)) },
                        content = { Box(Modifier.fillMaxSize().background(Color.White)) },
                    )
            }
            repeat(80) { frame() }
            assertTrue(events > 0, "Tracer must observe $name")
            open.value = true
            repeat(8) { frame() }
            val before = IntArray(400 * 600)
            bitmap.readPixels(before)
            events = 0
            repeat(30) { frame() }
            val after = IntArray(400 * 600)
            bitmap.readPixels(after)
            assertFalse(
                before.contentEquals(after),
                "The animation must actually change rendered pixels",
            )
            assertEquals(0, events, "$name recomposed during the middle of its opening animation")
            repeat(40) { frame() }
            open.value = false
            repeat(8) { frame() }
            events = 0
            repeat(30) { frame() }
            assertEquals(0, events, "$name recomposed during the middle of its closing animation")
        } finally {
            scene.close()
            recomposer.close()
            Composer.setTracer(null)
        }
    }
}
