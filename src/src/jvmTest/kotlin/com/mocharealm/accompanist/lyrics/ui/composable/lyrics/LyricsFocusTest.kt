package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import com.mocharealm.accompanist.lyrics.ui.internal.test.TestSceneDispatcher

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.*
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

@OptIn(InternalComposeUiApi::class)
class LyricsFocusTest {
    @Test
    fun focusUpdatesRenderedAlphaScaleAndBlurInBothDirections() {
        val focused = mutableStateOf(true)
        val blur = mutableFloatStateOf(0f)
        val dispatcher = TestSceneDispatcher()
        val recomposer = FrameRecomposer(dispatcher)
        val scene = CanvasLayersComposeScene(recomposer, size = IntSize(100, 100))
        val bitmap = ImageBitmap(100, 100)
        val canvas = Canvas(bitmap)
        val clear = Paint().apply { blendMode = BlendMode.Clear }
        var nanos = 0L
        fun settle(): Int {
            repeat(120) {
                dispatcher.runCurrent()
                Snapshot.sendApplyNotifications()
                nanos += 16_666_667L
                recomposer.performFrame(nanos)
                scene.measureAndLayout()
                canvas.drawRect(0f, 0f, 100f, 100f, clear)
                scene.draw(canvas)
            }
            return IntArray(100 * 100).also { bitmap.readPixels(it) }.maxOf { it ushr 24 }
        }
        fun alphaAt(x: Int, y: Int): Int =
            IntArray(100 * 100).also { bitmap.readPixels(it) }[y * 100 + x] ushr 24
        try {
            scene.setContent {
                LyricsLineItem(focused.value, false, {}, {}, { blur.floatValue }, isInteractive = false) {
                    Box(Modifier.size(80.dp).background(Color.White))
                }
            }
            assertEquals(255, settle())
            focused.value = false
            assertEquals(102, settle(), "Unfocused line must render with alpha 0.4")
            assertEquals(0, alphaAt(79, 40), "Unfocused scale must shrink the painted bounds")
            focused.value = true
            assertEquals(255, settle(), "Refocusing must restore alpha 1")
            assertEquals(255, alphaAt(79, 40), "Refocusing must restore scale 1")
            blur.floatValue = 8f
            settle()
            assertTrue(alphaAt(84, 40) > 0, "Changing only blur must update the parent paint")
            blur.floatValue = 0f
            settle()
            assertEquals(0, alphaAt(84, 40), "Removing blur must restore sharp bounds")
        } finally {
            scene.close()
            recomposer.close()
            dispatcher.runCurrent()
        }
    }
}
