package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import com.mocharealm.accompanist.lyrics.ui.internal.effects.LyricsReveal

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.*
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlin.test.*
import kotlinx.coroutines.Dispatchers

@OptIn(InternalComposeUiApi::class)
class LyricsRevealTest {
    @Test
    fun shrinkingItemKeepsPixelsOutsideLayoutForEveryRevealOrigin() {
        for (origin in
            listOf(TransformOrigin.Center, TransformOrigin(0f, 0f), TransformOrigin(1f, 1f))) {
            val recomposer = FrameRecomposer(Dispatchers.Unconfined)
            val scene = CanvasLayersComposeScene(recomposer, size = IntSize(300, 400))
            val bitmap = ImageBitmap(300, 400)
            val canvas = Canvas(bitmap)
            var visible by mutableStateOf(true)
            var itemTop = 0
            var itemHeight = 0
            var nanos = 0L
            fun frame() {
                Snapshot.sendApplyNotifications()
                nanos += 16_666_667
                recomposer.performFrame(nanos)
                scene.measureAndLayout()
                canvas.drawRect(0f, 0f, 300f, 400f, Paint().apply { blendMode = BlendMode.Clear })
                scene.draw(canvas)
            }
            fun assertOverflow(phase: String) {
                assertTrue(itemHeight in 1..79, "$phase must actually shrink layout: $itemHeight")
                val pixels = IntArray(300 * 400)
                bitmap.readPixels(pixels)
                // Sample well outside the shrinking item, beyond ordinary blur spill.
                val y = if (origin.pivotFractionY == 1f) itemTop - 12 else itemTop + itemHeight + 12
                assertTrue(
                    (30..260).any { x -> (pixels[y * 300 + x] ushr 24) > 12 },
                    "$phase must retain visible content beyond layout at origin $origin",
                )
            }
            try {
                scene.setContent {
                    Column(Modifier.padding(top = 150.dp)) {
                        LyricsLineItem(
                            isFocused = false,
                            isRightAligned = origin.pivotFractionX == 1f,
                            onLineClicked = {},
                            onLinePressed = {},
                            blurRadius = { 1f },
                            modifier =
                                Modifier.onGloballyPositioned {
                                    itemTop = it.positionInRoot().y.toInt()
                                    itemHeight = it.size.height
                                },
                        ) {
                            // The real main line adds a second reveal layer above the caption.
                            LyricsReveal(true) {
                                Column {
                                    LyricsReveal(visible, origin = origin) {
                                        Canvas(Modifier.size(180.dp, 100.dp)) {
                                            drawRect(Color.White)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                repeat(120) { frame() }
                visible = false
                repeat(10) { frame() }
                assertOverflow("exit")
                repeat(120) { frame() }
                visible = true
                repeat(5) { frame() }
                assertOverflow("entry")
            } finally {
                scene.close()
                recomposer.close()
            }
        }
    }

    @Test
    fun exitRetainsContentAndCanReverseBeforeDisposal() {
        val recomposer = FrameRecomposer(Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(recomposer, size = IntSize(200, 200))
        val bitmap = ImageBitmap(200, 200)
        val canvas = Canvas(bitmap)
        var visible by mutableStateOf(true)
        var disposed = 0
        var attached = 0
        var textMeasures = 0
        var nanos = 0L
        fun frame() {
            Snapshot.sendApplyNotifications()
            nanos += 16_666_667
            recomposer.performFrame(nanos)
            scene.measureAndLayout()
            canvas.drawRect(0f, 0f, 200f, 200f, Paint().apply { blendMode = BlendMode.Clear })
            scene.draw(canvas)
        }
        try {
            scene.setContent {
                Column {
                    LyricsReveal(visible) {
                        DisposableEffect(Unit) {
                            attached++
                            onDispose { disposed++ }
                        }
                        Canvas(
                            Modifier.size(100.dp).layout { measurable, constraints ->
                                textMeasures++
                                val placeable = measurable.measure(constraints)
                                layout(placeable.width, placeable.height) { placeable.place(0, 0) }
                            }
                        ) {
                            drawRect(Color.White)
                        }
                    }
                }
            }
            repeat(3) { frame() }
            val initialMeasures = textMeasures
            visible = false
            repeat(2) { frame() }
            val transitionMeasures = textMeasures
            assertTrue(transitionMeasures <= initialMeasures + 1)
            repeat(6) { frame() }
            assertEquals(0, disposed, "Exit must not dispose content before effects finish")
            assertEquals(
                transitionMeasures,
                textMeasures,
                "Height animation must reuse fixed child measurement",
            )
            visible = true
            repeat(120) { frame() }
            assertEquals(1, attached, "Reversing exit reuses the existing content")
            assertEquals(0, disposed)
            visible = false
            repeat(12) { frame() }
            val pixels = IntArray(200 * 200)
            bitmap.readPixels(pixels)
            assertTrue(pixels.any { (it ushr 24) in 1..254 }, "Exit must visibly blur/fade")
            assertEquals(0, disposed)
            repeat(120) { frame() }
            assertEquals(1, disposed)
        } finally {
            scene.close()
            recomposer.close()
        }
    }
}
