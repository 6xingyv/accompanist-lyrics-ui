package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.*
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.*
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import com.mocharealm.accompanist.lyrics.ui.internal.effects.LyricsReveal
import com.mocharealm.accompanist.lyrics.ui.internal.layout.settledHeight
import com.mocharealm.accompanist.lyrics.ui.internal.playback.LyricsPlaybackState
import com.mocharealm.accompanist.lyrics.ui.internal.rendering.LyricsRenderResources
import com.mocharealm.accompanist.lyrics.ui.internal.rendering.PreparedLineText
import com.mocharealm.accompanist.lyrics.ui.internal.rendering.RowPaints
import com.mocharealm.accompanist.lyrics.ui.internal.rendering.drawPreparedRow
import com.mocharealm.accompanist.lyrics.ui.internal.test.prepare
import com.mocharealm.accompanist.lyrics.ui.internal.test.source
import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedLyrics
import com.mocharealm.accompanist.lyrics.ui.profile.*
import kotlinx.coroutines.Dispatchers
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.test.*

@OptIn(InternalComposeUiApi::class)
class InlinePhoneticRevealTest {
    @Test
    fun hiddenCaptionsKeepTheirNodesAcrossRapidReversalsWithoutReservingHeight() {
        var visible by mutableStateOf(false)
        var creations = 0
        var disposals = 0
        var height = -1
        val recomposer = FrameRecomposer(Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(recomposer, size = IntSize(300, 200))
        val canvas = Canvas(ImageBitmap(300, 200))
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
                Box(Modifier.onGloballyPositioned { height = it.size.height }) {
                    LyricsReveal(visible, keepContent = true) {
                        DisposableEffect(Unit) {
                            creations++
                            onDispose { disposals++ }
                        }
                        Spacer(Modifier.size(100.dp, 50.dp))
                    }
                }
            }
            repeat(120) { frame() }
            assertEquals(0, height)
            repeat(120) { index ->
                if (index % 2 == 0) visible = !visible
                frame()
            }
            visible = false
            repeat(120) { frame() }
            assertEquals(0, height, "Hidden content must leave no spacing")
            assertEquals(1, creations, "Caption nodes must survive every reversal")
            assertEquals(0, disposals)
        } finally {
            scene.close()
            recomposer.close()
        }
        assertEquals(1, disposals)
    }

    @Test
    fun separatePronunciationRevealPreservesRenderedPositionsAndSweepForLtrAndRtl() {
        for ((text, direction) in listOf("生活", "سلام").flatMap { text ->
            LayoutDirection.entries.map { text to it }
        }) {
            val base = if (text == "生活") CjkProfile else ArabicProfile
            val profile = object : LyricsProfile by base {
                override fun effects(units: List<ProfileTextUnit>, accompaniment: Boolean) =
                    base.effects(units, accompaniment).copy(glow = false)
            }
            val line = prepare(source(text).copy(
                syllables = listOf(KaraokeSyllable(text, 1000, 3000, phonetic = "reading")),
            ), width = 268f, profiles = listOf(profile))
            val row = line.rows.single()
            val lyrics = PreparedLyrics(listOf(line))
            val playback = LyricsPlaybackState(lyrics)
            val resources = LyricsRenderResources(lyrics, Color.White, Density(1f), direction)
            val raster = resources.raster(line)
            val paints = RowPaints(Color.White)
            val recomposer = FrameRecomposer(Dispatchers.Unconfined)
            val scene = CanvasLayersComposeScene(recomposer, size = IntSize(300, 200))
            val actual = ImageBitmap(300, 200)
            val canvas = Canvas(actual)
            var nanos = 0L
            try {
                scene.setContent {
                    CompositionLocalProvider(LocalLayoutDirection provides direction) {
                        PreparedLineText(line, playback, resources,
                            currentTimeProvider = { playback.row(row).time.intValue }, verticalPadding = 0.dp)
                    }
                }
                for (time in listOf(0, 1500, Int.MAX_VALUE)) {
                    playback.row(row).time.intValue = time
                    repeat(3) {
                        Snapshot.sendApplyNotifications()
                        nanos += 16_666_667L
                        recomposer.performFrame(nanos)
                        scene.measureAndLayout()
                        canvas.drawRect(0f, 0f, 300f, 200f, Paint().apply { blendMode = BlendMode.Clear })
                        scene.draw(canvas)
                    }
                    val expected = ImageBitmap(300, 200)
                    CanvasDrawScope().draw(Density(1f), direction, Canvas(expected), Size(300f, 200f)) {
                        translate(left = 16f) {
                            drawPreparedRow(row, time, resources.row(row), Color.White, paints, false, raster.rows.single())
                        }
                    }
                    val actualPixels = IntArray(300 * 200).also { actual.readPixels(it) }
                    val expectedPixels = IntArray(300 * 200).also { expected.readPixels(it) }
                    val captionTop = (row.height - row.phoneticHeight).roundToInt()
                    val captionRange = captionTop * 300 until row.height.roundToInt() * 300
                    assertTrue(captionRange.any { actualPixels[it] ushr 24 > 0 }, "Caption must be painted")
                    val alphaDifference = captionRange.sumOf {
                        abs((actualPixels[it] ushr 24) - (expectedPixels[it] ushr 24))
                    }
                    assertTrue(alphaDifference <= 100, "$text ($direction) at $time moved/changed pronunciation pixels: $alphaDifference")
                }
            } finally {
                scene.close()
                recomposer.close()
            }
        }
    }

    @Test
    fun wrappedPhoneticsRemoveAllSpacingAndFollowCaptionRevealOnReversal() {
        for (initiallyShown in listOf(true, false)) {
            for (words in listOf(listOf("生活", "道路", "世界"), listOf("سلام", "عالم", "سلام"))) {
                val line = prepare(source(words.joinToString("")).copy(
                    syllables = words.mapIndexed { index, text ->
                        KaraokeSyllable(text, 1000 + index * 1000, 2000 + index * 1000,
                            phonetic = "reading")
                    },
                ), width = 110f)
                assertTrue(line.rows.size > 1, "Fixture must wrap")
                val lyrics = PreparedLyrics(listOf(line))
                val playback = LyricsPlaybackState(lyrics)
                val resources = LyricsRenderResources(lyrics, Color.White, Density(1f), LayoutDirection.Ltr)
                val raster = resources.raster(line)
                line.rows.forEach { playback.row(it).time.intValue = Int.MAX_VALUE }
                val originalHeight = line.rows.sumOf { (it.height - it.phoneticHeight).roundToInt() }
                val captionHeight = line.rows.sumOf { (it.phoneticHeight + it.phoneticSpacingBefore).toDouble() }
                assertEquals(0f, line.rows.first().phoneticSpacingBefore)
                assertTrue(line.rows.drop(1).all { it.phoneticSpacingBefore == 8f })
                assertEquals(originalHeight.toFloat(), line.settledHeight(playback, false, false, 1f), 0.01f)
                var shown by mutableStateOf(initiallyShown)
                var actualHeight = 0
                var referenceHeight = 0
                val recomposer = FrameRecomposer(Dispatchers.Unconfined)
                val scene = CanvasLayersComposeScene(recomposer, size = IntSize(320, 800))
                val canvas = Canvas(ImageBitmap(320, 800))
                var nanos = 0L
                fun frame() {
                    Snapshot.sendApplyNotifications()
                    nanos += 16_666_667L
                    recomposer.performFrame(nanos)
                    scene.measureAndLayout()
                    scene.draw(canvas)
                    assertTrue(abs(actualHeight - referenceHeight) <= line.rows.size,
                        "Inline height must follow caption spring, actual=$actualHeight reference=$referenceHeight")
                }
                try {
                    scene.setContent {
                        Row {
                            PreparedLineText(line, playback, resources,
                                currentTimeProvider = { Int.MAX_VALUE },
                                modifier = Modifier.width(142.dp).onGloballyPositioned { actualHeight = it.size.height },
                                verticalPadding = 0.dp, showTranslation = false, showPhonetic = shown)
                            Column(Modifier.onGloballyPositioned { referenceHeight = it.size.height }) {
                                Spacer(Modifier.height(originalHeight.dp))
                                LyricsReveal(shown) {
                                    Spacer(Modifier.size(110.dp, captionHeight.toFloat().dp))
                                }
                            }
                        }
                    }
                    repeat(120) { frame() }
                    if (!shown) assertEquals(originalHeight, actualHeight, "Initially hidden leaves no spacing")
                    shown = !shown
                    repeat(8) { frame() }
                    assertTrue(actualHeight > originalHeight && actualHeight < line.height.roundToInt())
                    shown = !shown
                    repeat(120) { frame() }
                    shown = false
                    repeat(120) { frame() }
                    assertEquals(originalHeight, actualHeight, "All inline phonetic gaps must collapse")
                    shown = true
                    repeat(120) { frame() }
                    assertEquals(line.height.roundToInt(), actualHeight)
                    assertSame(raster, resources.raster(line), "Toggling reuses the original raster")
                    assertTrue(line.rows.all { playback.row(it).time.intValue == Int.MAX_VALUE })
                } finally {
                    scene.close()
                    recomposer.close()
                }
            }
        }
    }

    @Test
    fun wrappedRowsWithoutInlinePhoneticsReserveNoCaptionSpacing() {
        val line = prepare(source("生活道路世界"), width = 65f)
        assertTrue(line.rows.size > 1)
        assertTrue(line.rows.all { it.phoneticHeight == 0f && it.phoneticSpacingBefore == 0f })
        assertEquals(line.rows.sumOf { it.height.toDouble() }.toFloat(), line.height, 0.01f)
    }
}
