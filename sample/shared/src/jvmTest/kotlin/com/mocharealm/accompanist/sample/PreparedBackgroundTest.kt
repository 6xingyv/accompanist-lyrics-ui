package com.mocharealm.accompanist.sample

import androidx.compose.ui.graphics.*
import androidx.compose.ui.unit.IntSize
import com.mocharealm.accompanist.sample.ui.composable.background.backgroundBitmap
import com.mocharealm.accompanist.sample.ui.composable.background.prepareBackground
import kotlin.test.*
import kotlinx.coroutines.runBlocking

class PreparedBackgroundTest {
    @Test
    fun preparedBackgroundIsBoundedOpaqueAndRespondsToArtworkAndLuminance() = runBlocking {
        val red = backgroundBitmap(IntArray(64 * 64) { 0xffff0000.toInt() }, 64, 64)
        val blue = backgroundBitmap(IntArray(64 * 64) { 0xff0000ff.toInt() }, 64, 64)
        val viewport = IntSize(1080, 2400)
        val bright = prepareBackground(red, 0f, viewport, 2.75f)
        val dark = prepareBackground(red, 1f, viewport, 2.75f)
        val next = prepareBackground(blue, 0f, viewport, 2.75f)
        assertEquals(512, bright.height)
        assertEquals(230, bright.width)
        val pixels = IntArray(bright.width * bright.height)
        bright.readPixels(pixels)
        assertTrue(pixels.all { it ushr 24 == 255 })
        val center = bright.toPixelMap()[bright.width / 2, bright.height / 2]
        val darkCenter = dark.toPixelMap()[dark.width / 2, dark.height / 2]
        val nextCenter = next.toPixelMap()[next.width / 2, next.height / 2]
        assertTrue(center.red > 0.9f && center.blue < 0.01f)
        assertTrue(darkCenter.red < center.red * 0.6f)
        assertTrue(nextCenter.blue > 0.9f && nextCenter.red < 0.01f)
    }
}
