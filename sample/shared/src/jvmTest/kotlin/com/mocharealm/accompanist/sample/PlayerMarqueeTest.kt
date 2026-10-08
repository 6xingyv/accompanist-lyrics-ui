package com.mocharealm.accompanist.sample

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.*
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.sample.ui.composable.player.*
import kotlinx.coroutines.Dispatchers
import java.io.File
import kotlin.test.*

@OptIn(InternalComposeUiApi::class)
class PlayerMarqueeTest {
    @Test
    fun matchesRustHoldSmoothstepAndSeamlessLoop() {
        val distance = 500f
        assertEquals(0f, marqueeOffset(0, distance))
        assertEquals(0f, marqueeOffset(1599, distance))
        assertEquals(0f, marqueeOffset(1600, distance))
        assertEquals(distance * .15625f, marqueeOffset(1600 + 3125, distance), .0001f)
        assertEquals(distance * .5f, marqueeOffset(1600 + 6250, distance), .0001f)
        assertEquals(distance * .84375f, marqueeOffset(1600 + 9375, distance), .0001f)
        assertEquals(0f, marqueeOffset(1600 + 12500, distance))
        val offsets = (0..9).map { marqueeOffset(1600 + 1250 * it, distance) }
        assertTrue(offsets.zipWithNext().all { (before, after) -> after >= before })
        assertEquals(marqueeOffset(14099, distance), marqueeOffset(-1, distance))
    }

    @Test
    fun aFullViewportAnd48RendererPixelsSeparateTheCopies() {
        val distance = marqueeCycleDistance(400f, 200f, 8f, 8f)
        val firstExit = 400f + 8f
        assertEquals(200f + 8f + 48f, distance - firstExit)
        assertEquals(200f + 8f, distance - (firstExit + 48f))
        assertEquals(0f, distance - distance)
    }

    @Test
    fun overflowingTextFadesOnlyInPaddingAndUsesThePlaybackClock() {
        val recomposer = FrameRecomposer(Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(frameRecomposer = recomposer, size = IntSize(260, 90))
        val position = mutableIntStateOf(0)
        val value = mutableStateOf("█".repeat(40))
        var clockReads = 0
        var nanos = 0L
        fun settle() {
            repeat(5) {
                Snapshot.sendApplyNotifications()
                nanos += 16_666_667
                recomposer.performFrame(nanos)
                scene.measureAndLayout()
            }
        }
        fun draw(name: String): IntArray {
            val bitmap = ImageBitmap(260, 90)
            scene.draw(Canvas(bitmap))
            File("build/marquee-preview/$name.png").also { file ->
                file.parentFile.mkdirs()
                org.jetbrains.skia.Image.makeFromBitmap(bitmap.asSkiaBitmap()).use { image ->
                    image.encodeToData()!!.use { file.writeBytes(it.bytes) }
                }
            }
            return IntArray(260 * 90).also { bitmap.readPixels(it) }
        }
        fun columnAlpha(pixels: IntArray, x: Int) = (0 until 90).maxOf { pixels[it * 260 + x] ushr 24 }
        try {
            scene.setContent {
                Box(Modifier.padding(start = 24.dp, top = 20.dp)) {
                    PlayerMarqueeText(value.value,
                        TextStyle(fontSize = 20.sp, lineHeight = 26.sp, fontFamily = FontFamily.Monospace),
                        1f, { clockReads++; position.intValue }, Modifier.width(200.dp))
                }
            }
            settle()
            val initial = draw("hold")
            assertEquals(255, columnAlpha(initial, 220), "The normal text column is solid")
            assertTrue(columnAlpha(initial, 224) in 220..254)
            assertTrue(columnAlpha(initial, 230) in 1..80)
            assertEquals(0, columnAlpha(initial, 232), "Text must not reach neighbouring controls")
            // System fonts can give the first block glyph a negative left side bearing.
            assertTrue(columnAlpha(initial, 23) in 0..254, "Glyph overhang stays faded inside the padding")
            assertEquals(0, columnAlpha(initial, 15), "Resting text must not reach the artwork")
            assertTrue(initial.contentEquals(draw("paused")), "An unchanged playback clock must hold its pixels")
            position.intValue = 5000
            settle()
            val moving = draw("moving")
            assertEquals(255, columnAlpha(moving, 24), "The left end of the normal column stays opaque")
            assertTrue(columnAlpha(moving, 17) in 1..80)
            assertTrue(columnAlpha(moving, 22) in 180..254)
            assertEquals(0, columnAlpha(moving, 15), "Text must not reach the artwork")
            assertFalse(initial.contentEquals(moving))
            value.value = "Short"
            settle()
            clockReads = 0
            val short = draw("short")
            assertEquals(0, clockReads, "Non-overflowing text bypasses the marquee and its clock")
            assertEquals(0, columnAlpha(short, 17))
            assertEquals(0, columnAlpha(short, 224))
        } finally { scene.close(); recomposer.close() }
    }
}
