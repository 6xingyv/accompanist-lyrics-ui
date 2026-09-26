package com.mocharealm.accompanist.sample

import androidx.compose.ui.unit.IntSize
import com.mocharealm.accompanist.sample.ui.composable.artworkSize
import kotlin.test.Test
import kotlin.test.assertEquals

class ArtworkSizeTest {
    @Test
    fun matchesMeasuredPixelsWithoutUpscalingOrDistortion() {
        val original = IntSize(3000, 3000)
        // 60dp phone cover at density 3, and a larger tablet cover.
        assertEquals(IntSize(180, 180), artworkSize(original, IntSize(180, 180)))
        assertEquals(IntSize(1200, 1200), artworkSize(original, IntSize(1200, 1200)))
        assertEquals(IntSize(180, 90), artworkSize(IntSize(3000, 1500), IntSize(180, 180)))
        assertEquals(IntSize(90, 180), artworkSize(IntSize(1500, 3000), IntSize(180, 180)))
        assertEquals(IntSize(64, 32), artworkSize(IntSize(64, 32), IntSize(180, 180)))
    }
}
