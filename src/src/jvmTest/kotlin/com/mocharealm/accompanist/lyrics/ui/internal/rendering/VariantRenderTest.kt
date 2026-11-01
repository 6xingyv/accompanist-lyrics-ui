package com.mocharealm.accompanist.lyrics.ui.internal.rendering

import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.*
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import com.mocharealm.accompanist.lyrics.ui.internal.playback.LyricsPlaybackState
import com.mocharealm.accompanist.lyrics.ui.internal.test.TestSceneDispatcher
import com.mocharealm.accompanist.lyrics.ui.internal.test.prepare
import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedLine
import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedLyrics
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import kotlin.test.*

@OptIn(InternalComposeUiApi::class)
class VariantRenderTest {
    private fun romajiLine(alignment: KaraokeAlignment = KaraokeAlignment.Start) =
        KaraokeLine.MainKaraokeLine(
            listOf(
                KaraokeSyllable("生 ", 1000, 2000, phonetic = "seikatsu"),
                KaraokeSyllable("道", 2000, 3000, phonetic = "douro"),
            ),
            null,
            alignment,
            1000,
            3000,
        )

    /** Compact slots fit exactly, so the reserved widths alone force a second aligned row. */
    private fun squeezed(): PreparedLine {        val probe = prepare(romajiLine(), width = 1000f)
        val compact =
            assertNotNull(probe.compact).runs.flatMap { it.groups }.sumOf { it.width.toDouble() }
                .toFloat()
        return prepare(romajiLine(), width = compact + 0.5f)
    }

    @Test
    fun hiddenPronunciationPaintsTheCompactGeometry() {
        val width = 1100
        val height = 220
        val line = squeezed()
        val compact = assertNotNull(line.compact)
        val lyrics = PreparedLyrics(listOf(line))
        val playback = LyricsPlaybackState(lyrics)
        for (row in line.rows + compact.rows) playback.row(row).time.intValue = Int.MAX_VALUE
        val resources = LyricsRenderResources(lyrics, Color.White, Density(1f), LayoutDirection.Ltr)
        resources.raster(line)
        val compactRaster = resources.raster(compact)
        val dispatcher = TestSceneDispatcher()
        val recomposer = FrameRecomposer(dispatcher)
        val scene = CanvasLayersComposeScene(recomposer, size = IntSize(width, height))
        val actual = ImageBitmap(width, height)
        val canvas = Canvas(actual)
        var nanos = 0L
        fun frame() {
            dispatcher.runCurrent()
            Snapshot.sendApplyNotifications()
            nanos += 16_666_667L
            recomposer.performFrame(nanos)
            scene.measureAndLayout()
            canvas.drawRect(
                0f,
                0f,
                width.toFloat(),
                height.toFloat(),
                Paint().apply { blendMode = BlendMode.Clear },
            )
            scene.draw(canvas)
        }
        try {
            scene.setContent {
                PreparedLineText(
                    line,
                    playback,
                    resources,
                    currentTimeProvider = { Int.MAX_VALUE },
                    verticalPadding = 0.dp,
                    showTranslation = false,
                    showPhonetic = false,
                )
            }
            repeat(160) { frame() }
            val expected = ImageBitmap(width, height)
            CanvasDrawScope().draw(
                Density(1f),
                LayoutDirection.Ltr,
                Canvas(expected),
                Size(width.toFloat(), height.toFloat()),
            ) {
                translate(left = 16f) {
                    compact.rows.forEachIndexed { index, row ->
                        translate(top = -row.top) {
                            drawPreparedRow(
                                row,
                                Int.MAX_VALUE,
                                resources.row(row),
                                Color.White,
                                RowPaints(Color.White),
                                false,
                                compactRaster.rows.getOrNull(index),
                            )
                        }
                    }
                }
            }
            val actualPixels = IntArray(width * height).also { actual.readPixels(it) }
            val expectedPixels = IntArray(width * height).also { expected.readPixels(it) }
            val difference =
                actualPixels.indices.sumOf {
                    abs((actualPixels[it] ushr 24) - (expectedPixels[it] ushr 24))
                }
            assertTrue(
                difference <= 400,
                "Hidden pronunciation must paint the compact geometry: alpha difference $difference",
            )
        } finally {
            scene.close()
            recomposer.close()
            dispatcher.runCurrent()
        }
    }

    @Test
    fun hidingPronunciationSlidesTheRowBeforeCommitting() {
        val width = 1100
        val height = 220
        val line = prepare(romajiLine(KaraokeAlignment.End), width = 1000f)
        val compact = assertNotNull(line.compact)
        assertEquals(1, line.rows.size)
        assertEquals(1, compact.rows.size)
        val freed = line.rows.single().width - compact.rows.single().width
        assertTrue(freed > 1f, "Fixture must free caption width")
        val lyrics = PreparedLyrics(listOf(line))
        val playback = LyricsPlaybackState(lyrics)
        for (row in line.rows + compact.rows) playback.row(row).time.intValue = Int.MAX_VALUE
        val resources = LyricsRenderResources(lyrics, Color.White, Density(1f), LayoutDirection.Ltr)
        resources.raster(line)
        resources.raster(compact)
        var shown by mutableStateOf(true)
        val dispatcher = TestSceneDispatcher()
        val recomposer = FrameRecomposer(dispatcher)
        val scene = CanvasLayersComposeScene(recomposer, size = IntSize(width, height))
        val surface = ImageBitmap(width, height)
        val canvas = Canvas(surface)
        var nanos = 0L
        var pixels = IntArray(width * height)
        fun frame() {
            dispatcher.runCurrent()
            Snapshot.sendApplyNotifications()
            nanos += 16_666_667L
            recomposer.performFrame(nanos)
            scene.measureAndLayout()
            canvas.drawRect(
                0f,
                0f,
                width.toFloat(),
                height.toFloat(),
                Paint().apply { blendMode = BlendMode.Clear },
            )
            scene.draw(canvas)
            pixels = IntArray(width * height).also { surface.readPixels(it) }
        }
        fun inkLeft(): Int {
            for (x in 0 until width) for (y in 0 until height) {
                if (pixels[y * width + x] ushr 24 != 0) return x
            }
            return width
        }
        try {
            scene.setContent {
                PreparedLineText(
                    line,
                    playback,
                    resources,
                    currentTimeProvider = { Int.MAX_VALUE },
                    verticalPadding = 0.dp,
                    showTranslation = false,
                    showPhonetic = shown,
                )
            }
            repeat(140) { frame() }
            val alignedLeft = inkLeft()
            shown = false
            repeat(5) { frame() }
            val midLeft = inkLeft()
            repeat(200) { frame() }
            val compactLeft = inkLeft()
            assertEquals(
                freed,
                (compactLeft - alignedLeft).toFloat(),
                2.5f,
                "Settled geometry must be the compact one",
            )
            assertTrue(
                midLeft > alignedLeft && midLeft < compactLeft,
                "Units must slide between both geometries: aligned=$alignedLeft mid=$midLeft compact=$compactLeft",
            )
        } finally {
            scene.close()
            recomposer.close()
            dispatcher.runCurrent()
        }
    }
}
