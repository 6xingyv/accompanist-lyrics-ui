package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.runtime.Composer
import androidx.compose.runtime.CompositionTracer
import androidx.compose.runtime.InternalComposeTracingApi
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.IntSize
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import com.mocharealm.accompanist.lyrics.ui.composable.list.LyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.profile.LatinProfile
import com.mocharealm.accompanist.lyrics.ui.profile.LyricsProfile
import com.mocharealm.accompanist.lyrics.ui.preparation.MeasuredLyricsLine
import com.mocharealm.accompanist.lyrics.ui.profile.ProfileTextUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

@OptIn(InternalComposeTracingApi::class, InternalComposeUiApi::class)
class LyricsRecompositionTest {
    @Test
    fun captionTogglesKeepCurrentAggregateAtAnchor() {
        val lyrics = SyncedLyrics(List(12) { index ->
            KaraokeLine.MainKaraokeLine(
                listOf(KaraokeSyllable("Main lyric", index * 10000, (index + 1) * 10000, phonetic = "pronunciation")),
                "Translation", KaraokeAlignment.Start, index * 10000, (index + 1) * 10000,
            )
        })
        val translation = mutableStateOf(true)
        val phonetic = mutableStateOf(true)
        val state = LyricsLazyListState()
        val recomposer = FrameRecomposer(kotlinx.coroutines.Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(frameRecomposer = recomposer, size = IntSize(400, 900))
        val canvas = Canvas(ImageBitmap(400, 900))
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
                KaraokeLyricsView(state, lyrics, { 35000 }, {}, {},
                    showTranslation = translation.value, showPhonetic = phonetic.value)
            }
            val deadline = System.nanoTime() + 10_000_000_000L
            while (state.items.isEmpty() && System.nanoTime() < deadline) { frame(); Thread.sleep(5) }
            assertTrue(state.items.isNotEmpty())
            repeat(180) { frame() }
            val anchor = state.heights.top(3) - state.position + state.chain.offset(3)
            for (toggle in listOf(translation, phonetic, translation, phonetic)) {
                toggle.value = !toggle.value
                repeat(90) {
                    frame()
                    assertEquals(anchor, state.heights.top(3) - state.position + state.chain.offset(3), 1.0,
                        "Caption resize must not move the current lyric, frame=$it")
                    assertEquals(0f, state.chain.offset(3), 0.1f, "Caption resize must not excite the anchor spring")
                }
            }
        } finally { scene.close(); recomposer.close() }
    }

    @Test
    fun leadingVocalsEndWithoutMovingOrResizingTheirMainItem() {
        val acc =
            KaraokeLine.AccompanimentKaraokeLine(
                listOf(KaraokeSyllable("echo", 500, 3500)),
                null,
                KaraokeAlignment.Start,
                500,
                3500,
            )
        val main =
            KaraokeLine.MainKaraokeLine(
                listOf(KaraokeSyllable("main", 2000, 10000)),
                null,
                KaraokeAlignment.Start,
                2000,
                10000,
                accompanimentLines = listOf(acc),
            )
        // Some parsers expose the same accompaniment both before its owner and nested in it.
        val lyrics = SyncedLyrics(listOf(acc, main))
        val position = mutableIntStateOf(3400)
        val state = LyricsLazyListState()
        val recomposer = FrameRecomposer(kotlinx.coroutines.Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(frameRecomposer = recomposer, size = IntSize(600, 900))
        val canvas = Canvas(ImageBitmap(600, 900))
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
                KaraokeLyricsView(
                    listState = state,
                    lyrics = lyrics,
                    currentPosition = { position.intValue },
                    onLineClicked = {},
                    onLinePressed = {},
                )
            }
            val deadline = System.nanoTime() + 10_000_000_000L
            while (state.items.isEmpty() && System.nanoTime() < deadline) {
                frame()
                Thread.sleep(5)
            }
            repeat(240) { frame() }
            assertEquals(1, state.items.size)
            val height = state.heights.height(0)
            val scroll = state.position
            val measures = state.measurePasses
            repeat(40) {
                position.intValue += 16
                frame()
                assertEquals(height, state.heights.height(0), "Vocals must keep their space")
                assertEquals(
                    scroll,
                    state.position,
                    0.01,
                    "Vocals ending must not restart following",
                )
            }
            assertEquals(measures, state.measurePasses, "Crossing the vocal end must only redraw")
        } finally {
            scene.close()
            recomposer.close()
        }
    }

    @Test
    fun playbackRedrawsWithoutRecomposingLyrics() {
        val events = mutableMapOf<String, Int>()
        val tracer =
            object : CompositionTracer {
                override fun isTraceInProgress() = true

                override fun traceEventStart(key: Int, dirty1: Int, dirty2: Int, info: String) {
                    if (info.startsWith("com.mocharealm.accompanist.lyrics.ui.")) {
                        val name = info.substringBefore(" (")
                        events[name] = (events[name] ?: 0) + 1
                    }
                }

                override fun traceEventEnd() {}
            }
        val uiThread = Thread.currentThread()
        val uiRasterizations = AtomicInteger()
        val listState = LyricsLazyListState()
        val measurements = AtomicInteger()
        val draws = AtomicInteger()
        val profile =
            object : LyricsProfile by LatinProfile {
                override fun prepare(
                    line: MeasuredLyricsLine,
                    group: List<KaraokeSyllable>,
                    measurer: TextMeasurer,
                    style: TextStyle,
                ): List<ProfileTextUnit> {
                    measurements.incrementAndGet()
                    return LatinProfile.prepare(line, group, measurer, style)
                }

                override fun DrawScope.draw(unit: ProfileTextUnit, color: Color, shadow: Shadow) {
                    if (Thread.currentThread() === uiThread) uiRasterizations.incrementAndGet()
                    draws.incrementAndGet()
                    with(LatinProfile) { draw(unit, color, shadow) }
                }
            }
        val lyrics =
            SyncedLyrics(
                List(6) { index ->
                    val start = index * 12000
                    KaraokeLine.MainKaraokeLine(
                        listOf(
                            KaraokeSyllable("Singing", start, start + 10000, phonetic = "singing")
                        ),
                        "Translation",
                        KaraokeAlignment.Start,
                        start,
                        start + 10000,
                        accompanimentLines =
                            if (index == 0)
                                listOf(
                                    KaraokeLine.AccompanimentKaraokeLine(
                                        listOf(KaraokeSyllable("echo", 6000, 7000)),
                                        null,
                                        KaraokeAlignment.Start,
                                        6000,
                                        7000,
                                    )
                                )
                            else null,
                    )
                }
            )
        val captions = mutableStateOf(true)
        val position = mutableIntStateOf(1000)
        val recomposer = FrameRecomposer(kotlinx.coroutines.Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(frameRecomposer = recomposer, size = IntSize(600, 900))
        val bitmap = ImageBitmap(600, 900)
        val canvas = Canvas(bitmap)
        var nanos = 0L
        fun frame() {
            Snapshot.sendApplyNotifications()
            nanos += 16_666_667L
            recomposer.performFrame(nanos)
            scene.measureAndLayout()
            scene.draw(canvas)
        }
        Composer.setTracer(tracer)
        try {
            scene.setContent {
                KaraokeLyricsView(
                    listState = listState,
                    lyrics = lyrics,
                    currentPosition = { position.intValue },
                    onLineClicked = {},
                    onLinePressed = {},
                    renderProfiles = listOf(profile),
                    showTranslation = captions.value,
                    showPhonetic = captions.value,
                )
            }
            val deadline = System.nanoTime() + 10_000_000_000L
            while (
                !events.keys.any { it.endsWith("PreparedLineText") } && System.nanoTime() < deadline
            ) {
                frame()
                Thread.sleep(5)
            }
            assertTrue(draws.get() > 0, "Prepared scene must actually be drawn")
            assertTrue(
                events.keys.any { it.endsWith("PreparedLineText") },
                "Tracing must observe actual lyrics composition",
            )
            repeat(120) { frame() }
            val preparedCount = measurements.get()
            val drawCount = draws.get()
            val initialFirstHeight = listState.heights.height(0)
            val itemStructure = listState.items
            captions.value = false
            repeat(25) { frame() }
            captions.value = true
            repeat(120) { frame() }
            assertEquals(
                preparedCount,
                measurements.get(),
                "Caption toggles cannot rebuild shaping/layout",
            )
            assertEquals(drawCount, draws.get(), "Caption toggles cannot rasterize text again")
            assertTrue(
                itemStructure === listState.items,
                "Caption height animation cannot replace the list geometry index",
            )

            val beforePixels = IntArray(600 * 900)
            bitmap.readPixels(beforePixels)
            events.clear()
            repeat(120) {
                position.intValue += 16
                frame()
            }
            println("Steady playback composition events: $events")
            assertEquals(
                emptyMap(),
                events,
                "Playback should invalidate draw scopes, not composition",
            )
            assertEquals(drawCount, draws.get(), "Playback must reuse recorded text layers")
            val afterPixels = IntArray(600 * 900)
            bitmap.readPixels(afterPixels)
            assertTrue(
                !beforePixels.contentEquals(afterPixels),
                "Playback must actually change rendered pixels",
            )
            assertEquals(preparedCount, measurements.get())

            assertEquals(
                0,
                uiRasterizations.get(),
                "Item creation/drawing must never rasterize profiles on the UI thread",
            )
            kotlinx.coroutines.runBlocking { listState.scrollToItem(5) }
            repeat(3) { frame() }
            kotlinx.coroutines.runBlocking { listState.scrollToItem(0) }
            repeat(3) { frame() }
            assertEquals(
                drawCount,
                draws.get(),
                "Recycled items must reuse scene-owned raster pages",
            )
            events.clear()
            position.intValue = -1
            repeat(120) { frame() }
            events.clear()
            position.intValue = 6000
            frame()
            repeat(120) { frame() }
            println("Accompaniment entry composition events: $events")
            assertTrue(
                events.keys.any { it.endsWith("PreparedLineText") },
                "Accompaniment must enter composition",
            )
            events.clear()
            repeat(60) {
                position.intValue += 8
                frame()
            }
            println("Steady accompaniment composition events: $events")
            assertEquals(emptyMap(), events)
            assertEquals(
                preparedCount,
                measurements.get(),
                "Showing cached accompaniment must not prepare it again",
            )
            position.intValue = 10000
            frame()
            repeat(120) { frame() }
            println("Accompaniment exit composition events: $events")
            events.clear()
            repeat(30) {
                position.intValue += 8
                frame()
            }
            assertEquals(emptyMap(), events)
            assertEquals(preparedCount, measurements.get())

            position.intValue = 12500
            frame()
            repeat(120) { frame() }
            println("Focus transition composition events: $events")
            assertEquals(
                1, // The previous line already lost focus when its accompaniment exited.
                events["com.mocharealm.accompanist.lyrics.ui.composable.lyrics.LyricsLineItem"],
            )
            assertEquals(
                null,
                events["com.mocharealm.accompanist.lyrics.ui.internal.rendering.PreparedLineText"],
            )
            events.clear()
            repeat(60) {
                position.intValue += 16
                frame()
            }
            println("After focus transition composition events: $events")
            assertEquals(
                emptyMap(),
                events,
                "Focus effects must settle without continuing recomposition",
            )
            assertEquals(preparedCount, measurements.get())
            position.intValue = 72000
            repeat(240) { frame() }
            position.intValue = 1000
            repeat(240) { frame() }
            assertEquals(
                initialFirstHeight,
                listState.heights.height(0),
                "Looping must restore the same settled accompaniment/caption height",
            )
            assertEquals(0, listState.firstVisibleItemIndex)
        } finally {
            scene.close()
            recomposer.close()
            Composer.setTracer(null)
        }
    }
}
