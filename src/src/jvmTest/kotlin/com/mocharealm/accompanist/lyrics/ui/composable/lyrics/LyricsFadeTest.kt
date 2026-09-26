package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.*
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.ui.internal.effects.lyricsEdgeFade
import kotlinx.coroutines.Dispatchers
import kotlin.math.abs
import kotlin.test.*

@OptIn(InternalComposeUiApi::class)
class LyricsFadeTest {
    @Test
    fun independentEdgesTrackAnchorAndViewportAndAllowZeroAndOverlap() {
        var top by mutableStateOf<LyricsFade>(LyricsFade.ToAnchor(16.dp))
        var bottom by mutableStateOf<LyricsFade>(LyricsFade.Fraction(0.5f))
        var anchor by mutableStateOf(40.dp)
        val recomposer = FrameRecomposer(Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(recomposer, size = IntSize(100, 100))
        val bitmap = ImageBitmap(100, 200)
        val canvas = Canvas(bitmap)
        val clear = Paint().apply { blendMode = BlendMode.Clear }
        var nanos = 0L
        fun check(height: Int, topLength: Float, bottomLength: Float) {
            Snapshot.sendApplyNotifications()
            nanos += 16_666_667L
            recomposer.performFrame(nanos)
            scene.measureAndLayout()
            canvas.drawRect(0f, 0f, 100f, 200f, clear)
            scene.draw(canvas)
            val pixels = IntArray(100 * 200)
            bitmap.readPixels(pixels)
            for (y in 0 until height) {
                val t = if (topLength == 0f) 1f else ((y + 0.5f) / topLength).coerceIn(0f, 1f)
                val b = if (bottomLength == 0f) 1f else ((height - y - 0.5f) / bottomLength).coerceIn(0f, 1f)
                val actual = pixels[y * 100 + 50] ushr 24
                assertTrue(abs(actual - 255f * t * b) <= 2f, "y=$y alpha=$actual expected=${255f * t * b}")
            }
        }
        try {
            scene.setContent {
                Box(Modifier.fillMaxSize().graphicsLayer {
                    compositingStrategy = CompositingStrategy.Offscreen
                }.lyricsEdgeFade(top, bottom, anchor).background(Color.White))
            }
            check(100, 24f, 50f)
            bottom = LyricsFade.Fraction(0.2f)
            check(100, 24f, 20f)
            scene.size = IntSize(100, 200)
            anchor = 80.dp
            check(200, 64f, 40f)
            anchor = 8.dp
            bottom = LyricsFade.Fixed(0.dp)
            check(200, 0f, 0f)
            top = LyricsFade.Fixed(400.dp)
            bottom = LyricsFade.Fraction(1f)
            check(200, 200f, 200f)
        } finally {
            scene.close()
            recomposer.close()
        }
    }
}
